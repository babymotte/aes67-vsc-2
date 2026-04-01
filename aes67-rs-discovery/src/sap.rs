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

use crate::{
    Session,
    error::{DiscoveryError, DiscoveryResult},
};
use aes67_rs_sdp::SdpWrapper;
use sap_rs::{Event, Sap};
use std::time::SystemTime;
use tokio::select;
use tosub::SubsystemHandle;
use tracing::{debug, info};
use worterbuch_client::{Worterbuch, topic};

pub async fn start_discovery(
    subsys: &SubsystemHandle,
    app_id: String,
    wb: Worterbuch,
) -> DiscoveryResult<Sap> {
    info!("Starting SAP discovery …");

    let (sap, mut events) = Sap::new(subsys).await?;

    let sapc = sap.clone();
    subsys.spawn("sap", |s| async move {
        // TODO fetch current discovery entries
        // TODO track session age and remove old ones

        loop {
            select! {
                _ = s.shutdown_requested() => break,
                evt = events.recv() => match evt {
                    Some(msg) => process_event(msg, &app_id, &wb).await?,
                    None => break,
                }
            }
        }

        sapc.delete_all_sessions().await?;

        info!("SAP discovery stopped.");

        Ok::<(), DiscoveryError>(())
    });

    Ok(sap)
}

async fn process_event(msg: Event, instance_name: &str, wb: &Worterbuch) -> DiscoveryResult<()> {
    match msg {
        Event::SessionFound(sa) => {
            let key = topic!(
                instance_name,
                "discovery",
                "sap",
                sa.originating_source.to_string(),
                sa.msg_id_hash
            );

            debug!(
                "SDP {} was announced by {}:\n{}",
                sa.msg_id_hash, sa.originating_source, sa.sdp
            );

            let session = Session {
                description: SdpWrapper(sa.sdp),
                timestamp: SystemTime::now(),
            };

            wb.set_async(key, session).await?;
        }
        Event::SessionLost(sa) => {
            let key = topic!(
                instance_name,
                "discovery",
                "sap",
                sa.originating_source.to_string(),
                sa.msg_id_hash
            );
            debug!(
                "SDP {} was deleted by {}.",
                sa.msg_id_hash, sa.originating_source
            );
            wb.delete_async(key).await?;
        }
    }

    Ok(())
}
