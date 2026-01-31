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

mod common;
mod play;
mod record;
mod session_manager;
mod telemetry;

use aes67_rs_jack_vsc::io_handler::JackIoHandler;
use aes67_rs_vsc_management_agent::{config::AppConfig, init_management_agent};
use std::time::Duration;
use tosub::SubsystemHandle;
use tracing::info;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let app_id = "aes67-jack-vsc".to_owned();
    let config = AppConfig::load(&app_id).await?;
    let app_id = config.name.clone();

    telemetry::init(&app_id, config.telemetry.as_ref()).await?;

    tosub::build_root(app_id.clone())
        .catch_signals()
        .with_timeout(Duration::from_secs(5))
        .start(|subsys| async move { run(subsys, app_id).await })
        .await?;

    Ok(())
}

async fn run(subsys: SubsystemHandle, id: String) -> miette::Result<()> {
    info!("Starting {} …", id);

    init_management_agent(&subsys, id, JackIoHandler::new()).await?;

    subsys.shutdown_requested().await;

    Ok(())
}
