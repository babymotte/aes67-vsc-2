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

mod telemetry;

use miette::Result;
use std::time::Duration;
use aes67_vsc_2::{config::Config, webserver::start_webserver, worterbuch::start_worterbuch};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::info;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let config = Config::load().await?;

    telemetry::init(&config).await?;

    info!("Starting {} instance '{}' …", config.app.name, config.app.instance.name);

    Toplevel::new(|s| async move {
        s.start(SubsystemBuilder::new("aes67-vsc-2", |s| {
            run(s, config)
        }));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await?;

    Ok(())
}

async fn run(subsys: SubsystemHandle, config: Config) -> Result<()> {
    let wb = start_worterbuch(&subsys, config.clone()).await?;

    start_webserver(&subsys, config, wb);

    subsys.on_shutdown_requested().await;

    Ok(())
}
