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
use tracing::{info, instrument, warn};

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
    pub fn insert(&mut self, rtp: RtpReader, timestamp_offset: u64) {
        // TODO make sure this does not break when rtp timestamp wraps around

        let payload = rtp.payload();

        let bpf = self.desc.bytes_per_frame();
        let frames_in_link_offset = self.desc.frames_in_link_offset() as u64;
        let egress_timestamp = 
        // wrapped ingress timestamp including random offset
        rtp.timestamp() as u64
        // subtract random timestamp offset from SDP to get the actual wrapped ingress timestamp
        - self.desc.rtp_offset as u64
        // add calibrated timestamp offset to transform it into the unwrapped ingress media clock time
        + timestamp_offset
        // add the number of frames in the link offset to convert it into the egress media clock time
        + frames_in_link_offset;
        let buffer = unsafe { self.shmem.as_slice_mut() };
        let frames_in_buffer = (buffer.len() / bpf) as u64;
        let frame_index = egress_timestamp % frames_in_buffer;
        let byte_index = frame_index as usize * bpf;
        let end_index = byte_index as usize + payload.len();

        if end_index <= buffer.len() {
            buffer[byte_index..end_index].copy_from_slice(payload);
        } else {
            let modulo = end_index - buffer.len();

            if modulo % bpf != 0 {
                panic!("wrap within frame");
            }

            let pivot = payload.len() - modulo;
            buffer[byte_index..].copy_from_slice(&payload[..pivot]);
            buffer[..modulo].copy_from_slice(&payload[pivot..]);
        }
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
