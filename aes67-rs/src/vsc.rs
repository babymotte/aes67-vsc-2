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
use crate::sender::config::SenderConfig;
use crate::sender::start_sender;
use crate::time::Clock;
use crate::{
    error::{
        ReceiverInternalError, ReceiverInternalResult, ToBoxedResult, VscApiResult,
        VscInternalError, VscInternalResult,
    },
    receiver::{api::ReceiverApi, config::ReceiverConfig, start_receiver},
    sender::api::SenderApi,
};
use pnet::datalink::NetworkInterface;
use std::collections::HashMap;
use tokio::select;
use tokio::sync::{mpsc, oneshot};
use tosub::Subsystem;
use tracing::info;
use worterbuch_client::{Worterbuch, topic};

type ApiMessageSender = mpsc::Sender<VscApiMessage>;

enum VscApiMessage {
    CreateSender(
        SenderConfig,
        oneshot::Sender<SenderInternalResult<(SenderApi, Monitoring)>>,
    ),
    UpdateSender(
        SenderConfig,
        oneshot::Sender<SenderInternalResult<SenderApi>>,
    ),
    DestroySenderById(u32, oneshot::Sender<SenderInternalResult<()>>),
    CreateReceiver(
        ReceiverConfig,
        oneshot::Sender<ReceiverInternalResult<(ReceiverApi, Monitoring, Clock)>>,
    ),
    UpdateReceiver(
        ReceiverConfig,
        oneshot::Sender<ReceiverInternalResult<ReceiverApi>>,
    ),
    DestroyReceiverById(u32, oneshot::Sender<ReceiverInternalResult<()>>),
    Stop(oneshot::Sender<()>),
}

#[derive(Debug, Clone)]
pub struct VirtualSoundCardApi {
    api_tx: mpsc::Sender<VscApiMessage>,
}

impl VirtualSoundCardApi {
    pub async fn new(
        name: String,
        subsys: &Subsystem,
        worterbuch_client: Worterbuch,
        clock: Clock,
        audio_nic: NetworkInterface,
    ) -> VscApiResult<Self> {
        Ok(
            VirtualSoundCardApi::try_new(name, subsys, worterbuch_client, clock, audio_nic)
                .await
                .boxed()?,
        )
    }

    async fn try_new(
        name: String,
        subsys: &Subsystem,
        worterbuch_client: Worterbuch,
        clock: Clock,
        audio_nic: NetworkInterface,
    ) -> VscInternalResult<Self> {
        let api_tx =
            VirtualSoundCardApi::create_vsc(name, subsys, worterbuch_client, clock, audio_nic)
                .await?;
        Ok(VirtualSoundCardApi { api_tx })
    }

    async fn create_vsc(
        name: String,
        subsys: &Subsystem,
        worterbuch_client: Worterbuch,
        clock: Clock,
        audio_nic: NetworkInterface,
    ) -> VscInternalResult<ApiMessageSender> {
        let subsystem_name = name.clone();
        let (api_tx, api_rx) = mpsc::channel(1024);
        #[cfg(feature = "tokio-metrics")]
        let wb = worterbuch_client.clone();

        subsys.spawn(subsystem_name.clone(), |s| async move {
            VirtualSoundCard::new(name, api_rx, s, worterbuch_client, clock, audio_nic)?
                .run()
                .await;
            Ok::<(), VscInternalError>(())
        });

        Ok(api_tx)
    }

    pub async fn create_sender(
        &self,
        config: SenderConfig,
    ) -> VscApiResult<(SenderApi, Monitoring)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateSender(config, tx))
            .await
            .ok();
        Ok(rx.await.map_err(SenderInternalError::from)??)
    }

    pub async fn update_sender(&self, config: SenderConfig) -> VscApiResult<SenderApi> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateSender(config, tx))
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
    ) -> VscApiResult<(ReceiverApi, Monitoring, Clock)> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiver(config, tx))
            .await
            .ok();
        Ok(rx.await.map_err(ReceiverInternalError::from)??)
    }

    pub async fn update_receiver(&self, config: ReceiverConfig) -> VscApiResult<ReceiverApi> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateReceiver(config, tx))
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
    clock: Clock,
    audio_nic: NetworkInterface,
    api_rx: mpsc::Receiver<VscApiMessage>,
    txs: HashMap<u32, SenderApi>,
    rxs: HashMap<u32, ReceiverApi>,
    // tx_names: HashMap<u32, String>,
    // rx_names: HashMap<u32, String>,
    monitoring: Monitoring,
    subsys: Subsystem,
    wb: Worterbuch,
}

