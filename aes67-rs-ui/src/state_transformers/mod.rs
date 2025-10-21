mod available_sessions;

use aes67_rs::{config::Config, error::WebUIResult};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use worterbuch_client::Worterbuch;

pub async fn start(
    subsys: SubsystemHandle,
    config: Config,
    worterbuch_client: Worterbuch,
) -> WebUIResult<()> {
    subsys.start(SubsystemBuilder::new("available-sessions", |s| {
        available_sessions::start(s, config, worterbuch_client)
    }));

    Ok(())
}
