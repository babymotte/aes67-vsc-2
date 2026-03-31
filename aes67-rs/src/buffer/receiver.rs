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
    buffer::AudioBufferPointer,
    error::ReceiverInternalResult,
    formats::{Frames, SampleReader},
    monitoring::Monitoring,
    receiver::config::ReceiverConfig,
};
use std::{fmt::Debug, time::Duration};
use tokio::sync::watch;
use tracing::{debug, warn};

pub fn receiver_buffer_channel(
    config: ReceiverConfig,
    monitoring: Monitoring,
) -> (ReceiverBufferProducer, ReceiverBufferConsumer) {
    let buffer_len = config.duration_to_frames(Duration::from_secs(1)) as usize;
    let (tx, rx) = watch::channel(0);
    let buffer = vec![0f32; buffer_len].into_boxed_slice();
    let buffer_pointer = AudioBufferPointer::from_slice(&buffer);
    (
        ReceiverBufferProducer {
            buffer,
            config: config.clone(),
            tx,
        },
        ReceiverBufferConsumer {
            buffer_pointer,
            config,
            rx,
            monitoring,
        },
    )
}

#[derive(Debug, Clone)]
pub struct ReceiverBufferProducer {
    buffer: Box<[f32]>,
    config: ReceiverConfig,
    tx: watch::Sender<Frames>,
}

#[derive(Debug, Clone)]
pub struct ReceiverBufferConsumer {
    buffer_pointer: AudioBufferPointer,
    config: ReceiverConfig,
    rx: watch::Receiver<Frames>,
    monitoring: Monitoring,
}

pub enum ReadResult {
    Ok(usize),
    NotReady(usize),
    TooLate,
}

impl ReceiverBufferProducer {
    /// Deinterlace and write audio data into the shared buffer. The buffer is partitioned into equally sized strips,
    /// one for each channel, so that audio data can be retrieved individually per channel.
    pub fn write(&mut self, payload: &[u8], ingress_time: Frames) {
        let buf = &mut self.buffer[..];
        let sample_format = &self.config.audio_format.frame_format.sample_format;
        let channels = self.config.audio_format.frame_format.channels;
        let chunk_size = buf.len() / channels;
        let channel_partitions = buf.chunks_mut(chunk_size);

        let bytes_per_input_sample: usize = self
            .config
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
            .send(ingress_time + self.config.frames_in_buffer(payload.len()) - 1)
            .ok();
    }
}

impl ReceiverBufferConsumer {
    /// Read data from the shared buffer. Before reading, this function will block until the requested data is available.
    pub fn read<'a>(
        &mut self,
        buffers: impl Iterator<Item = Option<&'a mut [f32]>>,
        ingress_time: Frames,
        buffer_size: usize,
    ) -> ReceiverInternalResult<ReadResult> {
        let buf = self.buffer_pointer.buffer::<f32>();

        let mut latest_received_frame = 0;

        for (channel, output_buffer) in buffers.enumerate() {
            let Some(output_buffer) = output_buffer else {
                continue;
            };

            debug_assert_eq!(
                buffer_size,
                output_buffer.len(),
                "expected buffer of length {}, but got buffer of length {}",
                buffer_size,
                output_buffer.len()
            );

            let last_requested_frame = ingress_time + output_buffer.len() as Frames - 1;
            latest_received_frame = *self.rx.borrow();

            // TODO allow partial buffer read?

            if latest_received_frame < last_requested_frame {
                let missing = last_requested_frame - latest_received_frame;
                debug!(
                    "Requested frame {} has not been received yet (latest received frame is {}, need to wait for {} frames).",
                    last_requested_frame, latest_received_frame, missing,
                );
                return Ok(ReadResult::NotReady(missing as usize));
            }

            let oldest_frame_in_buffer =
                latest_received_frame - self.config.frames_in_buffer(buf.len()) + 1;
            if oldest_frame_in_buffer > ingress_time {
                warn!(
                    "The requested data is not in the receiver buffer anymore (requested frames: [{}; {}]; oldest frame in buffer: {}; {} frames late)!",
                    ingress_time,
                    last_requested_frame,
                    oldest_frame_in_buffer,
                    oldest_frame_in_buffer - ingress_time
                );
                return Ok(ReadResult::TooLate);
            }

            let channels = self.config.audio_format.frame_format.channels;
            let chunk_size = buf.len() / channels;
            let mut channel_partitions = buf.chunks(chunk_size);

            let rtp_buffer = channel_partitions
                .nth(channel)
                .expect("bug in buffer partitioning logic");

            let start_index = (ingress_time % rtp_buffer.len() as u64) as usize;
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

        if latest_received_frame > 0 {
            self.report_playout(ingress_time, latest_received_frame);
        }

        Ok(ReadResult::Ok(buffer_size))
    }
}

mod monitoring {
    use crate::monitoring::RxStats;

    use super::*;

    impl ReceiverBufferConsumer {
        pub(crate) fn report_playout(
            &mut self,
            ingress_time: Frames,
            latest_received_frame: Frames,
        ) {
            self.monitoring.receiver_stats(RxStats::Playout {
                ingress_time,
                latest_received_frame,
            });
        }
    }
}
