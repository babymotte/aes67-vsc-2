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
    receiver::{
        api::{AudioDataRequest, DataState, ReceiverApi, ReceiverApiMessage},
        config::{ReceiverConfig, RxDescriptor},
    },
    socket::create_rx_socket,
    time::MediaClock,
    utils::AverageCalculationBuffer,
};
use rtp_rs::{RtpReader, Seq};
use std::{net::SocketAddr, thread, time::Duration, u32};
use tokio::{
    net::UdpSocket,
    runtime, select,
    sync::{
        mpsc::{self},
        oneshot::{self},
    },
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{debug, error, info, instrument, trace, warn};
use worterbuch_client::Worterbuch;

#[instrument(skip(wb, clock))]
pub(crate) async fn start_receiver<C: MediaClock>(
    id: String,
    config: ReceiverConfig,
    wb: Option<Worterbuch>,
    clock: C,
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
        let receiver_future = Receiver::start(id, desc, config, wb, clock, api_rx, socket);
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
    config: ReceiverConfig,
    desc: RxDescriptor,
    wb: Option<Worterbuch>,
    clock: C,
    api_rx: mpsc::Receiver<ReceiverApiMessage>,
    last_timestamp: Option<u32>,
    last_sequence_number: Option<Seq>,
    timestamp_offset: Option<u64>,
    delay_buffer: AverageCalculationBuffer<i64>,
    rtp_packet_buffer: FloatingPointAudioBuffer,
    latest_received_frame: u64,
    measured_link_offset: AverageCalculationBuffer<u64>,
    socket: UdpSocket,
}

impl<C: MediaClock> Receiver<C> {
    async fn start(
        id: String,
        desc: RxDescriptor,
        config: ReceiverConfig,
        wb: Option<Worterbuch>,
        clock: C,
        api_rx: mpsc::Receiver<ReceiverApiMessage>,
        socket: UdpSocket,
    ) {
        let recv_id = id.clone();

        let delay_calculation_interval = config.delay_calculation_interval.unwrap_or(1);
        let delay_calculation_buffer_len =
            (delay_calculation_interval as f32 * 1_000.0 / desc.packet_time).round() as usize;
        let desc_rx = desc.clone();
        let packet_buffer_len = desc.audio_format.samples_in_buffer(config.buffer_time);
        let measured_link_offset_buffer = vec![0; 1000].into();

        let subsystem_name = id.clone();
        let subsystem = move |s: SubsystemHandle| async move {
            Receiver {
                id,
                subsys: s,
                desc,
                config,
                wb,
                clock,
                api_rx,
                last_sequence_number: None,
                last_timestamp: None,
                timestamp_offset: None,
                delay_buffer: AverageCalculationBuffer::new(
                    vec![0; delay_calculation_buffer_len].into(),
                ),
                rtp_packet_buffer: FloatingPointAudioBuffer::new(
                    vec![0f32; packet_buffer_len].into(),
                    desc_rx,
                ),
                latest_received_frame: 0,
                measured_link_offset: AverageCalculationBuffer::new(measured_link_offset_buffer),
                socket,
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

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => {
                    self.handle_api_message(api_msg)?;
                },
                Ok((len, addr)) = self.socket.recv_from(&mut receive_buffer) => {
                    let time = self.clock.current_media_time()?;
                    self.rtp_data_received(&receive_buffer[..len], addr, time)?;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

        info!("Receiver '{}' stopped.", self.id);

        Ok(())
    }

    fn handle_api_message(&mut self, api_msg: ReceiverApiMessage) -> ReceiverInternalResult<()> {
        match api_msg {
            ReceiverApiMessage::GetInfo { tx } => _ = tx.send(self.desc.clone()),
            ReceiverApiMessage::DataRequest { req, tx } => _ = tx.send(self.write_out_buf(req)),
            ReceiverApiMessage::Stop => self.subsys.request_shutdown(),
        }
        Ok(())
    }

    fn write_out_buf(&mut self, req: AudioDataRequest) -> DataState {
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
            debug!(
                "Not all requested frames have been received yet (requested frames: [{}; {}]; last received frame: {})!",
                req.playout_time, last_frame_in_request_buffer, self.latest_received_frame
            );
            return DataState::NotReady;
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

        self.measure_link_offset(req.playout_time);

        self.rtp_packet_buffer.read(buf, req.playout_time)
    }

    fn measure_link_offset(&mut self, requested_playout_time: u64) {
        let data_ready_since = self.latest_received_frame
            + self.desc.frames_in_link_offset() as u64
            - requested_playout_time;

        if let Some(measured_link_offset) = self.measured_link_offset.update(data_ready_since) {
            let link_offset_ms =
                measured_link_offset as f32 / self.desc.audio_format.sample_rate as f32 * 1_000.0;

            if link_offset_ms < self.desc.link_offset {
                info!(
                    "Measured minimum link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                );
            } else {
                warn!(
                    "Measured minimum  link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                );
            }
        }
    }

    fn rtp_data_received(
        &mut self,
        data: &[u8],
        addr: SocketAddr,
        media_time_at_reception: u64,
    ) -> ReceiverInternalResult<()> {
        // TODO how to detect lost packets?

        if addr.ip() != self.desc.origin_ip {
            warn!(
                "Received packet from wrong sender: {} (expected {})",
                addr, self.desc.origin_ip
            );
            // TODO report polluted multicast group
            return Ok(());
        }

        let rtp = match RtpReader::new(data) {
            Ok(it) => it,
            Err(e) => {
                warn!("received malformed rtp packet: {e:?}");
                // TODO report misbehaving sender
                return Ok(());
            }
        };

        let seq = rtp.sequence_number();
        let ts = rtp.timestamp();

        let mut ts_wrapped = false;
        let mut seq_wrapped = false;

        if let (Some(last_ts), Some(last_seq)) = (self.last_timestamp, self.last_sequence_number) {
            let expected_seq = last_seq.next();
            let expected_ts = last_ts.wrapping_add(self.desc.frames_per_packet() as u32);
            if seq != expected_seq {
                warn!(
                    "Inconsistent sequence number: {} (last was {})",
                    u16::from(seq),
                    u16::from(last_seq)
                );

                let diff = seq - expected_seq;
                let consistent_ts =
                    expected_ts as i64 + self.desc.frames_per_packet() as i64 * diff as i64;
                if consistent_ts == ts as i64 {
                    info!(
                        "Timestamp of out-of-order packet is consistent with sequence id, queuing it for playout"
                    );
                    // TODO report out-of-order packets
                } else {
                    warn!(
                        "Timestamp of out-of-order packet is not consistent with sequence id, discarding it"
                    );
                    // TODO report misbehaving sender/polluted multicast group
                    return Ok(());
                }
            }

            ts_wrapped = ts < last_ts;
            seq_wrapped = u16::from(seq) < u16::from(last_seq);
        }

        if seq_wrapped || self.timestamp_offset.is_none() {
            self.calibrate_timestamp_offset(ts)?;
        }

        if ts_wrapped {
            info!("RTP timestamp wrapped");
            self.calibrate_timestamp_offset(ts)?;
        }

        self.last_sequence_number = Some(seq);
        self.last_timestamp = Some(ts);

        let Some(ingress_timestamp) = self.unwrapped_timestamp(&rtp) else {
            return Ok(());
        };

        let playout_time = ingress_timestamp + self.desc.frames_in_link_offset() as u64;
        if media_time_at_reception > playout_time {
            warn!("Packet {} is late for playout.", u16::from(seq));
            // TODO report late packet
            return Ok(());
        }
        if ingress_timestamp > media_time_at_reception {
            let diff = ingress_timestamp - media_time_at_reception;
            let diff_usec =
                (diff as f64 * 1_000_000.0 / self.desc.audio_format.sample_rate as f64) as u64;
            // TODO this could well happen in local testing due to send jitter. On an actual network it's very unlikely
            warn!(
                "Packet {} was received {diff} frames / {diff_usec} µs before it was sent, sender and receiver clocks must be out of sync.",
                u16::from(seq)
            );
            return Ok(());
        }
        let delay = media_time_at_reception as i64 - ingress_timestamp as i64;
        if let Some(average) = self.delay_buffer.update(delay) {
            let micros = (average * 1_000_000) / self.desc.audio_format.sample_rate as i64;
            let packets = average as f32 / self.desc.frames_per_packet() as f32;
            info!("Network delay: {average} frames / {micros} µs / {packets:.1} packets");
        }
        let last_received_frame =
            ingress_timestamp + self.desc.frames_in_buffer(rtp.payload()) as u64 - 1;
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
    fn calibrate_timestamp_offset(&mut self, rtp_timestamp: u32) -> ReceiverInternalResult<()> {
        info!("Calibrating timestamp offset at RTP timestamp {rtp_timestamp}");

        let media_time = self.clock.current_media_time()?;

        let local_wrapped_timestamp = (media_time % u32::MAX as u64) as u32;

        if local_wrapped_timestamp < rtp_timestamp {
            warn!(
                "Either the clock has wrapped while packet was in flight or the local clock is not properly synced to PTP. Skipping calibration."
            );
            return Ok(());
        }

        let timestamp_wrap = u32::MAX as u64;
        let timestamp_wraps = media_time / timestamp_wrap;

        info!("Sender timestamp has wrapped {timestamp_wraps} times");

        // the offset is the time of the last wrap in media time,
        // i.e. offset + rtp.timestamp should give us an accurate
        // unwrapped media clock timestamp of an rtp packet
        let offset = timestamp_wraps * timestamp_wrap;

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

        Ok(())
    }
}
