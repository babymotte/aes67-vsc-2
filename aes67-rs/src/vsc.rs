/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::app::{propagate_exit, spawn_child_app, wait_for_start};
use crate::config::PtpMode;
use crate::error::{SenderInternalError, SenderInternalResult};
use crate::monitoring::{Monitoring, VscState, start_monitoring_service};
use crate::sender::config::SenderConfig;
use crate::sender::start_sender;
use crate::time::get_clock;
use crate::{
    error::{
        ReceiverInternalError, ReceiverInternalResult, ToBoxedResult, VscApiResult,
        VscInternalError, VscInternalResult,
    },
    receiver::{
        api::ReceiverApi,
        config::{ReceiverConfig, RxDescriptor},
        start_receiver,
    },
    sender::api::SenderApi,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_graceful_shutdown::SubsystemHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

type ApiMessageSender = mpsc::Sender<VscApiMessage>;

enum VscApiMessage {
    CreateSender(
        String,
        Box<SenderConfig>,
        oneshot::Sender<SenderInternalResult<(SenderApi, u32)>>,
    ),
    DestroySenderById(u32, oneshot::Sender<SenderInternalResult<()>>),
    CreateReceiver(
        String,
        Box<ReceiverConfig>,
        Option<PtpMode>,
        oneshot::Sender<ReceiverInternalResult<(ReceiverApi, Monitoring, u32)>>,
    ),
    DestroyReceiverById(u32, oneshot::Sender<ReceiverInternalResult<()>>),
    Stop(oneshot::Sender<()>),
}

#[derive(Debug, Clone)]
pub struct VirtualSoundCardApi {
    api_tx: mpsc::Sender<VscApiMessage>,
}

impl VirtualSoundCardApi {
    pub async fn new(name: String, shutdown_token: CancellationToken) -> VscApiResult<Self> {
        Ok(VirtualSoundCardApi::try_new(name, shutdown_token)
            .await
            .boxed()?)
    }

    async fn try_new(name: String, shutdown_token: CancellationToken) -> VscInternalResult<Self> {
        let api_tx = VirtualSoundCardApi::create_vsc(name, shutdown_token).await?;
        Ok(VirtualSoundCardApi { api_tx })
    }

    async fn create_vsc(
        name: String,
        shutdown_token: CancellationToken,
    ) -> VscInternalResult<ApiMessageSender> {
        let subsystem_name = format!("aes67-vsc-{name}");
        let (api_tx, api_rx) = mpsc::channel(1024);

        let subsystem = |s: SubsystemHandle| async move {
            VirtualSoundCard::new(name, api_rx, s.create_cancellation_token())?
                .run()
                .await;
            Ok::<(), VscInternalError>(())
        };

        let mut app = spawn_child_app(subsystem_name.clone(), subsystem, shutdown_token.clone())?;
        wait_for_start(subsystem_name, &mut app).await?;
        propagate_exit(app, shutdown_token);
        Ok(api_tx)
    }

    pub async fn create_sender(&self, config: SenderConfig) -> VscApiResult<(SenderApi, u32)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateSender(
                config.id.to_owned(),
                Box::new(config),
                tx,
            ))
            .await
            .ok();
        Ok(rx.await.map_err(SenderInternalError::from)??)
    }

    pub async fn destroy_sender(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DestroySenderById(id, tx))
            .await
            .ok();
        Ok(rx.await.map_err(SenderInternalError::from)??)
    }

    pub async fn create_receiver(
        &self,
        config: ReceiverConfig,
        ptp_mode: Option<PtpMode>,
    ) -> VscApiResult<(ReceiverApi, Monitoring, u32)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiver(
                config.id().to_owned(),
                Box::new(config),
                ptp_mode,
                tx,
            ))
            .await
            .ok();
        Ok(rx.await.map_err(ReceiverInternalError::from)??)
    }

    pub async fn destroy_receiver(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DestroyReceiverById(id, tx))
            .await
            .ok();
        Ok(rx.await.map_err(ReceiverInternalError::from)??)
    }

    pub async fn close(self) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx.send(VscApiMessage::Stop(tx)).await.ok();
        Ok(rx.await.map_err(VscInternalError::from).boxed()?)
    }
}

