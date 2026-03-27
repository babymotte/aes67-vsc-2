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
    buffer::{AudioBufferPointer, SenderBufferConsumer, sender_buffer_channel},
    error::{SenderInternalError, SenderInternalResult, WrappedRtpPacketBuildError},
    formats::Frames,
    monitoring::Monitoring,
    sender::{
        api::{SenderApi, SenderApiMessage},
        config::SenderConfig,
    },
    socket::create_tx_socket,
    utils::{U32_WRAP, set_realtime_priority},
};
use pnet::datalink::NetworkInterface;
use rtp_rs::{RtpPacketBuilder, Seq};
use std::{
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tosub::SubsystemHandle;
use tracing::{info, instrument, warn};
#[cfg(feature = "tokio-metrics")]
use worterbuch_client::Worterbuch;

#[instrument(skip(monitoring, subsys, wb))]
pub(crate) async fn start_sender(
    app_id: String,
    id: String,
    label: String,
    iface: NetworkInterface,
    config: SenderConfig,
    monitoring: Monitoring,
    subsys: &SubsystemHandle,
    #[cfg(feature = "tokio-metrics")] wb: Worterbuch,
) -> SenderInternalResult<SenderApi> {
    let sender_id = id.clone();
    let (api_tx, api_rx) = mpsc::channel(1024);
    let (tx, rx) = sender_buffer_channel(config.clone());
    let target = config.target;
    let socket = create_tx_socket(target, iface)?;

    let subsystem_name = id.clone();
    let subsystem = async move |s: SubsystemHandle| {
        let sender = Sender {
            id,
            label,
            subsys: s.clone(),
            config,
            api_rx,
            sequence_number: Seq::from(rand::random::<u16>()),
            rx,
            rtp_buffer: [0u8; 65536],
            socket,
            target_address: target,
            monitoring,
            ssrc: rand::random(),
        };

        let (tx, rx) = oneshot::channel();
        let exit = Arc::new(AtomicBool::new(false));
        let exit_clone = exit.clone();
        let sid = sender_id.clone();

        thread::spawn(move || {
            set_realtime_priority();
            let res = sender.run(exit_clone);
            tx.send(res).ok();
            info!("Sender thread for '{}' stopped.", sid);
        });

        select! {
            _ = s.shutdown_requested() => {
                info!("Shutdown of sender '{}' requested via subsystem shutdown.", sender_id);
                exit.store(true, Ordering::SeqCst);
                Ok(())
            }
            res = rx => {
                s.request_local_shutdown();
                res?
            }
        }
    };

    subsys.spawn(subsystem_name.clone(), subsystem);

    info!("Sender '{subsystem_name}' started successfully.");

    Ok(SenderApi::new(api_tx, tx))
}

struct Sender {
    id: String,
    label: String,
    subsys: SubsystemHandle,
    config: SenderConfig,
    api_rx: mpsc::Receiver<SenderApiMessage>,
    sequence_number: Seq,
    rx: SenderBufferConsumer,
    rtp_buffer: [u8; 65536],
    socket: UdpSocket,
    target_address: SocketAddr,
    monitoring: Monitoring,
    ssrc: u32,
}

impl Sender {
    fn run(mut self, exit: Arc<AtomicBool>) -> SenderInternalResult<()> {
        info!("Sender '{}' started.", self.id);

        self.report_sender_created(AudioBufferPointer::from_slice(&self.rx.buffer));

        while !exit.load(Ordering::SeqCst) {
            // read packet data

            let recv = self.rx.read();
            if let Ok(recv) = recv {
                self.send(recv.0, recv.1, recv.2)?;
            } else {
                break;
            }

            match self.api_rx.try_recv() {
                Ok(api_msg) => {
                    self.handle_api_message(api_msg)?;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // no API message, continue
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    warn!("API channel closed, shutting down sender '{}'.", self.id);
                    break;
                }
            }
        }

        self.report_sender_destroyed();

        info!("Sender '{}' stopped.", self.id);

        Ok(())
    }

    fn handle_api_message(&mut self, api_msg: SenderApiMessage) -> SenderInternalResult<()> {
        match api_msg {
            SenderApiMessage::Stop => {
                self.subsys.request_local_shutdown();
            }
        }
        Ok(())
    }

    fn send(
        &mut self,
        payload_len: usize,
        ptime_frames: Frames,
        ingress_time: Frames,
    ) -> SenderInternalResult<()> {
        let payload_type = self.config.payload_type;
        let seq = self.sequence_number;
        self.sequence_number = seq.next();
        let timestamp = (ingress_time % U32_WRAP) as u32;

        self.report_packet_time(ptime_frames);

        let payload = &self.rx.buffer[..payload_len];

        let len = RtpPacketBuilder::new()
            .payload_type(payload_type)
            .sequence(seq)
            .timestamp(timestamp)
            .payload(payload)
            .ssrc(self.ssrc)
            .build_into(&mut self.rtp_buffer)
            .map_err(WrappedRtpPacketBuildError)?;

        self.report_packet_size(len);

        if len > 1500 {
            return Err(SenderInternalError::MaxMTUExceeded(len));
        }

        self.socket
            .send_to(&self.rtp_buffer[..len], self.target_address)?;

        Ok(())
    }
}

mod monitoring {
    use crate::{
        buffer::AudioBufferPointer,
        formats::Frames,
        monitoring::{SenderState, TxStats},
    };

    use super::*;

    impl Sender {
        pub(crate) fn report_sender_created(&self, buffer: AudioBufferPointer) {
            self.monitoring.sender_state(SenderState::Created {
                id: self.id.clone(),
                label: self.label.clone(),
                config: self.config.clone(),
                address: buffer,
            });
        }

        pub(crate) fn report_packet_time(&self, ptime_frames: Frames) {
            self.monitoring
                .sender_stats(TxStats::PacketTime(ptime_frames));
        }

        pub(crate) fn report_packet_size(&self, packet_size: usize) {
            self.monitoring
                .sender_stats(TxStats::PacketSize(packet_size));
        }

        pub(crate) fn report_sender_destroyed(&self) {
            self.monitoring.sender_state(SenderState::Destroyed {
                id: self.id.clone(),
            });
        }
    }
}
