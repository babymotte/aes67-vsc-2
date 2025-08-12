use crate::{
    config::Config,
    error::Aes67Vsc2Result,
    formats::{AudioFormat, BufferFormat},
    receiver::config::RxDescriptor,
};
use rtp_rs::RtpReader;
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf};
use std::fmt::Debug;
use tokio::sync::oneshot;
use tracing::{info, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    pub format: BufferFormat,
    pub address: String,
}

pub struct AudioBuffer {
    shmem: Shmem,
    desc: RxDescriptor,
}

impl AudioBuffer {
    pub fn insert(&mut self, rtp: RtpReader) {
        let payload = rtp.payload();

        let egress_timestamp = rtp.timestamp() + self.desc.link_offset;

        let buffer = unsafe { self.shmem.as_slice_mut() };
        // TODO remove check once this has proven consistent
        if buffer.len() % payload.len() != 0 {
            panic!("payload and buffer sizes aren't aligned");
        }

        let index = (egress_timestamp as usize * payload.len()) % buffer.len();

        buffer[index..index + payload.len()].copy_from_slice(payload);
    }
}

#[instrument]
pub fn create_shared_memory_buffer(
    config: &Config,
    path_tx: oneshot::Sender<String>,
    descriptor: RxDescriptor,
) -> Aes67Vsc2Result<AudioBuffer> {
    let rx_config = config.receiver_config.as_ref().expect("no receiver config");
    let link_offset = rx_config.link_offset;
    let buffer_overhead = rx_config.buffer_overhead;
    let audio_format = AudioFormat::from(&descriptor);

    let buffer_format =
        BufferFormat::for_rtp_playout_buffer(link_offset, buffer_overhead, audio_format);

    let (buffer, path) = create_audio_buffer(buffer_format, descriptor)?;
    info!("Created shared memory buffer at {path}");
    path_tx.send(path).ok();

    Ok(buffer)
}

#[instrument]
pub fn create_audio_buffer(
    format: BufferFormat,
    desc: RxDescriptor,
) -> Aes67Vsc2Result<(AudioBuffer, String)> {
    let len = format.buffer_len;
    info!("Creating shared memory buffer with length {} …", len);
    let shmem = ShmemConf::new().size(len).create()?;

    let id = shmem.get_os_id().to_owned();

    info!("Created shared memory {id}");

    Ok((AudioBuffer { shmem, desc }, id))
}

#[instrument]
pub fn open_audio_buffer(
    address: &str,
    audio_format: AudioFormat,
    desc: RxDescriptor,
) -> Aes67Vsc2Result<AudioBuffer> {
    info!("Opening shared memory {} …", address);
    let shmem = ShmemConf::new().os_id(address).open()?;
    let buffer_len = unsafe { shmem.as_slice().len() };
    let format = BufferFormat {
        buffer_len,
        audio_format,
    };

    info!(
        "Opened shared memory buffer with length {}",
        format.buffer_len
    );

    Ok(AudioBuffer { shmem, desc })
}
