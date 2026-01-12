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

//! This module implements an AES67 compatible receiver.
//! Once started it uses the provided configuration to open a datagram socket and, if applicable, joins a multicast group tp receive RTP data.
//! RTP data is decoded and written to the appropriate frame of a shared memory buffer based on the receiver's current PTP media clock.

pub mod api;
pub mod config;

use crate::{
    app::{spawn_child_app, wait_for_start},
    buffer::{AudioBufferPointer, ReceiverBufferProducer, receiver_buffer_channel},
    error::ReceiverInternalResult,
    monitoring::{Monitoring, ReceiverState, RxStats},
    receiver::{
        api::{ReceiverApi, ReceiverApiMessage},
        config::{ReceiverConfig, RxDescriptor},
    },
    socket::create_rx_socket,
    time::{Clock, MediaClock},
    utils::U32_WRAP,
};
use pnet::datalink::NetworkInterface;
use rtp_rs::{RtpReader, Seq};
use std::net::SocketAddr;
use tokio::{net::UdpSocket, select, sync::mpsc};
use tokio_graceful_shutdown::SubsystemHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};
#[cfg(feature = "tokio-metrics")]
use worterbuch_client::Worterbuch;

#[instrument(skip(clock, monitoring, shutdown_token, wb))]
pub(crate) async fn start_receiver(
    app_id: String,
    id: String,
    label: String,
    iface: NetworkInterface,
    config: ReceiverConfig,
    clock: Clock,
    monitoring: Monitoring,
    shutdown_token: CancellationToken,
    #[cfg(feature = "tokio-metrics")] wb: Worterbuch,
) -> ReceiverInternalResult<ReceiverApi> {
    let receiver_id = id.clone();
    let (api_tx, api_rx) = mpsc::channel(1024);
    let desc = RxDescriptor::try_from(&config)?;
    let (tx, rx) = receiver_buffer_channel(desc.clone(), monitoring.clone());
    let socket = create_rx_socket(&config.session, iface)?;

    let subsystem_name = id.clone();
    let subsystem = async move |s: &mut SubsystemHandle| {
        Receiver {
            id,
            label,
            subsys: s,
            desc,
            clock,
            api_rx,
            last_sequence_number: None,
            last_timestamp: None,
            timestamp_offset: None,
            socket,
            monitoring,
            tx,
        }
        .run()
        .await
    };

    let mut app = spawn_child_app(
        #[cfg(feature = "tokio-metrics")]
        app_id,
        subsystem_name.clone(),
        subsystem,
        shutdown_token,
        #[cfg(feature = "tokio-metrics")]
        wb,
    )?;
    wait_for_start(subsystem_name, &mut app).await?;

    info!("Receiver '{receiver_id}' started successfully.");
    Ok(ReceiverApi::new(api_tx, rx))
}

struct Receiver<'a> {
    id: String,
    label: String,
    subsys: &'a mut SubsystemHandle,
    desc: RxDescriptor,
    clock: Clock,
    api_rx: mpsc::Receiver<ReceiverApiMessage>,
    last_timestamp: Option<u32>,
    last_sequence_number: Option<Seq>,
    timestamp_offset: Option<u64>,
    socket: UdpSocket,
    monitoring: Monitoring,
    tx: ReceiverBufferProducer,
}

