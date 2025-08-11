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

use crate::{config::Config, error::Aes67Vsc2Result};
use axum::{
    Router,
    extract::{Path, State},
    response::Html,
    routing::get,
};
use tokio::net::TcpListener;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};
use worterbuch_client::Worterbuch;

pub fn start_webserver(
    subsys: &SubsystemHandle,
    config: Config,
    wb: worterbuch_client::Worterbuch,
) {
    info!("Starting webserver subsystem");
    subsys.start(SubsystemBuilder::new("webserver", |subsys| {
        webserver(subsys, wb, config)
    }));
}

#[instrument(skip(subsys, wb), ret, err)]
async fn webserver(
    subsys: SubsystemHandle,
    wb: Worterbuch,
    config: Config,
) -> Aes67Vsc2Result<()> {
    let app = Router::new()
        .route("/{*path}", get(handler).with_state(wb.clone()))
        .layer(TraceLayer::new_for_http());

    info!(
        "Listening on {}:{} …",
        config.webserver.bind_address, config.webserver.port
    );
    let listener = TcpListener::bind(format!(
        "{}:{}",
        config.webserver.bind_address, config.webserver.port
    ))
    .await?;
    info!("REST endpoint up at http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { subsys.on_shutdown_requested().await })
        .await?;

    Ok(())
}

#[instrument(skip(wb), ret)]
async fn handler(
    Path(path): Path<Vec<String>>,
    State(wb): State<Worterbuch>,
) -> Aes67Vsc2Result<Html<String>> {
    let greet = wb
        .get(path.join("/"))
        .await?
        .unwrap_or("&lt;unknown&gt;".to_owned());

    Ok(Html(format!("<h1>Hello, {greet}!</h1>")))
}
