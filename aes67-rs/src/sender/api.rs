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
        unsafe {
            self.tx.write(channel_buffers, ingress_time);
        }
    }
}
