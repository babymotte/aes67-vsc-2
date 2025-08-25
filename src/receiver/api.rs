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

use crate::{buffer::AudioBufferPointer, error::Aes67Vsc2Result, receiver::config::RxDescriptor};
use tokio::sync::{mpsc, oneshot};
use tracing::instrument;

#[derive(Debug, PartialEq)]
pub enum DataState {
    Ready,
    InvalidChannelNumber,
    Missed,
    SyncError,
}

#[derive(Debug)]
pub struct AudioDataRequest {
    pub buffer: AudioBufferPointer,
    pub playout_time: u64,
}

#[derive(Debug)]
pub struct ChannelAudioDataRequest {
    pub buffer: AudioBufferPointer,
    pub channel: usize,
    pub playout_time: u64,
}

#[derive(Debug)]
pub enum ReceiverApiMessage {
    GetInfo(oneshot::Sender<RxDescriptor>),
    DataRequest(AudioDataRequest, oneshot::Sender<DataState>),
    Stop,
}

#[derive(Debug, Clone)]
pub struct ReceiverApi {
    api_tx: mpsc::Sender<ReceiverApiMessage>,
}

impl ReceiverApi {
    pub fn new(api_tx: mpsc::Sender<ReceiverApiMessage>) -> Self {
        Self { api_tx }
    }

    #[instrument(skip(self))]
    pub async fn stop(&self) {
        self.api_tx.send(ReceiverApiMessage::Stop).await.ok();
    }

    #[instrument(skip(self), ret, err)]
    pub async fn info(&self) -> Aes67Vsc2Result<RxDescriptor> {
        let (tx, rx) = oneshot::channel();
        self.api_tx.send(ReceiverApiMessage::GetInfo(tx)).await.ok();
        Ok(rx.await?)
    }

    pub fn receive_all(
        &self,
        media_time: u64,
        buffer_ptr: usize,
        buffer_len: usize,
    ) -> Aes67Vsc2Result<DataState> {
        let (tx, rx) = oneshot::channel();
        let buffer = AudioBufferPointer::new(buffer_ptr, buffer_len);
        let req = AudioDataRequest {
            buffer,
            playout_time: media_time,
        };
        self.api_tx
            .blocking_send(ReceiverApiMessage::DataRequest(req, tx))
            .ok();
        Ok(rx.blocking_recv()?)
    }
}
