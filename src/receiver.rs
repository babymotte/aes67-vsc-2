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
    buffer::FloatingPointAudioBuffer,
    error::{ReceiverInternalError, ReceiverInternalResult},
    monitoring::{Monitoring, ObservabilityEvent, ReceiverEvent, RxStats, Stats},
    receiver::{
        api::{AudioDataRequest, DataState, ReceiverApi, ReceiverApiMessage},
        config::{ReceiverConfig, RxDescriptor},
    },
    socket::create_rx_socket,
    time::MediaClock,
    utils::U32_WRAP,
};
use rtp_rs::{RtpReader, Seq};
use std::{net::SocketAddr, thread, time::Duration};
use tokio::{
    net::UdpSocket,
    runtime, select,
    sync::{
        mpsc,
        oneshot::{self},
    },
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{debug, error, info, instrument, trace, warn};

#[instrument(skip(clock, monitoring))]
pub(crate) async fn start_receiver<C: MediaClock>(
    id: String,
    config: ReceiverConfig,
    clock: C,
    monitoring: Monitoring,
) -> ReceiverInternalResult<ReceiverApi> {
    let receiver_id = id.clone();
    let (result_tx, result_rx) = oneshot::channel();
    let (api_tx, api_rx) = mpsc::channel(1024);
    let desc = RxDescriptor::try_from(&config)?;
    let socket = create_rx_socket(&config.session, config.interface_ip)?;
    thread::Builder::new().name(id.clone()).spawn(move || {
        // set_realtime_priority();

        let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
            Ok(it) => it,
            Err(e) => {
                result_tx.send(Err(ReceiverInternalError::from(e))).ok();
                return;
            }
        };
        let receiver_future = Receiver::start(id, desc, config, clock, api_rx, socket, monitoring);
        result_tx.send(Ok(())).ok();
        runtime.block_on(receiver_future);
    })?;

    result_rx.await??;
    info!("Receiver '{receiver_id}' started successfully.");
    Ok(ReceiverApi::new(api_tx))
}

struct Receiver<C: MediaClock> {
    id: String,
    subsys: SubsystemHandle,
    desc: RxDescriptor,
    clock: C,
    api_rx: mpsc::Receiver<ReceiverApiMessage>,
    last_timestamp: Option<u32>,
    last_sequence_number: Option<Seq>,
    timestamp_offset: Option<u64>,
    rtp_packet_buffer: FloatingPointAudioBuffer,
    latest_received_frame: u64,
    latest_played_frame: u64,
    socket: UdpSocket,
    monitoring: Monitoring,
}

impl<C: MediaClock> Receiver<C> {
    async fn start(
        id: String,
        desc: RxDescriptor,
        config: ReceiverConfig,
        clock: C,
        api_rx: mpsc::Receiver<ReceiverApiMessage>,
        socket: UdpSocket,
        monitoring: Monitoring,
    ) {
        let recv_id = id.clone();

        let desc_rx = desc.clone();
        let packet_buffer_len = desc.audio_format.samples_in_buffer(config.buffer_time);

        let subsystem_name = id.clone();
        let subsystem = move |s: SubsystemHandle| async move {
            Receiver {
                id,
                subsys: s,
                desc,
                clock,
                api_rx,
                last_sequence_number: None,
                last_timestamp: None,
                timestamp_offset: None,

                rtp_packet_buffer: FloatingPointAudioBuffer::new(
                    vec![0f32; packet_buffer_len].into(),
                    desc_rx,
                ),
                latest_received_frame: 0,
                latest_played_frame: 0,
                socket,
                monitoring,
            }
            .run()
            .await
        };
        if let Err(e) = Toplevel::new(|s| async move {
            s.start(SubsystemBuilder::new(subsystem_name, subsystem));
        })
        .handle_shutdown_requests(Duration::from_secs(1))
        .await
        {
            error!("Receiver '{recv_id}' subsystem failed to shut down: {e}");
        }
    }

