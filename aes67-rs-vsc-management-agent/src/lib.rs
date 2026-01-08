pub mod config;
mod error;
mod netinf_watcher;

use crate::{
    config::{AppConfig, DEFAULT_PORT},
    error::{ManagementAgentError, ManagementAgentResult},
};
use aes67_rs::{
    app::{propagate_exit, spawn_child_app},
    config::Config,
    error::{VscApiError, VscApiResult},
    nic::find_nic_with_name,
    time::get_clock,
    vsc::VirtualSoundCardApi,
};
use aes67_rs_discovery::{sap::start_sap_discovery, state_transformers};
use axum::{
    extract::State,
    routing::{get, post},
};
use axum_server::Handle;
use miette::Report;
use miette::{IntoDiagnostic, Result};
use serde_json::json;
use std::{io, net::SocketAddr, process::Stdio, time::Duration};
use tokio::{
    process, select, spawn,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, warn};
use worterbuch::{
    PersistenceMode,
    server::{CloneableWbApi, axum::build_worterbuch_router},
};
use worterbuch_client::{KeyValuePair, Worterbuch, topic};

pub enum ManagementAgentApi {
    Config(ManagementAgentConfigApi),
    Vsc(ManagementAgentVscApi),
}

enum VscApiMessage {
    StartVsc(oneshot::Sender<VscApiResult<()>>),
}

pub struct ManagementAgentConfigApi(mpsc::Sender<VscApiMessage>);

impl ManagementAgentConfigApi {
    pub fn new(subsys: &SubsystemHandle, app_id: String, wb: Worterbuch) -> Self {
        let (api_tx, api_rx) = mpsc::channel(1);

        subsys.start(SubsystemBuilder::new(
            "api",
            async |s: &mut SubsystemHandle| {
                let api_actor = VscApiActor::new(s, app_id, api_rx, wb);
                api_actor.run().await
            },
        ));

        Self(api_tx)
    }

    pub async fn start_vsc(self) -> Result<ManagementAgentApi, (Report, ManagementAgentApi)> {
        let (tx, rx) = oneshot::channel();
        if let Err(e) = self
            .0
            .send(VscApiMessage::StartVsc(tx))
            .await
            .into_diagnostic()
        {
            return Err((e, ManagementAgentApi::Config(self)));
        }

        if let Err(e) = rx.await.into_diagnostic() {
            return Err((e, ManagementAgentApi::Config(self)));
        }

        Ok(ManagementAgentApi::Vsc(ManagementAgentVscApi(self.0)))
    }
}

pub struct ManagementAgentVscApi(mpsc::Sender<VscApiMessage>);

