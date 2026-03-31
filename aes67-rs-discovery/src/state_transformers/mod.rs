use crate::error::DiscoveryResult;
use tosub::SubsystemHandle;
use worterbuch_client::Worterbuch;

mod sessions;

pub async fn start(
    subsys: SubsystemHandle,
    id: String,
    worterbuch: Worterbuch,
) -> DiscoveryResult<()> {
    let instance_name = id.clone();
    let wb = worterbuch.clone();
    let sessions = subsys.spawn("sessions", async |s| {
        sessions::start(s, instance_name, wb).await
    });

    sessions.join().await;

    Ok(())
}
