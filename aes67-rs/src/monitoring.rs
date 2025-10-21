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

mod health;
mod observability;
mod stats;

use crate::{
    app::{propagate_exit, spawn_child_app},
    error::{ChildAppError, ChildAppResult},
    formats::{Frames, MilliSeconds},
    monitoring::{health::health, observability::observability, stats::stats},
    receiver::config::RxDescriptor,
    sender::config::TxDescriptor,
};
use rtp_rs::Seq;
use std::{net::IpAddr, time::SystemTime};
use tokio::{
    spawn,
    sync::{
        broadcast,
        mpsc::{self, error::TrySendError},
    },
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use worterbuch_client::Worterbuch;

#[derive(Debug, Clone)]
pub enum MonitoringEvent {
    State(StateEvent),
    Stats(Stats),
}

#[derive(Debug, Clone)]
pub enum Report {
    State(StateEvent),
    Stats(StatsReport),
    Health(HealthReport),
}

#[derive(Debug, Clone)]
pub enum StateEvent {
    Vsc(VscState),
    Sender(SenderState),
    Receiver(ReceiverState),
}

#[derive(Debug, Clone)]
pub enum VscState {
    VscCreated,
}

#[derive(Debug, Clone)]
pub enum SenderState {
    Created {
        id: String,
        descriptor: TxDescriptor,
        label: String,
    },
    Renamed {
        id: String,
        label: String,
    },
    Destroyed {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub enum ReceiverState {
    Created {
        id: String,
        descriptor: RxDescriptor,
        label: String,
    },
    Renamed {
        id: String,
        label: String,
    },
    Destroyed {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub enum StatsReport {
    Vsc(VscStatsReport),
    Sender(SenderStatsReport),
    Receiver(ReceiverStatsReport),
}

#[derive(Debug, Clone)]
pub enum VscStatsReport {}

#[derive(Debug, Clone)]
pub enum SenderStatsReport {}

#[derive(Debug, Clone)]
pub enum ReceiverStatsReport {
    MediaClockOffsetChanged {
        receiver: String,
        offset: u64,
    },
    NetworkDelay {
        receiver: String,
        delay_frames: i64,
        delay_millis: MilliSeconds,
    },
    MeasuredLinkOffset {
        receiver: String,
        link_offset_frames: Frames,
        link_offset_ms: MilliSeconds,
    },
    LostPackets {
        receiver: String,
        lost_packets: usize,
        timestamp: SystemTime,
    },
    LatePackets {
        receiver: String,
        late_packets: usize,
        timestamp: SystemTime,
    },
    Muted {
        receiver: String,
        muted: bool,
    },
}

#[derive(Debug, Clone)]
pub enum HealthReport {
    Vsc(VscHealthReport),
    Sender(SenderHealthReport),
    Receiver(ReceiverHealthReport),
}

#[derive(Debug, Clone)]
pub enum VscHealthReport {}

#[derive(Debug, Clone)]
pub enum SenderHealthReport {}

#[derive(Debug, Clone)]
pub enum ReceiverHealthReport {}

#[derive(Debug, Clone)]
pub enum Stats {
    Tx(TxStats),
    Rx(RxStats),
    Playout(PoStats),
}

#[derive(Debug, Clone)]
pub enum TxStats {
    BufferUnderrun,
    PacketTime(u64),
    PacketSize(usize),
}

#[derive(Debug, Clone)]
pub enum RxStats {
    Started(RxDescriptor),
    BufferUnderrun,
    InconsistentTimestamp,
    PacketReceived {
        seq: Seq,
        payload_len: usize,
        ingress_time: Frames,
        media_time_at_reception: Frames,
    },
    OutOfOrderPacket {
        expected_timestamp: Frames,
        expected_sequence_number: Seq,
        actual_sequence_number: Seq,
    },
    MalformedRtpPacket(String),
    TimeTravellingPacket {
        sequence_number: Seq,
        ingress_time: Frames,
        media_time_at_reception: Frames,
    },
    Playout {
        ingress_time: Frames,
        latest_received_frame: Frames,
    },
    Stopped,
    MediaClockOffsetChanged(Frames, u32),
    PacketFromWrongSender(IpAddr),
    Muted(bool),
}

#[derive(Debug, Clone)]
pub enum PoStats {
    BufferUnderrun,
}

#[derive(Debug, Clone)]
struct MonitoringParent(mpsc::Sender<(MonitoringEvent, String)>);

impl MonitoringParent {
    fn child(&self, id: String) -> Monitoring {
        let (child, start) = self.deferred_child(id);
        spawn(start);
        child
    }

    fn deferred_child(&self, id: String) -> (Monitoring, impl Future<Output = ()> + 'static) {
        let (tx, mut rx) = mpsc::channel(1024);
        let parent_tx = self.0.clone();
        let parent = MonitoringParent(parent_tx.clone());
        let start = async move {
            while let Some(evt) = rx.recv().await {
                if parent_tx.send((evt, id.clone())).await.is_err() {
                    break;
                }
            }
        };
        (Monitoring { parent, tx }, start)
    }
}

#[derive(Debug, Clone)]
pub struct Monitoring {
    parent: MonitoringParent,
    tx: mpsc::Sender<MonitoringEvent>,
}

impl Monitoring {
    pub fn child(&self, id: String) -> Monitoring {
        self.parent.child(id)
    }

    pub async fn vsc_state(&self, state: VscState) {
        self.tx
            .send(MonitoringEvent::State(StateEvent::Vsc(state)))
            .await
            .ok();
    }

    pub async fn sender_state(&self, state: SenderState) {
        self.tx
            .send(MonitoringEvent::State(StateEvent::Sender(state)))
            .await
            .ok();
    }

    pub async fn receiver_state(&self, state: ReceiverState) {
        self.tx
            .send(MonitoringEvent::State(StateEvent::Receiver(state)))
            .await
            .ok();
    }

    pub fn sender_stats(&self, stats: TxStats) {
        if let Err(TrySendError::Full(_)) =
            self.tx.try_send(MonitoringEvent::Stats(Stats::Tx(stats)))
        {
            warn!("Dropping sender stats, buffer is full!");
        }
    }

    pub fn receiver_stats(&self, stats: RxStats) {
        if let Err(TrySendError::Full(_)) =
            self.tx.try_send(MonitoringEvent::Stats(Stats::Rx(stats)))
        {
            warn!("Dropping receiver stats, buffer is full!");
        }
    }

    pub fn playout_stats(&self, stats: PoStats) {
        if let Err(TrySendError::Full(_)) = self
            .tx
            .try_send(MonitoringEvent::Stats(Stats::Playout(stats)))
        {
            warn!("Dropping playout stats, buffer is full!");
        }
    }
}

pub fn start_monitoring_service(
    root_id: String,
    shutdown_token: CancellationToken,
    worterbuch_client: Worterbuch,
) -> ChildAppResult<Monitoring> {
    let (mon_tx, mon_rx) = mpsc::channel::<(MonitoringEvent, String)>(1024);

    let client_name = root_id.clone();

    let parent = MonitoringParent(mon_tx);

    let (child, start) = parent.deferred_child(root_id);

    monitoring(
        client_name,
        mon_rx,
        start,
        shutdown_token,
        worterbuch_client,
    )?;

    Ok(child)
}

fn monitoring(
    client_name: String,
    mon_rx: mpsc::Receiver<(MonitoringEvent, String)>,
    start: impl Future<Output = ()> + Send + 'static,
    shutdown_token: CancellationToken,
    worterbuch_client: Worterbuch,
) -> ChildAppResult<()> {
    let name = format!("{client_name}-monitoring");

    let (stats_tx, stats_rx) = mpsc::channel::<Report>(1024);
    let (observ_tx, observ_rx) = broadcast::channel::<Report>(1024);

    let stats = |s: SubsystemHandle| stats(s, mon_rx, stats_tx);
    let health = |s: SubsystemHandle| health(s, stats_rx, observ_tx);
    let observability =
        |s: SubsystemHandle| observability(s, client_name, observ_rx, worterbuch_client);

    let subsystem = |s: SubsystemHandle| async move {
        spawn(start);
        s.start(SubsystemBuilder::new("stats", stats));
        s.start(SubsystemBuilder::new("health", health));
        s.start(SubsystemBuilder::new("observability", observability));
        s.on_shutdown_requested().await;
        Ok::<(), ChildAppError>(())
    };

    propagate_exit(
        spawn_child_app(name, subsystem, shutdown_token.clone())?,
        shutdown_token,
    );

    Ok(())
}