impl ManagementAgentVscApi {
    pub async fn create_sender(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn create_receiver(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn update_sender(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn update_receiver(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn delete_sender(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn delete_receiver(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn stop_vsc(self) -> Result<ManagementAgentConfigApi> {
        // TODO
        Ok(ManagementAgentConfigApi(self.0))
    }
}

struct VscApiActor<'a> {
    subsys: &'a mut SubsystemHandle,
    rx: mpsc::Receiver<VscApiMessage>,
    wb: Worterbuch,
    app_id: String,
    vsc_api: Option<VirtualSoundCardApi>,
}

impl<'a> VscApiActor<'a> {
    fn new(
        subsys: &'a mut SubsystemHandle,
        app_id: String,
        rx: mpsc::Receiver<VscApiMessage>,
        wb: Worterbuch,
    ) -> Self {
        Self {
            subsys,
            rx,
            wb,
            app_id,
            vsc_api: None,
        }
    }

    async fn run(mut self) -> Result<()> {
        loop {
            select! {
                Some(msg) = self.rx.recv() => self.process_api_message(msg).await?,
                else => break,
            }
        }

        Ok(())
    }

    async fn process_api_message(&mut self, msg: VscApiMessage) -> Result<()> {
        match msg {
            VscApiMessage::StartVsc(tx) => tx.send(self.start_vsc().await).ok(),
        };

        Ok(())
    }

    async fn start_vsc(&mut self) -> VscApiResult<()> {
        if self.vsc_api.is_some() {
            return Err(VscApiError::AlreadyRunning);
        }

        let config = Config::load(&self.app_id, &self.wb).await?;

        info!("Starting AES67-VSC …");
        info!("Using configuration: {:?}", config);

        let name = self.app_id.clone();
        let shutdown_token = self.subsys.create_cancellation_token();
        let wb = self.wb.clone();
        let clock = get_clock(
            name.clone(),
            config.ptp,
            config.audio.sample_rate,
            wb.clone(),
        )
        .await?;
        let audio_nic = find_nic_with_name(&config.audio.nic)?;

        let vsc_api = VirtualSoundCardApi::new(name, shutdown_token, wb, clock, audio_nic).await?;

        self.vsc_api = Some(vsc_api);

        Ok(())
    }
}

pub trait IoHandler: Send + Sync + 'static {
    // TODO define methods for handling AES67 I/O
}

pub async fn init_management_agent(
    subsys: &SubsystemHandle,
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
    subsys.start(SubsystemBuilder::new(
        "discovery",
        async |s: &mut SubsystemHandle| {
            let instance_name = id.clone();
            s.start(SubsystemBuilder::new(
                "state-transformers",
                async |s: &mut SubsystemHandle| state_transformers::start(s, id, wbc).await,
            ));
            start_sap_discovery(&instance_name, wbd, s.create_cancellation_token()).await
        },
    ));

    // TODO get VSC config from worterbuch
    // TODO register I/O handler

    let management_agent = Aes67VscRestApi::new(subsys, config, worterbuch).await?;

    Ok(())
}

// TODO track sessions from which receivers were created and keep them up to date with changed SDP files

pub struct Aes67VscRestApi {}

impl Aes67VscRestApi {
    pub async fn new<'a>(
        subsys: &SubsystemHandle,
        persistent_config: AppConfig,
        worterbuch: CloneableWbApi,
    ) -> Result<ManagementAgentApi> {
        let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

        let app_id = persistent_config.name.clone();

        let shutdown_token = subsys.create_cancellation_token();

        let wbc = wb.clone();
        propagate_exit(
            spawn_child_app(
                #[cfg(feature = "tokio-metrics")]
                persistent_config.vsc.app.name.clone(),
                "aes67-rs-vsc-management-agent".to_owned(),
                async |s: &mut SubsystemHandle| {
                    run_rest_api(s, persistent_config, worterbuch, wbc).await
                },
                shutdown_token.clone(),
                #[cfg(feature = "tokio-metrics")]
                wb,
            )
            .into_diagnostic()?,
            shutdown_token,
        );

        let api = ManagementAgentConfigApi::new(subsys, app_id, wb);

        match api.start_vsc().await {
            Err((e, api)) => {
                error!("Could not start AES67-VSC REST API: {}", e);
                Ok(api)
            }
            Ok(api) => Ok(api),
        }
    }
}

async fn run_rest_api(
    subsys: &SubsystemHandle,
    mut persistent_config: AppConfig,
    worterbuch: CloneableWbApi,
    wb: Worterbuch,
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
        subsys,
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

    info!("REST API is listening on {}", listener.local_addr()?);

    spawn(async move {
        match process::Command::new("xdg-open")
            .arg(format!("http://127.0.0.1:{port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            Ok(status) => match status.code() {
                Some(0) => (),
                Some(status) => {
                    warn!("Could not open web UI, xdg-open returned with status {status}")
                }
                None => warn!("Attempt to open web UI returned with unknown status"),
            },
            Err(e) => error!("Could not open web UI: {e}"),
        }
    });

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
        _ = subsys.on_shutdown_requested() => {
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

async fn app_name<'a>(State(app_id): State<String>) -> String {
    app_id.clone()
}

async fn refresh_netinfs(
    State(netinf_watcher): State<netinf_watcher::Handle>,
) -> ManagementAgentResult<&'static str> {
    netinf_watcher.refresh().await;
    Ok("Network interfaces refresh triggered")
}
