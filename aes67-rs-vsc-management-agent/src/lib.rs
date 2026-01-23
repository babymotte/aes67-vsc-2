pub mod config;
pub mod error;
mod netinf_watcher;
mod rest;

use crate::{
    config::{AppConfig, DEFAULT_PORT},
    error::{IoHandlerResult, ManagementAgentError, ManagementAgentResult},
    rest::{
        app_name, refresh_netinfs, vsc_rx_create, vsc_rx_delete, vsc_rx_update, vsc_start,
        vsc_stop, vsc_tx_create, vsc_tx_delete, vsc_tx_update,
    },
};
use aes67_rs::{
    config::Config,
    error::{VscApiError, VscApiResult},
    formats::{AudioFormat, FrameFormat, Seconds},
    monitoring::Monitoring,
    nic::find_nic_with_name,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    sender::{api::SenderApi, config::SenderConfig},
    time::{Clock, get_clock},
    vsc::VirtualSoundCardApi,
};
use aes67_rs_discovery::{sap::start_sap_discovery, state_transformers};
use axum::routing::{get, post};
use axum_server::Handle;
use miette::{Context, IntoDiagnostic, Result};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{io, net::SocketAddr, time::Duration};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::Subsystem;
use tracing::{error, info, warn};
use worterbuch::{
    PersistenceMode,
    server::{CloneableWbApi, axum::build_worterbuch_router},
};
use worterbuch_client::{Key, KeyValuePair, Worterbuch, topic};

enum VscApiMessage {
    StartVsc(oneshot::Sender<ManagementAgentResult<()>>),
    StopVsc(oneshot::Sender<ManagementAgentResult<()>>),
    CreateSender(u32, oneshot::Sender<ManagementAgentResult<()>>),
    CreateReceiver(u32, oneshot::Sender<ManagementAgentResult<()>>),
    UpdateSender(u32, oneshot::Sender<ManagementAgentResult<()>>),
    UpdateReceiver(u32, oneshot::Sender<ManagementAgentResult<()>>),
    DeleteSender(u32, oneshot::Sender<ManagementAgentResult<()>>),
    DeleteReceiver(u32, oneshot::Sender<ManagementAgentResult<()>>),
    Exit,
}

#[derive(Clone)]
pub struct ManagementAgentApi {
    // app_id: String,
    api_tx: mpsc::Sender<VscApiMessage>,
    // wb: Worterbuch,
}