    async fn run(mut self) -> ReceiverInternalResult<()> {
        let mut receive_buffer = [0; 65_535];

        info!("Receiver '{}' started.", self.id);

        self.report_receiver_created().await;
        self.report_receiver_started().await;

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => {
                    self.handle_api_message(api_msg).await?;
                },
                Ok((len, addr)) = self.socket.recv_from(&mut receive_buffer) => {
                    let time = self.clock.current_media_time()?;
                    self.rtp_data_received(&receive_buffer[..len], addr, time).await?;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

        self.report_receiver_stopped().await;
        self.report_receiver_destroyed().await;

        info!("Receiver '{}' stopped.", self.id);

        Ok(())
    }

    async fn handle_api_message(
        &mut self,
        api_msg: ReceiverApiMessage,
    ) -> ReceiverInternalResult<()> {
        match api_msg {
            ReceiverApiMessage::GetInfo { tx } => _ = tx.send(self.desc.clone()),
            ReceiverApiMessage::DataRequest { req, tx } => {
                _ = tx.send(self.write_out_buf(req).await)
            }
            ReceiverApiMessage::Stop => self.subsys.request_shutdown(),
        }
        Ok(())
    }

    async fn write_out_buf(&mut self, mut req: AudioDataRequest) -> DataState {
        // get the playout buffer
        // this is safe because the thread from which we borrow this buffer is blocked until we send
        // the response back, so no concurrent reads and writes can occur
        let buf = unsafe { req.buffer.buffer_mut::<f32>() };

        if self.latest_received_frame == 0 {
            return DataState::NotReady;
        }

        let buf_frames = buf.len() / self.desc.audio_format.frame_format.channels;
        let last_frame_in_request_buffer = req.playout_time + buf_frames as u64 - 1;

        if last_frame_in_request_buffer > self.latest_received_frame {
            // we have not received enough data to fill the buffer yet
            trace!(
                "Not all requested frames have been received yet (requested frames: [{}; {}]; last received frame: {})!",
                req.playout_time, last_frame_in_request_buffer, self.latest_received_frame
            );
            return DataState::NotReady;
        }

        if last_frame_in_request_buffer > self.latest_played_frame {
            self.latest_played_frame = last_frame_in_request_buffer;
        }

        let oldest_frame_in_buffer =
            self.latest_received_frame - self.rtp_packet_buffer.frames() as u64 + 1;
        if oldest_frame_in_buffer > req.playout_time {
            warn!(
                "The requested data is not in the receiver buffer anymore (requested frames: [{}; {}]; oldest frame in buffer: {})!",
                req.playout_time, last_frame_in_request_buffer, oldest_frame_in_buffer
            );
            return DataState::Missed;
        }

        let state = self.rtp_packet_buffer.read(buf, req.playout_time);

        self.report_playout(&req).await;

        state
    }

    async fn rtp_data_received(
        &mut self,
        data: &[u8],
        addr: SocketAddr,
        media_time_at_reception: u64,
    ) -> ReceiverInternalResult<()> {
        if addr.ip() != self.desc.origin_ip {
            self.report_packet_from_wrong_sender(addr).await;
            return Ok(());
        }

        let rtp = match RtpReader::new(data) {
            Ok(it) => it,
            Err(e) => {
                self.report_malformed_packet(e).await;
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
                        self.report_out_of_order_packet(&rtp, expected_seq, expected_ts, ts_offset)
                            .await;
                    }
                } else {
                    warn!(
                        "Timestamp of out-of-order packet {} is not consistent with sequence id, discarding it",
                        u16::from(rtp.sequence_number())
                    );
                    self.report_inconsistent_timestamp().await;
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

        let Some(ingress_timestamp) = self.unwrapped_timestamp(&rtp) else {
            return Ok(());
        };

        self.report_packet_received(media_time_at_reception, &rtp, seq, ingress_timestamp);

        if ingress_timestamp > media_time_at_reception {
            self.report_time_travelling_packet(media_time_at_reception, &rtp, ingress_timestamp)
                .await;
            return Ok(());
        }

        let last_received_frame = ingress_timestamp + frames_in_packet as u64 - 1;
        if last_received_frame > self.latest_received_frame {
            self.latest_received_frame = last_received_frame;
        }

        self.rtp_packet_buffer
            .insert(rtp.payload(), ingress_timestamp);

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
            warn!(
                "Either the clock has wrapped while packet was in flight or the local clock is not properly synced to PTP. Skipping calibration."
            );
            return Ok(());
        }

        let timestamp_wraps = media_time / U32_WRAP;

        debug!("Sender timestamp has wrapped {timestamp_wraps} times");

        // the offset is the time of the last wrap in media time,
        // i.e. offset + rtp.timestamp should give us an accurate
        // unwrapped media clock timestamp of an rtp packet
        let offset = timestamp_wraps * U32_WRAP;

        self.report_media_clock_offset_changed(offset, rtp_timestamp)
            .await;

        self.timestamp_offset = Some(offset);

        Ok(())
    }
}

mod monitoring {
    use super::*;
    use tokio::sync::mpsc::error::TrySendError;

