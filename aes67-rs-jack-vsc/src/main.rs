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

use aes67_rs::utils::prevent_deep_c_state;
use aes67_rs_jack_vsc::io_handler::JackIoHandler;
use aes67_rs_vsc_management_agent::{
    config::{AppConfig, Args},
    init_management_agent,
};
use std::time::Duration;
use tosub::SubsystemHandle;
use tracing::info;

#[tokio::main(flavor = "current_thread")]
async fn main() -> miette::Result<()> {
    let args = Args::get();

    let app_id = "aes67-jack-vsc".to_owned();
    let config = AppConfig::load(&args.config).await?;

    telemetry::init(&app_id, config.telemetry.as_ref()).await?;

    info!("Starting {} …", app_id);

    let _guard = prevent_deep_c_state()?;

    let app_idc = app_id.clone();

    tosub::build_root(app_id.clone())
        .catch_signals()
        .with_timeout(Duration::from_secs(5))
        .start(|subsys| async move { run(subsys, app_idc, args, config).await })
        .await?;

    info!("{} stopped.", app_id);

    drop(_guard);

    Ok(())
}

async fn run(
    subsys: SubsystemHandle,
    id: String,
    args: Args,
    config: AppConfig,
) -> miette::Result<()> {
    let io_handler = JackIoHandler::new(&subsys);

    init_management_agent(
        &subsys,
        id.clone(),
        config.web_ui.port,
        args.data_dir,
        io_handler,
    )
    .await?;

    subsys.shutdown_requested().await;

    Ok(())
}
