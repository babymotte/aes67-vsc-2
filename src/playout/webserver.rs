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

use crate::{config::Config, error::Aes67Vsc2Result, playout::api::PlayoutApiMessage};
use axum::{Json, Router, extract::State, routing::post};
use std::net::SocketAddr;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};

pub fn start_webserver(
    subsys: &SubsystemHandle,
    config: Config,
    api_tx: mpsc::Sender<PlayoutApiMessage>,
    ready_tx: oneshot::Sender<SocketAddr>,
) {
    info!("Starting webserver subsystem");
    subsys.start(SubsystemBuilder::new("webserver", |subsys| {
        webserver(subsys, config, api_tx, ready_tx)
    }));
}

#[instrument(skip(subsys), ret, err)]
async fn webserver(
    subsys: SubsystemHandle,
    config: Config,
    api_tx: mpsc::Sender<PlayoutApiMessage>,
    ready_tx: oneshot::Sender<SocketAddr>,
) -> Aes67Vsc2Result<()> {
    let app = Router::new()
        .route("/stop", post(stop))
        .with_state(api_tx)
        .layer(TraceLayer::new_for_http());

    let web_config = &config
        .playout_config
        .as_ref()
        .expect("no playout config")
        .webserver;

    info!(
        "Listening on {}:{} …",
        web_config.bind_address, web_config.port
    );
    let listener =
        TcpListener::bind(format!("{}:{}", web_config.bind_address, web_config.port)).await?;
    let local_address = listener.local_addr()?;
    info!("REST endpoint up at http://{}", local_address);
    ready_tx.send(local_address).ok();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { subsys.on_shutdown_requested().await })
        .await?;

    Ok(())
}

#[instrument(ret, err)]
async fn stop(
    State(api_tx): State<mpsc::Sender<PlayoutApiMessage>>,
) -> Aes67Vsc2Result<Json<bool>> {
    match api_tx.send(PlayoutApiMessage::Stop).await {
        Ok(_) => Ok(Json(true)),
        Err(_) => Ok(Json(false)),
    }
}
