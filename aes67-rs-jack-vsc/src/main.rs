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

use crate::{play::start_playout, record::start_recording};
use aes67_rs::{
    nic::find_nic_with_name, receiver::config::RxDescriptor, sender::config::TxDescriptor,
    time::get_clock, vsc::VirtualSoundCardApi,
};
use aes67_rs_discovery::sap::start_sap_discovery;
use aes67_rs_jack_vsc::JackIoHandler;
use aes67_rs_vsc_management_agent::{Aes67VscRestApi, config::AppConfig, init_management_agent};
use miette::{IntoDiagnostic, miette};
use serde_json::json;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio_graceful_shutdown::{
    SubsystemBuilder, SubsystemHandle, Toplevel, errors::SubsystemError,
};
use tracing::{error, info};
use worterbuch::PersistenceMode;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let app_id = "aes67-jack-vsc".to_owned();
    let config = AppConfig::load(&app_id).await?;
    let app_id = config.name.clone();

    telemetry::init(&app_id, config.telemetry.as_ref()).await?;

    let res = Toplevel::new(async move |s: &mut SubsystemHandle| {
        s.start(SubsystemBuilder::new(
            app_id.clone(),
            async move |s: &mut SubsystemHandle| run(s, app_id).await,
        ));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(2))
    .await;

    if let Err(e) = &res {
        for e in e.get_subsystem_errors() {
            match e {
                SubsystemError::Failed(_, err) => {
                    error!("{e}: {err}");
                    eprintln!("{:?}", err.get_error());
                }
                SubsystemError::Panicked(_) => {
                    error!("{e}");
                }
            }
        }
        return Err(miette!("aes67-jack-vsc exited with errors"));
    }

    Ok(())
}

async fn run(subsys: &mut SubsystemHandle, id: String) -> miette::Result<()> {
    info!("Starting {} …", id);

    init_management_agent(&subsys, id, JackIoHandler::new()).await?;

    subsys.on_shutdown_requested().await;

    Ok(())
}

// async fn run(subsys: &mut SubsystemHandle, config: PersistentConfig) -> miette::Result<()> {
//     let id = config.vsc.instance_name().to_owned();

//     let vsc_config = config.vsc.clone();

//     let ptp_mode = vsc_config.ptp;

//     let clock = get_clock(id.to_owned(), ptp_mode, vsc_config.sample_rate, wb.clone()).await?;
//     let audio_nic = find_nic_with_name(&config.vsc.audio.nic)?;

//     Aes67VscRestApi::new(config, worterbuch, subsys.create_cancellation_token()).await?;

//     let vsc = VirtualSoundCardApi::new(
//         id.to_owned(),
//         subsys.create_cancellation_token(),
//         wb,
//         clock.clone(),
//         audio_nic,
//     )
//     .await
//     .into_diagnostic()?;

//     for tx_config in vsc_config.senders {
//         let descriptor = TxDescriptor::try_from(&tx_config).into_diagnostic()?;
//         let vsc = vsc.clone();
//         let app_id = id.clone();
//         let clk = clock.clone();
//         subsys.start(SubsystemBuilder::new(
//             format!("sender/{}", descriptor.id),
//             async move |s: &mut SubsystemHandle| match vsc
//                 .create_sender(tx_config)
//                 .await
//                 .into_diagnostic()
//             {
//                 Ok((sender, _)) => start_recording(app_id, s, sender, descriptor, clk).await,
//                 Err(e) => {
//                     error!("Error creating sender '{}': {}", descriptor.id, e);
//                     Ok(())
//                 }
//             },
//         ));
//     }

//     for rx_config in vsc_config.receivers {
//         let descriptor = RxDescriptor::try_from(&rx_config).into_diagnostic()?;
//         let vsc = vsc.clone();
//         let app_id = id.clone();
//         let clk = clock.clone();
//         let rt = Handle::current();
//         subsys.start(SubsystemBuilder::new(
//             format!("receiver/{}", descriptor.id),
//             async move |s: &mut SubsystemHandle| match vsc
//                 .create_receiver(rx_config)
//                 .await
//                 .into_diagnostic()
//             {
//                 Ok((receiver, monitoring, _)) => {
//                     start_playout(app_id, s, receiver, descriptor, clk, monitoring, rt).await
//                 }
//                 Err(e) => {
//                     error!("Error creating receiver '{}': {}", descriptor.id, e);
//                     Ok(())
//                 }
//             },
//         ));
//     }

//     subsys.on_shutdown_requested().await;

//     Ok(())
// }
