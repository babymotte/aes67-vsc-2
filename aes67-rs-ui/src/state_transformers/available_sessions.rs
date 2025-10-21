use aes67_rs::{config::Config, error::WebUIResult, receiver::config::Session};
use tokio::{select, sync::mpsc};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::info;
use worterbuch_client::{TypedPStateEvent, Worterbuch, topic};

pub async fn start(
    subsys: SubsystemHandle,
    config: Config,
    worterbuch_client: Worterbuch,
) -> WebUIResult<()> {
    info!("Starting available sessions state transformer …");

    let (used_sessions, _) = worterbuch_client
        .psubscribe::<Session>(
            topic![config.instance_name(), "rx", "?", "config", "session"],
            true,
            false,
            None,
        )
        .await?;

    let (all_sessions, _) = worterbuch_client
        .psubscribe::<String>(topic!["discovery", "sap", "?", "?"], true, false, None)
        .await?;

    ProcessLopp { worterbuch_client }
        .start(subsys, used_sessions, all_sessions)
        .await?;

    Ok(())
}

struct ProcessLopp {
    worterbuch_client: Worterbuch,
}

impl ProcessLopp {
    async fn start(
        self,
        subsys: SubsystemHandle,
        mut used_sessions: mpsc::UnboundedReceiver<TypedPStateEvent<Session>>,
        mut all_sessions: mpsc::UnboundedReceiver<TypedPStateEvent<String>>,
    ) -> WebUIResult<()> {
        loop {
            select! {
                _ = subsys.on_shutdown_requested() => break,
                Some(event) = used_sessions.recv() => self.process_used_session(event).await?,
                Some(event) = all_sessions.recv() => self.process_session(event).await?,
                else => break,
            }
        }

        info!("Available sessions state transformer stopped.");

        Ok(())
    }

    async fn process_used_session(&self, event: TypedPStateEvent<Session>) -> WebUIResult<()> {
        info!("Processing used session event: {:?}", event);
        Ok(())
    }

    async fn process_session(&self, event: TypedPStateEvent<String>) -> WebUIResult<()> {
        info!("Processing session event: {:?}", event);
        Ok(())
    }
}
