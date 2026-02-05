pub mod config;
pub mod error;
mod netinf_watcher;
mod rest;

use crate::{
    error::{IoHandlerResult, ManagementAgentError, ManagementAgentResult},
    rest::{
        app_name, refresh_netinfs, vsc_rx_config_create, vsc_rx_create, vsc_rx_delete,
        vsc_rx_update, vsc_start, vsc_stop, vsc_tx_config_create, vsc_tx_create, vsc_tx_delete,
        vsc_tx_update,
    },
};
use aes67_rs::{
    config::{Config, adjust_labels_for_channel_count},
    error::{ConfigError, VscApiError, VscApiResult},
    formats::{AudioFormat, FrameFormat, Seconds, Session, SessionId},
    monitoring::Monitoring,
    nic::find_nic_with_name,
    receiver::{
        api::ReceiverApi,
        config::{PartialReceiverConfig, ReceiverConfig, RefClk, SessionInfo},
    },
    sender::{
        api::SenderApi,
        config::{PartialSenderConfig, SenderConfig},
    },
    time::{Clock, get_clock},
    vsc::VirtualSoundCardApi,
};
use aes67_rs_discovery::{DiscoveryApi, start_discovery};
use axum::routing::{get, post};
use axum_server::Handle;
use miette::{Context, IntoDiagnostic, Result};
use pnet::datalink::NetworkInterface;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::SubsystemHandle;
use tracing::{error, info, warn};
use worterbuch::{
    PersistenceMode,
    server::{CloneableWbApi, axum::build_worterbuch_router},
};
use worterbuch_client::{Key, KeyValuePair, Worterbuch, topic};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

