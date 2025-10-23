mod available_sessions;

use crate::error::WebUIResult;
use aes67_rs::config::Config;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use worterbuch_client::Worterbuch;

pub async fn start(
    subsys: SubsystemHandle,
    config: Config,
    worterbuch: Worterbuch,
) -> WebUIResult<()> {
    subsys.start(SubsystemBuilder::new("available-sessions", |s| {
        available_sessions::start(s, config, worterbuch)
    }));

    Ok(())
}
