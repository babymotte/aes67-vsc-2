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

use crate::{
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    receiver::api::{ReceiverApiMessage, ReceiverInfo},
};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
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
    api_tx: mpsc::Sender<ReceiverApiMessage>,
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
    api_tx: mpsc::Sender<ReceiverApiMessage>,
    ready_tx: oneshot::Sender<SocketAddr>,
) -> Aes67Vsc2Result<()> {
    let app = Router::new()
        .route("/stop", post(stop))
        .route("/info", get(info))
        .with_state(api_tx)
        .layer(TraceLayer::new_for_http());

    let web_config = &config
        .receiver_config
        .as_ref()
        .expect("no receiver config")
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

#[instrument(ret)]
async fn stop(
    State(api_tx): State<mpsc::Sender<ReceiverApiMessage>>,
) -> Aes67Vsc2Result<Json<bool>> {
    match api_tx.send(ReceiverApiMessage::Stop).await {
        Ok(_) => Ok(Json(true)),
        Err(_) => Ok(Json(false)),
    }
}

#[instrument(ret)]
async fn info(
    State(api_tx): State<mpsc::Sender<ReceiverApiMessage>>,
) -> Aes67Vsc2Result<Json<ReceiverInfo>> {
    let (tx, rx) = oneshot::channel();
    api_tx.send(ReceiverApiMessage::GetInfo(tx)).await.ok();
    match rx.await {
        Ok(addr) => Ok(Json(addr)),
        Err(_) => Err(Aes67Vsc2Error::Other(
            "error getting shared memory address".to_owned(),
        )),
    }
}
