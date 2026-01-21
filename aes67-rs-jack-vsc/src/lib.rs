pub(crate) mod common;
pub(crate) mod play;
pub(crate) mod record;
pub(crate) mod session_manager;
pub(crate) mod telemetry;

use crate::play::start_playout;
use aes67_rs::{
    monitoring::Monitoring,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    sender::api::SenderApi,
    time::Clock,
};
use aes67_rs_vsc_management_agent::{IoHandler, error::IoHandlerResult};
use std::collections::HashMap;
use tokio_graceful_shutdown::{NestedSubsystem, SubsystemHandle};
use tracing::error;

pub struct JackIoHandler {
    clients: HashMap<u32, NestedSubsystem>,
}

impl JackIoHandler {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }
}

impl IoHandler for JackIoHandler {
    async fn sender_created(
        &mut self,
        id: u32,
        sender_api: SenderApi,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        todo!()
    }

    async fn sender_updated(&mut self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn sender_deleted(&mut self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn receiver_created(
        &mut self,
        app_id: String,
        subsys: &mut SubsystemHandle,
        receiver: ReceiverApi,
        config: ReceiverConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        let id = config.id;
        let playout = start_playout(app_id, subsys, receiver, config, clock, monitoring).await?;
        self.clients.insert(id, playout);
        Ok(())
    }

    async fn receiver_updated(&mut self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn receiver_deleted(&mut self, id: u32) -> IoHandlerResult<()> {
        let Some(playout) = self.clients.remove(&id) else {
            error!("No playout found for receiver id {}", id);
            return Ok(());
        };
        playout.initiate_shutdown();
        Ok(())
    }
}
