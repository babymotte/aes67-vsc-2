use aes67_rs::{monitoring::Monitoring, receiver::api::ReceiverApi, sender::api::SenderApi};
use aes67_rs_vsc_management_agent::{IoHandler, error::IoHandlerResult};

pub struct JackIoHandler {}

impl JackIoHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl IoHandler for JackIoHandler {
    async fn sender_created(
        &self,
        id: u32,
        sender_api: SenderApi,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        todo!()
    }

    async fn sender_updated(&self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn sender_deleted(&self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn receiver_created(
        &self,
        id: u32,
        receiver_api: ReceiverApi,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        todo!()
    }

    async fn receiver_updated(&self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }

    async fn receiver_deleted(&self, id: u32) -> IoHandlerResult<()> {
        todo!()
    }
}