enum VscApiMessage {
    StartVsc(oneshot::Sender<ManagementAgentResult<()>>),
    StopVsc(oneshot::Sender<ManagementAgentResult<()>>),
    CreateSender(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    CreateReceiver(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    UpdateSender(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    UpdateReceiver(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    DeleteSender(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    DeleteReceiver(SessionId, oneshot::Sender<ManagementAgentResult<()>>),
    Exit,
    CreateSenderConfig(oneshot::Sender<ManagementAgentResult<()>>),
    CreateReceiverConfig(Option<Sdp>, oneshot::Sender<ManagementAgentResult<()>>),
}

#[derive(serde::Deserialize)]
pub struct Sdp {
    pub sdp: Option<SdpSource>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SdpSource {
    Content(String),
    Url(String),
    SessionId(String),
}

#[derive(Clone)]
pub struct ManagementAgentApi {
    api_tx: mpsc::Sender<VscApiMessage>,
}

impl ManagementAgentApi {
    pub fn new(
        subsys: &SubsystemHandle,
        app_id: String,
        wb: Worterbuch,
        io_handler: impl IoHandler + 'static,
    ) -> Self {
        let (api_tx, api_rx) = mpsc::channel(1);

        let discovery = start_discovery(subsys, app_id.clone(), wb.clone());

        let app_idc = app_id.clone();
        let wbc = wb.clone();
        let discc = discovery.clone();
        subsys.spawn("api", |s| async move {
            let api_actor = VscApiActor::new(s, app_idc, api_rx, wbc, io_handler, discc);
            api_actor.run().await
        });

        Self { api_tx }
    }

    pub async fn start_vsc(&self) -> ManagementAgentResult<()> {
        info!("Starting VSC …");
        let (tx, rx) = oneshot::channel();

        self.api_tx.send(VscApiMessage::StartVsc(tx)).await?;

        rx.await??;

        info!("VSC started.");

        Ok(())
    }

    pub async fn stop_vsc(&self) -> ManagementAgentResult<()> {
        info!("Stopping VSC …");
        let (tx, rx) = oneshot::channel();

        self.api_tx.send(VscApiMessage::StopVsc(tx)).await?;

        rx.await??;

        Ok(())
    }
    pub async fn create_sender_config(&self) -> ManagementAgentResult<()> {
        info!("Creating sender config …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateSenderConfig(tx))
            .await?;

        rx.await??;

        Ok(())
    }
    pub async fn create_receiver_config(&self, sdp: Option<Sdp>) -> ManagementAgentResult<()> {
        info!("Creating receiver config …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiverConfig(sdp, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn create_sender(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Instantiating sender {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn create_receiver(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Instantiating receiver {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn update_sender(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Updating sender {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn update_receiver(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Updating receiver {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn delete_sender(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Deleting sender {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DeleteSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn delete_receiver(&self, id: SessionId) -> ManagementAgentResult<()> {
        info!("Deleting receiver {id} …");
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DeleteReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    async fn exit(&self) -> ManagementAgentResult<()> {
        info!("Exiting …");
        self.api_tx.send(VscApiMessage::Exit).await?;
        Ok(())
    }
}

struct VscApiActor<IOH: IoHandler> {
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<VscApiMessage>,
    wb: Worterbuch,
    app_id: String,
    vsc_api: Option<VirtualSoundCardApi>,
    io_handler: IOH,
    discovery: DiscoveryApi,
}

impl<IOH: IoHandler> VscApiActor<IOH> {
    fn new(
        subsys: SubsystemHandle,
        app_id: String,
        rx: mpsc::Receiver<VscApiMessage>,
        wb: Worterbuch,
        io_handler: IOH,
        discovery: DiscoveryApi,
    ) -> Self {
        Self {
            subsys,
            rx,
            wb,
            app_id,
            vsc_api: None,
            io_handler,
            discovery,
        }
    }

    async fn run(mut self) -> Result<()> {
        loop {
            select! {
                Some(msg) = self.rx.recv() => self.process_api_message(msg).await?,
                _ = self.subsys.shutdown_requested() => break,
                else => break,
            }
        }

        Ok(())
    }

    async fn process_api_message(&mut self, msg: VscApiMessage) -> Result<()> {
        match msg {
            VscApiMessage::StartVsc(tx) => {
                let _ = tx.send(self.start_vsc().await);
            }
            VscApiMessage::StopVsc(tx) => {
                let _ = tx.send(self.stop_vsc().await);
            }

            VscApiMessage::CreateSenderConfig(tx) => {
                let _ = tx.send(self.create_sender_config().await);
            }
            VscApiMessage::CreateReceiverConfig(sdp, tx) => {
                let _ = tx.send(self.create_receiver_config(sdp).await);
            }
            VscApiMessage::CreateSender(id, tx) => {
                let _ = tx.send(self.create_sender(id).await);
            }
            VscApiMessage::CreateReceiver(id, tx) => {
                let _ = tx.send(self.create_receiver(id).await);
            }
            VscApiMessage::UpdateSender(id, tx) => {
                let _ = tx.send(self.update_sender(id).await);
            }
            VscApiMessage::UpdateReceiver(id, tx) => {
                let _ = tx.send(self.update_receiver(id).await);
            }
            VscApiMessage::DeleteSender(id, tx) => {
                let _ = tx.send(self.delete_sender(id).await);
            }
            VscApiMessage::DeleteReceiver(id, tx) => {
                let _ = tx.send(self.delete_receiver(id).await);
            }
            VscApiMessage::Exit => {
                self.subsys.request_global_shutdown();
            }
        };

        Ok(())
    }

    async fn start_vsc(&mut self) -> ManagementAgentResult<()> {
        if self.vsc_api.is_some() {
            return Err(VscApiError::AlreadyRunning.into());
        }

        let config = Config::load(&self.app_id, &self.wb).await?;

        info!("Starting AES67-VSC …");
        info!("Using configuration: {:?}", config);

        let name = self.app_id.clone();
        let wb = self.wb.clone();
        let clock = get_clock(
            name.clone(),
            config.ptp,
            config.audio.sample_rate,
            wb.clone(),
        )
        .await?;
        let audio_nic = find_nic_with_name(&config.audio.nic)?;

        let vsc_api = VirtualSoundCardApi::new(name, &self.subsys, wb, clock, audio_nic).await?;

        self.vsc_api = Some(vsc_api);

        self.autostart_senders().await?;

        self.autostart_receivers().await?;

        Ok(())
    }

    async fn autostart_senders(&mut self) -> Result<(), ManagementAgentError> {
        info!("Autostarting senders …");
        let senders = self
            .wb
            .pget::<bool>(topic!(self.app_id, "config", "tx", "?", "autostart"))
            .await?;

        let senders_to_autostart = senders
            .into_iter()
            .filter_map(|kvp| if kvp.value { Some(kvp.key) } else { None });

        for sender in senders_to_autostart {
            let Some(id) = sender.split('/').nth(3).and_then(|id| id.parse().ok()) else {
                warn!("Could not parse sender id from key {}", sender);
                continue;
            };
            if let Err(e) = self
                .create_sender(id)
                .await
                .into_diagnostic()
                .wrap_err_with(|| format!("Could not autostart sender {}", id))
            {
                error!("{e}");
            }
        }

        Ok(())
    }

    async fn autostart_receivers(&mut self) -> Result<(), ManagementAgentError> {
        info!("Autostarting receivers …");
        let receivers = self
            .wb
            .pget::<bool>(topic!(self.app_id, "config", "rx", "?", "autostart"))
            .await?;

        let receivers_to_autostart = receivers
            .into_iter()
            .filter_map(|kvp| if kvp.value { Some(kvp.key) } else { None });

        let _: () = for receiver in receivers_to_autostart {
            let Some(id) = receiver.split('/').nth(3).and_then(|id| id.parse().ok()) else {
                warn!("Could not parse receiver id from key {}", receiver);
                continue;
            };
            if let Err(e) = self
                .create_receiver(id)
                .await
                .into_diagnostic()
                .wrap_err_with(|| format!("Could not autostart receiver {}", id))
            {
                error!("{e}");
            }
        };
        Ok(())
    }

    async fn stop_vsc(&mut self) -> ManagementAgentResult<()> {
        match self.vsc_api.take() {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                vsc_api.close().await?;
            }
        }

        Ok(())
    }

    async fn create_sender_config(&self) -> ManagementAgentResult<()> {
        self.wb
            .locked(topic!(self.app_id, "config", "tx"), || {
                self.do_create_sender_config()
            })
            .await?
    }

    async fn do_create_sender_config(&self) -> ManagementAgentResult<()> {
        let id = self.next_id(true);

        let mut config = PartialSenderConfig::default();

        if let Some(multicast_address) = self.get_free_multicast_address(id).await? {
            let multicast_address = SocketAddr::new(
                multicast_address,
                config.target.map(|a| a.port()).unwrap_or(5004),
            );
            config.target = Some(multicast_address);
        }

        self.publish_sender_config(id, &config).await?;

        Ok(())
    }

    async fn create_receiver_config(&mut self, sdp: Option<Sdp>) -> ManagementAgentResult<()> {
        self.wb
            .locked(topic!(self.app_id, "config", "rx"), || {
                self.do_create_receiver_config(sdp)
            })
            .await?
    }

    async fn do_create_receiver_config(&self, sdp: Option<Sdp>) -> ManagementAgentResult<()> {
        let config = match sdp {
            Some(sdp) => match sdp.sdp {
                Some(SdpSource::Content(content)) => {
                    PartialReceiverConfig::from_sdp_content(&content)?
                }
                Some(SdpSource::Url(url)) => PartialReceiverConfig::from_sdp_url(&url).await?,
                Some(SdpSource::SessionId(session_id)) => {
                    self.fetch_config_from_session(session_id).await?
                }
                None => PartialReceiverConfig::default(),
            },
            None => PartialReceiverConfig::default(),
        };

        let id = self.next_id(false);

        self.publish_receiver_config(id, &config).await?;

        Ok(())
    }

    async fn fetch_config_from_session(
        &self,
        session_id: String,
    ) -> ManagementAgentResult<PartialReceiverConfig> {
        let Some(session_info) = self.discovery.fetch_session_info(session_id).await? else {
            return Err(ManagementAgentError::ConfigError(
                ConfigError::MissingReceiverConfig,
            ));
        };
        Ok(PartialReceiverConfig::from_session_info(&session_info))
    }

    fn next_id(&self, _tx: bool) -> SessionId {
        // TODO make sure this is not used yet
        // we are already in a locked context here, so anything we do here is atomic,
        // just don't use async wb API calls here
        // TODO also make sure that ids are monotonically increasing
        rand::random_range(100_000_000_000..MAX_SAFE_INTEGER)
    }

    async fn create_sender(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_sender_config(id).await?;
                let (api, monitoring, clock) = vsc_api.create_sender(config.clone()).await?;
                if let Err(e) = self
                    .io_handler
                    .sender_created(
                        self.app_id.clone(),
                        self.subsys.clone(),
                        api,
                        config.clone(),
                        clock,
                        monitoring,
                    )
                    .await
                {
                    error!("Could not create I/O handler for sender '{}': {}", id, e);
                    vsc_api.destroy_sender(id).await?;
                    return Err(e.into());
                } else {
                    self.announce_session(config).await?;
                }
            }
        }

        Ok(())
    }

    async fn announce_session(&mut self, config: SenderConfig) -> Result<(), ManagementAgentError> {
        Ok(match self.session_info_from_sender_config(&config).await {
            Ok(info) => {
                self.discovery.announce_session(info).await?;
            }
            Err(e) => {
                warn!("Could not create session info from sender config: {}", e);
            }
        })
    }

    async fn revoke_session(&self, id: SessionId) -> Result<(), ManagementAgentError> {
        self.discovery.revoke_session(id).await?;
        Ok(())
    }

    async fn create_receiver(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_receiver_config(id).await?;
                let (api, monitoring, clock) = vsc_api.create_receiver(config.clone()).await?;
                if let Err(e) = self
                    .io_handler
                    .receiver_created(
                        self.app_id.clone(),
                        self.subsys.clone(),
                        api,
                        config,
                        clock,
                        monitoring,
                    )
                    .await
                {
                    vsc_api.destroy_receiver(id).await?;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    async fn update_sender(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_sender_config(id).await?;
                vsc_api.update_sender(config).await?;
                self.io_handler.sender_updated(id).await?;
            }
        }

        Ok(())
    }

    async fn update_receiver(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_receiver_config(id).await?;
                vsc_api.update_receiver(config).await?;
                self.io_handler.receiver_updated(id).await?;
            }
        }

        Ok(())
    }

    async fn delete_sender(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                // Stop the JACK client (buffer producer) first, then destroy the sender
                // core (buffer consumer). This ensures the JACK callback can't access
                // the buffer after the consumer is destroyed.
                self.io_handler.sender_deleted(id).await?;
                self.revoke_session(id).await?;
                let res = vsc_api.destroy_sender(id).await;
                if let Err(e) = res {
                    self.wb
                        .pdelete_async(topic!(self.app_id, "tx", id, "#"), true)
                        .await?;
                    self.wb
                        .set_async(topic!(self.app_id, "config", "tx", id, "autostart"), false)
                        .await?;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    async fn delete_receiver(&mut self, id: SessionId) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                // Stop the JACK client (buffer consumer) first, then destroy the receiver
                // core (buffer producer). This ensures the JACK callback can't access
                // the buffer after the producer is destroyed.
                self.io_handler.receiver_deleted(id).await?;
                let res = vsc_api.destroy_receiver(id).await;
                if let Err(e) = res {
                    self.wb
                        .pdelete_async(topic!(self.app_id, "rx", id, "#"), true)
                        .await?;
                    self.wb
                        .set_async(topic!(self.app_id, "config", "rx", id, "autostart"), false)
                        .await?;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    async fn publish_sender_config(
        &self,
        id: SessionId,
        config: &PartialSenderConfig,
    ) -> VscApiResult<()> {
        self.wb
            .set_async(topic!(self.app_id, "config", "tx", id, "autostart"), false)
            .await?;

        if let Some(label) = &config.label {
            self.wb
                .set_async(topic!(self.app_id, "config", "tx", id, "name"), label)
                .await?;
        }

        if let Some(audio_format) = &config.audio_format {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "tx", id, "channels"),
                    audio_format.frame_format.channels,
                )
                .await?;
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "tx", id, "sampleFormat"),
                    audio_format.frame_format.sample_format,
                )
                .await?;
        }
        if let Some(target) = &config.target {
            // IP was already set while finding a  free multicast address
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "tx", id, "destinationPort"),
                    target.port(),
                )
                .await?;
        }
        if let Some(packet_time) = &config.packet_time {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "tx", id, "packetTime"),
                    packet_time,
                )
                .await?;
        }
        if let Some(payload_type) = &config.payload_type {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "tx", id, "payloadType"),
                    payload_type,
                )
                .await?;
        }
        self.wb
            .set_async(
                topic!(self.app_id, "config", "tx", id, "channelLabels"),
                &config.channel_labels,
            )
            .await?;
        self.wb
            .set_async(
                topic!(self.app_id, "config", "tx", id, "session", "version"),
                &config.version,
            )
            .await?;
        Ok(())
    }

    async fn fetch_sender_config(&self, id: SessionId) -> VscApiResult<SenderConfig> {
        let label = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "name"),
                "sender name not configured",
            )
            .await
            .unwrap_or_else(|_| id.to_string());

        let channels = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "channels"),
                "sender channels not configured",
            )
            .await?;

        let mut channel_labels = self
            .wb
            .get(topic!(self.app_id, "config", "tx", id, "channelLabels"))
            .await?
            .unwrap_or_else(|| Vec::with_capacity(channels));
        adjust_labels_for_channel_count(channels, &mut channel_labels);

        let sample_rate = self
            .config_param(
                topic!(self.app_id, "config", "audio", "sampleRate"),
                "audio sample rate not configured",
            )
            .await?;
        let sample_format = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "sampleFormat"),
                "sender sample format not configured",
            )
            .await?;

        let frame_format = FrameFormat {
            channels,
            sample_format,
        };
        let audio_format = AudioFormat {
            sample_rate,
            frame_format,
        };
        let target_ip = self
            .config_param::<String>(
                topic!(self.app_id, "config", "tx", id, "destinationIP"),
                "sender destination IP not configured",
            )
            .await?
            .parse()?;
        let target_port = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "destinationPort"),
                "sender destination port not configured",
            )
            .await?;
        let target = SocketAddr::new(target_ip, target_port);
        let payload_type = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "payloadType"),
                "sender payload type not configured",
            )
            .await?;
        let packet_time = self
            .config_param(
                topic!(self.app_id, "config", "tx", id, "packetTime"),
                "sender packet time not configured",
            )
            .await?;

        Ok(SenderConfig {
            id,
            label,
            audio_format,
            target,
            payload_type,
            channel_labels,
            packet_time,
        })
    }

    async fn publish_receiver_config(
        &self,
        id: SessionId,
        config: &PartialReceiverConfig,
    ) -> VscApiResult<()> {
        self.wb
            .set_async(topic!(self.app_id, "config", "rx", id, "autostart"), false)
            .await?;

        if let Some(label) = &config.label {
            self.wb
                .set_async(topic!(self.app_id, "config", "rx", id, "name"), label)
                .await?;
        }

        if let Some(audio_format) = &config.audio_format {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "channels"),
                    audio_format.frame_format.channels,
                )
                .await?;
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "sampleFormat"),
                    audio_format.frame_format.sample_format,
                )
                .await?;
        }
        if let Some(source) = &config.source {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "sourceIP"),
                    source.ip().to_string(),
                )
                .await?;
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "sourcePort"),
                    source.port(),
                )
                .await?;
        }
        if let Some(origin_ip) = &config.origin_ip {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "originIP"),
                    origin_ip.to_string(),
                )
                .await?;
        }
        if let Some(link_offset) = &config.link_offset {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "linkOffset"),
                    link_offset,
                )
                .await?;
        }
        if let Some(rtp_offset) = &config.rtp_offset {
            self.wb
                .set_async(
                    topic!(self.app_id, "config", "rx", id, "rtpOffset"),
                    rtp_offset,
                )
                .await?;
        }
        self.wb
            .set_async(
                topic!(self.app_id, "config", "rx", id, "channelLabels"),
                &config.channel_labels,
            )
            .await?;
        Ok(())
    }

    async fn fetch_receiver_config(&self, id: SessionId) -> VscApiResult<ReceiverConfig> {
        let label = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "name"),
                "receiver name not configured",
            )
            .await
            .unwrap_or_else(|_| id.to_string());

        let channels = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "channels"),
                "receiver channels not configured",
            )
            .await?;
        let sample_rate = self
            .config_param(
                topic!(self.app_id, "config", "audio", "sampleRate"),
                "audio sample rate not configured",
            )
            .await?;
        let sample_format = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "sampleFormat"),
                "receiver sample format not configured",
            )
            .await?;

        let frame_format = FrameFormat {
            channels,
            sample_format,
        };
        let audio_format = AudioFormat {
            sample_rate,
            frame_format,
        };
        let source_ip = self
            .config_param::<String>(
                topic!(self.app_id, "config", "rx", id, "sourceIP"),
                "receiver source IP not configured",
            )
            .await?
            .parse()?;
        let source_port = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "sourcePort"),
                "receiver source port not configured",
            )
            .await?;
        let source = SocketAddr::new(source_ip, source_port);
        let channel_labels = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "channelLabels"),
                "receiver channel labels not configured",
            )
            .await?;

        let delay_calculation_interval = self
            .wb
            .get::<Seconds>(topic!(self.app_id, "config", "delayCalculationInterval"))
            .await?;

        let link_offset = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "linkOffset"),
                "receiver link offset not configured",
            )
            .await?;

        let rtp_offset = self
            .config_param(
                topic!(self.app_id, "config", "rx", id, "rtpOffset"),
                "receiver rtp offset not configured",
            )
            .await?;

        let origin_ip = self
            .config_param::<String>(
                topic!(self.app_id, "config", "rx", id, "originIP"),
                "receiver origin IP not configured",
            )
            .await?
            .parse()?;

        let config = ReceiverConfig {
            id,
            audio_format,
            channel_labels,
            delay_calculation_interval,
            label,
            link_offset,
            rtp_offset,
            source,
            origin_ip,
        };
        Ok(config)
    }

    async fn config_param<T: DeserializeOwned>(
        &self,
        key: Key,
        msg: &'static str,
    ) -> VscApiResult<T> {
        let value = self
            .wb
            .get(key)
            .await?
            .ok_or(VscApiError::SenderConfigIncomplete(msg))?;
        Ok(value)
    }

    async fn session_info_from_sender_config(
        &self,
        config: &SenderConfig,
    ) -> ManagementAgentResult<SessionInfo> {
        let id = config.id;
        let version = self.session_version(config.id).await?.unwrap_or(1);

        let iface_name = self
            .wb
            .get::<String>(topic!(self.app_id, "config", "audio", "nic"))
            .await?
            .ok_or_else(|| {
                ConfigError::NoSuchNIC("no multicast audio network interface configured".to_owned())
            })?;
        let iface = find_nic_with_name(iface_name)?;

        let id = Session { id, version };
        let name = config.label.clone();
        let destination_ip = config.target.ip();
        let destination_port = config.target.port();
        let channels = config.audio_format.frame_format.channels;
        let sample_format = config.audio_format.frame_format.sample_format;
        let sample_rate = config.audio_format.sample_rate;
        let packet_time = config.packet_time;
        let origin_ip = self.local_ip(&iface).await?;
        let channel_labels = config.channel_labels.clone();
        let rtp_offset = 0;
        let payload_type = config.payload_type;
        let refclk = self.refclock().await?;

        Ok(SessionInfo {
            id,
            name,
            destination_ip,
            destination_port,
            channels,
            sample_format,
            sample_rate,
            packet_time,
            origin_ip,
            channel_labels,
            rtp_offset,
            payload_type,
            refclk,
        })
    }

    async fn session_version(&self, id: SessionId) -> ManagementAgentResult<Option<u64>> {
        let key = topic!(self.app_id, "config", "tx", id, "session", "version");
        let version = self.wb.get(key).await?;
        Ok(version)
    }

    async fn local_ip(&self, iface: &NetworkInterface) -> ManagementAgentResult<IpAddr> {
        // TODO how does this work with unicast streams?

        let ip = iface
            .ips
            .iter()
            .filter(|a| a.is_ipv4())
            .next()
            .or_else(|| iface.ips.first())
            .map(|a| a.ip())
            .ok_or_else(|| {
                ConfigError::NoSuchNIC(format!(
                    "could not find IP address for network interface '{}'",
                    iface.name
                ))
            })?;

        Ok(ip)
    }

    async fn refclock(&self) -> ManagementAgentResult<RefClk> {
        let standard = "IEEE1588-2008".to_owned();

        // TODO get mac of grandmaster
        let mac = "00-1D-C1-FF-FE-0F-FA-D0".to_owned();

        // TODO get from config
        let domain = 0;

        Ok(RefClk {
            standard,
            mac,
            domain,
        })
    }

    async fn get_free_multicast_address(
        &self,
        id: SessionId,
    ) -> ManagementAgentResult<Option<IpAddr>> {
        let sessions: Vec<IpAddr> = self
            .wb
            .pget::<SessionInfo>(topic!(self.app_id, "discovery", "sessions", "?", "config"))
            .await?
            .into_iter()
            .map(|kvp| kvp.value.destination_ip)
            .collect();

        let assigned_ips: Vec<IpAddr> = self
            .wb
            .pget::<String>(topic!(self.app_id, "config", "tx", "?", "destinationIP"))
            .await?
            .into_iter()
            .filter_map(|kvp| kvp.value.parse().ok())
            .collect();

        for prefix in 67..=255 {
            if let Some(addr) = self
                .find_multicast_address_in_prefix(id, &sessions, &assigned_ips, prefix)
                .await?
            {
                return Ok(Some(addr));
            }
        }
        for prefix in 0..67 {
            if let Some(addr) = self
                .find_multicast_address_in_prefix(id, &sessions, &assigned_ips, prefix)
                .await?
            {
                return Ok(Some(addr));
            }
        }

        Ok(None)
    }

    async fn find_multicast_address_in_prefix(
        &self,
        id: u64,
        sessions: &Vec<IpAddr>,
        assigned_ips: &Vec<IpAddr>,
        prefix: u8,
    ) -> ManagementAgentResult<Option<IpAddr>> {
        for j in 1..=255 {
            let candidate = IpAddr::V4(Ipv4Addr::from([239, 69, prefix, j]));
            if let Some(addr) = self
                .try_get_multicast_address(id, candidate, sessions, assigned_ips)
                .await?
            {
                return Ok(Some(addr));
            }
        }
        Ok(None)
    }

    async fn try_get_multicast_address(
        &self,
        id: SessionId,
        candidate: IpAddr,
        sessions: &[IpAddr],
        assigned_ips: &[IpAddr],
    ) -> ManagementAgentResult<Option<IpAddr>> {
        if sessions.iter().any(|it| it == &candidate)
            || assigned_ips.iter().any(|it| it == &candidate)
        {
            return Ok(None);
        }

        self.wb
            .set(
                topic!(self.app_id, "config", "tx", id, "destinationIP"),
                candidate.to_string(),
            )
            .await?;

        return Ok::<Option<IpAddr>, ManagementAgentError>(Some(candidate));
    }
}

