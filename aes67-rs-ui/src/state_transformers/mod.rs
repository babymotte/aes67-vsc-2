mod available_sessions;
mod sessions;

use crate::error::WebUIResult;
use aes67_rs::config::Config;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use worterbuch_client::Worterbuch;

pub async fn start(
    subsys: &mut SubsystemHandle,
    config: Config,
    worterbuch: Worterbuch,
) -> WebUIResult<()> {
    let cfg = config.clone();
    let wb = worterbuch.clone();
    subsys.start(SubsystemBuilder::new(
        "sessions",
        async |s: &mut SubsystemHandle| sessions::start(s, cfg, wb).await,
    ));

    let cfg = config.clone();
    let wb = worterbuch.clone();
    subsys.start(SubsystemBuilder::new(
        "available-sessions",
        async |s: &mut SubsystemHandle| available_sessions::start(s, cfg, wb).await,
    ));

    Ok(())
}
