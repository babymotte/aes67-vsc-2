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

use aes67_rs::{
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result, ConfigError, ToBoxedResult},
    receiver::{api::ReceiverApi, config::RxDescriptor},
    telemetry,
    time::SystemMediaClock,
    vsc::VirtualSoundCardApi,
};
use miette::IntoDiagnostic;
use std::time::Duration;
use tokio::runtime;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::info;

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

async fn run(subsys: SubsystemHandle, mut config: Config) -> Aes67Vsc2Result<()> {
    let Some(rx_config) = config.receiver_config.take() else {
        return Err(Aes67Vsc2Error::ConfigError(Box::new(
            ConfigError::MissingReceiverConfig,
        )));
    };

    let id = config.instance_name();

    info!(
        "Starting {} instance '{}' with session description:\n{}",
        config.app.name,
        config.app.instance.name,
        rx_config.session.marshal()
    );

    let vsc = VirtualSoundCardApi::new(id).await.boxed()?;
    let descriptor = RxDescriptor::try_from(&rx_config).boxed()?;

    let system_clock = SystemMediaClock::new(descriptor.audio_format);
    let (receiver, _) = vsc.create_receiver(rx_config).await.boxed()?;

    start_playout(subsys, receiver, descriptor).await?;

    Ok(())
}

async fn start_playout(
    subsys: SubsystemHandle,
    receiver: ReceiverApi,
    descriptor: RxDescriptor,
) -> Aes67Vsc2Result<()> {
    // TODO

    subsys.on_shutdown_requested().await;

    Ok(())
}
