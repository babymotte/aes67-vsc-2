pub mod config;
mod error;
mod state_transformers;

use crate::{
    config::{DEFAULT_PORT, PersistentConfig},
    error::{WebUIError, WebUIResult},
};
use aes67_rs::{
    app::{propagate_exit, spawn_child_app},
    config::Config,
    error::Aes67Vsc2Result,
};
use axum::{extract::State, routing::get};
use axum_server::Handle;
use miette::{IntoDiagnostic, Result};
use std::{io, net::SocketAddr, process::Stdio, time::Duration};
use tokio::{process, select, spawn};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};
use worterbuch::server::{CloneableWbApi, axum::build_worterbuch_router};

pub struct Aes67VscUi {}

impl Aes67VscUi {
    pub async fn new<'a>(
        persistent_config: PersistentConfig,
        worterbuch: CloneableWbApi,
        shutdown_token: CancellationToken,
    ) -> Result<()> {
        propagate_exit(
            spawn_child_app(
                "aes67-rs-ui".to_owned(),
                |s| run(s, persistent_config, worterbuch),
                shutdown_token.clone(),
            )
            .into_diagnostic()?,
            shutdown_token,
        );
        Ok(())
    }
}

async fn run(
    subsys: SubsystemHandle,
    mut persistent_config: PersistentConfig,
    worterbuch: CloneableWbApi,
) -> WebUIResult<()> {
    info!("Starting AES67-VSC web UI …");

    let cfg = persistent_config.vsc.clone();
    let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());
    subsys.start(SubsystemBuilder::new("state-transformers", |s| {
        state_transformers::start(s, cfg, wb)
    }));

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
                    return Err(WebUIError::IoError(io::Error::other(
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
    .route("/api/v1/backend/wb-servers", get(wb_servers))
    .route(
        "/api/v1/backend/app-name",
        get(app_name).with_state(persistent_config.vsc.clone()),
    );

    info!("Web UI is listening on {}", listener.local_addr()?);

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

    info!("AES67-VSC web UI stopped.");

    Ok(())
}

#[instrument]
async fn wb_servers() -> Aes67Vsc2Result<String> {
    // TODO choose random port at start and propagate that here
    Ok("127.0.0.1:8080".to_owned())
}

#[instrument]
async fn app_name(State(config): State<Config>) -> Aes67Vsc2Result<String> {
    Ok(config.app.name)
}
