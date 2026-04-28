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
    new_frames: usize,
    compensation: i64,
}

impl SenderApi {
    pub fn new(api_tx: mpsc::Sender<SenderApiMessage>, tx: SenderBufferProducer) -> Self {
        Self {
            api_tx,
            tx,
            ingress_time: 0,
            buffer_len_frames: 0,
            new_frames: 0,
            compensation: 0,
        }
    }

    #[instrument(skip(self))]
    pub fn stop(&self) {
        if let Err(e) = self.api_tx.try_send(SenderApiMessage::Stop) {
            error!("Failed to send stop message to sender API: {e}");
        }
    }

    pub fn start_write(&mut self, ingress_time: u64, buffer_len: usize, compensation: i64) {
        self.ingress_time = (ingress_time as i64 - compensation) as Frames;
        self.buffer_len_frames = buffer_len;
        self.new_frames = (buffer_len as i64 + compensation) as usize;
        self.compensation = compensation;
    }

    pub fn write_channel(&mut self, ch: usize, channel_buffer: &[f32]) {
        if self.compensation > 0 {
            let offset_frames = self.compensation as usize;
            // insert first sample as many times as is required for the compensation, then write the actual buffer
            for i in 0..offset_frames {
                self.tx.write_channel(ch, i, &channel_buffer[0..1]);
            }
            self.tx.write_channel(ch, offset_frames, channel_buffer);
        } else if self.compensation < 0 {
            let buf = &channel_buffer[(-self.compensation) as usize..];
            self.tx.write_channel(ch, 0, buf);
        } else {
            self.tx.write_channel(ch, 0, channel_buffer);
        }
    }

    pub fn end_write(&mut self) -> SenderInternalResult<()> {
        self.tx.send_packets(self.ingress_time, self.new_frames)
    }
}
