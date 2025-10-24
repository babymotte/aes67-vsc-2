use crate::error::DiscoveryResult;
use sap_rs::{Event, Sap};
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
            recv = events.recv() => match recv {
                Some(msg) => {
                    match msg {
                        Event::SessionFound(sa) => {
                            let key = topic!("discovery/sap", sa.originating_source.to_string(), sa.msg_id_hash);
                            let sdp = sa.sdp.marshal();
                            debug!("SDP {} was announced by {}:\n{}", sa.msg_id_hash, sa.originating_source, sdp);
                            worterbuch_client.set(key, sdp).await?;
                        },
                        Event::SessionLost(sa) => {
                            let key = topic!("discovery/sap", sa.originating_source.to_string(), sa.msg_id_hash);
                            debug!("SDP {} was deleted by {}.", sa.msg_id_hash, sa.originating_source);
                            worterbuch_client.delete::<String>(key).await?;
                        },
                    }
                },
                None => break,
            }
        }
    }

    Ok(())
}
