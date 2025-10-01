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
    error::{SenderInternalError, SenderInternalResult},
    formats::{BufferFormat, Frames, SampleReader, SampleWriter},
    receiver::{api::DataState, config::RxDescriptor},
    sender::config::TxDescriptor,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    slice::{from_raw_parts, from_raw_parts_mut},
    time::Duration,
};
use tokio::sync::mpsc;
use tracing::error;

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
        Self::new(slice.as_ptr() as usize, slice.len())
    }

    pub fn buffer<T>(&self) -> &[T] {
        unsafe { from_raw_parts(self.ptr as *const T, self.len) }
    }

    /// Gets the actual audio buffer from the pointer as a mutable slice.
    /// # Safety
    /// The audio buffer this pointer refers to belongs to a different thread or process. It is only safe
    /// to read from or write to this buffer if some kind of synchronization mechanism is in place. In the
    /// receiver this is achieved by sending the pointer to the receiver through a channel along with a
    /// tokio::sync::oneshot::Sender<DataState> that signals the owner of the buffer that write operation
    /// is complete and it is now safe to read from the buffer.
    pub unsafe fn buffer_mut<T>(&mut self) -> &mut [T] {
        unsafe { from_raw_parts_mut(self.ptr as *mut T, self.len) }
    }

    /// Gets the actual audio buffer from the pointer as an AudioBuffer.
    /// # Safety
    /// The audio buffer this pointer refers to belongs to a different thread or process. It is only safe
    /// to read from or write to this buffer if some kind of synchronization mechanism is in place. In the
    /// receiver this is achieved by sending the pointer to the receiver through a channel along with a
    /// tokio::sync::oneshot::Sender<DataState> that signals the owner of the buffer that write operation
    /// is complete and it is now safe to read from the buffer.
    pub unsafe fn audio_buffer<'a, 'b>(
        &'a mut self,
        desc: &'b RxDescriptor,
    ) -> AudioBuffer<'a, 'b> {
        unsafe {
            let buf = self.buffer_mut();
            AudioBuffer { buf, desc }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
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

    pub fn frames(&self) -> usize {
        self.buf.len() / self.desc.audio_format.frame_format.channels
    }

    pub fn insert(&mut self, payload: &[u8], playout_time: u64) {
        let sample_format = &self.desc.audio_format.frame_format.sample_format;
        let buffer_len = self.buf.len();
        let channels = self.desc.audio_format.frame_format.channels;

        let bytes_per_input_sample: usize = self
            .desc
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        for (offset, sample) in payload.chunks(bytes_per_input_sample).enumerate() {
            let index =
                ((playout_time * channels as u64 + offset as u64) % buffer_len as u64) as usize;
            self.buf[index] = sample_format.read_sample(sample);
        }
    }

    pub fn insert_deinterlaced(&mut self, payload: &[u8], playout_time: u64) {
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

    pub fn read(&self, output_buffer: &mut [f32], playout_time: u64) -> DataState {
        let buffer_len = self.buf.len();
        let channels = self.desc.audio_format.frame_format.channels;

        let start_index = ((playout_time * channels as u64) % buffer_len as u64) as usize;
        let end_index: usize = start_index + output_buffer.len();
        if end_index <= buffer_len {
            output_buffer.copy_from_slice(&self.buf[start_index..end_index]);
        } else {
            let remainder = end_index % buffer_len;
            let pivot = output_buffer.len() - remainder;
            output_buffer[..pivot].copy_from_slice(&self.buf[start_index..]);
            output_buffer[pivot..].copy_from_slice(&self.buf[..remainder]);
        }
        DataState::Ready
    }

    pub fn read_deinterlaced(
        &self,
        output_buffer: &mut [f32],
        playout_time: u64,
        channel: usize,
    ) -> DataState {
        let channels = self.desc.audio_format.frame_format.channels;
        let chunk_size = self.buf.len() / channels;
        let mut channel_partitions = self.buf.chunks(chunk_size);

        let Some(rtp_buffer) = channel_partitions.nth(channel) else {
            return DataState::InvalidChannelNumber;
        };

        let start_index = (playout_time % rtp_buffer.len() as u64) as usize;
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

pub fn receiver_buffer_channel(
    descriptor: RxDescriptor,
) -> (ReceiverBufferProducer, ReceiverBufferConsumer) {
    let buffer_len = descriptor.duration_to_frames(Duration::from_secs(1)) as usize;
    let (tx, rx) = mpsc::channel(buffer_len);
    let buffer = vec![0f32; buffer_len].into_boxed_slice();
    let buffer_pointer = AudioBufferPointer::from_slice(&buffer);
    (
        ReceiverBufferProducer {
            buffer,
            descriptor: descriptor.clone(),
            tx,
        },
        ReceiverBufferConsumer {
            buffer_pointer,
            descriptor,
            rx,
            last_received_frame: 0,
        },
    )
}

pub struct ReceiverBufferProducer {
    buffer: Box<[f32]>,
    descriptor: RxDescriptor,
    tx: mpsc::Sender<Frames>,
}

pub struct ReceiverBufferConsumer {
    buffer_pointer: AudioBufferPointer,
    descriptor: RxDescriptor,
    rx: mpsc::Receiver<Frames>,
    last_received_frame: Frames,
}

impl ReceiverBufferProducer {
    /// Deinterlace and write audio data into the shared buffer. The buffer is partitioned into equally sized strips,
    /// one for each channel, so that audio data can be retrieved individually per channel.
    pub async fn write(&mut self, payload: &[u8], ingress_time: Frames) {
        let buf = &mut self.buffer[..];
        let sample_format = &self.descriptor.audio_format.frame_format.sample_format;
        let channels = self.descriptor.audio_format.frame_format.channels;
        let chunk_size = buf.len() / channels;
        let channel_partitions = buf.chunks_mut(chunk_size);

        let bytes_per_input_sample: usize = self
            .descriptor
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
                let index = ((ingress_time + offset as u64) % output_buffer.len() as u64) as usize;
                output_buffer[index] = sample_format.read_sample(sample);
            }
        }

        self.tx
            .send(ingress_time + self.descriptor.frames_in_buffer(payload.len()) - 1)
            .await
            .ok();
    }
}

// TODO return proper errors
impl ReceiverBufferConsumer {
    /// Write data from the shared buffer. Before reading, this function will block until the requested data is available.
    pub unsafe fn read(&mut self, buffers: &mut [&mut [f32]], egress_time: Frames) {
        let Some(buf_len) = buffers.first().map(|it| it.len()) else {
            return;
        };

        debug_assert!(
            buffers.iter().all(|it| it.len() == buf_len),
            "inconsistent buffer lengths"
        );

        let last_requested_frame = egress_time + buf_len as Frames - 1;

        if last_requested_frame > self.last_received_frame {
            loop {
                let Some(frame) = self.rx.blocking_recv() else {
                    return;
                };
                self.last_received_frame = frame;
                if self.last_received_frame >= last_requested_frame {
                    break;
                }
            }
        }

        let buf = self.buffer_pointer.buffer::<f32>();

        for (channel, output_buffer) in buffers.iter_mut().enumerate() {
            let channels = self.descriptor.audio_format.frame_format.channels;
            let chunk_size = buf.len() / channels;
            let mut channel_partitions = buf.chunks(chunk_size);

            let Some(rtp_buffer) = channel_partitions.nth(channel) else {
                return;
            };

            let start_index = (egress_time % rtp_buffer.len() as u64) as usize;
            let end_index: usize = start_index + output_buffer.len();
            if end_index <= rtp_buffer.len() {
                output_buffer.copy_from_slice(&rtp_buffer[start_index..end_index]);
            } else {
                let remainder = end_index - rtp_buffer.len();
                let pivot = output_buffer.len() - remainder;
                output_buffer[..pivot].copy_from_slice(&rtp_buffer[start_index..]);
                output_buffer[pivot..].copy_from_slice(&rtp_buffer[..remainder]);
            }
        }
    }
}

pub fn sender_buffer_channel(
    descriptor: TxDescriptor,
) -> (SenderBufferProducer, SenderBufferConsumer) {
    let (tx, rx) = mpsc::channel(65536);
    let buffer = vec![0u8; 65536].into_boxed_slice();
    let buffer_pointer = AudioBufferPointer::from_slice(&buffer);
    (
        SenderBufferProducer {
            buffer_pointer,
            descriptor: descriptor.clone(),
            tx,
        },
        SenderBufferConsumer {
            buffer,
            descriptor,
            rx,
        },
    )
}

#[derive(Debug, Clone)]
pub struct SenderBufferProducer {
    buffer_pointer: AudioBufferPointer,
    descriptor: TxDescriptor,
    tx: mpsc::Sender<(usize, Frames, Frames)>,
}

pub struct SenderBufferConsumer {
    pub buffer: Box<[u8]>,
    descriptor: TxDescriptor,
    rx: mpsc::Receiver<(usize, Frames, Frames)>,
}

// TODO return proper errors
// TODO generalize start and end indices/packet time
impl SenderBufferProducer {
    pub unsafe fn write(&mut self, channel_buffers: &[AudioBufferPointer], ingress_time: Frames) {
        let channels = self.descriptor.audio_format.frame_format.channels;

        debug_assert_eq!(
            channels,
            channel_buffers.len(),
            "expected {} buffers, but got {}",
            channels,
            channel_buffers.len()
        );

        // TODO cache that?
        let target_bytes_per_sample = self
            .descriptor
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        let Some(buffer_len) = channel_buffers.first().map(|it| it.len()) else {
            error!("no buffers provided");
            return;
        };

        let output_len = buffer_len * channels * target_bytes_per_sample;

        unsafe {
            let audio_buffer = self.buffer_pointer.buffer_mut::<u8>();

            for (ch, buf) in channel_buffers.iter().enumerate() {
                debug_assert_eq!(
                    buf.len(),
                    buffer_len,
                    "provided channel buffers have inconsistent lengths"
                );

                let buf = buf.buffer::<f32>();

                for (sample_index, source_sample) in buf.iter().enumerate() {
                    let target_index = sample_index * target_bytes_per_sample * channels
                        + ch * target_bytes_per_sample;
                    let dest_buf =
                        &mut audio_buffer[target_index..target_index + target_bytes_per_sample];
                    self.descriptor
                        .audio_format
                        .frame_format
                        .sample_format
                        .write_sample(*source_sample, dest_buf);
                }
            }
        }

        self.tx
            .blocking_send((output_len, buffer_len as Frames, ingress_time))
            .ok();
    }
}

impl SenderBufferConsumer {
    pub async fn read(&mut self) -> SenderInternalResult<(usize, Frames, Frames)> {
        let Some(data) = self.rx.recv().await else {
            return Err(SenderInternalError::ProducerClosed);
        };

        Ok(data)
    }
}
