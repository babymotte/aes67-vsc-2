use crate::{buffer::AudioBufferPointer, formats::Frames};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct SendRequest {
    pub channel_buffers: Box<[AudioBufferPointer]>,
    pub ingress_time: u64,
}

#[derive(Debug)]
pub enum SenderApiMessage {
    Send(SendRequest),
    Stop,
}

#[derive(Debug, Clone)]
pub struct SenderApi {
    api_tx: mpsc::Sender<SenderApiMessage>,
}

impl SenderApi {
    pub fn new(api_tx: mpsc::Sender<SenderApiMessage>) -> Self {
        Self { api_tx }
    }

    pub fn send_blocking(&self, channel_buffers: Box<[AudioBufferPointer]>, ingress_time: Frames) {
        let req = SendRequest {
            channel_buffers,
            ingress_time,
        };
        self.api_tx.blocking_send(SenderApiMessage::Send(req)).ok();
    }
}
