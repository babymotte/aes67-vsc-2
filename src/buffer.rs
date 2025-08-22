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
    formats::{BufferFormat, SampleReader},
    receiver::{AudioDataRequest, DataState, config::RxDescriptor},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    slice::{from_raw_parts, from_raw_parts_mut},
};

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

    pub fn insert(&mut self, payload: &[u8], playout_time: u64) {
        let bpf = self.desc.bytes_per_frame();
        let frames_in_buffer = (self.buf.len() / bpf) as u64;
        let frame_index = playout_time % frames_in_buffer;
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

pub struct FloatingPointAudioBuffer {
    buf: Box<[f32]>,
    desc: RxDescriptor,
}

impl FloatingPointAudioBuffer {
    pub fn new(buf: Box<[f32]>, desc: RxDescriptor) -> Self {
        if buf.len() % desc.audio_format.frame_format.channels != 0 {
            panic!("buffer length must be a multiple of the number of channels")
        }
        Self { buf, desc }
    }

    pub fn insert(&mut self, payload: &[u8], playout_time: u64) {
        let sample_format = &self.desc.audio_format.frame_format.sample_format;
        let channels = self.desc.audio_format.frame_format.channels;
        let chunk_size = self.buf.len() / channels;
        let channel_partitions = self.buf.chunks_mut(chunk_size);

        let bytes_per_input_sample: usize = self
            .desc
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        for (channel_index, output_buffer) in channel_partitions.enumerate() {
            for (offset, sample) in payload
                .chunks(bytes_per_input_sample)
                .skip(channel_index)
                .step_by(channels)
                .enumerate()
            {
                let index = ((playout_time + offset as u64) % output_buffer.len() as u64) as usize;
                output_buffer[index] = sample_format.read_sample(sample);
            }
        }
    }

    pub fn read(&self, req: AudioDataRequest) -> DataState {
        let output_buffer = req.buffer.buffer_mut::<f32>();

        let channels = self.desc.audio_format.frame_format.channels;
        let chunk_size = self.buf.len() / channels;
        let channel_partitions = self.buf.chunks(chunk_size);

        let Some(rtp_buffer) = channel_partitions.skip(req.channel).next() else {
            return DataState::InvalidChannelNumber;
        };

        let start_index = (req.playout_time % rtp_buffer.len() as u64) as usize;
        let end_index: usize = start_index + output_buffer.len();
        if end_index <= rtp_buffer.len() {
            output_buffer.copy_from_slice(&rtp_buffer[start_index..end_index]);
        } else {
            let remainder = end_index - rtp_buffer.len();
            let pivot = output_buffer.len() - remainder;
            output_buffer[..pivot].copy_from_slice(&rtp_buffer[start_index..]);
            output_buffer[pivot..].copy_from_slice(&rtp_buffer[..remainder]);
        }
        DataState::Ready
    }
}
