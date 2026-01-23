use crate::error::DiscoveryResult;
use tosub::Subsystem;
use worterbuch_client::Worterbuch;

mod available_sessions;
mod sessions;

pub async fn start(subsys: Subsystem, id: String, worterbuch: Worterbuch) -> DiscoveryResult<()> {
    let instance_name = id.clone();
    let wb = worterbuch.clone();
    let mut sessions = subsys.spawn("sessions", async |s| {
        sessions::start(s, instance_name, wb).await
    });

    let instance_name = id.clone();
    let wb = worterbuch.clone();
    let mut available_sessions = subsys.spawn("available-sessions", async |s| {
        available_sessions::start(s, instance_name, wb).await
    });

    sessions.join().await;
    available_sessions.join().await;

    Ok(())
}
