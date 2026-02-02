pub mod error;
pub mod sap;
pub mod state_transformers;

use crate::error::{DiscoveryError, DiscoveryResult};
use aes67_rs::receiver::config::SessionInfo;
use aes67_rs_sdp::SdpWrapper;
use sap_rs::Sap;
use sdp::SessionDescription;
use serde::{Deserialize, Serialize};
use std::{hash::Hash, time::SystemTime};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::SubsystemHandle;
use tracing::info;
use worterbuch_client::{Worterbuch, topic};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub description: SdpWrapper,
    pub timestamp: SystemTime,
}

impl Hash for Session {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.description.origin.session_id.hash(state);
        self.description.origin.session_version.hash(state);
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.description.origin.session_id == other.description.origin.session_id
            && self.description.origin.session_version == other.description.origin.session_version
            && self.timestamp == other.timestamp
    }
}

impl Eq for Session {}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(other.timestamp.cmp(&self.timestamp))
    }
}

impl Ord for Session {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.timestamp.cmp(&self.timestamp)
    }
}

enum DiscoveryApiMessage {
    FetchSessionInfo {
        session_id: String,
        tx: oneshot::Sender<DiscoveryResult<Option<SessionInfo>>>,
    },

    AnnounceSession {
        session_info: SessionInfo,
        tx: oneshot::Sender<DiscoveryResult<()>>,
    },

    RevokeSession {
        session_id: u64,
        tx: oneshot::Sender<DiscoveryResult<()>>,
    },
}

#[derive(Clone)]
pub struct DiscoveryApi {
    tx: mpsc::Sender<DiscoveryApiMessage>,
}

impl DiscoveryApi {
    pub async fn fetch_session_info(
        &self,
        session_id: String,
    ) -> DiscoveryResult<Option<SessionInfo>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(DiscoveryApiMessage::FetchSessionInfo { session_id, tx })
            .await
            .ok();
        rx.await?
    }

    pub async fn announce_session(&self, session_info: SessionInfo) -> DiscoveryResult<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(DiscoveryApiMessage::AnnounceSession { session_info, tx })
            .await
            .ok();
        rx.await?
    }

    pub async fn revoke_session(&self, session_id: u64) -> DiscoveryResult<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(DiscoveryApiMessage::RevokeSession { session_id, tx })
            .await
            .ok();
        rx.await?
    }
}

struct ApiActor {
    subsys: SubsystemHandle,
    api_rx: mpsc::Receiver<DiscoveryApiMessage>,
    app_id: String,
    wb: Worterbuch,
    sap: Sap,
}

pub fn start_discovery(subsys: &SubsystemHandle, app_id: String, wb: Worterbuch) -> DiscoveryApi {
    let (api_tx, api_rx) = mpsc::channel(1024);
    subsys.spawn("discovery", |s| discovery(s, app_id, wb, api_rx));
    DiscoveryApi { tx: api_tx }
}

async fn discovery(
    subsys: SubsystemHandle,
    app_id: String,
    wb: Worterbuch,
    api_rx: mpsc::Receiver<DiscoveryApiMessage>,
) -> DiscoveryResult<()> {
    start_state_transformers(&subsys, app_id.clone(), wb.clone());
    let sap = start_sap_discovery(&subsys, app_id.clone(), wb.clone()).await?;

    subsys.spawn("api-actor", |s| async {
        ApiActor {
            subsys: s,
            api_rx,
            app_id,
            wb,
            sap,
        }
        .run()
        .await
    });

    Ok(())
}

fn start_state_transformers(subsys: &SubsystemHandle, app_idc: String, wbc: Worterbuch) {
    subsys.spawn("state-transformers", |s| async {
        state_transformers::start(s, app_idc, wbc).await
    });
}

async fn start_sap_discovery(
    subsys: &SubsystemHandle,
    app_id: String,
    wb: Worterbuch,
) -> DiscoveryResult<Sap> {
    sap::start_discovery(subsys, app_id, wb).await
}

impl ApiActor {
    async fn run(mut self) -> DiscoveryResult<()> {
        loop {
            select! {
                Some(msg) = self.api_rx.recv() => self.process_message(msg).await?,
                _ = self.subsys.shutdown_requested() => break,
            }
        }

        Ok(())
    }

    async fn process_message(&self, msg: DiscoveryApiMessage) -> DiscoveryResult<()> {
        match msg {
            DiscoveryApiMessage::FetchSessionInfo { session_id, tx } => {
                tx.send(self.fetch_session_info(session_id).await).ok();
            }
            DiscoveryApiMessage::AnnounceSession { session_info, tx } => {
                tx.send(self.announce_session(session_info).await).ok();
            }
            DiscoveryApiMessage::RevokeSession { session_id, tx } => {
                tx.send(self.revoke_session(session_id).await).ok();
            }
        }
        Ok(())
    }

    async fn fetch_session_info(&self, session_id: String) -> DiscoveryResult<Option<SessionInfo>> {
        let key = topic!(self.app_id, "discovery", "sessions", &session_id, "config");
        let res = self.wb.get(key).await?;
        Ok(res)
    }

    async fn announce_session(&self, session_info: SessionInfo) -> Result<(), DiscoveryError> {
        info!(
            "Announcing session: {} with version {}",
            session_info.id.id, session_info.id.version
        );

        let sd = SessionDescription::from(&session_info);

        self.sap.announce_session(sd).await?;
        Ok(())
    }

    async fn revoke_session(&self, session_id: u64) -> Result<(), DiscoveryError> {
        info!("Revoking session {}", session_id);

        self.sap.delete_session(session_id).await?;
        Ok(())
    }
}
