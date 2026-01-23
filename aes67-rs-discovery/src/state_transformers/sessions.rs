use crate::{Session, error::DiscoveryResult};
use aes67_rs::receiver::config::SessionInfo;
use std::{
    collections::{BTreeSet, HashMap, hash_map::Entry},
    time::Duration,
};
use tokio::{select, sync::mpsc};
use tosub::Subsystem;
use tracing::{debug, info, warn};
use worterbuch_client::{TypedPStateEvent, Worterbuch, topic};

pub async fn start(
    subsys: Subsystem,
    instance_name: String,
    worterbuch_client: Worterbuch,
) -> DiscoveryResult<()> {
    info!("Starting sessions state transformer …");

    let (all_sessions, _) = worterbuch_client
        .psubscribe::<Session>(
            topic![instance_name, "discovery", "sap", "?", "?"],
            true,
            false,
            Some(Duration::from_millis(100)),
        )
        .await?;

    let sessions_by_name = HashMap::new();

    ProcessLoop {
        worterbuch_client,
        sessions_by_id: sessions_by_name,
        instance_name,
    }
    .start(subsys, all_sessions)
    .await?;

    Ok(())
}

struct ProcessLoop {
    worterbuch_client: Worterbuch,
    sessions_by_id: HashMap<u64, BTreeSet<Session>>,
    instance_name: String,
}

impl ProcessLoop {
    async fn start(
        mut self,
        subsys: Subsystem,
        mut all_sessions: mpsc::UnboundedReceiver<TypedPStateEvent<Session>>,
    ) -> DiscoveryResult<()> {
        loop {
            select! {
                _ = subsys.shutdown_requested() => break,
                Some(event) = all_sessions.recv() => self.process_session(event).await?,
                else => break,
            }
        }

        info!("Sessions state transformer stopped.");

        Ok(())
    }

    async fn process_session(&mut self, event: TypedPStateEvent<Session>) -> DiscoveryResult<()> {
        match event {
            TypedPStateEvent::KeyValuePairs(kvps) => {
                for kvp in kvps {
                    self.session_added(kvp.value).await?;
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

    async fn session_added(&mut self, session: Session) -> DiscoveryResult<()> {
        debug!("Session added: {:?}", session);

        let id = session.description.origin.session_id;

        let sessions = self.sessions_by_id.entry(id).or_default();
        sessions.retain(|e| {
            e.description.origin.session_version != session.description.origin.session_version
        });
        sessions.insert(session);

        let latest = sessions
            .iter()
            .next()
            .expect("cannot be empty, we just added something");

        self.worterbuch_client
            .set_async(
                topic!(self.instance_name, "discovery", "sessions", id),
                &latest.description,
            )
            .await?;
        self.worterbuch_client
            .set_async(
                topic!(self.instance_name, "discovery", "sessions", id, "name"),
                &latest.description.session_name,
            )
            .await?;

        match SessionInfo::try_from(&latest.description.0) {
            Ok(session) => {
                self.worterbuch_client
                    .set_async(
                        topic!(self.instance_name, "discovery", "sessions", id, "config"),
                        session,
                    )
                    .await?;
            }
            Err(err) => warn!(
                "Failed to convert SessionDescription to SessionInfo: {}",
                err
            ),
        }

        Ok(())
    }

    async fn session_removed(&mut self, _key: String, session: Session) -> DiscoveryResult<()> {
        debug!("Session removed: {:?}", session);

        let id = session.description.origin.session_id;

        if let Entry::Occupied(mut e) = self.sessions_by_id.entry(id) {
            let sessions = e.get_mut();
            sessions.retain(|e| {
                e.description.origin.session_version != session.description.origin.session_version
            });

            match sessions.iter().next() {
                Some(latest) => {
                    self.worterbuch_client
                        .set_async(
                            topic!(self.instance_name, "discovery", "sessions", id),
                            &latest.description,
                        )
                        .await?;

                    self.worterbuch_client
                        .set_async(
                            topic!(self.instance_name, "discovery", "sessions", id, "name"),
                            &latest.description.session_name,
                        )
                        .await?;

                    match SessionInfo::try_from(&latest.description.0) {
                        Ok(session) => {
                            self.worterbuch_client
                                .set_async(
                                    topic!(
                                        self.instance_name,
                                        "discovery",
                                        "sessions",
                                        id,
                                        "config"
                                    ),
                                    session,
                                )
                                .await?;
                        }
                        Err(err) => warn!(
                            "Failed to convert SessionDescription to SessionInfo: {}",
                            err
                        ),
                    }
                }
                None => {
                    self.worterbuch_client
                        .delete_async(topic!(self.instance_name, "discovery", "sessions", id))
                        .await?;
                    self.worterbuch_client
                        .delete_async(topic!(
                            self.instance_name,
                            "discovery",
                            "sessions",
                            id,
                            "name"
                        ))
                        .await?;
                    e.remove();
                }
            }
        }

        Ok(())
    }
}
