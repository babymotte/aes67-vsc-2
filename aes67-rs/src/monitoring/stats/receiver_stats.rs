use crate::{
    formats::Frames,
    monitoring::{ReceiverStatsReport, Report, RxStats, StatsReport},
    receiver::config::RxDescriptor,
    time::{
        MICROS_PER_MILLI_F, MICROS_PER_SEC, MILLIS_PER_SEC_F,
    },
    utils::{AverageCalculationBuffer, U16_WRAP},
};
use rtp_rs::Seq;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub struct ReceiverStats {
    id: String,
    tx: mpsc::Sender<Report>,
    desc: Option<RxDescriptor>,
    delay_buffer: AverageCalculationBuffer<i64>,
    measured_link_offset: AverageCalculationBuffer<Frames>,
    timestamp_offset: Option<u64>,
    skipped_packets: HashMap<Frames, Seq>,
    lost_packet_counter: usize,
    late_packet_counter: usize,
}

impl ReceiverStats {
    pub fn new(id: String, tx: mpsc::Sender<Report>) -> Self {
        Self {
            id,
            tx,
            desc: None,
            measured_link_offset: AverageCalculationBuffer::new(vec![0; 1000].into()),
            delay_buffer: AverageCalculationBuffer::new(vec![0; 1000].into()),
            timestamp_offset: None,
            skipped_packets: HashMap::new(),
            lost_packet_counter: 0,
            late_packet_counter: 0,
        }
    }

