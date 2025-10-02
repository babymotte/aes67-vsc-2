/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

mod playout_stats;
mod receiver_stats;
mod sender_stats;

use crate::monitoring::{
    MonitoringEvent, ReceiverState, Report, RxStats, SenderState, StateEvent, Stats, VscState,
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
    rx: mpsc::Receiver<(MonitoringEvent, String)>,
    tx: mpsc::Sender<Report>,
) -> Result<(), &'static str> {
    StatsActor::new(subsys, rx, tx).run().await;
    Ok(())
}

struct StatsActor {
    subsys: SubsystemHandle,
    rx: mpsc::Receiver<(MonitoringEvent, String)>,
    tx: mpsc::Sender<Report>,
    senders: HashMap<String, SenderStats>,
    receivers: HashMap<String, ReceiverStats>,
    playouts: HashMap<String, PlayoutStats>,
}

impl StatsActor {
    fn new(
        subsys: SubsystemHandle,
        rx: mpsc::Receiver<(MonitoringEvent, String)>,
        tx: mpsc::Sender<Report>,
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

    async fn process_event(&mut self, evt: MonitoringEvent, src: String) {
        match evt {
            MonitoringEvent::State(evt) => self.process_state(evt).await,
            MonitoringEvent::Stats(evt) => self.process_stats(evt, src).await,
        }
    }

    async fn process_state(&mut self, evt: StateEvent) {
        match &evt {
            StateEvent::Vsc(s) => self.process_vsc_state(s).await,
            StateEvent::Sender(s) => self.process_sender_state(s).await,
            StateEvent::Receiver(s) => self.process_receiver_state(s).await,
        }

        self.tx.send(Report::State(evt)).await.ok();
    }

    async fn process_stats(&mut self, evt: Stats, src: String) {
        match evt {
            Stats::Tx(stats) => self.tx_stats(src).process(stats).await,
            Stats::Rx(stats) => self.rx_stats(src).process(stats).await,
            Stats::Playout(stats) => self.playout_stats(src).process(stats).await,
        }
    }

    fn tx_stats(&mut self, src: String) -> &mut SenderStats {
        match self.senders.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(SenderStats::new(src, self.tx.clone())),
        }
    }

    fn rx_stats(&mut self, src: String) -> &mut ReceiverStats {
        match self.receivers.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(ReceiverStats::new(src, self.tx.clone())),
        }
    }

    fn playout_stats(&mut self, src: String) -> &mut PlayoutStats {
        match self.playouts.entry(src.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(PlayoutStats::new(src, self.tx.clone())),
        }
    }

    async fn process_vsc_state(&mut self, s: &VscState) {
        // TODO
    }

    async fn process_sender_state(&mut self, s: &SenderState) {
        // TODO
    }

    async fn process_receiver_state(&mut self, s: &ReceiverState) {
        match s {
            ReceiverState::ReceiverCreated { name, descriptor } => {
                self.rx_stats(name.to_owned())
                    .process(RxStats::Started(descriptor.to_owned()))
                    .await
            }
            ReceiverState::ReceiverDestroyed { name } => {
                warn!("receiver destroyed: {name}");
                // TODO
            }
        }
    }
}
