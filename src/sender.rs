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

use crate::{
    buffer::AudioBufferPointer,
    error::{Aes67Vsc2Result, WrappedRtpPacketBuildError},
    socket::create_tx_socket,
    utils::RequestResponseClientChannel,
};
use rtp_rs::{RtpPacketBuilder, Seq};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use tokio::select;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};

pub async fn start_sender(
    subsys: &SubsystemHandle,
    requests: RequestResponseClientChannel<AudioBufferPointer, (Seq, u64)>,
    local_ip: IpAddr,
    port: u16,
    target_address: SocketAddr,
) -> Aes67Vsc2Result<()> {
    let socket = create_tx_socket(local_ip, port)?;

    subsys.start(SubsystemBuilder::new("sender", move |s| async move {
        SenderActor {
            requests,
            subsys: s,
            socket,
            target_address,
            rtp_buffer: [0u8; 1500],
        }
        .run()
        .await
    }));

    Ok(())
}

struct SenderActor {
    subsys: SubsystemHandle,
    requests: RequestResponseClientChannel<AudioBufferPointer, (Seq, u64)>,
    socket: UdpSocket,
    target_address: SocketAddr,
    rtp_buffer: [u8; 1500],
}

impl SenderActor {
    async fn run(mut self) -> Aes67Vsc2Result<()> {
        // TODO adjust buffer size based on audio format
        let audio_buffer = vec![0u8; 288];

        loop {
            let audio_buffer_ptr = AudioBufferPointer::from_slice(&audio_buffer[..]);
            select! {
                Some((seq, ingress_time)) = self.requests.request(audio_buffer_ptr) => self.data_received(seq, ingress_time, &audio_buffer)?,
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

        Ok(())
    }

    fn data_received(&mut self, seq: Seq, ingress_time: u64, buf: &[u8]) -> Aes67Vsc2Result<()> {
        let len = RtpPacketBuilder::new()
            .payload_type(97)
            .sequence(seq)
            .timestamp((ingress_time % u32::MAX as u64) as u32)
            .payload(buf)
            .build_into(&mut self.rtp_buffer)
            .map_err(WrappedRtpPacketBuildError)?;

        self.socket
            .send_to(&self.rtp_buffer[..len], &self.target_address)?;

        Ok(())
    }
}
