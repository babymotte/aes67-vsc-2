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

use crate::config::Config;
use tokio::select;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{info, instrument};
use worterbuch_client::{ConnectionResult, KeyValuePair, OnDisconnect, Worterbuch, topic};

#[instrument(skip(subsys))]
pub async fn start_worterbuch(
    subsys: &SubsystemHandle,
    config: Config,
) -> ConnectionResult<worterbuch_client::Worterbuch> {
    info!("Starting worterbuch client subsystem");
    let (wb, on_disconnect, _) = worterbuch_client::connect_with_default_config().await?;
    let wbc = wb.clone();
    subsys.start(SubsystemBuilder::new("worterbuch", |subsys| {
        worterbuch(subsys, wbc, on_disconnect, config)
    }));
    Ok(wb)
}

async fn worterbuch(
    subsys: SubsystemHandle,
    wb: Worterbuch,
    on_disconnect: OnDisconnect,
    config: Config,
) -> ConnectionResult<()> {
    wb.set_client_name(format!("{}/{}", config.app.name, config.app.instance.name))
        .await;
    wb.set_grave_goods(
        vec![topic!(config.app.name, config.app.instance.name, "#").as_ref()].as_ref(),
    )
    .await;
    wb.set_last_will(&[KeyValuePair::of(
        topic!(config.app.name, config.app.instance.name, "running"),
        false,
    )])
    .await;
    wb.set(
        topic!(config.app.name, config.app.instance.name, "running"),
        true,
    )
    .await;
    select! {
        _ = on_disconnect => {
            subsys.request_shutdown();
        },
        _ = subsys.on_shutdown_requested() => (),
    }
    Ok(())
}
