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
    config::Config,
    error::Aes67Vsc2Result,
    formats::{AudioFormat, BufferFormat},
    receiver::{api::ReceiverInfo, config::RxDescriptor},
};
use rtp_rs::RtpReader;
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf};
use std::{fmt::Debug, slice::from_raw_parts};
use tokio::sync::oneshot;
use tracing::{info, instrument, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    pub format: BufferFormat,
    pub address: String,
}

pub struct AudioBufferRef {
    shared_memory_ptr: usize,
    buffer_len: usize,
}

impl AudioBufferRef {
    pub fn buffer(&self) -> &[u8] {
        unsafe { from_raw_parts(self.shared_memory_ptr as *const u8, self.buffer_len) }
    }
}

pub struct AudioBuffer {
    shmem: Shmem,
    desc: RxDescriptor,
}

impl AudioBuffer {
    pub fn get_ref(&self) -> AudioBufferRef {
        let buffer_len = unsafe { self.shmem.as_slice().len() };
        let shared_memory_ptr = self.shmem.as_ptr() as usize;
        AudioBufferRef {
            shared_memory_ptr,
            buffer_len,
        }
    }

    pub fn insert(&mut self, rtp: RtpReader, timestamp_offset: u64) {
        // TODO make sure this does not break when rtp timestamp wraps around

        let payload = rtp.payload();

        let bpf = self.desc.bytes_per_frame();
        let ingress_timestamp =
        // wrapped ingress timestamp including random offset
        rtp.timestamp() as u64
        // subtract random timestamp offset from SDP to get the actual wrapped ingress timestamp
        - self.desc.rtp_offset as u64
        // add calibrated timestamp offset to transform it into the unwrapped ingress media clock time
        + timestamp_offset;
        let buffer = unsafe { self.shmem.as_slice_mut() };
        let frames_in_buffer = (buffer.len() / bpf) as u64;
        let frame_index = ingress_timestamp % frames_in_buffer;
        let byte_index = frame_index as usize * bpf;
        let end_index = byte_index + payload.len();

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
    let buffer_time = rx_config.buffer_time;
    let audio_format = AudioFormat::from(&descriptor);

    let buffer_format = BufferFormat::for_rtp_playout_buffer(buffer_time, audio_format);

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
pub fn open_audio_buffer(receiver_info: ReceiverInfo) -> Aes67Vsc2Result<AudioBuffer> {
    info!("Opening shared memory {} …", receiver_info.shmem_address);
    let shmem = ShmemConf::new().os_id(receiver_info.shmem_address).open()?;
    let buffer_len = unsafe { shmem.as_slice().len() };
    let format = BufferFormat {
        buffer_len,
        audio_format: receiver_info.descriptor.audio_format,
    };

    info!(
        "Opened shared memory buffer with length {}",
        format.buffer_len
    );

    Ok(AudioBuffer {
        shmem,
        desc: receiver_info.descriptor,
    })
}
