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

pub fn sender_buffer_channel(config: SenderConfig) -> (SenderBufferProducer, SenderBufferConsumer) {
    let (tx, rx) = mpsc::channel((1_000.0 / config.packet_time.get()).ceil() as usize);
    let buffer = vec![0u8; config.audio_format.bytes_per_buffer(1_000.0)].into_boxed_slice();
    let buffer_pointer = AudioBufferPointer::from_slice(&buffer);
    let target_bytes_per_sample = config
        .audio_format
        .frame_format
        .sample_format
        .bytes_per_sample();
    (
        SenderBufferProducer {
            buffer_pointer,
            config: config.clone(),
            min_phases: 5,
            tx,
            unsent_frames: 0,
            received_frames: 0,
            sent_frames: 0,
            target_bytes_per_sample,
            sent_packets: 0,
        },
        SenderBufferConsumer { buffer, rx },
    )
}

#[derive(Debug, Clone)]
pub struct SenderBufferProducer {
    buffer_pointer: AudioBufferPointer,
    config: SenderConfig,
    min_phases: usize,
    tx: mpsc::Sender<(Frames, usize)>,
    unsent_frames: usize,
    received_frames: usize,
    sent_frames: usize,
    target_bytes_per_sample: usize,
    sent_packets: usize,
}

pub struct SenderBufferConsumer {
    pub buffer: Box<[u8]>,
    rx: mpsc::Receiver<(Frames, usize)>,
}

impl SenderBufferProducer {
    pub fn write_channel(&mut self, channel: usize, channel_buffer: &[f32]) {
        let received_frames = self.received_frames;
        let bytes_per_sample = self.target_bytes_per_sample;
        let channels = self.config.audio_format.frame_format.channels;
        let bytes_per_frame = bytes_per_sample * channels;
        let channel_offset = channel * bytes_per_sample;

        unsafe {
            let audio_buffer = self.buffer_pointer.buffer_mut::<u8>();

            for (frame_index, source_sample) in channel_buffer.iter().enumerate() {
                let frame_offset = (frame_index + received_frames) * bytes_per_frame;
                let start_index = frame_offset + channel_offset;
                let end_index = start_index + bytes_per_sample;

                let dest_buf = &mut audio_buffer[start_index..end_index];

                self.config
                    .audio_format
                    .frame_format
                    .sample_format
                    .write_sample(*source_sample, dest_buf);
            }
        }
    }

    pub fn send_packets(
        &mut self,
        ingress_time: Frames,
        buffer_len_frames: usize,
        buffer_size_changed: bool,
    ) -> SenderInternalResult<()> {
        let ptime_frames = self.config.ptime_frames() as usize;
        let available_frames = buffer_len_frames + self.unsent_frames;
        let spillover_frames = available_frames % ptime_frames;
        let frames_to_send = available_frames - spillover_frames;
        let packets = frames_to_send / ptime_frames;
        let sent_packets = self.sent_packets;

        for i in 0..packets {
            self.tx.try_send((
                ingress_time + i as Frames * ptime_frames as Frames,
                sent_packets + i,
            ))?;
        }

        if (spillover_frames == 0 && self.sent_packets >= self.min_phases) || buffer_size_changed {
            self.received_frames = 0;
            self.sent_frames = 0;
            self.unsent_frames = 0;
            self.sent_packets = 0;
        } else {
            self.received_frames += buffer_len_frames;
            self.sent_frames += frames_to_send;
            self.unsent_frames = spillover_frames;
            self.sent_packets += packets;
        }

        Ok(())
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
