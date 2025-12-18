use crate::error::WebUIResult;
use aes67_rs::{config::Config, receiver::config::SessionId};
use aes67_rs_discovery::Session;
use std::{collections::HashMap, time::Duration};
use tokio::{select, sync::mpsc};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::info;
use worterbuch_client::{TypedPStateEvent, Worterbuch, topic};

pub async fn start(
    subsys: &mut SubsystemHandle,
    config: Config,
    worterbuch_client: Worterbuch,
) -> WebUIResult<()> {
    info!("Starting available sessions state transformer …");

    let (used_sessions, _) = worterbuch_client
        .psubscribe(
            topic![config.instance_name(), "rx", "?", "config", "session"],
            true,
            false,
            None,
        )
        .await?;

    let (all_sessions, _) = worterbuch_client
        .psubscribe(
            topic!["discovery", "sessions", "?"],
            true,
            false,
            Some(Duration::from_millis(100)),
        )
        .await?;

    let sessions_by_name = HashMap::new();

    ProcessLopp {
        worterbuch_client,
        sessions_by_name,
    }
    .start(subsys, used_sessions, all_sessions)
    .await?;

    Ok(())
}

struct ProcessLopp {
    worterbuch_client: Worterbuch,
    sessions_by_name: HashMap<String, Session>,
}

impl ProcessLopp {
    async fn start(
        mut self,
        subsys: &mut SubsystemHandle,
        mut used_sessions: mpsc::UnboundedReceiver<TypedPStateEvent<SessionId>>,
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

    async fn process_used_session(&self, event: TypedPStateEvent<SessionId>) -> WebUIResult<()> {
        info!("Processing used session event: {:?}", event);
        Ok(())
    }

    async fn process_session(&mut self, event: TypedPStateEvent<String>) -> WebUIResult<()> {
        match event {
            TypedPStateEvent::KeyValuePairs(kvps) => {
                for kvp in kvps {
                    self.session_added(kvp.key, kvp.value).await?;
                }
            }
            TypedPStateEvent::Deleted(kvps) => {
                for kvp in kvps {
                    self.session_removed(kvp.key, kvp.value).await?;
                }
            }
        }

        Ok(())
    }

    async fn session_added(&mut self, _key: String, session: String) -> WebUIResult<()> {
        info!("Session added:\n{session}");
        // TODO
        Ok(())
    }

    async fn session_removed(&mut self, _key: String, session: String) -> WebUIResult<()> {
        info!("Session removed:\n{session}");
        // TODO
        Ok(())
    }
}