impl<'a> Receiver<'a> {
    async fn run(mut self) -> ReceiverInternalResult<()> {
        let mut receive_buffer = [0; 65_535];

        info!("Receiver '{}' started.", self.id);

        self.report_receiver_created(AudioBufferPointer::from_slice(&receive_buffer))
            .await;

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => {
                    self.handle_api_message(api_msg).await?;
                },
                Ok((len, addr)) = self.socket.recv_from(&mut receive_buffer) => {
                    let time = self.clock.current_media_time()?;
                    self.rtp_data_received(&receive_buffer[..len], addr, time).await?;
                },
                _ = self.subsys.on_shutdown_requested() => {
                    info!("Shutdown of receiver '{}' requested.", self.id);
                    break;
                },
                else => break,
            }
        }

        self.report_receiver_destroyed().await;

        info!("Receiver '{}' stopped.", self.id);

        Ok(())
    }

    async fn handle_api_message(
        &mut self,
        api_msg: ReceiverApiMessage,
    ) -> ReceiverInternalResult<()> {
        match api_msg {
            ReceiverApiMessage::Stop(tx) => {
                self.subsys.request_local_shutdown();
                self.subsys.wait_for_children().await;
                tx.send(()).ok();
            }
        }
        Ok(())
    }

    async fn rtp_data_received(
        &mut self,
        data: &[u8],
        addr: SocketAddr,
        media_time_at_reception: u64,
    ) -> ReceiverInternalResult<()> {
        if addr.ip() != self.desc.origin_ip {
            self.report_packet_from_wrong_sender(addr);
            return Ok(());
        }

        let rtp = match RtpReader::new(data) {
            Ok(it) => it,
            Err(e) => {
                self.report_malformed_packet(e);
                return Ok(());
            }
        };

        let seq = rtp.sequence_number();
        let ts = rtp.timestamp();

        let mut ts_wrapped = false;
        let mut seq_wrapped = false;

        let frames_in_packet = self.desc.frames_in_buffer(rtp.payload().len());

        if let (Some(last_ts), Some(last_seq)) = (self.last_timestamp, self.last_sequence_number) {
            let expected_seq = last_seq.next();
            let expected_ts = last_ts.wrapping_add(frames_in_packet as u32);
            if seq != expected_seq {
                debug!(
                    "Inconsistent sequence number: {} (last was {})",
                    u16::from(seq),
                    u16::from(last_seq)
                );

                let diff = seq - expected_seq;
                let consistent_ts = expected_ts as i64 + frames_in_packet as i64 * diff as i64;
                if consistent_ts == ts as i64 {
                    debug!(
                        "Timestamp of out-of-order packet is consistent with sequence id, queuing it for playout"
                    );
                    if let Some(ts_offset) = self.timestamp_offset {
                        self.report_out_of_order_packet(&rtp, expected_seq, expected_ts, ts_offset);
                    }
                } else {
                    warn!(
                        "Timestamp of out-of-order packet {} is not consistent with sequence id, discarding it",
                        u16::from(rtp.sequence_number())
                    );
                    self.report_inconsistent_timestamp();
                    return Ok(());
                }
            }

            ts_wrapped = ts < last_ts;
            seq_wrapped = u16::from(seq) < u16::from(last_seq);
        }

        if seq_wrapped || self.timestamp_offset.is_none() {
            self.calibrate_timestamp_offset(ts).await?;
        }

        if ts_wrapped {
            debug!("RTP timestamp wrapped");
            self.calibrate_timestamp_offset(ts).await?;
        }

        self.last_sequence_number = Some(seq);
        self.last_timestamp = Some(ts);

        let Some(ingress_time) = self.unwrapped_timestamp(&rtp) else {
            return Ok(());
        };

        self.report_packet_received(media_time_at_reception, &rtp, seq, ingress_time);

        if ingress_time > media_time_at_reception {
            self.report_time_travelling_packet(media_time_at_reception, &rtp, ingress_time);
            self.calibrate_timestamp_offset(ts).await?;
            // TODO check how far packet is off and if it is save to insert into the buffer
            // return Ok(());
        }

        self.tx.write(rtp.payload(), ingress_time).await;

        Ok(())
    }

    fn unwrapped_timestamp(&self, rtp: &RtpReader) -> Option<u64> {
        self.timestamp_offset
            .map(|ts_offset| ts_offset + rtp.timestamp() as u64 - self.desc.rtp_offset as u64)
    }

    #[instrument(skip(self))]
    async fn calibrate_timestamp_offset(
        &mut self,
        rtp_timestamp: u32,
    ) -> ReceiverInternalResult<()> {
        let media_time = self.clock.current_media_time()?;

        let local_wrapped_timestamp = (media_time % U32_WRAP) as u32;

        if local_wrapped_timestamp < rtp_timestamp {
            // dbg!(rtp_timestamp - local_wrapped_timestamp);
            // warn!(
            //     "Either the clock has wrapped while packet was in flight or the local clock is not properly synced to PTP. Skipping calibration."
            // );
            return Ok(());
        }

        let timestamp_wraps = media_time / U32_WRAP;

        debug!("Sender timestamp has wrapped {timestamp_wraps} times");

        // the offset is the time of the last wrap in media time,
        // i.e. offset + rtp.timestamp should give us an accurate
        // unwrapped media clock timestamp of an rtp packet
        let offset = timestamp_wraps * U32_WRAP;

        self.report_media_clock_offset_changed(offset, rtp_timestamp);

        self.timestamp_offset = Some(offset);

        Ok(())
    }
}

