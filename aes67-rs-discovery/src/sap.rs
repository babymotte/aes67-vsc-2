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
