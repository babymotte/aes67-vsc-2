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
    discovery::start_sap_discovery, nic::find_nic_with_name, receiver::config::RxDescriptor,
    sender::config::TxDescriptor, time::get_clock, vsc::VirtualSoundCardApi,
};
use aes67_rs_ui::{Aes67VscUi, config::PersistentConfig};
use miette::{IntoDiagnostic, miette};
use serde_json::json;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio_graceful_shutdown::{
    SubsystemBuilder, SubsystemHandle, Toplevel, errors::SubsystemError,
};
use tracing::{error, info};
use worterbuch::PersistenceMode;
use worterbuch_client::{KeyValuePair, topic};

#[tokio::main]
async fn main() -> miette::Result<()> {
    let app_id = "aes67-jack-vsc";

    let config = PersistentConfig::load(app_id).await?;

    telemetry::init(&config.vsc).await?;

    let res = Toplevel::new(async move |s: &mut SubsystemHandle| {
        s.start(SubsystemBuilder::new(
            app_id,
            async move |s: &mut SubsystemHandle| run(s, config).await,
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

async fn run(subsys: &mut SubsystemHandle, config: PersistentConfig) -> miette::Result<()> {
    let id = config.vsc.instance_name().to_owned();

    info!("Starting {} …", id);

    let dirs = directories::BaseDirs::new();
    let data_home = dirs
        .map(|d| d.data_dir().to_owned())
        .expect("could not find data dir");

    let data_dir = data_home.join(&id).join("data");

    let mut wb_config = worterbuch::Config::new().await?;
    wb_config.load_env_with_prefix("AES67_VSC")?;
    wb_config.persistence_mode = PersistenceMode::ReDB;
    wb_config.use_persistence = true;
    wb_config.data_dir = data_dir.display().to_string();
    wb_config.ws_endpoint = None;
    wb_config.tcp_endpoint = None;
    wb_config.unix_endpoint = None;
    let worterbuch = worterbuch::spawn_worterbuch(&subsys, wb_config).await?;

    let wb = worterbuch_client::local_client_wrapper(worterbuch.clone());

    wb.set_grave_goods(&[
        &topic!(id, "metrics", "#"),
        &topic!(id, "tx", "#"),
        &topic!(id, "rx", "#"),
    ])
    .await
    .ok();
    wb.set_last_will(&[KeyValuePair {
        key: topic!(id, "running"),
        value: json!(false),
    }])
    .await
    .ok();

    let wbd = wb.clone();
    subsys.start(SubsystemBuilder::new(
        "discovery",
        async |s: &mut SubsystemHandle| {
            start_sap_discovery(wbd, s.create_cancellation_token()).await
        },
    ));

    let vsc_config = config.vsc.clone();

    let ptp_mode = vsc_config.ptp;

    let clock = get_clock(id.to_owned(), ptp_mode, vsc_config.sample_rate, wb.clone()).await?;
    let audio_nic = find_nic_with_name(&config.vsc.audio.nic)?;

    Aes67VscUi::new(config, worterbuch, subsys.create_cancellation_token()).await?;

    let vsc = VirtualSoundCardApi::new(
        id.to_owned(),
        subsys.create_cancellation_token(),
        wb,
        clock.clone(),
        audio_nic,
    )
    .await
    .into_diagnostic()?;

    for tx_config in vsc_config.senders {
        let descriptor = TxDescriptor::try_from(&tx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let app_id = id.clone();
        let clk = clock.clone();
        subsys.start(SubsystemBuilder::new(
            format!("sender/{}", descriptor.id),
            async move |s: &mut SubsystemHandle| match vsc
                .create_sender(tx_config)
                .await
                .into_diagnostic()
            {
                Ok((sender, _)) => start_recording(app_id, s, sender, descriptor, clk).await,
                Err(e) => {
                    error!("Error creating sender '{}': {}", descriptor.id, e);
                    Ok(())
                }
            },
        ));
    }

    for rx_config in vsc_config.receivers {
        let descriptor = RxDescriptor::try_from(&rx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let app_id = id.clone();
        let clk = clock.clone();
        let rt = Handle::current();
        subsys.start(SubsystemBuilder::new(
            format!("receiver/{}", descriptor.id),
            async move |s: &mut SubsystemHandle| match vsc
                .create_receiver(rx_config)
                .await
                .into_diagnostic()
            {
                Ok((receiver, monitoring, _)) => {
                    start_playout(app_id, s, receiver, descriptor, clk, monitoring, rt).await
                }
                Err(e) => {
                    error!("Error creating receiver '{}': {}", descriptor.id, e);
                    Ok(())
                }
            },
        ));
    }

    subsys.on_shutdown_requested().await;

    Ok(())
}
