use crate::{play::start_playout, record::start_recording};
use aes67_rs::{
    monitoring::Monitoring,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    sender::{api::SenderApi, config::SenderConfig},
    time::Clock,
};
use aes67_rs_vsc_management_agent::{IoHandler, error::IoHandlerResult};
use miette::{IntoDiagnostic, miette};
use std::collections::HashMap;
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::SubsystemHandle;
use tracing::{error, info, warn};

pub struct JackIoHandlerActor {
    clients: HashMap<u32, SubsystemHandle>,
}

impl JackIoHandlerActor {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<JackIoHandlerMessage>) {
        info!("JACK I/O handler actor started.");
        loop {
            select! {
                Some(msg) = rx.recv() => self.process_message(msg).await,
                else => break,
            }
        }
        info!("JACK I/O handler actor stopped.");
    }

    async fn process_message(&mut self, msg: JackIoHandlerMessage) {
        match msg {
            JackIoHandlerMessage::SenderCreated(
                app_id,
                subsys,
                receiver,
                config,
                clock,
                monitoring,
                resp_tx,
            ) => {
                let res = self
                    .sender_created(app_id, subsys, receiver, config, clock, monitoring)
                    .await;
                let _ = resp_tx.send(res);
            }
            JackIoHandlerMessage::SenderUpdated(id, resp_tx) => {
                let res = self.sender_updated(id).await;
                let _ = resp_tx.send(res);
            }
            JackIoHandlerMessage::SenderDeleted(id, resp_tx) => {
                let res = self.sender_deleted(id).await;
                let _ = resp_tx.send(res);
            }
            JackIoHandlerMessage::ReceiverCreated(
                app_id,
                subsys,
                receiver,
                config,
                clock,
                monitoring,
                resp_tx,
            ) => {
                let res = self
                    .receiver_created(app_id, subsys, receiver, config, clock, monitoring)
                    .await;
                let _ = resp_tx.send(res);
            }
            JackIoHandlerMessage::ReceiverUpdated(id, resp_tx) => {
                let res = self.receiver_updated(id).await;
                let _ = resp_tx.send(res);
            }
            JackIoHandlerMessage::ReceiverDeleted(id, resp_tx) => {
                let res = self.receiver_deleted(id).await;
                let _ = resp_tx.send(res);
            }
        }
    }

    async fn sender_created(
        &mut self,
        app_id: String,
        subsys: SubsystemHandle,
        sender: SenderApi,
        config: SenderConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        let id = config.id;
        let recording = start_recording(app_id, subsys, sender, config, clock, monitoring).await?;
        self.clients.insert(id, recording);
        Ok(())
    }

    async fn sender_updated(&mut self, id: u32) -> IoHandlerResult<()> {
        return Err(miette!("not implemented").into());
    }

    async fn sender_deleted(&mut self, id: u32) -> IoHandlerResult<()> {
        let Some(recording) = self.clients.remove(&id) else {
            error!("No recording found for sender id {}", id);
            return Ok(());
        };
        recording.request_local_shutdown();
        Ok(())
    }

    async fn receiver_created(
        &mut self,
        app_id: String,
        subsys: SubsystemHandle,
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
        return Err(miette!("not implemented").into());
    }

    async fn receiver_deleted(&mut self, id: u32) -> IoHandlerResult<()> {
        let Some(playout) = self.clients.remove(&id) else {
            error!("No playout found for receiver id {}", id);
            return Ok(());
        };
        playout.request_local_shutdown();
        Ok(())
    }
}

impl Drop for JackIoHandlerActor {
    fn drop(&mut self) {
        warn!("JACK I/O handler dropped. Shutting down all playout SubsystemHandles …");
        for (_, playout) in self.clients.drain() {
            playout.request_local_shutdown();
        }
    }
}

enum JackIoHandlerMessage {
    SenderCreated(
        String,
        SubsystemHandle,
        SenderApi,
        SenderConfig,
        Clock,
        Monitoring,
        oneshot::Sender<IoHandlerResult<()>>,
    ),
    SenderUpdated(u32, oneshot::Sender<IoHandlerResult<()>>),
    SenderDeleted(u32, oneshot::Sender<IoHandlerResult<()>>),
    ReceiverCreated(
        String,
        SubsystemHandle,
        ReceiverApi,
        ReceiverConfig,
        Clock,
        Monitoring,
        oneshot::Sender<IoHandlerResult<()>>,
    ),
    ReceiverUpdated(u32, oneshot::Sender<IoHandlerResult<()>>),
    ReceiverDeleted(u32, oneshot::Sender<IoHandlerResult<()>>),
}

#[derive(Clone)]
pub struct JackIoHandler {
    tx: mpsc::Sender<JackIoHandlerMessage>,
}

impl JackIoHandler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(1);
        let actor = JackIoHandlerActor::new();
        tokio::spawn(async move {
            actor.run(rx).await;
        });
        Self { tx }
    }
}

impl IoHandler for JackIoHandler {
    async fn sender_created(
        &self,
        app_id: String,
        subsys: SubsystemHandle,
        sender: SenderApi,
        config: SenderConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::SenderCreated(
            app_id, subsys, sender, config, clock, monitoring, resp_tx,
        );
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn sender_updated(&self, id: u32) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::SenderUpdated(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn sender_deleted(&self, id: u32) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::SenderDeleted(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn receiver_created(
        &self,
        app_id: String,
        subsys: SubsystemHandle,
        receiver: ReceiverApi,
        config: ReceiverConfig,
        clock: Clock,
        monitoring: Monitoring,
    ) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::ReceiverCreated(
            app_id, subsys, receiver, config, clock, monitoring, resp_tx,
        );
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn receiver_updated(&mut self, id: u32) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::ReceiverUpdated(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn receiver_deleted(&mut self, id: u32) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::ReceiverDeleted(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }
}
