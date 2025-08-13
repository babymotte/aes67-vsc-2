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

use aes67_vsc_2::{
    config::Config, error::Aes67Vsc2Error, playout::jack::start_jack_playout,
    receiver::start_receiver, telemetry, time::statime_linux::statime_linux,
    utils::find_network_interface, worterbuch::start_worterbuch,
};
use miette::Result;
use std::time::Duration;
use tokio_graceful_shutdown::{SubsystemBuilder, Toplevel};
use tracing::info;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let config = Config::load().await?;

    telemetry::init(&config).await?;

    let rx_config = config
        .receiver_config
        .as_ref()
        .expect("no receiver config")
        .clone();

    info!(
        "Starting {} instance '{}' with session description:\n{}",
        config.app.name,
        config.app.instance.name,
        config
            .receiver_config
            .as_ref()
            .expect("no receiver config")
            .session
            .marshal()
    );

    Toplevel::new(move |s| async move {
        s.start(SubsystemBuilder::new("aes67-vsc-2", move |s| async move {
            let wb = start_worterbuch(&s, config.clone()).await?;
            let iface = find_network_interface(rx_config.interface_ip)?;
            let clock = statime_linux(
                iface,
                rx_config.interface_ip,
                wb.clone(),
                config.instance_name(),
            )
            .await;

            let receiver_api =
                start_receiver(&s, config.clone(), false, wb.clone(), clock.clone()).await?;
            info!("Receiver API running at {}", receiver_api.url());

            let playout_api =
                start_jack_playout(&s, config.clone(), false, wb.clone(), clock.clone()).await?;
            info!("Playout API running at {}", playout_api.url());

            Ok::<(), Aes67Vsc2Error>(())
        }));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await?;

    Ok(())
}