impl ManagementAgentApi {
    pub fn new(
        subsys: &Subsystem,
        app_id: String,
        wb: Worterbuch,
        io_handler: impl IoHandler + Send + Sync + 'static,
    ) -> Self {
        let (api_tx, api_rx) = mpsc::channel(1);

        let app_idc = app_id.clone();
        let wbc = wb.clone();
        subsys.spawn("api", |s| async move {
            let api_actor = VscApiActor::new(s, app_idc, api_rx, wbc, io_handler);
            api_actor.run().await
        });

        Self {
            // app_id,
            api_tx,
            // wb
        }
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

    pub async fn create_sender(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn create_receiver(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::CreateReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn update_sender(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn update_receiver(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::UpdateReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn delete_sender(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DeleteSender(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    pub async fn delete_receiver(&self, id: u32) -> ManagementAgentResult<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .send(VscApiMessage::DeleteReceiver(id, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    async fn exit(&self) -> ManagementAgentResult<()> {
        self.api_tx.send(VscApiMessage::Exit).await?;
        Ok(())
    }
}

struct VscApiActor<IOH: IoHandler> {
    subsys: Subsystem,
    rx: mpsc::Receiver<VscApiMessage>,
    wb: Worterbuch,
    app_id: String,
    vsc_api: Option<VirtualSoundCardApi>,
    io_handler: IOH,
}

impl<IOH: IoHandler> VscApiActor<IOH> {
    fn new(
        subsys: Subsystem,
        app_id: String,
        rx: mpsc::Receiver<VscApiMessage>,
        wb: Worterbuch,
        io_handler: IOH,
    ) -> Self {
        Self {
            subsys,
            rx,
            wb,
            app_id,
            vsc_api: None,
            io_handler,
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
            .pget::<bool>(topic!(
                self.app_id,
                "config",
                "tx",
                "senders",
                "?",
                "autostart"
            ))
            .await?;

        let senders_to_autostart = senders
            .into_iter()
            .filter_map(|kvp| if kvp.value { Some(kvp.key) } else { None });

        for sender in senders_to_autostart {
            let Some(id) = sender.split('/').nth(4).and_then(|id| id.parse().ok()) else {
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
                eprintln!("{e:?}");
            }
        }

        Ok(())
    }

    async fn autostart_receivers(&mut self) -> Result<(), ManagementAgentError> {
        info!("Autostarting receivers …");
        let receivers = self
            .wb
            .pget::<bool>(topic!(
                self.app_id,
                "config",
                "rx",
                "receivers",
                "?",
                "autostart"
            ))
            .await?;

        let receivers_to_autostart = receivers
            .into_iter()
            .filter_map(|kvp| if kvp.value { Some(kvp.key) } else { None });

        Ok(for receiver in receivers_to_autostart {
            let Some(id) = receiver.split('/').nth(4).and_then(|id| id.parse().ok()) else {
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
                eprintln!("{e:?}");
            }
        })
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

    async fn create_sender(&mut self, id: u32) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_sender_config(id).await?;
                let (api, monitoring) = vsc_api.create_sender(config).await?;
                if let Err(e) = self.io_handler.sender_created(id, api, monitoring).await {
                    error!("Could not create I/O handler for sender '{}': {}", id, e);
                    vsc_api.destroy_sender(id).await?;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    async fn create_receiver(&mut self, id: u32) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                let config = self.fetch_receiver_config(id).await?;
                let (receiver, monitoring, clock) = vsc_api.create_receiver(config.clone()).await?;
                if let Err(e) = self
                    .io_handler
                    .receiver_created(
                        self.app_id.clone(),
                        self.subsys.clone(),
                        receiver,
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

    async fn update_sender(&mut self, id: u32) -> ManagementAgentResult<()> {
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

    async fn update_receiver(&mut self, id: u32) -> ManagementAgentResult<()> {
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

    async fn delete_sender(&mut self, id: u32) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                vsc_api.destroy_sender(id).await?;
                self.io_handler.sender_deleted(id).await?;
            }
        }

        Ok(())
    }

    async fn delete_receiver(&mut self, id: u32) -> ManagementAgentResult<()> {
        match &self.vsc_api {
            None => return Err(VscApiError::NotRunning.into()),
            Some(vsc_api) => {
                vsc_api.destroy_receiver(id).await?;
                self.io_handler.receiver_deleted(id).await?;
            }
        }

        Ok(())
    }

    async fn fetch_sender_config(&self, id: u32) -> VscApiResult<SenderConfig> {
        let label = self
            .config_param(
                topic!(self.app_id, "config", "tx", "senders", id, "name"),
                "sender name not configured",
            )
            .await
            .unwrap_or_else(|_| id.to_string());

        let channels = self
            .config_param(
                topic!(self.app_id, "config", "tx", "senders", id, "channels"),
                "sender channels not configured",
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
                topic!(self.app_id, "config", "tx", "senders", id, "sampleFormat"),
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
                topic!(self.app_id, "config", "tx", "senders", id, "destinationIP"),
                "sender destination IP not configured",
            )
            .await?
            .parse()?;
        let target_port = self
            .config_param(
                topic!(
                    self.app_id,
                    "config",
                    "tx",
                    "senders",
                    id,
                    "destinationPort"
                ),
                "sender destination port not configured",
            )
            .await?;
        let target = SocketAddr::new(target_ip, target_port);
        // TODO how do we manage payload type correctly?
        let payload_type = 97;
        // TODO fetch channel labels from worterbuch
        let channel_labels = None;

        Ok(SenderConfig {
            id,
            label,
            audio_format,
            target,
            payload_type,
            channel_labels,
        })
    }

    async fn fetch_receiver_config(&self, id: u32) -> VscApiResult<ReceiverConfig> {
        let label = self
            .config_param(
                topic!(self.app_id, "config", "rx", "receivers", id, "name"),
                "receiver name not configured",
            )
            .await
            .unwrap_or_else(|_| id.to_string());

        let channels = self
            .config_param(
                topic!(self.app_id, "config", "rx", "receivers", id, "channels"),
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
                topic!(self.app_id, "config", "rx", "receivers", id, "sampleFormat"),
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
                topic!(self.app_id, "config", "rx", "receivers", id, "sourceIP"),
                "receiver source IP not configured",
            )
            .await?
            .parse()?;
        let source_port = self
            .config_param(
                topic!(self.app_id, "config", "rx", "receivers", id, "sourcePort"),
                "receiver source port not configured",
            )
            .await?;
        let source = SocketAddr::new(source_ip, source_port);
        // TODO how do we manage payload type correctly?
        let payload_type = 97;
        // TODO fetch channel labels from worterbuch
        let channel_labels = None;

        let delay_calculation_interval = self
            .wb
            .get::<Seconds>(topic!(self.app_id, "config", "delayCalculationInterval"))
            .await?;

        let link_offset = self
            .config_param(
                topic!(self.app_id, "config", "rx", "receivers", id, "linkOffset"),
                "receiver link offset not configured",
            )
            .await?;

        let rtp_offset = self
            .config_param(
                topic!(self.app_id, "config", "rx", "receivers", id, "rtpOffset"),
                "receiver rtp offset not configured",
            )
            .await?;

        let origin_ip = self
            .config_param::<String>(
                topic!(self.app_id, "config", "rx", "receivers", id, "originIP"),
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
            payload_type,
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
}

pub trait IoHandler: Clone + Send + Sync + 'static {
    fn sender_created(
        &self,
        id: u32,
        sender_api: SenderApi,
        monitoring: Monitoring,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn sender_updated(&self, id: u32) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn sender_deleted(&self, id: u32) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_created(
        &self,
        app_id: String,
        subsys: Subsystem,
        receiver: ReceiverApi,
        config: ReceiverConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_updated(&mut self, id: u32) -> impl Future<Output = IoHandlerResult<()>> + Send;

    fn receiver_deleted(&mut self, id: u32) -> impl Future<Output = IoHandlerResult<()>> + Send;
}

pub async fn init_management_agent(
    subsys: &Subsystem,
    app_id: String,
    io_handler: impl IoHandler,
) -> Result<()> {
    let config = AppConfig::load(&app_id).await?;

    let id = config.name.clone();

    let dirs = directories::BaseDirs::new();
    let data_home = dirs
        .map(|d| d.data_dir().to_owned())
        .expect("could not find data dir");

    let data_dir = data_home.join(&id).join("data");

    let mut wb_config = worterbuch::Config::new().await?;
    wb_config.load_env_with_prefix("AES67_VSC")?;
    wb_config.persistence_mode = PersistenceMode::ReDB;
    wb_config.use_persistence = true;
    wb_config.data_dir = data_dir.display().to_string();
    wb_config.ws_endpoint = None;
    wb_config.tcp_endpoint = None;
    wb_config.unix_endpoint = None;
    wb_config.extended_monitoring = true;
    let worterbuch = worterbuch::spawn_worterbuch(subsys, wb_config).await?;

    let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

    wb.set_grave_goods(&[
        &topic!(id, "metrics", "#"),
        &topic!(id, "discovery", "#"),
        &topic!(id, "tx", "#"),
        &topic!(id, "rx", "#"),
        &topic!(id, "networkInterfaces", "#"),
    ])
    .await
    .ok();
    wb.set_last_will(&[KeyValuePair {
        key: topic!(id, "running"),
        value: json!(false),
    }])
    .await
    .ok();

    let wbc = wb.clone();
    let wbd = wb.clone();
    let id = app_id.clone();
    subsys.spawn("discovery", async |s| {
        let instance_name = id.clone();
        s.spawn("state-transformers", async |s| {
            state_transformers::start(s, id, wbc).await
        });
        s.spawn("sap", |s| start_sap_discovery(instance_name, wbd, s));
        s.shutdown_requested().await;
        Ok::<(), ManagementAgentError>(())
    });

    Aes67VscRestApi::new(subsys, config, worterbuch, io_handler).await?;

    Ok(())
}

// TODO track sessions from which receivers were created and keep them up to date with changed SDP files

pub struct Aes67VscRestApi {}

impl Aes67VscRestApi {
    pub async fn new<'a>(
        subsys: &Subsystem,
        persistent_config: AppConfig,
        worterbuch: CloneableWbApi,
        io_handler: impl IoHandler,
    ) -> Result<()> {
        let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

        let app_id = persistent_config.name.clone();

        let wbc = wb.clone();

        let autostart = wb
            .get(topic!(app_id, "config", "autostart"))
            .await?
            .unwrap_or(false);

        info!("Starting VSC management agent …");
        let api = ManagementAgentApi::new(subsys, app_id, wb, io_handler);

        if autostart {
            info!("Autostarting VSC …");
            if let Err(e) = api
                .start_vsc()
                .await
                .into_diagnostic()
                .wrap_err("Could not start AES67-VSC REST API")
            {
                error!("{e}");
                eprintln!("{e:?}");
            };
        }

        let name = "aes67-rs-vsc-management-agent".to_owned();

        info!("Starting VSC management agent REST API …");
        subsys.spawn(name, async |s| {
            run_rest_api(s, persistent_config, worterbuch, wbc, api).await
        });

        Ok(())
    }
}

async fn run_rest_api(
    subsys: Subsystem,
    mut persistent_config: AppConfig,
    worterbuch: CloneableWbApi,
    wb: Worterbuch,
    api: ManagementAgentApi,
) -> ManagementAgentResult<()> {
    info!("Starting AES67-VSC REST API …");

    let netinf_watcher = start_network_interface_watcher(persistent_config.name.clone(), wb).await;

    let mut port = persistent_config.web_ui.port;

    let listener = loop {
        match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(it) => {
                if port != persistent_config.web_ui.port {
                    persistent_config.web_ui.port = port;
                    persistent_config.store().await;
                }
                break it;
            }
            Err(_) => {
                if port >= u16::MAX {
                    port = DEFAULT_PORT;
                } else {
                    port += 1;
                }
                if port == persistent_config.web_ui.port {
                    return Err(ManagementAgentError::IoError(io::Error::other(
                        "could not find a free port",
                    )));
                }
            }
        }
    };

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
        get(app_name).with_state(persistent_config.name.clone()),
    )
    .route(
        "/api/v1/refresh/netinf",
        post(refresh_netinfs).with_state(netinf_watcher),
    );

    let app = app
        .route("/api/v1/vsc/start", post(vsc_start).with_state(api.clone()))
        .route("/api/v1/vsc/stop", post(vsc_stop).with_state(api.clone()))
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
