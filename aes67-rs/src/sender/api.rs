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
    buffer::{AudioBufferPointer, SenderBufferProducer},
    formats::Frames,
};
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum SenderApiMessage {
    Stop,
}

#[derive(Debug, Clone)]
pub struct SenderApi {
    api_tx: mpsc::Sender<SenderApiMessage>,
    tx: SenderBufferProducer,
}

impl SenderApi {
    pub fn new(api_tx: mpsc::Sender<SenderApiMessage>, tx: SenderBufferProducer) -> Self {
        Self { api_tx, tx }
    }

    pub fn send_blocking(&mut self, channel_buffers: &[AudioBufferPointer], ingress_time: Frames) {
        self.tx.write(channel_buffers, ingress_time);
    }
}
