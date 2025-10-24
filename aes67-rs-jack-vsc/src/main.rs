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
    discovery::start_sap_discovery, receiver::config::RxDescriptor, sender::config::TxDescriptor,
    time::get_clock, vsc::VirtualSoundCardApi,
};
use aes67_rs_ui::{Aes67VscUi, config::PersistentConfig};
use miette::IntoDiagnostic;
use serde_json::json;
use std::time::Duration;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{error, info};
use worterbuch::PersistenceMode;
use worterbuch_client::{KeyValuePair, topic};

#[tokio::main]
async fn main() -> miette::Result<()> {
    let app_id = "aes67-jack-vsc";

    let config = PersistentConfig::load(app_id).await?;

    telemetry::init(&config.vsc).await?;

    Toplevel::new(move |s| async move {
        s.start(SubsystemBuilder::new(app_id, move |s| run(s, config)));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await
    .into_diagnostic()?;

    Ok(())
}

async fn run(subsys: SubsystemHandle, config: PersistentConfig) -> miette::Result<()> {
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
    subsys.start(SubsystemBuilder::new("discovery", |s| {
        start_sap_discovery(wbd, s.create_cancellation_token())
    }));

    let vsc_config = config.vsc.clone();

    Aes67VscUi::new(config, worterbuch, subsys.create_cancellation_token()).await?;

    let vsc = VirtualSoundCardApi::new(id.to_owned(), subsys.create_cancellation_token(), wb)
        .await
        .into_diagnostic()?;

    let ptp_mode = vsc_config.ptp;

    for tx_config in vsc_config.senders {
        let descriptor = TxDescriptor::try_from(&tx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let ptp_mode = ptp_mode.clone();
        let app_id = id.clone();
        subsys.start(SubsystemBuilder::new(
            format!("sender/{}", descriptor.id),
            |s| async move {
                match vsc.create_sender(tx_config).await.into_diagnostic() {
                    Ok((sender, _)) => {
                        let clock = get_clock(ptp_mode, descriptor.audio_format)?;
                        start_recording(app_id, s, sender, descriptor, clock).await
                    }
                    Err(e) => {
                        error!("Error creating sender '{}': {}", descriptor.id, e);
                        Ok(())
                    }
                }
            },
        ));
    }

    for rx_config in vsc_config.receivers {
        let descriptor = RxDescriptor::try_from(&rx_config).into_diagnostic()?;
        let vsc = vsc.clone();
        let ptp_mode = ptp_mode.clone();
        let app_id = id.clone();
        subsys.start(SubsystemBuilder::new(
            format!("receiver/{}", descriptor.id),
            |s| async move {
                match vsc
                    .create_receiver(rx_config, ptp_mode.clone())
                    .await
                    .into_diagnostic()
                {
                    Ok((receiver, monitoring, _)) => {
                        let clock = get_clock(ptp_mode, descriptor.audio_format)?;
                        start_playout(app_id, s, receiver, descriptor, clock, monitoring).await
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
