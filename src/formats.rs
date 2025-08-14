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

use crate::{error::Aes67Vsc2Error, receiver::config::RxDescriptor};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, time::Duration};

pub type Seconds = u32;
pub type MilliSeconds = f32;
pub type NanoSeconds = u128;
pub type Frames = u64;
pub type FramesPerSecond = usize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct BufferFormat {
    pub buffer_len: usize,
    pub audio_format: AudioFormat,
}

impl BufferFormat {
    pub fn for_rtp_playout_buffer(buffer_time: MilliSeconds, audio_format: AudioFormat) -> Self {
        let buffer_len = audio_format.bytes_per_buffer(buffer_time);
        Self {
            buffer_len,
            audio_format,
        }
    }

    pub fn frames_per_buffer(&self) -> usize {
        self.buffer_len / self.audio_format.frame_format.bytes_per_frame()
    }

    pub fn bytes_per_buffer(&self) -> usize {
        self.audio_format.frame_format.bytes_per_frame() * self.frames_per_buffer()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct AudioFormat {
    pub sample_rate: FramesPerSecond,
    pub frame_format: FrameFormat,
}

impl AudioFormat {
    pub fn bytes_per_buffer(&self, link_offset: MilliSeconds) -> usize {
        self.samples_per_link_offset_buffer(link_offset)
            * self.frame_format.sample_format.bytes_per_sample()
    }

    pub fn samples_per_link_offset_buffer(&self, link_offset: MilliSeconds) -> usize {
        self.frame_format.channels * self.frames_per_link_offset_buffer(link_offset)
    }

    pub fn frames_per_link_offset_buffer(&self, link_offset: MilliSeconds) -> usize {
        frames_per_link_offset_buffer(link_offset, self.sample_rate)
    }
}

impl From<&RxDescriptor> for AudioFormat {
    fn from(value: &RxDescriptor) -> Self {
        Self {
            sample_rate: value.audio_format.sample_rate,
            frame_format: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct FrameFormat {
    pub channels: usize,
    pub sample_format: SampleFormat,
}

impl From<&RxDescriptor> for FrameFormat {
    fn from(value: &RxDescriptor) -> Self {
        Self {
            channels: value.audio_format.frame_format.channels,
            sample_format: value.audio_format.frame_format.sample_format,
        }
    }
}

impl FrameFormat {
    pub fn bytes_per_frame(&self) -> usize {
        self.samples_per_frame() * self.sample_format.bytes_per_sample()
    }

    pub fn samples_per_frame(&self) -> usize {
        self.channels
    }

    pub fn sample_index_in_buffer_frame(&self, channel: usize) -> usize {
        channel * self.sample_format.bytes_per_sample()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum SampleFormat {
    // TODO implement other sample formats
    L16,
    L24,
}

impl FromStr for SampleFormat {
    type Err = Aes67Vsc2Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "L16" => Ok(SampleFormat::L24),
            "L24" => Ok(SampleFormat::L24),
            other => Err(Aes67Vsc2Error::UnknownSampleFormat(other.to_owned())),
        }
    }
}

pub trait SampleReader<S> {
    fn read_sample(&self, buffer: &[u8]) -> S;
}

impl SampleReader<f32> for SampleFormat {
    fn read_sample(&self, buffer: &[u8]) -> f32 {
        self.read_f32(buffer)
    }
}

impl SampleReader<i32> for SampleFormat {
    fn read_sample(&self, buffer: &[u8]) -> i32 {
        self.read_i32(buffer)
    }
}

impl SampleFormat {
    fn read_f32(&self, buffer: &[u8]) -> f32 {
        match self {
            SampleFormat::L16 => bytes_to_f32_2_bytes(buffer),
            SampleFormat::L24 => bytes_to_f32_3_bytes(buffer),
        }
    }

    fn read_i32(&self, buffer: &[u8]) -> i32 {
        match self {
            SampleFormat::L16 => bytes_to_i32_2_bytes(buffer),
            SampleFormat::L24 => bytes_to_i32_3_bytes(buffer),
        }
    }

    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::L16 => 2,
            SampleFormat::L24 => 3,
        }
    }
}

fn bytes_to_f32_2_bytes(bytes: &[u8]) -> f32 {
    let value = bytes_to_i32_2_bytes(bytes);
    if value >= 0 {
        value as f32 / i16::MAX as f32
    } else {
        (value + 1) as f32 / i16::MAX as f32
    }
}

fn bytes_to_i32_2_bytes(bytes: &[u8]) -> i32 {
    i16::from_be_bytes([bytes[0], bytes[1]]) as i32
}

fn bytes_to_f32_3_bytes(bytes: &[u8]) -> f32 {
    let value = bytes_to_i32_3_bytes(bytes);

    if value >= 0 {
        value as f32 / 0x7FFFFF as f32 // Max 24-bit signed value
    } else {
        (value + 1) as f32 / 0x7FFFFF as f32 // Max 24-bit signed value
    }
}

fn bytes_to_i32_3_bytes(bytes: &[u8]) -> i32 {
    let mut value = ((bytes[0] as i32) << 16) | ((bytes[1] as i32) << 8) | (bytes[2] as i32);

    // Sign extend from 24-bit to 32-bit
    if value & 0x800000 != 0 {
        value |= !0xFFFFFF;
    }
    value
}

pub fn rtp_header_len() -> usize {
    12
}

pub fn max_samplerate() -> FramesPerSecond {
    96000
}

pub fn max_bit_depth() -> usize {
    32
}

pub fn max_packet_time() -> MilliSeconds {
    4.0
}

pub fn bytes_per_sample(bit_depth: usize) -> usize {
    bit_depth / 8
}

pub fn bytes_per_frame(channels: usize, sample_format: SampleFormat) -> usize {
    channels * sample_format.bytes_per_sample()
}

pub fn frames_per_packet(sample_rate: FramesPerSecond, packet_time: MilliSeconds) -> usize {
    f32::ceil((sample_rate as f32 * packet_time) / 1000.0) as usize
}

pub fn samples_per_packet(
    channels: usize,
    sample_rate: FramesPerSecond,
    packet_time: MilliSeconds,
) -> usize {
    channels * frames_per_packet(sample_rate, packet_time)
}

pub fn packets_in_link_offset(link_offset: MilliSeconds, packet_time: MilliSeconds) -> usize {
    f32::ceil(link_offset / packet_time) as usize
}

pub fn frames_per_link_offset_buffer(
    link_offset: MilliSeconds,
    sample_rate: FramesPerSecond,
) -> usize {
    f32::ceil((sample_rate as f32 * link_offset) / Duration::from_secs(1).as_millis() as f32)
        as usize
}

pub fn link_offset_buffer_size(
    channels: usize,
    link_offset: MilliSeconds,
    sample_rate: FramesPerSecond,
    sample_format: SampleFormat,
) -> usize {
    samples_per_link_offset_buffer(channels, link_offset, sample_rate)
        * sample_format.bytes_per_sample()
}

pub fn rtp_payload_size(
    sample_rate: FramesPerSecond,
    packet_time: MilliSeconds,
    channels: usize,
    sample_format: SampleFormat,
) -> usize {
    frames_per_packet(sample_rate, packet_time) * bytes_per_frame(channels, sample_format)
}

pub fn rtp_packet_size(
    sample_rate: FramesPerSecond,
    packet_time: MilliSeconds,
    channels: usize,
    sample_format: SampleFormat,
) -> usize {
    rtp_header_len() + rtp_payload_size(sample_rate, packet_time, channels, sample_format)
}

pub fn samples_per_link_offset_buffer(
    channels: usize,
    link_offset: MilliSeconds,
    sample_rate: FramesPerSecond,
) -> usize {
    channels * frames_per_link_offset_buffer(link_offset, sample_rate)
}

pub fn rtp_buffer_size(
    link_offset: MilliSeconds,
    packet_time: MilliSeconds,
    sample_rate: FramesPerSecond,
    channels: usize,
    sample_format: SampleFormat,
) -> usize {
    packets_in_link_offset(link_offset, packet_time)
        * rtp_packet_size(sample_rate, packet_time, channels, sample_format)
}

pub fn to_link_offset(samples: usize, sample_rate: FramesPerSecond) -> usize {
    f32::ceil((samples as f32 * 1000.0) / sample_rate as f32) as usize
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn frames_per_link_offset_buffer_works() {
        assert_eq!(192, frames_per_link_offset_buffer(4.0, 48_000));
    }
}
