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
    formats::Frames,
    monitoring::{Report, TxStats},
    time::Time,
};
use rtp_rs::Seq;
use tokio::sync::mpsc;
use tracing::info;

pub struct SenderStats {
    id: String,
    tx: mpsc::Sender<Report>,
    ptime_frames: Frames,
    packet_size: usize,
    expected_next_ingress_time: u64,
}

impl SenderStats {
    pub fn new(id: String, tx: mpsc::Sender<Report>) -> Self {
        Self {
            id,
            tx,
            ptime_frames: 0,
            packet_size: 0,
            expected_next_ingress_time: 0,
        }
    }

    pub(crate) async fn process(&mut self, stats: TxStats) {
        match stats {
            TxStats::BufferUnderrun => {
                // TODO
            }
            TxStats::PacketSent {
                ptime_frames,
                packet_size,
                ingress_time,
                seq,
                pre_send,
                post_send,
            } => {
                self.process_packet_sent(
                    ptime_frames,
                    packet_size,
                    ingress_time,
                    seq,
                    pre_send,
                    post_send,
                )
                .await
            }
        }
    }

    async fn process_packet_sent(
        &mut self,
        ptime_frames: Frames,
        packet_size: usize,
        ingress_time: Frames,
        seq: Seq,
        pre_send: Time,
        post_send: Time,
    ) {
        let pre_post_send_diff = post_send - pre_send;
        // if pre_post_send_diff.media_duration > 0 {
        info!("Sending packet {:?} took {}", seq, pre_post_send_diff);
        // }

        if self.ptime_frames != ptime_frames {
            let old_ptime_frames = self.ptime_frames;
            self.ptime_frames = ptime_frames;
            info!(
                "Packet time changed from {} to {}",
                old_ptime_frames, ptime_frames
            );
            // TODO: report packet time change
        }

        if self.packet_size != packet_size {
            let old_packet_size = self.packet_size;
            self.packet_size = packet_size;
            info!(
                "Packet size changed from {} to {}",
                old_packet_size, packet_size
            );
            // TODO: report packet size change
        }

        if ingress_time != self.expected_next_ingress_time {
            info!(
                "Ingress time mismatch: expected {}, got {}",
                self.expected_next_ingress_time, ingress_time
            );
            // TODO report ingress time mismatch
        }
        self.expected_next_ingress_time = ingress_time + ptime_frames;
    }
}