mod monitoring {
    use crate::buffer::AudioBufferPointer;

    use super::*;

    impl<'a> Receiver<'a> {
        pub(crate) async fn report_receiver_created(&mut self, buffer: AudioBufferPointer) {
            self.monitoring
                .receiver_state(ReceiverState::Created {
                    id: self.id.clone(),
                    label: self.label.clone(),
                    descriptor: self.desc.clone(),
                    address: buffer,
                })
                .await;
        }

        pub(crate) fn report_packet_received(
            &mut self,
            media_time_at_reception: u64,
            rtp: &RtpReader<'_>,
            seq: Seq,
            ingress_time: u64,
        ) {
            self.monitoring.receiver_stats(RxStats::PacketReceived {
                seq,
                payload_len: rtp.payload().len(),
                ingress_time,
                media_time_at_reception,
            });
        }

        pub(crate) fn report_media_clock_offset_changed(
            &mut self,
            offset: u64,
            rtp_timestamp: u32,
        ) {
            self.monitoring
                .receiver_stats(RxStats::MediaClockOffsetChanged(offset, rtp_timestamp));
        }

        pub(crate) fn report_packet_from_wrong_sender(&mut self, addr: SocketAddr) {
            self.monitoring
                .receiver_stats(RxStats::PacketFromWrongSender(addr.ip()));
        }

        pub(crate) fn report_malformed_packet(&mut self, e: rtp_rs::RtpReaderError) {
            self.monitoring
                .receiver_stats(RxStats::MalformedRtpPacket(format!("{e:?}")));
        }

        pub(crate) fn report_inconsistent_timestamp(&mut self) {
            self.monitoring
                .receiver_stats(RxStats::InconsistentTimestamp);
        }

        pub(crate) fn report_out_of_order_packet(
            &mut self,
            rtp: &RtpReader<'_>,
            expected_seq: Seq,
            expected_ts: u32,
            ts_offset: u64,
        ) {
            self.monitoring.receiver_stats(RxStats::OutOfOrderPacket {
                expected_timestamp: ts_offset + expected_ts as u64,
                expected_sequence_number: expected_seq,
                actual_sequence_number: rtp.sequence_number(),
            });
        }

        pub(crate) fn report_time_travelling_packet(
            &mut self,
            media_time_at_reception: u64,
            rtp: &RtpReader<'_>,
            ingress_time: u64,
        ) {
            self.monitoring
                .receiver_stats(RxStats::TimeTravellingPacket {
                    sequence_number: rtp.sequence_number(),
                    ingress_time,
                    media_time_at_reception,
                });
        }

        pub(crate) async fn report_receiver_destroyed(&mut self) {
            self.monitoring
                .receiver_state(ReceiverState::Destroyed {
                    id: self.id.clone(),
                })
                .await;
        }
    }
}
