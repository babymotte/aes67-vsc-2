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
use tokio::sync::{mpsc, oneshot};
use tosub::Subsystem;
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
    Stop(oneshot::Sender<()>),
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
        let (tx, rx) = oneshot::channel();
        self.api_tx.send(ReceiverApiMessage::Stop(tx)).await.ok();
        rx.await.ok();
    }

    pub async fn receive<'a>(
        &mut self,
        buffers: impl Iterator<Item = Option<&'a mut [f32]>>,
        ingress_time: Frames,
        subsys: &Subsystem,
    ) -> ReceiverInternalResult<bool> {
        self.rx.read(buffers, ingress_time, subsys).await
    }
}
