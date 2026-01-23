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

mod receiver_health;
use crate::monitoring::Report;
use std::collections::{HashMap, hash_map::Entry};
use tokio::{
    select,
    sync::{broadcast, mpsc},
};
use tosub::Subsystem;
use tracing::info;

pub async fn health(
    subsys: Subsystem,
    rx: mpsc::Receiver<Report>,
    tx: broadcast::Sender<Report>,
) -> Result<(), &'static str> {
    HealthActor::new(subsys, rx, tx).run().await;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SenderHealth {}
impl SenderHealth {
    fn new(_src: String) -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct ReceiverHealth {}
impl ReceiverHealth {
    fn new(_src: String) -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct PlayoutHealth {}
impl PlayoutHealth {
    fn new(_src: String) -> Self {
        Self {}
    }
}

struct HealthActor {
    subsys: Subsystem,
    rx: mpsc::Receiver<Report>,
    tx: broadcast::Sender<Report>,
    senders: HashMap<String, SenderHealth>,
    receivers: HashMap<String, ReceiverHealth>,
    playouts: HashMap<String, PlayoutHealth>,
}

impl HealthActor {
    fn new(subsys: Subsystem, rx: mpsc::Receiver<Report>, tx: broadcast::Sender<Report>) -> Self {
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
                recv = self.rx.recv() => match recv {
                    Some(report) => self.process_report(report).await,
                    None => {
                        info!("Health report channel closed; shutting down subsystem");
                        self.subsys.request_global_shutdown();
                        break;
                    }
                },
                _ = self.subsys.shutdown_requested() => break,
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
