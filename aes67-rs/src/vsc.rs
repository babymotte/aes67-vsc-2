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

use crate::error::{SenderInternalError, SenderInternalResult};
use crate::monitoring::{Monitoring, VscState, start_monitoring_service};
use crate::sender::config::{SenderConfig, TxDescriptor};
use crate::sender::start_sender;
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
    time::SystemMediaClock,
};
use std::{collections::HashMap, thread};
use tokio::{
    runtime,
    sync::{mpsc, oneshot},
};
use tracing::info;

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
        oneshot::Sender<ReceiverInternalResult<(ReceiverApi, u32)>>,
    ),
    DestroyReceiverById(u32, oneshot::Sender<ReceiverInternalResult<()>>),
    Stop(oneshot::Sender<()>),
}

#[derive(Debug, Clone)]
pub struct VirtualSoundCardApi {
    api_tx: mpsc::Sender<VscApiMessage>,
}

impl VirtualSoundCardApi {
    pub fn new_blocking(name: String) -> VscApiResult<Self> {
        Ok(VirtualSoundCardApi::try_new_blocking(name).boxed()?)
    }

    pub async fn new(name: String) -> VscApiResult<Self> {
        Ok(VirtualSoundCardApi::try_new(name).await.boxed()?)
    }

    fn try_new_blocking(name: String) -> VscInternalResult<Self> {
        let (result_rx, api_tx) = VirtualSoundCardApi::create_vsc(name)?;
        result_rx.blocking_recv()??;
        Ok(VirtualSoundCardApi { api_tx })
    }

    async fn try_new(name: String) -> VscInternalResult<Self> {
        let (result_rx, api_tx) = VirtualSoundCardApi::create_vsc(name)?;
        result_rx.await??;
        Ok(VirtualSoundCardApi { api_tx })
    }

    fn create_vsc(
        name: String,
    ) -> Result<
        (
            oneshot::Receiver<Result<(), VscInternalError>>,
            mpsc::Sender<VscApiMessage>,
        ),
        VscInternalError,
    > {
        let (result_tx, result_rx) = oneshot::channel();
        let (api_tx, api_rx) = mpsc::channel(1024);
        thread::Builder::new()
            .name(format!("aes67-vsc-{name}"))
            .spawn(move || {
                let vsc_future = VirtualSoundCard::new(name, api_rx).run();
                let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(it) => it,
                    Err(e) => {
                        result_tx.send(Err(VscInternalError::from(e))).ok();
                        return;
                    }
                };
                result_tx.send(Ok(())).ok();
                runtime.block_on(vsc_future);
            })?;
        Ok((result_rx, api_tx))
    }

    pub fn create_sender_blocking(&self, config: SenderConfig) -> VscApiResult<(SenderApi, u32)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::CreateSender(
                config.id.to_owned(),
                Box::new(config),
                tx,
            ))
            .ok();
        Ok(rx.blocking_recv().map_err(SenderInternalError::from)??)
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

    pub fn destroy_sender_blocking(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::DestroySenderById(id, tx))
            .ok();
        Ok(rx.blocking_recv().map_err(SenderInternalError::from)??)
    }

    pub async fn destroy_sender(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DestroySenderById(id, tx))
            .await
            .ok();
        Ok(rx.await.map_err(SenderInternalError::from)??)
    }

    pub fn create_receiver_blocking(
        &self,
        config: ReceiverConfig,
    ) -> VscApiResult<(ReceiverApi, u32)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::CreateReceiver(
                config.id().to_owned(),
                Box::new(config),
                tx,
            ))
            .ok();
        Ok(rx.blocking_recv().map_err(ReceiverInternalError::from)??)
    }

    pub async fn create_receiver(
        &self,
        config: ReceiverConfig,
    ) -> VscApiResult<(ReceiverApi, u32)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiver(
                config.id().to_owned(),
                Box::new(config),
                tx,
            ))
            .await
            .ok();
        Ok(rx.await.map_err(ReceiverInternalError::from)??)
    }

    pub fn destroy_receiver_blocking(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::DestroyReceiverById(id, tx))
            .ok();
        Ok(rx.blocking_recv().map_err(ReceiverInternalError::from)??)
    }

    pub async fn destroy_receiver(&self, id: u32) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DestroyReceiverById(id, tx))
            .await
            .ok();
        Ok(rx.await.map_err(ReceiverInternalError::from)??)
    }

    pub fn close_blocking(self) -> VscApiResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx.blocking_send(VscApiMessage::Stop(tx)).ok();
        Ok(rx.blocking_recv().map_err(VscInternalError::from).boxed()?)
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
    tx_names: HashMap<u32, String>,
    rx_names: HashMap<u32, String>,
    tx_counter: u32,
    rx_counter: u32,
    monitoring: Monitoring,
}

impl VirtualSoundCard {
    fn new(name: String, api_rx: mpsc::Receiver<VscApiMessage>) -> Self {
        let monitoring = start_monitoring_service(name.clone());
        VirtualSoundCard {
            name,
            api_rx,
            txs: HashMap::new(),
            rxs: HashMap::new(),
            tx_names: HashMap::new(),
            rx_names: HashMap::new(),
            tx_counter: 0,
            rx_counter: 0,
            monitoring,
        }
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
                VscApiMessage::CreateReceiver(id, config, tx) => {
                    tx.send(self.create_receiver(id, *config).await).ok();
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
        name: String,
        config: SenderConfig,
    ) -> SenderInternalResult<(SenderApi, u32)> {
        self.tx_counter += 1;
        let id = self.tx_counter;
        let display_name = format!("{}/tx/{}", self.name, name);
        info!("Creating sender '{display_name}' …");

        let desc = TxDescriptor::try_from(&config)?;
        let clock = SystemMediaClock::new(desc.audio_format);
        let sender_api = start_sender(
            display_name.clone(),
            config,
            self.monitoring.child(display_name.clone()),
        )
        .await?;

        self.tx_names.insert(id, name.clone());
        self.txs.insert(id, sender_api.clone());

        info!("Sender '{display_name}' successfully created.");
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
        name: String,
        config: ReceiverConfig,
    ) -> ReceiverInternalResult<(ReceiverApi, u32)> {
        self.rx_counter += 1;
        let id = self.rx_counter;
        let display_name = format!("{}/rx/{}", self.name, name);
        info!("Creating receiver '{display_name}' …");

        let desc = RxDescriptor::try_from(&config)?;
        let clock = SystemMediaClock::new(desc.audio_format);
        let receiver_api = start_receiver(
            display_name.clone(),
            config,
            clock,
            self.monitoring.child(display_name.clone()),
        )
        .await?;

        self.rx_names.insert(id, name.clone());
        self.rxs.insert(id, receiver_api.clone());

        info!("Receiver '{display_name}' successfully created.");
        Ok((receiver_api, id))
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
