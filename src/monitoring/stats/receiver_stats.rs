use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use crate::{
    formats::Frames,
    monitoring::{ObservabilityEvent, ReceiverStatsReport, RxStats},
    receiver::config::RxDescriptor,
    utils::AverageCalculationBuffer,
};
use miette::{IntoDiagnostic, Result};
use rtp_rs::Seq;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub struct ReceiverStats {
    id: String,
    desc: Option<RxDescriptor>,
    latest_received_frame: Frames,
    delay_buffer: AverageCalculationBuffer<Frames>,
    measured_link_offset: AverageCalculationBuffer<Frames>,
    timestamp_offset: Option<u64>,
    skipped_packets: HashMap<Frames, Seq>,
}

impl ReceiverStats {
    pub fn new(id: String) -> Self {
        Self {
            id,
            desc: None,
            latest_received_frame: 0,
            measured_link_offset: AverageCalculationBuffer::new(vec![0; 1000].into()),
            delay_buffer: AverageCalculationBuffer::new(vec![0; 1000].into()),
            timestamp_offset: None,
            skipped_packets: HashMap::new(),
        }
    }

    pub(crate) async fn process(
        &mut self,
        stats: RxStats,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        match stats {
            RxStats::Started(rx_descriptor) => {
                self.desc = Some(rx_descriptor);
            }
            RxStats::BufferUnderrun => {
                // TODO
            }
            RxStats::MulticastGroupPolluted => {
                // TODO
            }
            RxStats::PacketReceived {
                seq,
                payload_len,
                ingress_timestamp,
                media_time_at_reception,
            } => {
                self.process_packet_reception(
                    seq,
                    payload_len,
                    ingress_timestamp,
                    media_time_at_reception,
                    observ_tx,
                )
                .await;
            }
            RxStats::OutOfOrderPacket {
                expected_timestamp,
                expected_sequence_number,
                actual_sequence_number,
            } => {
                self.process_out_of_order_packet(
                    expected_timestamp,
                    expected_sequence_number,
                    actual_sequence_number,
                )
                .await;
            }
            RxStats::MalformedRtpPacket(e) => {
                warn!("received malformed rtp packet: {e:?}");
            }
            RxStats::LatePacket { seq, delay } => {
                self.process_late_packet(seq, delay, observ_tx).await;
            }
            RxStats::TimeTravellingPacket {
                sequence_number,
                ingress_timestamp,
                media_time_at_reception,
            } => {
                self.process_time_travelling_packet(
                    sequence_number,
                    ingress_timestamp,
                    media_time_at_reception,
                    observ_tx,
                )
                .await;
            }
            RxStats::Playout {
                playout_time,
                latest_received_frame,
            } => {
                self.process_playout(playout_time, latest_received_frame, observ_tx)
                    .await;
            }
            RxStats::Stopped => {
                // TODO
            }
            RxStats::MediaClockOffsetChanged(offset, rtp_timestamp) => {
                self.process_media_clock_offset_change(offset, rtp_timestamp, observ_tx)
                    .await;
            }
            RxStats::PacketFromWrongSender(ip) => {
                self.process_packet_from_wrong_sender(ip, observ_tx).await;
            }
        }
    }

    async fn process_time_travelling_packet(
        &mut self,
        sequence_number: Seq,
        ingress_timestamp: u64,
        media_time_at_reception: u64,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        let Some(desc) = &self.desc else {
            return;
        };
        let diff = ingress_timestamp - media_time_at_reception;
        let diff_usec = (diff as f64 * 1_000_000.0 / desc.audio_format.sample_rate as f64) as u64;
        warn!(
            "Packet {} was received {diff} frames / {diff_usec} µs before it was sent, sender and receiver clocks must be out of sync.",
            u16::from(sequence_number)
        );
        // TODO collect stats + publish
    }