impl VirtualSoundCard {
    fn new(
        name: String,
        api_rx: mpsc::Receiver<VscApiMessage>,
        subsys: Subsystem,
        worterbuch_client: Worterbuch,
        clock: Clock,
        audio_nic: NetworkInterface,
    ) -> VscInternalResult<Self> {
        info!("Creating virtual sound card '{}' …", name);
        let monitoring =
            start_monitoring_service(name.clone(), &subsys, worterbuch_client.clone())?;
        Ok(VirtualSoundCard {
            name,
            api_rx,
            txs: HashMap::new(),
            rxs: HashMap::new(),
            // tx_names: HashMap::new(),
            // rx_names: HashMap::new(),
            monitoring,
            subsys,
            #[cfg(any(feature = "tokio-metrics", feature = "statime"))]
            wb: worterbuch_client,
            clock,
            audio_nic,
        })
    }

    async fn run(mut self) {
        let vsc_id = self.name.clone();

        self.monitoring.vsc_state(VscState::VscCreated).await;

        loop {
            select! {
                Some(msg) = self.api_rx.recv() => {
                    match msg {
                        VscApiMessage::CreateSender(config, tx) => {
                            tx.send(self.create_sender(config).await).ok();
                        }
                        VscApiMessage::UpdateSender(config, tx) => {
                            tx.send(self.update_sender(config).await).ok();
                        }
                        VscApiMessage::DestroySenderById(id, tx) => {
                            tx.send(self.destroy_sender(id).await).ok();
                        }
                        VscApiMessage::CreateReceiver(config, tx) => {
                            tx.send(self.create_receiver(config).await).ok();
                        }
                        VscApiMessage::UpdateReceiver(config, tx) => {
                            tx.send(self.update_receiver(config).await).ok();
                        }
                        VscApiMessage::DestroyReceiverById(id, tx) => {
                            tx.send(self.destroy_receiver(id).await).ok();
                        }
                        VscApiMessage::Stop(tx) => {
                            info!("Stopping virtual sound card '{vsc_id}' …");
                            self.subsys.request_local_shutdown();
                            tx.send(()).ok();
                            break;
                        }
                    }
                }
                _ = self.subsys.shutdown_requested() => {
                    info!("Shutdown requested, stopping virtual sound card '{vsc_id}' …");
                    break;
                },
            }
        }

        info!("Virtual sound card '{vsc_id}' stopped.");
        self.wb.set(topic!(vsc_id, "running"), false).await.ok();
    }

    async fn create_sender(
        &mut self,
        config: SenderConfig,
    ) -> SenderInternalResult<(SenderApi, Monitoring)> {
        let id = config.id;
        let label = config.label.clone();
        let qualified_id = format!("{}/tx/{}", self.name, id);
        info!("Creating sender '{label}' ({qualified_id}) …");

        let monitoring = self.monitoring.child(qualified_id.clone());
        let sender_api = start_sender(
            self.name.clone(),
            qualified_id.clone(),
            label,
            self.audio_nic.clone(),
            config,
            monitoring.clone(),
            &self.subsys,
            #[cfg(feature = "tokio-metrics")]
            self.wb.clone(),
        )
        .await?;

        // self.tx_names.insert(id, name.clone());
        self.txs.insert(id, sender_api.clone());

        info!("Sender {qualified_id} successfully created.");
        Ok((sender_api, monitoring))
    }

    async fn update_sender(&mut self, config: SenderConfig) -> SenderInternalResult<SenderApi> {
        // TODO
        todo!()
    }

    async fn destroy_sender(&mut self, id: u32) -> SenderInternalResult<()> {
        info!("Destroying sender '{id}' …");

        let Some(api) = self.txs.remove(&id) else {
            return Err(SenderInternalError::NoSuchSender(id));
        };

        api.stop().await;

        info!("Sender '{id}' successfully destroyed.");

        Ok(())
    }

    async fn create_receiver(
        &mut self,
        config: ReceiverConfig,
    ) -> ReceiverInternalResult<(ReceiverApi, Monitoring, Clock)> {
        let id = config.id;
        let label = config.label.clone();
        let qualified_id = format!("{}/rx/{}", self.name, id);
        info!("Creating receiver '{label}' ({qualified_id}) …");

        let clock = self.clock.clone();
        let monitoring = self.monitoring.child(qualified_id.clone());
        let receiver_api = start_receiver(
            self.name.clone(),
            qualified_id.clone(),
            label,
            self.audio_nic.clone(),
            config,
            clock.clone(),
            monitoring.clone(),
            &self.subsys,
            #[cfg(feature = "tokio-metrics")]
            self.wb.clone(),
        )
        .await?;

        // self.rx_names.insert(id, name.clone());
        self.rxs.insert(id, receiver_api.clone());

        info!("Receiver {qualified_id} successfully created.");
        Ok((receiver_api, monitoring, clock))
    }

    async fn update_receiver(
        &mut self,
        config: ReceiverConfig,
    ) -> ReceiverInternalResult<ReceiverApi> {
        // TODO
        todo!()
    }

    async fn destroy_receiver(&mut self, id: u32) -> ReceiverInternalResult<()> {
        info!("Destroying receiver '{id}' …");

        let Some(api) = self.rxs.remove(&id) else {
            return Err(ReceiverInternalError::NoSuchReceiver(id));
        };

        api.stop().await;

        info!("Receiver '{id}' successfully destroyed.");

        Ok(())
    }
}
