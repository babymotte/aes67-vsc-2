mod observability;
mod stats;

use crate::{
    formats::{Frames, MilliSeconds},
    monitoring::{observability::observability, stats::stats},
    receiver::config::RxDescriptor,
};
use rtp_rs::{RtpReaderError, Seq};
use std::{io, net::IpAddr, thread, time::Duration};
use tokio::{
    runtime::{self},
    spawn,
    sync::mpsc,
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{info, warn};

#[derive(Debug)]
pub enum ObservabilityEvent {
    VscEvent(VscEvent),
    SenderEvent(SenderEvent),
    ReceiverEvent(ReceiverEvent),
    Stats(StatsReport),
}

#[derive(Debug)]
pub enum VscEvent {
    VscCreated,
}

#[derive(Debug)]
pub enum SenderEvent {
    SenderCreated { name: String, sdp: String },
    SenderDestroyed { name: String },
}

#[derive(Debug)]
pub enum ReceiverEvent {
    ReceiverCreated {
        name: String,
        descriptor: RxDescriptor,
    },
    ReceiverDestroyed {
        name: String,
    },
}

#[derive(Debug)]
pub enum StatsReport {
    Vsc(VscStatsReport),
    Sender(SenderStatsReport),
    Receiver(ReceiverStatsReport),
}

#[derive(Debug)]
pub enum VscStatsReport {}

#[derive(Debug)]
pub enum SenderStatsReport {}

#[derive(Debug)]
pub enum ReceiverStatsReport {
    MediaClockOffsetChanged {
        receiver: String,
        offset: u64,
    },
    NetworkDelay {
        receiver: String,
        delay: Frames,
    },
    MeasuredLinkOffset {
        receiver: String,
        link_offset_frames: Frames,
        link_offset_ms: MilliSeconds,
    },
}

#[derive(Debug)]
pub enum Stats {
    Tx(TxStats),
    Rx(RxStats),
    Playout(PoStats),
}

#[derive(Debug)]
pub enum TxStats {
    BufferUnderrun,
}

#[derive(Debug)]
pub enum RxStats {
    Started(RxDescriptor),
    BufferUnderrun,
    MulticastGroupPolluted,
    PacketReceived {
        seq: Seq,
        payload_len: usize,
        ingress_timestamp: Frames,
        media_time_at_reception: Frames,
    },
    OutOfOrderPacket {
        expected_timestamp: Frames,
        expected_sequence_number: Seq,
        actual_sequence_number: Seq,
    },
    MalformedRtpPacket(RtpReaderError),
    LatePacket {
        seq: Seq,
        delay: Frames,
    },
    TimeTravellingPacket {
        sequence_number: Seq,
        ingress_timestamp: Frames,
        media_time_at_reception: Frames,
    },
    Playout {
        playout_time: Frames,
        latest_received_frame: Frames,
    },
    Stopped,
    MediaClockOffsetChanged(Frames, u32),
    PacketFromWrongSender(IpAddr),
}

#[derive(Debug)]
pub enum PoStats {
    BufferUnderrun,
}

#[derive(Clone)]
struct MonitoringParent {
    stats_tx: mpsc::Sender<(Stats, String)>,
    observ_tx: mpsc::Sender<ObservabilityEvent>,
}

impl MonitoringParent {
    fn child(&self, id: String) -> Monitoring {
        let (child, start) = self.deferred_child(id);
        spawn(start);
        child
    }

    fn deferred_child(&self, id: String) -> (Monitoring, impl Future<Output = ()> + 'static) {
        let (stats_tx, mut stats_rx) = mpsc::channel(1024);
        let parent = self.clone();
        let parent_tx = self.stats_tx.clone();
        let start = async move {
            while let Some(evt) = stats_rx.recv().await {
                if parent_tx.send((evt, id.clone())).await.is_err() {
                    break;
                }
            }
        };
        (Monitoring { parent, stats_tx }, start)
    }
}

pub struct Monitoring {
    parent: MonitoringParent,
    stats_tx: mpsc::Sender<Stats>,
}

impl Monitoring {
    pub fn stats(&self) -> &mpsc::Sender<Stats> {
        &self.stats_tx
    }

    pub fn observability(&self) -> &mpsc::Sender<ObservabilityEvent> {
        &self.parent.observ_tx
    }

    pub fn child(&self, id: String) -> Monitoring {
        self.parent.child(id)
    }
}

pub fn start_monitoring_service(root_id: String) -> Monitoring {
    let (stats_tx, stats_rx) = mpsc::channel(1024);
    let (observ_tx, observ_rx) = mpsc::channel(1024);
    let stats_to_obs = observ_tx.clone();
    let client_name = root_id.clone();

    let parent = MonitoringParent {
        stats_tx,
        observ_tx,
    };

    let (child, start) = parent.deferred_child(root_id);

    let monitoring_thread =
        move || monitoring(client_name, stats_rx, stats_to_obs, observ_rx, start);

    thread::Builder::new()
        .name("monitoring".to_owned())
        .spawn(monitoring_thread)
        .expect("no dynmic input, cannot fail");

    child
}

fn monitoring(
    client_name: String,
    stats_rx: mpsc::Receiver<(Stats, String)>,
    observability_tx: mpsc::Sender<ObservabilityEvent>,
    observability_rx: mpsc::Receiver<ObservabilityEvent>,
    start: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let stats = |s: SubsystemHandle| stats(s, stats_rx, observability_tx);
    let observability = |s: SubsystemHandle| observability(s, client_name, observability_rx);
    let monitoring = async {
        spawn(start);
        if let Err(e) = Toplevel::new(|s| async move {
            s.start(SubsystemBuilder::new("stats", stats));
            s.start(SubsystemBuilder::new("observability", observability));
        })
        .handle_shutdown_requests(Duration::from_secs(1))
        .await
        {
            warn!("Monitoring subsystem failed to shut down: {e}");
        }
    };

    info!("Monitoring subsystem started.");

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(monitoring);

    info!("Monitoring subsystem stopped.");

    Ok(())
}
