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

use crate::{buffer::ReceiverBufferConsumer, error::ReceiverInternalResult, formats::Frames};
use tokio::sync::mpsc;
use tracing::instrument;

#[derive(Debug, PartialEq)]
pub enum DataState {
    Ready,
    NotReady,
    ReceiverNotReady,
    Missed,
    InvalidChannelNumber,
    SyncError,
}

#[derive(Debug)]
pub enum ReceiverApiMessage {
    Stop,
}

#[derive(Clone)]
pub struct ReceiverApi {
    api_tx: mpsc::Sender<ReceiverApiMessage>,
    rx: ReceiverBufferConsumer,
}

impl ReceiverApi {
    pub fn new(api_tx: mpsc::Sender<ReceiverApiMessage>, rx: ReceiverBufferConsumer) -> Self {
        Self { api_tx, rx }
    }

    #[instrument(skip(self))]
    pub async fn stop(&self) {
        self.api_tx.send(ReceiverApiMessage::Stop).await.ok();
    }

    #[must_use]
    pub fn receive_blocking<'a>(
        &mut self,
        buffers: impl Iterator<Item = Option<&'a mut [f32]>>,
        ingress_time: Frames,
    ) -> ReceiverInternalResult<bool> {
        unsafe { Ok(self.rx.read(buffers, ingress_time)?) }
    }
}