struct VirtualSoundCard {
    name: String,
    api_rx: mpsc::Receiver<VscApiMessage>,
    txs: HashMap<u32, SenderApi>,
    rxs: HashMap<u32, ReceiverApi>,
    // tx_names: HashMap<u32, String>,
    // rx_names: HashMap<u32, String>,
    tx_counter: u32,
    rx_counter: u32,
    monitoring: Monitoring,
    shutdown_token: CancellationToken,
}

impl VirtualSoundCard {
    fn new(
        name: String,
        api_rx: mpsc::Receiver<VscApiMessage>,
        shutdown_token: CancellationToken,
    ) -> VscInternalResult<Self> {
        let monitoring = start_monitoring_service(name.clone(), shutdown_token.clone())?;
        Ok(VirtualSoundCard {
            name,
            api_rx,
            txs: HashMap::new(),
            rxs: HashMap::new(),
            // tx_names: HashMap::new(),
            // rx_names: HashMap::new(),
            tx_counter: 0,
            rx_counter: 0,
            monitoring,
            shutdown_token,
        })
    }

    async fn run(mut self) {
        let vsc_id = self.name.clone();

        self.monitoring.vsc_state(VscState::VscCreated).await;

        while let Some(msg) = self.api_rx.recv().await {
            match msg {
                VscApiMessage::CreateSender(id, config, tx) => {
                    tx.send(self.create_sender(id, *config).await).ok();
                }
                VscApiMessage::DestroySenderById(id, tx) => {
                    tx.send(self.destroy_sender(id).await).ok();
                }
                VscApiMessage::CreateReceiver(id, config, ptp_mode, tx) => {
                    tx.send(self.create_receiver(id, *config, ptp_mode).await)
                        .ok();
                }
                VscApiMessage::DestroyReceiverById(id, tx) => {
                    tx.send(self.destroy_receiver(id).await).ok();
                }
                VscApiMessage::Stop(tx) => {
                    info!("Stopping Virtual sound card '{vsc_id}' …");
                    drop(self);
                    tx.send(()).ok();
                    break;
                }
            }
        }
    }

    async fn create_sender(
        &mut self,
        label: String,
        config: SenderConfig,
    ) -> SenderInternalResult<(SenderApi, u32)> {
        self.tx_counter += 1;
        let id = self.tx_counter;
        let qualified_id = format!("{}/tx/{}", self.name, id);
        info!("Creating sender '{label}' ({qualified_id}) …");

        let sender_api = start_sender(
            qualified_id.clone(),
            label,
            config,
            self.monitoring.child(qualified_id.clone()),
            self.shutdown_token.clone(),
        )
        .await?;

        // self.tx_names.insert(id, name.clone());
        self.txs.insert(id, sender_api.clone());

        info!("Sender {qualified_id} successfully created.");
        Ok((sender_api, id))
    }

    async fn destroy_sender(&mut self, id: u32) -> SenderInternalResult<()> {
        info!("Destroying sender '{id}' …");
        // TODO
        info!("Sender '{id}' successfully destroyed.");

        Ok(())
    }

    async fn create_receiver(
        &mut self,
        label: String,
        config: ReceiverConfig,
        ptp_mode: Option<PtpMode>,
    ) -> ReceiverInternalResult<(ReceiverApi, Monitoring, u32)> {
        self.rx_counter += 1;
        let id: u32 = self.rx_counter;
        let qulified_id = format!("{}/rx/{}", self.name, id);
        info!("Creating receiver '{label}' ({qulified_id}) …");

        let desc = RxDescriptor::try_from(&config)?;
        let clock = get_clock(ptp_mode, desc.audio_format)?;
        let monitoring = self.monitoring.child(qulified_id.clone());
        let receiver_api = start_receiver(
            qulified_id.clone(),
            label,
            config,
            clock,
            monitoring.clone(),
            self.shutdown_token.clone(),
        )
        .await?;

        // self.rx_names.insert(id, name.clone());
        self.rxs.insert(id, receiver_api.clone());

        info!("Receiver {qulified_id} successfully created.");
        Ok((receiver_api, monitoring, id))
    }

    async fn destroy_receiver(&mut self, id: u32) -> ReceiverInternalResult<()> {
        info!("Destroying receiver '{id}' …");
        // TODO
        info!("Receiver '{id}' successfully destroyed.");

        Ok(())
    }
}

impl Drop for VirtualSoundCard {
    fn drop(&mut self) {
        info!("Virtual sound card '{}' destroyed.", self.name);
    }
}
