pub mod error;
pub mod sap;
pub mod state_transformers;

use crate::error::{DiscoveryError, DiscoveryResult};
use aes67_rs::receiver::config::SessionInfo;
use aes67_rs_sdp::SdpWrapper;
use serde::{Deserialize, Serialize};
use std::{hash::Hash, time::SystemTime};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::SubsystemHandle;
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
}

struct ApiActor {
    subsys: SubsystemHandle,
    api_rx: mpsc::Receiver<DiscoveryApiMessage>,
    app_id: String,
    wb: Worterbuch,
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
) -> Result<(), DiscoveryError> {
    start_api_actor(&subsys, api_rx, app_id.clone(), wb.clone());
    start_state_transformers(&subsys, app_id.clone(), wb.clone());
    start_sap_discovery(&subsys, app_id.clone(), wb.clone());
    Ok::<(), DiscoveryError>(())
}

fn start_api_actor(
    subsys: &SubsystemHandle,
    api_rx: mpsc::Receiver<DiscoveryApiMessage>,
    app_id: String,
    wb: Worterbuch,
) {
    subsys.spawn("api-actor", |s| async {
        ApiActor {
            subsys: s,
            api_rx,
            app_id,
            wb,
        }
        .run()
        .await
    });
}

fn start_state_transformers(subsys: &SubsystemHandle, app_idc: String, wbc: Worterbuch) {
    subsys.spawn("state-transformers", |s| async {
        state_transformers::start(s, app_idc, wbc).await
    });
}

fn start_sap_discovery(subsys: &SubsystemHandle, app_id: String, wb: Worterbuch) {
    subsys.spawn("sap", |s| async {
        sap::start_discovery(app_id, wb, s).await
    });
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
        }
        Ok(())
    }

    async fn fetch_session_info(&self, session_id: String) -> DiscoveryResult<Option<SessionInfo>> {
        let key = topic!(self.app_id, "discovery", "sessions", &session_id, "config");
        let res = self.wb.get(key).await?;
        Ok(res)
    }
}
