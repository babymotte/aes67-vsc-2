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
    (
        SenderBufferProducer {
            buffer_pointer,
            config: config.clone(),
            tx,
            unsent_frames: 0,
            received_frames: 0,
            sent_frames: 0,
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
}

pub struct SenderBufferConsumer {
    pub buffer: Box<[u8]>,
    rx: mpsc::Receiver<(Frames, usize)>,
}

// TODO return proper errors

impl SenderBufferProducer {
    pub fn write_channel(&mut self, channel: usize, channel_buffer: &[f32]) {
        let channels = self.config.audio_format.frame_format.channels;

        let target_bytes_per_sample = self
            .config
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();

        let offset = self.sent_frames;

        self.write_samples(
            channel_buffer,
            offset,
            channel,
            channels,
            target_bytes_per_sample,
        );
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
        let packets = available_frames / ptime_frames;
        let packets_sent = self.sent_frames / ptime_frames;

        for i in 0..packets {
            self.tx.try_send((
                ingress_time + i as Frames * ptime_frames as Frames,
                packets_sent + i,
            ))?;
        }

        if spillover_frames == 0 || buffer_size_changed {
            self.received_frames = 0;
            self.sent_frames = 0;
            self.unsent_frames = 0;
        } else {
            self.received_frames += buffer_len_frames;
            self.sent_frames += packets * ptime_frames;
            self.unsent_frames = spillover_frames;
        }

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
