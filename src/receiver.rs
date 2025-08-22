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

// TODO
// - receiver fills RTP packets into receiver buffer and keeps track of sequence IDs to make sure packets are ordered correctly
// - receiver keeps track of newest sequence number to decide if playout is ready for a specific media time
// - playout system sends receiver signal with a mapping of receiver channels to playout buffers
// - receiver waits for data to be ready for playout
// - receiver fills playout buffer (converting RTP audio data to playout system sample format) according to mapping
// - receiver sends signal back to playout system that bufferes are filled
// - receiver reports any late packets
pub mod api;
pub mod config;
pub mod webserver;

use crate::{
    buffer::{AudioBufferPointer, FloatingPointAudioBuffer},
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    receiver::{
        api::{ReceiverApi, ReceiverApiMessage},
        config::{ReceiverConfig, RxDescriptor},
        webserver::start_webserver,
    },
    socket::create_rx_socket,
    time::MediaClock,
    utils::{AverageCalculationBuffer, RequestResponseServerChannel, set_realtime_priority},
};
use rtp_rs::{RtpReader, Seq};
use std::{io::ErrorKind, net::SocketAddr, thread, u32};
use tokio::{
    runtime, select,
    sync::{
        mpsc::{self, error::TryRecvError},
        oneshot::{self, error::RecvError},
    },
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};
use worterbuch_client::Worterbuch;

#[derive(Debug, PartialEq)]
pub enum DataState {
    Ready,
    Wait,
    InvalidChannelNumber,
    Missed,
}

pub struct AudioDataRequest {
    pub buffer: AudioBufferPointer,
    pub channel: usize,
    pub playout_time: u64,
}

#[instrument(skip(subsys, wb, clock, data_requests))]
pub async fn start_receiver<C: MediaClock>(
    subsys: &SubsystemHandle,
    config: Config,
    use_tls: bool,
    wb: Option<Worterbuch>,
    clock: C,
    data_requests: RequestResponseServerChannel<AudioDataRequest, DataState>,
) -> Aes67Vsc2Result<ReceiverApi> {
    let id = config.app.instance.name.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    subsys.start(SubsystemBuilder::new(format!("receiver-{id}"), |s| {
        run(s, config, ready_tx, wb, clock, data_requests)
    }));
    let api_address = ready_rx.await?;
    info!("Receiver '{id}' started successfully.");
    Ok(ReceiverApi::with_socket_addr(api_address, use_tls))
}

async fn run<C: MediaClock>(
    subsys: SubsystemHandle,
    config: Config,
    ready_tx: oneshot::Sender<SocketAddr>,
    wb: Option<Worterbuch>,
    clock: C,
    data_requests: RequestResponseServerChannel<AudioDataRequest, DataState>,
) -> Aes67Vsc2Result<()> {
    let (api_tx, api_rx) = mpsc::channel(1024);
    ReceiverActor::start(&subsys, api_rx, config.clone(), wb, clock, data_requests).await?;
    start_webserver(&subsys, config, api_tx, ready_tx);

    Ok(())
}

struct ReceiverActor {
    subsys: SubsystemHandle,
    config: Config,
    api_rx: mpsc::Receiver<ReceiverApiMessage>,
    rx_thread: oneshot::Receiver<Aes67Vsc2Result<()>>,
    rx_cancellation_token: oneshot::Sender<()>,
    desc: RxDescriptor,
}

impl ReceiverActor {
    #[instrument(skip(subsys, api_rx, _wb, clock, data_requests))]
    async fn start<C: MediaClock>(
        subsys: &SubsystemHandle,
        api_rx: mpsc::Receiver<ReceiverApiMessage>,
        config: Config,
        _wb: Option<Worterbuch>,
        clock: C,
        data_requests: RequestResponseServerChannel<AudioDataRequest, DataState>,
    ) -> Aes67Vsc2Result<()> {
        let (rx_cancellation_token, rxct) = oneshot::channel();
        let (rx_stopped, rx_thread) = oneshot::channel();

        let cfg = config.clone();
        let rx_config = cfg.receiver_config.clone().expect("no receiver config");

        let descriptor = RxDescriptor::try_from(&config)?;
        let desc = descriptor.clone();
        let desc_rx = desc.clone();

        let delay_calculation_interval = config
            .playout_config
            .as_ref()
            .map(|c| c.clock_drift_compensation_interval)
            .unwrap_or(1);
        let delay_calculation_buffer_len =
            (delay_calculation_interval as f32 * 1_000.0 / descriptor.packet_time).round() as usize;
        let packet_buffer_len = desc.audio_format.frames_in_buffer(rx_config.buffer_time);
        let measured_link_offset_buffer = vec![0; 1000].into();

        thread::Builder::new()
            .name(format!("{}-receiver-thread", config.instance_name()))
            .spawn(
                move || match runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(runtime) => {
                        let receiver = async {
                            RxThread {
                                config: cfg,
                                rx_cancellation_token: rxct,
                                rx_config,
                                desc: desc_rx.clone(),
                                last_sequence_number: None,
                                last_timestamp: None,
                                timestamp_offset: None,
                                clock,
                                delay_buffer: AverageCalculationBuffer::new(
                                    vec![0; delay_calculation_buffer_len].into(),
                                ),
                                rtp_packet_buffer: FloatingPointAudioBuffer::new(
                                    vec![0f32; packet_buffer_len].into(),
                                    desc_rx,
                                ),
                                data_requests,
                                latest_received_playout_time: 0,
                                strict_checks_disabled: true,
                                measured_link_offset: AverageCalculationBuffer::new(
                                    measured_link_offset_buffer,
                                ),
                            }
                            .run()
                            .await
                        };
                        let res = runtime.block_on(receiver);
                        rx_stopped.send(res).ok();
                    }
                    Err(e) => {
                        rx_stopped.send(Err(e.into())).ok();
                    }
                },
            )?;