    impl<C: MediaClock> Receiver<C> {
        pub(crate) async fn report_receiver_created(&mut self) {
            self.monitoring
                .observability()
                .send(ObservabilityEvent::ReceiverEvent(
                    ReceiverEvent::ReceiverCreated {
                        name: self.id.clone(),
                        descriptor: self.desc.clone(),
                    },
                ))
                .await
                .ok();
        }

        pub(crate) async fn report_receiver_started(&mut self) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::Started(self.desc.clone())))
                .await
                .ok();
        }

        pub(crate) fn report_packet_received(
            &mut self,
            media_time_at_reception: u64,
            rtp: &RtpReader<'_>,
            seq: Seq,
            ingress_timestamp: u64,
        ) {
            if let Err(TrySendError::Full(_)) =
                self.monitoring
                    .stats()
                    .try_send(Stats::Rx(RxStats::PacketReceived {
                        seq,
                        payload_len: rtp.payload().len(),
                        ingress_timestamp,
                        media_time_at_reception,
                    }))
            {
                warn!("dropped stats message, buffer is full");
            }
        }

        pub(crate) async fn report_playout(&mut self, req: &AudioDataRequest) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::Playout {
                    playout_time: req.playout_time,
                    latest_received_frame: self.latest_received_frame,
                }))
                .await
                .ok();
        }

        pub(crate) async fn report_media_clock_offset_changed(
            &mut self,
            offset: u64,
            rtp_timestamp: u32,
        ) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::MediaClockOffsetChanged(
                    offset,
                    rtp_timestamp,
                )))
                .await
                .ok();
        }

        pub(crate) async fn report_packet_from_wrong_sender(&mut self, addr: SocketAddr) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::PacketFromWrongSender(addr.ip())))
                .await
                .ok();
        }

        pub(crate) async fn report_malformed_packet(&mut self, e: rtp_rs::RtpReaderError) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::MalformedRtpPacket(e)))
                .await
                .ok();
        }

        pub(crate) async fn report_inconsistent_timestamp(&mut self) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::InconsistentTimestamp))
                .await
                .ok();
        }

        pub(crate) async fn report_out_of_order_packet(
            &mut self,
            rtp: &RtpReader<'_>,
            expected_seq: Seq,
            expected_ts: u32,
            ts_offset: u64,
        ) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::OutOfOrderPacket {
                    expected_timestamp: ts_offset + expected_ts as u64,
                    expected_sequence_number: expected_seq,
                    actual_sequence_number: rtp.sequence_number(),
                }))
                .await
                .ok();
        }

        pub(crate) async fn report_time_travelling_packet(
            &mut self,
            media_time_at_reception: u64,
            rtp: &RtpReader<'_>,
            ingress_timestamp: u64,
        ) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::TimeTravellingPacket {
                    sequence_number: rtp.sequence_number(),
                    ingress_timestamp,
                    media_time_at_reception,
                }))
                .await
                .ok();
        }

        pub(crate) async fn report_receiver_stopped(&mut self) {
            self.monitoring
                .stats()
                .send(Stats::Rx(RxStats::Stopped))
                .await
                .ok();
        }

        pub(crate) async fn report_receiver_destroyed(&mut self) {
            self.monitoring
                .observability()
                .send(ObservabilityEvent::ReceiverEvent(
                    ReceiverEvent::ReceiverDestroyed {
                        name: self.id.clone(),
                    },
                ))
                .await
                .ok();
        }
    }
}
