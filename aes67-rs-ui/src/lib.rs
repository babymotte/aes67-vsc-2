use std::{borrow::Cow, time::Duration};

use aes67_rs::error::Aes67Vsc2Result;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tokio_util::sync::CancellationToken;

pub struct Aes67VscUiApp {}

impl Aes67VscUiApp {
    pub async fn new<'a>(
        name: impl Into<Cow<'a, str>>,
        shutdown_token: CancellationToken,
    ) -> Aes67Vsc2Result<()> {
        Toplevel::new_with_shutdown_token(
            |s| async move {
                s.start(SubsystemBuilder::new("", start));
            },
            shutdown_token,
        )
        .handle_shutdown_requests(Duration::from_secs(1))
        .await;

        Ok(())
    }
}

async fn start(subsys: SubsystemHandle) -> Aes67Vsc2Result<()> {
    todo!()
}
