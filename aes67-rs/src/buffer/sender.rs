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
    formats::{Frames, MilliSeconds, SampleWriter},
    sender::config::SenderConfig,
};
use std::{fmt::Debug, ops::Range};
use tokio::sync::mpsc;

pub fn sender_buffer_channel(
    config: SenderConfig,
    phases: usize,
) -> (SenderBufferProducer, SenderBufferConsumer) {
    let (tx, rx) = mpsc::channel(phases);
    let max_ptime = 4.0;
    let max_producer_buffer_duration = 1_000.0;
    let buffer_len = config
        .audio_format
        .bytes_per_buffer((max_producer_buffer_duration + max_ptime) * phases as MilliSeconds);
    let buffer = vec![0u8; buffer_len].into_boxed_slice();
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
            tx,
            unsent_frames: 0,
            target_bytes_per_sample,
            phase: 0,
            phases,
        },
        SenderBufferConsumer { buffer, rx },
    )
}

#[derive(Debug, Clone)]
pub struct SenderBufferProducer {
    buffer_pointer: AudioBufferPointer,
    config: SenderConfig,
    tx: mpsc::Sender<OutgoingPacketPointer>,
    unsent_frames: usize,
    target_bytes_per_sample: usize,
    phase: usize,
    phases: usize,
}

pub struct OutgoingPacketPointer {
    pub ingress_time: Frames,
    pub payload_range: Range<usize>,
}

pub struct SenderBufferConsumer {
    pub buffer: Box<[u8]>,
    rx: mpsc::Receiver<OutgoingPacketPointer>,
}

impl SenderBufferProducer {
    pub fn write_channel(&mut self, channel: usize, offset_frames: usize, channel_buffer: &[f32]) {
        let phase_offset = self.phase * self.buffer_pointer.len() / self.phases;
        let unsent_frames = self.unsent_frames;
        let bytes_per_sample = self.target_bytes_per_sample;
        let channels = self.config.audio_format.frame_format.channels;
        let bytes_per_frame = bytes_per_sample * channels;
        let channel_offset = channel * bytes_per_sample;

        unsafe {
            let audio_buffer = self.buffer_pointer.buffer_mut::<u8>();

            for (frame_index, source_sample) in channel_buffer.iter().enumerate() {
                let frame_offset = (frame_index + unsent_frames + offset_frames) * bytes_per_frame;
                let start_index = phase_offset + frame_offset + channel_offset;
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
        written_frames: usize,
    ) -> SenderInternalResult<()> {
        let phase_offset = self.phase * self.buffer_pointer.len() / self.phases;
        let payload_len = self.config.send_buffer_len();
        let ptime_frames = self.config.ptime_frames() as usize;
        let available_frames = written_frames + self.unsent_frames;
        let spillover_frames = available_frames % ptime_frames;
        let frames_to_send = available_frames - spillover_frames;
        let packets = frames_to_send / ptime_frames;
        let first_packet_ingress_time = ingress_time - self.unsent_frames as Frames;

        for i in 0..packets {
            let ingress_time = first_packet_ingress_time + (i as Frames * ptime_frames as Frames);

            let start_index = phase_offset + i * payload_len;
            let end_index = start_index + payload_len;
            let payload_range = start_index..end_index;
            let packet_pointer = OutgoingPacketPointer {
                ingress_time,
                payload_range,
            };
            self.tx.try_send(packet_pointer)?;
        }

        let next_phase = (self.phase + 1) % self.phases;

        if spillover_frames > 0 {
            // copy spillover frames to beginning of next phase
            let bytes_per_sample = self.target_bytes_per_sample;
            let channels = self.config.audio_format.frame_format.channels;
            let bytes_per_frame = bytes_per_sample * channels;
            let start_index = phase_offset + packets * payload_len;
            let spillover_bytes = spillover_frames * bytes_per_frame;
            let end_index = start_index + spillover_bytes;
            let src_range = start_index..end_index;

            let next_phase_offset = next_phase * self.buffer_pointer.len() / self.phases;

            unsafe {
                let audio_buffer = self.buffer_pointer.buffer_mut::<u8>();
                audio_buffer.copy_within(src_range, next_phase_offset);
            }
        }

        self.phase = next_phase;
        self.unsent_frames = spillover_frames;

        Ok(())
    }
}

impl SenderBufferConsumer {
    pub fn recv(&mut self) -> SenderInternalResult<OutgoingPacketPointer> {
        let received = self.rx.blocking_recv();

        let Some(data) = received else {
            return Err(SenderInternalError::ProducerClosed);
        };

        Ok(data)
    }
}
