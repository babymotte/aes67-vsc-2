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
    receiver::config::RxDescriptor,
};
use rtp_rs::RtpReader;
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf};
use std::{
    fmt::Debug,
    slice::{from_raw_parts, from_raw_parts_mut},
};
use tokio::sync::oneshot;
use tracing::{info, instrument, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    pub format: BufferFormat,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBufferPointer {
    ptr: usize,
    len: usize,
}

impl AudioBufferPointer {
    pub fn new(ptr: usize, len: usize) -> Self {
        Self { ptr, len }
    }

    pub fn from_slice<T>(slice: &[T]) -> Self {
        let rf = slice.as_ref();
        Self::new(rf.as_ptr() as usize, rf.len())
    }

    pub fn buffer(&self) -> &[u8] {
        unsafe { from_raw_parts(self.ptr as *const u8, self.len) }
    }

    pub fn buffer_mut<T>(&self) -> &mut [T] {
        unsafe { from_raw_parts_mut(self.ptr as *mut T, self.len) }
    }

    pub fn audio_buffer<'a, 'b>(&'a self, desc: &'b RxDescriptor) -> AudioBuffer<'a, 'b> {
        let buf = self.buffer_mut();
        AudioBuffer { buf, desc }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

pub struct AudioBuffer<'a, 'b> {
    buf: &'a mut [u8],
    desc: &'b RxDescriptor,
}

impl<'a, 'b> AudioBuffer<'a, 'b> {
    pub fn new(buf: &'a mut [u8], desc: &'b RxDescriptor) -> Self {
        Self { buf, desc }
    }

    pub fn insert(&mut self, rtp: RtpReader, timestamp_offset: u64) {
        let payload = rtp.payload();

        let bpf = self.desc.bytes_per_frame();
        let ingress_timestamp =
        // wrapped ingress timestamp including random offset
        rtp.timestamp() as u64
        // subtract random timestamp offset from SDP to get the actual wrapped ingress timestamp
        - self.desc.rtp_offset as u64
        // add calibrated timestamp offset to transform it into the unwrapped ingress media clock time
        + timestamp_offset;
        let frames_in_buffer = (self.buf.len() / bpf) as u64;
        let frame_index = ingress_timestamp % frames_in_buffer;
        let byte_index = frame_index as usize * bpf;
        let end_index = byte_index + payload.len();

        if end_index <= self.buf.len() {
            self.buf[byte_index..end_index].copy_from_slice(payload);
        } else {
            let modulo = end_index - self.buf.len();

            if modulo % bpf != 0 {
                panic!("wrap within frame");
            }

            let pivot = payload.len() - modulo;
            self.buf[byte_index..].copy_from_slice(&payload[..pivot]);
            self.buf[..modulo].copy_from_slice(&payload[pivot..]);
        }
    }
}
