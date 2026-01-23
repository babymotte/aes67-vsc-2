use aes67_rs_sdp::SdpWrapper;
use sap_rs::{Event, Sap};
use std::time::SystemTime;
use tokio::select;
use tosub::Subsystem;
use tracing::{debug, info};
use worterbuch_client::{Worterbuch, topic};

use crate::{Session, error::DiscoveryResult};

pub async fn start_sap_discovery(
    instance_name: String,
    worterbuch_client: Worterbuch,
    subsys: Subsystem,
) -> DiscoveryResult<()> {
    info!("Starting SAP discovery …");

    let (_, mut events) = Sap::new().await?;

    // TODO fetch current discovery entries
    // TODO track session age and remove old ones

    loop {
        select! {
            _ = subsys.shutdown_requested() => break,
            evt = events.recv() => match evt {
                Some(msg) => process_event(msg, &instance_name, &worterbuch_client).await?,
                None => break,
            }
        }
    }

    Ok(())
}

async fn process_event(
    msg: Event,
    instance_name: &str,
    worterbuch_client: &Worterbuch,
) -> DiscoveryResult<()> {
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

            worterbuch_client.set_async(key, session).await?;
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
            worterbuch_client.delete_async(key).await?;
        }
    }

    Ok(())
}
