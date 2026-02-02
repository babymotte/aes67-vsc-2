use crate::{play::start_playout, record::start_recording};
use aes67_rs::{
    formats::SessionId,
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
    subsys: SubsystemHandle,
    tx_clients: HashMap<SessionId, SubsystemHandle>,
    rx_clients: HashMap<SessionId, SubsystemHandle>,
    rx: mpsc::Receiver<JackIoHandlerMessage>,
}

impl JackIoHandlerActor {
    pub(crate) fn new(subsys: SubsystemHandle, rx: mpsc::Receiver<JackIoHandlerMessage>) -> Self {
        Self {
            subsys,
            tx_clients: HashMap::new(),
            rx_clients: HashMap::new(),
            rx,
        }
    }

    async fn run(mut self) {
        info!("JACK I/O handler actor started.");
        loop {
            select! {
                Some(msg) = self.rx.recv() => self.process_message(msg).await,
                _ = self.subsys.shutdown_requested() => break,
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
        self.tx_clients.insert(id, recording);
        Ok(())
    }

    async fn sender_updated(&mut self, _id: SessionId) -> IoHandlerResult<()> {
        Err(miette!("not implemented").into())
    }

    async fn sender_deleted(&mut self, id: SessionId) -> IoHandlerResult<()> {
        let Some(recording) = self.tx_clients.remove(&id) else {
            error!("No recording found for sender id {}", id);
            return Ok(());
        };
        recording.request_local_shutdown();
        // Wait for the JACK client to be fully deactivated before returning.
        // This prevents the sender's resources from being destroyed while
        // the JACK callback is still running.
        recording.join().await;
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
        self.rx_clients.insert(id, playout);
        Ok(())
    }

    async fn receiver_updated(&mut self, _id: SessionId) -> IoHandlerResult<()> {
        Err(miette!("not implemented").into())
    }

    async fn receiver_deleted(&mut self, id: SessionId) -> IoHandlerResult<()> {
        let Some(playout) = self.rx_clients.remove(&id) else {
            error!("No playout found for receiver id {}", id);
            return Ok(());
        };
        playout.request_local_shutdown();
        // Wait for the JACK client to be fully deactivated before returning.
        // This prevents the receiver's resources from being destroyed while
        // the JACK callback is still running.
        playout.join().await;
        Ok(())
    }
}

impl Drop for JackIoHandlerActor {
    fn drop(&mut self) {
        warn!("JACK I/O handler dropped. Shutting down all JACK client SubsystemHandles …");
        for (_, playout) in self.rx_clients.drain() {
            playout.request_local_shutdown();
        }
        for (_, recording) in self.tx_clients.drain() {
            recording.request_local_shutdown();
        }
    }
}

pub(crate) enum JackIoHandlerMessage {
    SenderCreated(
        String,
        SubsystemHandle,
        SenderApi,
        SenderConfig,
        Clock,
        Monitoring,
        oneshot::Sender<IoHandlerResult<()>>,
    ),
    SenderUpdated(SessionId, oneshot::Sender<IoHandlerResult<()>>),
    SenderDeleted(SessionId, oneshot::Sender<IoHandlerResult<()>>),
    ReceiverCreated(
        String,
        SubsystemHandle,
        ReceiverApi,
        ReceiverConfig,
        Clock,
        Monitoring,
        oneshot::Sender<IoHandlerResult<()>>,
    ),
    ReceiverUpdated(SessionId, oneshot::Sender<IoHandlerResult<()>>),
    ReceiverDeleted(SessionId, oneshot::Sender<IoHandlerResult<()>>),
}

#[derive(Clone)]
pub struct JackIoHandler {
    tx: mpsc::Sender<JackIoHandlerMessage>,
}

impl JackIoHandler {
    pub fn new(subsys: &SubsystemHandle) -> Self {
        let (tx, rx) = mpsc::channel(1);

        subsys.spawn("jack_io_handler", |s| async {
            JackIoHandlerActor::new(s, rx).run().await;
            Ok::<(), miette::Error>(())
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

    async fn sender_updated(&self, id: SessionId) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::SenderUpdated(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn sender_deleted(&self, id: SessionId) -> IoHandlerResult<()> {
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

    async fn receiver_updated(&mut self, id: SessionId) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::ReceiverUpdated(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }

    async fn receiver_deleted(&mut self, id: SessionId) -> IoHandlerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let msg = JackIoHandlerMessage::ReceiverDeleted(id, resp_tx);
        self.tx.send(msg).await.into_diagnostic()?;
        resp_rx.await.into_diagnostic()??;
        Ok(())
    }
}