    pub(crate) async fn process(&mut self, stats: RxStats) {
        match stats {
            RxStats::Started(rx_descriptor) => {
                self.desc = Some(rx_descriptor);
            }
            RxStats::BufferUnderrun => {
                // TODO
            }
            RxStats::InconsistentTimestamp => {
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
            RxStats::TimeTravellingPacket {
                sequence_number,
                ingress_timestamp,
                media_time_at_reception,
            } => {
                self.process_time_travelling_packet(
                    sequence_number,
                    ingress_timestamp,
                    media_time_at_reception,
                )
                .await;
            }
            RxStats::Playout {
                playout_time,
                latest_received_frame,
            } => {
                self.process_playout(playout_time, latest_received_frame)
                    .await;
            }
            RxStats::Stopped => {
                // TODO
            }
            RxStats::MediaClockOffsetChanged(offset, rtp_timestamp) => {
                self.process_media_clock_offset_change(offset, rtp_timestamp)
                    .await;
            }
            RxStats::PacketFromWrongSender(ip) => {
                self.process_packet_from_wrong_sender(ip).await;
            }
        }
    }

    async fn process_time_travelling_packet(
        &mut self,
        sequence_number: Seq,
        ingress_timestamp: u64,
        media_time_at_reception: u64,
    ) {
        let Some(desc) = &self.desc else {
            return;
        };
        let diff = ingress_timestamp - media_time_at_reception;
        let diff_usec =
            (diff as f64 * MICROS_PER_SEC as f64 / desc.audio_format.sample_rate as f64) as u64;
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
    ) {
        // TODO detect and monitor late packets

        let Some(desc) = &self.desc else {
            return;
        };

        // TODO monitor and report packet time
        let frames_in_packet = desc.frames_in_buffer(payload_len) as i64;

        let delay =
            media_time_at_reception as i64 - ingress_timestamp as i64 - frames_in_packet;

        if delay < frames_in_packet {
            // TODO report clock sync issue
        }

        if delay >= 2 * frames_in_packet {
            // TODO report potential network or clock issue
        }

        if let Some(average) = self.delay_buffer.update(delay) {
            let delay_duration = desc.frames_to_duration_float(delay as f64);
            let micros = delay_duration.as_micros();
            let packets = average as f32 / frames_in_packet as f32;
            debug!("Network delay: {average} frames / {micros} µs / {packets:.1} packets");

            self.tx
                .send(Report::Stats(StatsReport::Receiver(
                    ReceiverStatsReport::NetworkDelay {
                        receiver: self.id.clone(),
                        delay_frames: delay,
                        delay_millis: micros as f32 / MICROS_PER_MILLI_F,
                    },
                )))
                .await
                .ok();
        }

        // TODO collect and publish stats on delay ranges and late packets

        self.skipped_packets.remove(&ingress_timestamp);
    }

    async fn process_playout(&mut self, playout_time: Frames, latest_received_frame: Frames) {
        let Some(desc) = &self.desc else {
            return;
        };

        let data_ready_since = latest_received_frame - playout_time;

        if let Some(measured_link_offset) = self.measured_link_offset.update(data_ready_since) {
            let link_offset_ms = measured_link_offset as f32 * MILLIS_PER_SEC_F
                / desc.audio_format.sample_rate as f32;

            if link_offset_ms < desc.link_offset {
                debug!(
                    "Measured link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms (ok)"
                );
            } else {
                debug!(
                    "Measured link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms (too high)"
                );
            }
            self.tx
                .send(Report::Stats(StatsReport::Receiver(
                    ReceiverStatsReport::MeasuredLinkOffset {
                        receiver: self.id.clone(),
                        link_offset_frames: measured_link_offset,
                        link_offset_ms,
                    },
                )))
                .await
                .ok();
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
                "Lost packets: {}",
                missed_timestamps
                    .iter()
                    .map(|(ts, seq)| format!("{{seq: {}, ts: {}}}", u16::from(*seq), ts))
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            self.lost_packet_counter += missed_timestamps.len();
            self.tx
                .send(Report::Stats(StatsReport::Receiver(
                    ReceiverStatsReport::LostPackets {
                        receiver: self.id.clone(),
                        lost_packets: self.lost_packet_counter,
                        timestamp: SystemTime::now(),
                    },
                )))
                .await
                .ok();
        }
    }

    async fn process_media_clock_offset_change(&mut self, offset: u64, rtp_timestamp: u32) {
        debug!("Calibrating timestamp offset at RTP timestamp {rtp_timestamp}");

        let needs_update = if let Some(previous_offset) = self.timestamp_offset {
            if previous_offset != offset {
                warn!(
                    "RTP timestamp offset changed from {previous_offset} to {offset}, this may lead to audio interruptions"
                );
                true
            } else {
                debug!("Offset did not change ({offset})");
                false
            }
        } else {
            debug!("Offset: {offset}");
            true
        };

        if needs_update {
            self.timestamp_offset = Some(offset);
            self.tx
                .send(Report::Stats(crate::monitoring::StatsReport::Receiver(
                    ReceiverStatsReport::MediaClockOffsetChanged {
                        receiver: self.id.clone(),
                        offset,
                    },
                )))
                .await
                .ok();
        }
    }

    async fn process_packet_from_wrong_sender(&self, ip: IpAddr) {
        let Some(desc) = &self.desc else {
            return;
        };
        warn!(
            "Received packet from wrong sender: {} (expected {})",
            ip, desc.origin_ip
        );
        // TODO collect stats + publish
    }

    async fn process_late_packet(&mut self, seq: Seq, timestamp: u64, delay: Frames) {
        let Some(desc) = &self.desc else {
            return;
        };

        let delay_usec = desc.frames_to_duration(delay).as_micros();
        warn!(
            "Late packet: {{seq: {}, ts: {}}} (received {} frames / {} µs after playout time)",
            u16::from(seq),
            timestamp,
            delay,
            delay_usec
        );
        self.late_packet_counter += 1;
        self.tx
            .send(Report::Stats(StatsReport::Receiver(
                ReceiverStatsReport::LatePackets {
                    receiver: self.id.clone(),
                    late_packets: self.late_packet_counter,
                    timestamp: SystemTime::now(),
                },
            )))
            .await
            .ok();
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
                    expected_sequence_number + (i % U16_WRAP) as u16,
                );
            }
        }
    }
}
