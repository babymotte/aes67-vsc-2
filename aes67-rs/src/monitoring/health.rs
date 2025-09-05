mod receiver_health;
use crate::monitoring::Report;
use std::collections::{HashMap, hash_map::Entry};
use tokio::{
    select,
    sync::{broadcast, mpsc},
};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::info;

pub async fn health(
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<Report>,
    tx: broadcast::Sender<Report>,
) -> Result<(), &'static str> {
    HealthActor::new(subsys, rx, tx).run().await;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SenderHealth {}
impl SenderHealth {
    fn new(src: String) -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct ReceiverHealth {}
impl ReceiverHealth {
    fn new(src: String) -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct PlayoutHealth {}
impl PlayoutHealth {
    fn new(src: String) -> Self {
        Self {}
    }
}

struct HealthActor {
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<Report>,
    tx: broadcast::Sender<Report>,
    senders: HashMap<String, SenderHealth>,
    receivers: HashMap<String, ReceiverHealth>,
    playouts: HashMap<String, PlayoutHealth>,
}

impl HealthActor {
    fn new(
        subsys: SubsystemHandle,
        rx: mpsc::Receiver<Report>,
        tx: broadcast::Sender<Report>,
    ) -> Self {
        Self {
            subsys,
            rx,
            tx,
            senders: HashMap::new(),
            receivers: HashMap::new(),
            playouts: HashMap::new(),
        }
    }

    async fn run(mut self) {
        info!("Health subsystem started.");
        loop {
            select! {
                Some(report) = self.rx.recv() => self.process_report(report).await,
                _ = self.subsys.on_shutdown_requested() => break,
                else => {
                    self.subsys.request_shutdown();
                    break;
                },
            }
        }
        info!("Health subsystem stopped.");
    }

    async fn process_report(&mut self, report: Report) {
        match report {
            Report::State(e) => {
                _ = {
                    // TODO
                    self.tx.send(Report::State(e))
                }
            }
            Report::Stats(e) => {
                _ = {
                    // TODO
                    self.tx.send(Report::Stats(e))
                }
            }
            Report::Health(e) => {
                _ = {
                    // TODO
                    self.tx.send(Report::Health(e))
                }
            }
        }
    }

    fn tx_health(&mut self, src: String) -> &mut SenderHealth {
        match self.senders.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(SenderHealth::new(src)),
        }
    }

    fn rx_health(&mut self, src: String) -> &mut ReceiverHealth {
        match self.receivers.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(ReceiverHealth::new(src)),
        }
    }

    fn playout_health(&mut self, src: String) -> &mut PlayoutHealth {
        match self.playouts.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(PlayoutHealth::new(src)),
        }
    }
}
