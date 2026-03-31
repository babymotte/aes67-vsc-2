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

use crate::{buffer::sender::SenderBufferProducer, error::SenderInternalResult, formats::Frames};
use tokio::sync::mpsc;
use tracing::{error, instrument};

#[derive(Debug)]
pub enum SenderApiMessage {
    Stop,
}

#[derive(Debug, Clone)]
pub struct SenderApi {
    api_tx: mpsc::Sender<SenderApiMessage>,
    tx: SenderBufferProducer,
    ingress_time: Frames,
    buffer_len_frames: usize,
    buffer_size_changed: bool,
}

impl SenderApi {
    pub fn new(api_tx: mpsc::Sender<SenderApiMessage>, tx: SenderBufferProducer) -> Self {
        Self {
            api_tx,
            tx,
            ingress_time: 0,
            buffer_len_frames: 0,
            buffer_size_changed: false,
        }
    }

    #[instrument(skip(self))]
    pub fn stop(&self) {
        if let Err(e) = self.api_tx.try_send(SenderApiMessage::Stop) {
            error!("Failed to send stop message to sender API: {e}");
        }
    }

    pub fn start_write(&mut self, ingress_time: u64, frames: usize) {
        self.ingress_time = ingress_time;
        self.buffer_size_changed = self.buffer_len_frames != frames;
        self.buffer_len_frames = frames;
    }

    pub fn write_channel(&mut self, ch: usize, channel_buffer: &[f32]) {
        debug_assert_eq!(
            self.buffer_len_frames,
            channel_buffer.len(),
            "expected buffer of length {}, but got buffer of length {}",
            self.buffer_len_frames,
            channel_buffer.len()
        );

        self.tx.write_channel(ch, channel_buffer);
    }

    pub fn end_write(&mut self) -> SenderInternalResult<()> {
        self.tx.send_packets(
            self.ingress_time,
            self.buffer_len_frames,
            self.buffer_size_changed,
        )
    }
}
