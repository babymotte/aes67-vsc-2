mod playout_stats;
mod receiver_stats;
mod sender_stats;

use crate::monitoring::{
    ObservabilityEvent, Stats,
    stats::{
        playout_stats::PlayoutStats, receiver_stats::ReceiverStats, sender_stats::SenderStats,
    },
};
use std::collections::{HashMap, hash_map::Entry};
use tokio::{select, sync::mpsc};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{info, warn};

pub async fn stats(
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<(Stats, String)>,
    tx: mpsc::Sender<ObservabilityEvent>,
) -> Result<(), &'static str> {
    StatsActor::new(subsys, rx, tx).run().await;
    Ok(())
}

struct StatsActor {
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<(Stats, String)>,
    tx: mpsc::Sender<ObservabilityEvent>,
    senders: HashMap<String, SenderStats>,
    receivers: HashMap<String, ReceiverStats>,
    playouts: HashMap<String, PlayoutStats>,
}

impl StatsActor {
    fn new(
        subsys: SubsystemHandle,
        rx: mpsc::Receiver<(Stats, String)>,
        tx: mpsc::Sender<ObservabilityEvent>,
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
        info!("Stats subsystem started.");
        loop {
            select! {
                Some((evt,src)) = self.rx.recv() => self.process_event(evt,src).await,
                _ = self.subsys.on_shutdown_requested() => break,
                else => {
                    self.subsys.request_shutdown();
                    break;
                },
            }
        }
        info!("Stats subsystem stopped.");
    }

    async fn process_event(&mut self, evt: Stats, src: String) {
        let tx = self.tx.clone();
        match evt {
            Stats::Tx(stats) => self.tx_stats(src).process(stats, tx).await,
            Stats::Rx(stats) => self.rx_stats(src).process(stats, tx).await,
            Stats::Playout(stats) => self.playout_stats(src).process(stats, tx).await,
        }
    }

    fn tx_stats(&mut self, src: String) -> &mut SenderStats {
        match self.senders.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(SenderStats::new(src)),
        }
    }

    fn rx_stats(&mut self, src: String) -> &mut ReceiverStats {
        match self.receivers.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(ReceiverStats::new(src)),
        }
    }

    fn playout_stats(&mut self, src: String) -> &mut PlayoutStats {
        match self.playouts.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(PlayoutStats::new(src)),
        }
    }
}
