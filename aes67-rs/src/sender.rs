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

pub mod api;
pub mod config;

use crate::{
    buffer::AudioBufferPointer,
    error::{SenderInternalError, SenderInternalResult, WrappedRtpPacketBuildError},
    formats::{Frames, SampleWriter},
    monitoring::Monitoring,
    sender::{
        api::{SendRequest, SenderApi, SenderApiMessage},
        config::{SenderConfig, TxDescriptor},
    },
    socket::create_tx_socket,
    utils::U32_WRAP,
};
use rtp_rs::{RtpPacketBuilder, Seq};
use std::{net::SocketAddr, thread, time::Duration};
use tokio::{
    net::UdpSocket,
    runtime, select,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing::{error, info, instrument};

#[instrument(skip(monitoring))]
pub(crate) async fn start_sender(
    id: String,
    config: SenderConfig,
    monitoring: Monitoring,
) -> SenderInternalResult<SenderApi> {
    let sender_id = id.clone();
    let (result_tx, result_rx) = oneshot::channel();
    let (api_tx, api_rx) = mpsc::channel(1024);
    let desc = TxDescriptor::try_from(&config)?;
    let socket = create_tx_socket(config.target, config.interface_ip)?;
    thread::Builder::new().name(id.clone()).spawn(move || {
        // set_realtime_priority();

        let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
            Ok(it) => it,
            Err(e) => {
                result_tx.send(Err(SenderInternalError::from(e))).ok();
                return;
            }
        };
        let sender_future = Sender::start(id, desc, config, api_rx, socket, monitoring);
        result_tx.send(Ok(())).ok();
        runtime.block_on(sender_future);
    })?;

    result_rx.await??;
    info!("Sender '{sender_id}' started successfully.");
    Ok(SenderApi::new(api_tx))
}

struct Sender {
    id: String,
    subsys: SubsystemHandle,
    desc: TxDescriptor,
    api_rx: mpsc::Receiver<SenderApiMessage>,
    sequence_number: Seq,
    audio_buffer: [u8; 65536],
    rtp_buffer: [u8; 65536],
    socket: UdpSocket,
    target_address: SocketAddr,
    monitoring: Monitoring,
    ssrc: u32,
}

impl Sender {
    async fn start(
        id: String,
        desc: TxDescriptor,
        config: SenderConfig,
        api_rx: mpsc::Receiver<SenderApiMessage>,
        socket: UdpSocket,
        monitoring: Monitoring,
    ) {
        let send_id = id.clone();
        let subsystem_name = id.clone();
        let subsystem = move |s: SubsystemHandle| async move {
            Sender {
                id,
                subsys: s,
                desc,
                api_rx,
                sequence_number: Seq::from(rand::random::<u16>()),
                audio_buffer: [0u8; 65536],
                rtp_buffer: [0u8; 65536],
                socket,
                target_address: config.target,
                monitoring,
                ssrc: rand::random(),
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
            error!("Receiver '{send_id}' subsystem failed to shut down: {e}");
        }
    }

    async fn run(mut self) -> SenderInternalResult<()> {
        info!("Sender '{}' started.", self.id);

        self.report_sender_created().await;

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => {
                    self.handle_api_message(api_msg).await?;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

        self.report_sender_destroyed().await;

        info!("Sender '{}' stopped.", self.id);

        Ok(())
    }

    async fn handle_api_message(&mut self, api_msg: SenderApiMessage) -> SenderInternalResult<()> {
        match api_msg {
            SenderApiMessage::Send(req) => _ = self.send(req).await,
            SenderApiMessage::Stop => self.subsys.request_shutdown(),
        }
        Ok(())
    }

    async fn send(&mut self, req: SendRequest) -> SenderInternalResult<()> {
        if req.channel_buffers.len() != self.desc.audio_format.frame_format.channels {
            return Err(SenderInternalError::ChannelCountMismatch {
                configured: self.desc.audio_format.frame_format.channels,
                provided: req.channel_buffers.len(),
            });
        }

        let (payload_len, ptime_frames) = self.interlace(&req.channel_buffers);
        // TODO send response

        let payload_type = self.desc.payload_type;
        let seq = self.sequence_number;
        self.sequence_number = seq.next();
        let timestamp = (req.ingress_time % U32_WRAP) as u32;

        let payload = &self.audio_buffer[..payload_len];

        self.report_packet_time(ptime_frames).await;

        let len = RtpPacketBuilder::new()
            .payload_type(payload_type)
            .sequence(seq)
            .timestamp(timestamp)
            .payload(payload)
            .ssrc(self.ssrc)
            .build_into(&mut self.rtp_buffer)
            .map_err(WrappedRtpPacketBuildError)?;

        self.report_packet_size(len).await;

        if len > 1500 {
            return Err(SenderInternalError::MaxMTUExceeded(len));
        }

        self.socket
            .send_to(&self.rtp_buffer[..len], self.target_address)
            .await?;

        Ok(())
    }

    fn interlace<'a>(&mut self, channel_buffers: &[AudioBufferPointer]) -> (usize, Frames) {
        let channels = self.desc.audio_format.frame_format.channels;

        debug_assert_eq!(
            channels,
            channel_buffers.len(),
            "expected {} buffers, but got {}",
            channels,
            channel_buffers.len()
        );

        let target_bytes_per_sample = self
            .desc
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        let buffer_len = channel_buffers.first().expect("cannot be empty").len();

        let output_len = buffer_len * channels * target_bytes_per_sample;

        for (ch, buf) in channel_buffers.iter().enumerate() {
            debug_assert_eq!(
                buf.len(),
                buffer_len,
                "provided channel buffers have inconsistent lengths"
            );

            let buf = buf.buffer::<f32>();

            for (sample_index, source_sample) in buf.iter().enumerate() {
                let target_index = sample_index * target_bytes_per_sample * channels
                    + ch * target_bytes_per_sample;
                let dest_buf =
                    &mut self.audio_buffer[target_index..target_index + target_bytes_per_sample];
                self.desc
                    .audio_format
                    .frame_format
                    .sample_format
                    .write_sample(*source_sample, dest_buf);
            }
        }

        (output_len, buffer_len as Frames)
    }
}

mod monitoring {
    use crate::{
        formats::Frames,
        monitoring::{SenderState, TxStats},
    };

    use super::*;

    impl Sender {
        pub(crate) async fn report_sender_created(&self) {
            self.monitoring
                .sender_state(SenderState::SenderCreated {
                    name: self.id.clone(),
                    descriptor: self.desc.clone(),
                })
                .await;
        }

        pub(crate) async fn report_packet_time(&self, ptime_frames: Frames) {
            self.monitoring
                .sender_stats(TxStats::PacketTime(ptime_frames));
        }

        pub(crate) async fn report_packet_size(&self, packet_size: usize) {
            self.monitoring
                .sender_stats(TxStats::PacketSize(packet_size));
        }

        pub(crate) async fn report_sender_destroyed(&self) {
            self.monitoring
                .sender_state(SenderState::SenderDestroyed {
                    name: self.id.clone(),
                })
                .await;
        }
    }
}
