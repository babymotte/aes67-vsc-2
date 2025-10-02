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

use crate::{play::start_playout, record::start_recording};
use aes67_rs::{
    config::Config, receiver::config::RxDescriptor, sender::config::TxDescriptor, telemetry,
    time::get_clock, vsc::VirtualSoundCardApi,
};
use miette::IntoDiagnostic;
use std::time::Duration;
use tokio::runtime;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{error, info};

fn main() -> miette::Result<()> {
    let config = Config::load().into_diagnostic()?;

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .into_diagnostic()?
        .block_on(async_main(config))?;

    Ok(())
}

async fn async_main(config: Config) -> miette::Result<()> {
    telemetry::init(&config).await.into_diagnostic()?;

    Toplevel::new(move |s| async move {
        s.start(SubsystemBuilder::new("jack-vsc", move |s| run(s, config)));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await
    .into_diagnostic()?;

    Ok(())
}

async fn run(subsys: SubsystemHandle, config: Config) -> miette::Result<()> {
    let id = config.instance_name();

    info!(
        "Starting {} instance '{}' …",
        config.app.name, config.app.instance.name
    );

    let vsc = VirtualSoundCardApi::new(id).await.into_diagnostic()?;

    let ptp_mode = config.ptp;

    for tx_config in config.senders {
        let descriptor = TxDescriptor::try_from(&tx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let ptp_mode = ptp_mode.clone();
        subsys.start(SubsystemBuilder::new(
            format!("sender/{}", descriptor.id),
            |s| async move {
                match vsc
                    .create_sender(tx_config, ptp_mode.clone())
                    .await
                    .into_diagnostic()
                {
                    Ok((sender, _)) => {
                        let clock = get_clock(ptp_mode, descriptor.audio_format)?;
                        start_recording(s, sender, descriptor, clock).await
                    }
                    Err(e) => {
                        error!("Error creating sender '{}': {}", descriptor.id, e);
                        Ok(())
                    }
                }
            },
        ));
    }

    for rx_config in config.receivers {
        let descriptor = RxDescriptor::try_from(&rx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let ptp_mode = ptp_mode.clone();
        subsys.start(SubsystemBuilder::new(
            format!("receiver/{}", descriptor.id),
            |s| async move {
                match vsc
                    .create_receiver(rx_config, ptp_mode.clone())
                    .await
                    .into_diagnostic()
                {
                    Ok((receiver, _)) => {
                        let clock = get_clock(ptp_mode, descriptor.audio_format)?;
                        start_playout(s, receiver, descriptor, clock).await
                    }
                    Err(e) => {
                        error!("Error creating receiver '{}': {}", descriptor.id, e);
                        Ok(())
                    }
                }
            },
        ));
    }

    subsys.on_shutdown_requested().await;

    Ok(())
}
