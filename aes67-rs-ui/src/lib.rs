use aes67_rs::{
    app::{propagate_exit, spawn_child_app},
    config::Config,
    error::{Aes67Vsc2Result, WebUIResult},
};
use axum::{Router, extract::State, routing::get};
use miette::{IntoDiagnostic, Result};
use std::{
    env::{self, current_dir},
    path::PathBuf,
};
use tokio_graceful_shutdown::SubsystemHandle;
use tokio_util::sync::CancellationToken;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{info, instrument};

pub struct Aes67VscUi {}

impl Aes67VscUi {
    pub async fn new<'a>(config: Config, shutdown_token: CancellationToken) -> Result<()> {
        propagate_exit(
            spawn_child_app(
                "aes67-rs-ui".to_owned(),
                |s| run(s, config),
                shutdown_token.clone(),
            )
            .into_diagnostic()?,
            shutdown_token,
        );
        Ok(())
    }
}

async fn run(subsys: SubsystemHandle, config: Config) -> WebUIResult<()> {
    info!("Starting AES67-VSC web UI …");

    let trace = TraceLayer::new_for_http();
    let web_root_path = PathBuf::from(
        env::var("AES67_VSC_UI_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                current_dir()
                    .expect("could not determine current working directory")
                    .join("web-frontend")
                    .join("dist")
            }),
    );

    let app = Router::new()
        .route("/api/v1/backend/wb-servers", get(wb_servers))
        .route("/api/v1/backend/app-name", get(app_name).with_state(config))
        .layer(trace)
        .fallback_service(
            ServeDir::new(&web_root_path)
                .fallback(ServeFile::new(web_root_path.join("index.html"))),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    info!("Web UI is listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    subsys.on_shutdown_requested().await;
    info!("AES67-VSC web UI stopped.");

    Ok(())
}

#[instrument]
async fn wb_servers() -> Aes67Vsc2Result<String> {
    // TODO
    Ok("wb.homelab:30081,wb.homelab:30082,wb.homelab:30083".to_owned())
}

#[instrument]
async fn app_name(State(config): State<Config>) -> Aes67Vsc2Result<String> {
    Ok(config.app.name)
}