        subsys.start(SubsystemBuilder::new("actor", |s| async move {
            ReceiverActor {
                subsys: s,
                api_rx,
                config,
                rx_thread,
                rx_cancellation_token,
                desc,
            }
            .run()
            .await
        }));

        Ok(())
    }

    async fn run(mut self) -> Aes67Vsc2Result<()> {
        info!(
            "Receiver actor '{}' started.",
            self.config.app.instance.name
        );

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => if !self.process_api_message(api_msg).await {
                    break;
                },
                term = &mut self.rx_thread => {
                    self.rx_thread_terminated(term);
                    break;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

        self.stop().await;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn process_api_message(&mut self, api_msg: ReceiverApiMessage) -> bool {
        info!("Received API message: {api_msg:?}");

        match api_msg {
            ReceiverApiMessage::Stop => return false,
            ReceiverApiMessage::GetInfo(sender) => _ = sender.send(self.desc.clone()),
        }

        true
    }

    #[instrument(skip(self))]
    async fn stop(self) {
        self.rx_cancellation_token.send(()).ok();
        self.subsys.request_shutdown();

        info!(
            "Receiver actor '{}' stopped.",
            self.config.app.instance.name
        );
    }

    #[instrument(skip(self, term))]
    fn rx_thread_terminated(&self, term: Result<Aes67Vsc2Result<()>, RecvError>) {
        match term {
            Ok(Ok(_)) => info!("RX thread terminated normally."),
            Ok(Err(e)) => error!("RX thread terminated with error: {e:?}"),
            Err(_) => error!("The RX thread crashed."),
        }
    }
}

struct RxThread<C: MediaClock> {
    config: Config,
    rx_config: ReceiverConfig,
    rx_cancellation_token: oneshot::Receiver<()>,
    desc: RxDescriptor,
    last_timestamp: Option<u32>,
    last_sequence_number: Option<Seq>,
    timestamp_offset: Option<u64>,
    clock: C,
    delay_buffer: AverageCalculationBuffer<i64>,
    rtp_packet_buffer: FloatingPointAudioBuffer,
    data_requests: RequestResponseServerChannel<AudioDataRequest, DataState>,
    latest_received_playout_time: u64,
    strict_checks_disabled: bool,
    measured_link_offset: AverageCalculationBuffer<u64>,
}

impl<C: MediaClock> RxThread<C> {
    async fn run(mut self) -> Aes67Vsc2Result<()> {
        set_realtime_priority();

        let socket = create_rx_socket(&self.rx_config.session, self.config.interface_ip)?;

        let mut receive_buffer = [0; 65_535];

        loop {
            select! {
                Ok((len, addr)) = socket.recv_from(&mut receive_buffer) => {
                    let media_time_at_reception = self.clock.current_media_time()?;
                    self.rtp_data_received(&receive_buffer[..len], addr, media_time_at_reception)?;
                },
                Some(req) = self.data_requests.on_request() => {
                    let media_time_at_reception = self.clock.current_media_time()?;
                    if !self.data_requested(req, media_time_at_reception)? {
                        info!("Playout closed.");
                        break;
                    }
                },
                _ = &mut self.rx_cancellation_token => break,
                else => break,
            }
        }

        info!("RX thread stopped.");

        Ok(())
    }

    fn rtp_data_received(
        &mut self,
        data: &[u8],
        addr: SocketAddr,
        media_time_at_reception: u64,
    ) -> Aes67Vsc2Result<()> {
        if addr.ip() != self.desc.origin_ip {
            warn!(
                "Received packet from wrong sender: {} (expected {})",
                addr, self.desc.origin_ip
            );
            return Ok(());
        }

        let rtp = match RtpReader::new(data) {
            Ok(it) => it,
            Err(e) => {
                warn!("received malformed rtp packet: {e:?}");
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
                    "inconsistent sequence number: {} (last was {})",
                    u16::from(seq),
                    u16::from(last_seq)
                );

                let diff = seq - expected_seq;
                let consistent_ts =
                    expected_ts as i64 + self.desc.frames_per_packet() as i64 * diff as i64;
                if consistent_ts == ts as i64 {
                    info!(
                        "timestamp of out-of-order packet is consistent with sequence id, queuing it for playout"
                    );
                } else {
                    warn!(
                        "timestamp of out-of-order packet is not consistent with sequence id, discarding it"
                    );
                    return Ok(());
                }

                // TODO check AES67 spec for exact rules how to handle this kind of situation
                // TODO track late packets
            }

            ts_wrapped = ts < last_ts;
            seq_wrapped = u16::from(seq) < u16::from(last_seq);
        }

        // TODO track late packets

        if seq_wrapped || self.timestamp_offset.is_none() {
            self.calibrate_timestamp_offset(ts, ts_wrapped)?;
        }

        if ts_wrapped {
            info!("RTP timestamp wrapped");
            if let Some(previous_offset) = self.timestamp_offset {
                let new_offset = previous_offset + u32::MAX as u64;
                info!("Updating RTP timestamp offset from {previous_offset} to {new_offset}");
                self.timestamp_offset = Some(new_offset);
            } else {
                self.calibrate_timestamp_offset(ts, ts_wrapped)?;
            }
        }

        self.last_sequence_number = Some(seq);
        self.last_timestamp = Some(ts);

        if let Some(ingress_timestamp) = self.unwrapped_timestamp(&rtp) {
            let playout_time = ingress_timestamp + self.desc.frames_in_link_offset() as u64;
            if !self.strict_checks_disabled && media_time_at_reception > playout_time {
                warn!(
                    "Packet {} is late for playout, dropping it.",
                    u16::from(seq)
                );
                return Ok(());
            }
            if !self.strict_checks_disabled && ingress_timestamp > media_time_at_reception {
                let diff = ingress_timestamp - media_time_at_reception;
                let diff_usec =
                    (diff as f64 * 1_000_000.0 / self.desc.audio_format.sample_rate as f64) as u64;
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
            if playout_time > self.latest_received_playout_time {
                self.latest_received_playout_time = playout_time;
            }

            self.rtp_packet_buffer.insert(rtp.payload(), playout_time);
        }

        Ok(())
    }

    fn unwrapped_timestamp(&self, rtp: &RtpReader) -> Option<u64> {
        self.timestamp_offset
            .map(|ts_offset| ts_offset + rtp.timestamp() as u64 - self.desc.rtp_offset as u64)
    }

    fn data_requested(
        &mut self,
        req: AudioDataRequest,
        media_time_at_reception: u64,
    ) -> Aes67Vsc2Result<bool> {
        if !self.strict_checks_disabled && media_time_at_reception > req.playout_time {
            return Ok(self.data_requests.respond(DataState::Missed));
        }

        let data_ready = req.playout_time <= self.latest_received_playout_time;
        if !data_ready {
            return Ok(self.data_requests.respond(DataState::Wait));
        } else {
            if let Some(measured_link_offset) = self
                .measured_link_offset
                .update(self.latest_received_playout_time - req.playout_time)
            {
                let link_offset_ms = measured_link_offset as f32
                    / self.desc.audio_format.sample_rate as f32
                    * 1_000.0;

                if link_offset_ms < self.desc.link_offset {
                    info!(
                        "measured minimum link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                    );
                } else {
                    warn!(
                        "measured minimum  link offset: {measured_link_offset} frames / {link_offset_ms:.1} ms"
                    );
                }
            }
        }

        Ok(self.data_requests.respond(self.rtp_packet_buffer.read(req)))
    }

    #[instrument(skip(self))]
    fn calibrate_timestamp_offset(
        &mut self,
        rtp_timestamp: u32,
        timestamp_wrapped: bool,
    ) -> Aes67Vsc2Result<()> {
        info!("Calibrating timestamp offset at RTP timestamp {rtp_timestamp}");

        let media_time = self.clock.current_media_time()?;
        let timestamp_wrap = u32::MAX as u64;
        let timestamp_wraps = media_time / timestamp_wrap;
        let timestamp_modulo = media_time % timestamp_wrap;

        info!("Sender timestamp has wrapped {timestamp_wraps} times");

        let diff = rtp_timestamp as i128 - timestamp_modulo as i128;
        if diff.abs() >= timestamp_modulo as i128 {
            warn!("calibrating timestamp offset close to wrap, calibration may be inaccurate");
        }
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