    async fn process_packet_reception(
        &mut self,
        seq: Seq,
        payload_len: usize,
        ingress_timestamp: Frames,
        media_time_at_reception: Frames,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        let Some(desc) = &self.desc else {
            return;
        };
        let delay = media_time_at_reception - ingress_timestamp;
        let frames_in_packet = desc.frames_in_buffer(payload_len);
        if let Some(average) = self.delay_buffer.update(delay) {
            let micros = (average * 1_000_000) / desc.audio_format.sample_rate as u64;
            let packets = average as f32 / frames_in_packet as f32;
            info!("Network delay: {average} frames / {micros} µs / {packets:.1} packets");
            // TODO send observability event
        }

        self.skipped_packets.remove(&ingress_timestamp);
    }

    async fn process_playout(
        &mut self,
        playout_time: Frames,
        latest_received_frame: Frames,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        let Some(desc) = &self.desc else {
            return;
        };
        let data_ready_since =
            latest_received_frame + desc.frames_in_link_offset() as u64 - playout_time;

        if let Some(measured_link_offset) = self.measured_link_offset.update(data_ready_since) {
            let link_offset_ms =
                measured_link_offset as f32 / desc.audio_format.sample_rate as f32 * 1_000.0;

            if link_offset_ms < desc.link_offset {
                info!(
                    "Measured minimum link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                );
            } else {
                warn!(
                    "Measured minimum  link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                );
            }
            // TODO send observability event
        }

        let mut missed_timestamps = vec![];

        self.skipped_packets.retain(|ts, seq| {
            let missed = ts > &playout_time;
            if missed {
                missed_timestamps.push((*ts, *seq));
            }
            !missed
        });

        if !missed_timestamps.is_empty() {
            missed_timestamps.sort_by(|(ts_a, _), (ts_b, _)| ts_a.cmp(ts_b));
            warn!(
                "The following packets were late for playout or lost: {}",
                missed_timestamps
                    .iter()
                    .map(|(ts, seq)| format!("seq {} / ts {}", u16::from(*seq), ts))
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            // TODO report lost packets
        }
    }

    async fn process_media_clock_offset_change(
        &mut self,
        offset: u64,
        rtp_timestamp: u32,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        debug!("Calibrating timestamp offset at RTP timestamp {rtp_timestamp}");
        if let Some(previous_offset) = self.timestamp_offset {
            if previous_offset != offset {
                warn!(
                    "RTP timestamp offset changed from {previous_offset} to {offset}, this may lead to audio interruptions"
                );
            } else {
                info!("Offset did not change ({offset})");
            }
        } else {
            info!("Offset: {offset}");
        }
        self.timestamp_offset = Some(offset);
        observ_tx
            .send(ObservabilityEvent::Stats(
                crate::monitoring::StatsReport::Receiver(
                    ReceiverStatsReport::MediaClockOffsetChanged {
                        receiver: self.id.clone(),
                        offset,
                    },
                ),
            ))
            .await
            .ok();
    }

    async fn process_packet_from_wrong_sender(
        &self,
        ip: IpAddr,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        let Some(desc) = &self.desc else {
            return;
        };
        warn!(
            "Received packet from wrong sender: {} (expected {})",
            ip, desc.origin_ip
        );
        // TODO collect stats + publish
    }

    async fn process_late_packet(
        &self,
        seq: Seq,
        delay: Frames,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        warn!(
            "Packet {} is {} frames late for playout.",
            u16::from(seq),
            delay
        );
        // TODO collect stats + publish
    }

    async fn process_out_of_order_packet(
        &mut self,
        expected_timestamp: Frames,
        expected_sequence_number: Seq,
        actual_sequence_number: Seq,
    ) {
        if actual_sequence_number > expected_sequence_number {
            let diff = (actual_sequence_number - expected_sequence_number) as u32;
            for i in 0..diff {
                let skipped_timestamp = expected_timestamp + i as u64;
                self.skipped_packets.insert(
                    skipped_timestamp,
                    expected_sequence_number + (i % u16::MAX as u32) as u16,
                );
            }
        }
    }
}
