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
    error::{SenderInternalError, SenderInternalResult},
    formats::{Frames, SampleWriter},
    sender::config::SenderConfig,
};
use std::fmt::Debug;
use tokio::sync::mpsc;
use tracing::error;

pub fn sender_buffer_channel(config: SenderConfig) -> (SenderBufferProducer, SenderBufferConsumer) {
    let (tx, rx) = mpsc::channel((1_000.0 / config.packet_time.get()).ceil() as usize);
    let buffer = vec![0u8; config.audio_format.bytes_per_buffer(1_000.0)].into_boxed_slice();
    let buffer_pointer = AudioBufferPointer::from_slice(&buffer);
    (
        SenderBufferProducer {
            buffer_pointer,
            config: config.clone(),
            tx,
            unsent_frames: 0,
            received_frames: 0,
            sent_frames: 0,
            buffer_len: 0,
        },
        SenderBufferConsumer { buffer, rx },
    )
}

#[derive(Debug, Clone)]
pub struct SenderBufferProducer {
    buffer_pointer: AudioBufferPointer,
    config: SenderConfig,
    tx: mpsc::Sender<(Frames, usize)>,
    unsent_frames: usize,
    received_frames: usize,
    sent_frames: usize,
    buffer_len: usize,
}

pub struct SenderBufferConsumer {
    pub buffer: Box<[u8]>,
    rx: mpsc::Receiver<(Frames, usize)>,
}

// TODO return proper errors
// TODO generalize start and end indices/packet time
impl SenderBufferProducer {
    pub fn write(
        &mut self,
        channel_buffers: &[(AudioBufferPointer, Option<AudioBufferPointer>)],
        ingress_time: Frames,
    ) -> SenderInternalResult<()> {
        let channels = self.config.audio_format.frame_format.channels;

        debug_assert_eq!(
            channels,
            channel_buffers.len(),
            "expected {} buffers, but got {}",
            channels,
            channel_buffers.len()
        );

        let target_bytes_per_sample = self
            .config
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        let Some(buffer_len_frames) = channel_buffers
            .first()
            .map(|(h, t)| h.len() + t.as_ref().map(AudioBufferPointer::len).unwrap_or(0))
        else {
            error!("no buffers provided");
            return SenderInternalResult::Err(SenderInternalError::NoBuffersProvided);
        };

        let ptime_frames = self.config.ptime_frames() as usize;
        let available_frames = buffer_len_frames + self.unsent_frames;
        let spillover_frames = available_frames % ptime_frames;
        let packets = available_frames / ptime_frames;

        // TODO acquire some kind of lock that prevents from writing to de-allocated memory

        let offset = self.sent_frames;
        let packets_sent = offset / ptime_frames;

        for (ch, (buf_head, buf_tail)) in channel_buffers.iter().enumerate() {
            debug_assert_eq!(
                buf_head.len() + buf_tail.as_ref().map(AudioBufferPointer::len).unwrap_or(0),
                buffer_len_frames,
                "provided channel buffers have inconsistent lengths"
            );

            let head = buf_head.buffer::<f32>();
            let tail = buf_tail.as_ref().map(|b| b.buffer::<f32>());

            self.write_samples(head, offset, ch, channels, target_bytes_per_sample);
            if let Some(tail) = tail {
                self.write_samples(
                    tail,
                    offset + head.len(),
                    ch,
                    channels,
                    target_bytes_per_sample,
                );
            }
        }

        for i in 0..packets {
            self.tx.try_send((
                ingress_time + i as Frames * ptime_frames as Frames,
                packets_sent + i,
            ))?;
        }

        let buffer_size_changed = self.buffer_len != 0 && self.buffer_len != buffer_len_frames;

        if spillover_frames == 0 || buffer_size_changed {
            self.received_frames = 0;
            self.sent_frames = 0;
            self.unsent_frames = 0;
        } else {
            self.received_frames += buffer_len_frames;
            self.sent_frames += packets * ptime_frames;
            self.unsent_frames = spillover_frames;
        }

        self.buffer_len = buffer_len_frames;

        Ok(())
    }

    fn write_samples(
        &mut self,
        source: &[f32],
        offset: usize,
        ch: usize,
        channels: usize,
        target_bytes_per_sample: usize,
    ) {
        unsafe {
            let audio_buffer = self.buffer_pointer.buffer_mut::<u8>();

            for (sample_index, source_sample) in source.iter().enumerate() {
                let start_index = (sample_index + offset) * target_bytes_per_sample * channels
                    + ch * target_bytes_per_sample;
                let end_index = start_index + target_bytes_per_sample;

                let dest_buf = &mut audio_buffer[start_index..end_index];
                self.config
                    .audio_format
                    .frame_format
                    .sample_format
                    .write_sample(*source_sample, dest_buf);
            }
        }
    }
}

impl SenderBufferConsumer {
    pub fn read(&mut self) -> SenderInternalResult<(Frames, usize)> {
        let received = self.rx.blocking_recv();

        let Some(data) = received else {
            return Err(SenderInternalError::ProducerClosed);
        };

        Ok(data)
    }
}