pub trait IoHandler: Clone + Send + Sync + 'static {
    fn sender_created(
        &self,
        app_id: String,
        subsys: SubsystemHandle,
        sender: SenderApi,
        config: SenderConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn sender_updated(&self, id: SessionId) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn sender_deleted(&self, id: SessionId) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_created(
        &self,
        app_id: String,
        subsys: SubsystemHandle,
        receiver: ReceiverApi,
        config: ReceiverConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_updated(
        &mut self,
        id: SessionId,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_deleted(
        &mut self,
        id: SessionId,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;
}

pub async fn init_management_agent(
    subsys: &SubsystemHandle,
    app_id: String,
    port: u16,
    data_dir: impl AsRef<Path>,
    io_handler: impl IoHandler,
) -> Result<()> {
    let mut wb_config = worterbuch::Config::new(None).await?;
    wb_config.load_env_with_prefix("AES67_VSC")?;
    wb_config.persistence_mode = PersistenceMode::ReDB;
    wb_config.use_persistence = true;
    wb_config.data_dir = data_dir.as_ref().display().to_string();
    wb_config.ws_endpoint = None;
    wb_config.tcp_endpoint = None;
    wb_config.unix_endpoint = None;
    wb_config.extended_monitoring = true;
    let worterbuch = worterbuch::spawn_worterbuch(subsys, wb_config).await?;

    let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

    wb.set_grave_goods(&[
        &topic!(app_id, "metrics", "#"),
        &topic!(app_id, "discovery", "#"),
        &topic!(app_id, "tx", "#"),
        &topic!(app_id, "rx", "#"),
        &topic!(app_id, "networkInterfaces", "#"),
    ])
    .await
    .ok();
    wb.set_last_will(&[KeyValuePair {
        key: topic!(app_id, "running"),
        value: json!(false),
    }])
    .await
    .ok();

    Aes67VscRestApi::new(subsys, app_id, port, worterbuch, io_handler).await?;

    Ok(())
}

// TODO track sessions from which receivers were created and keep them up to date with changed SDP files

pub struct Aes67VscRestApi {}

impl Aes67VscRestApi {
    pub async fn new<'a>(
        subsys: &SubsystemHandle,
        app_id: String,
        port: u16,
        worterbuch: CloneableWbApi,
        io_handler: impl IoHandler,
    ) -> Result<()> {
        let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

        let wbc = wb.clone();

        let autostart = wb
            .get(topic!(app_id, "config", "autostart"))
            .await?
            .unwrap_or(false);

        info!("Starting VSC management agent …");
        let api = ManagementAgentApi::new(subsys, app_id.clone(), wb, io_handler);

        if autostart {
            info!("Autostarting VSC …");
            if let Err(e) = api
                .start_vsc()
                .await
                .into_diagnostic()
                .wrap_err("Could not start AES67-VSC REST API")
            {
                error!("{e}");
            };
        }

        let name = "aes67-rs-vsc-management-agent".to_owned();

        info!("Starting VSC management agent REST API …");
        subsys.spawn(name, async move |s: SubsystemHandle| {
            run_rest_api(s, app_id, port, worterbuch, wbc, api).await
        });

        Ok(())
    }
}

async fn run_rest_api(
    subsys: SubsystemHandle,
    app_id: String,
    port: u16,
    worterbuch: CloneableWbApi,
    wb: Worterbuch,
    api: ManagementAgentApi,
) -> ManagementAgentResult<()> {
    info!("Starting AES67-VSC REST API …");

    let netinf_watcher = start_network_interface_watcher(app_id.clone(), wb).await;

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;

    let app = build_worterbuch_router(
        &subsys,
        worterbuch,
        false,
        port,
        "127.0.0.1".to_owned(),
        true,
    )
    .await?
    .route(
        "/api/v1/backend/app-name",
        get(app_name).with_state(app_id.clone()),
    )
    .route(
        "/api/v1/refresh/netinf",
        post(refresh_netinfs).with_state(netinf_watcher),
    );

    let app = app
        .route("/api/v1/vsc/start", post(vsc_start).with_state(api.clone()))
        .route("/api/v1/vsc/stop", post(vsc_stop).with_state(api.clone()))
        .route(
            "/api/v1/vsc/tx/create/config",
            post(vsc_tx_config_create).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/tx/create",
            post(vsc_tx_create).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/tx/update",
            post(vsc_tx_update).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/tx/delete",
            post(vsc_tx_delete).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/rx/create/config",
            post(vsc_rx_config_create).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/rx/create",
            post(vsc_rx_create).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/rx/update",
            post(vsc_rx_update).with_state(api.clone()),
        )
        .route(
            "/api/v1/vsc/rx/delete",
            post(vsc_rx_delete).with_state(api.clone()),
        );

    info!("REST API is listening on {}", listener.local_addr()?);
    info!("Web UI is available at http://127.0.0.1:{port}",);

    let handle = Handle::new();

    let mut server = axum_server::from_tcp(listener.into_std()?);
    server.http_builder().http2().enable_connect_protocol();

    let mut serve = Box::pin(
        server
            .handle(handle.clone())
            .serve(app.into_make_service_with_connect_info::<SocketAddr>()),
    );

    select! {
        res = &mut serve => res?,
        _ = subsys.shutdown_requested() => {
            handle.graceful_shutdown(Some(Duration::from_secs(5)));
            serve.await?;
        },
    }

    info!("AES67-VSC REST API stopped.");

    Ok(())
}

async fn start_network_interface_watcher(
    app_id: String,
    wb: worterbuch_client::Worterbuch,
) -> netinf_watcher::Handle {
    netinf_watcher::start(app_id, Duration::from_secs(3), wb).await
}
