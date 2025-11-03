use crate::{discovery::Session, error::DiscoveryResult, serde::SdpWrapper};
use sap_rs::{Event, Sap};
use std::time::SystemTime;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use worterbuch_client::{Worterbuch, topic};

pub async fn start_sap_discovery(
    worterbuch_client: Worterbuch,
    shutdown_token: CancellationToken,
) -> DiscoveryResult<()> {
    let (_, mut events) = Sap::new().await?;

    // TODO fetch current discovery entries
    // TODO track session age and remove old ones

    loop {
        select! {
            _ = shutdown_token.cancelled() => break,
            evt = events.recv() => match evt {
                Some(msg) => process_event(msg, &worterbuch_client).await?,
                None => break,
            }
        }
    }

    Ok(())
}

async fn process_event(msg: Event, worterbuch_client: &Worterbuch) -> DiscoveryResult<()> {
    match msg {
        Event::SessionFound(sa) => {
            let key = topic!(
                "discovery/sap",
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
                "discovery/sap",
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
