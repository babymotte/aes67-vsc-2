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

use crate::formats::{
    AudioFormat, FrameFormat, MilliSeconds, PayloadType, SampleFormat, SessionId, SessionVersion,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSenderConfig {
    pub label: Option<String>,
    pub audio_format: Option<AudioFormat>,
    pub target: Option<SocketAddr>,
    pub payload_type: Option<PayloadType>,
    pub channel_labels: Vec<String>,
    pub packet_time: Option<MilliSeconds>,
    pub version: Option<SessionVersion>,
}

impl Default for PartialSenderConfig {
    fn default() -> Self {
        Self {
            label: Some("".to_owned()),
            audio_format: Some(AudioFormat {
                sample_rate: 48_000,
                frame_format: FrameFormat {
                    channels: 2,
                    sample_format: SampleFormat::L24,
                },
            }),
            target: Some(SocketAddr::from(([239, 255, 0, 1], 5004))),
            payload_type: Some(98),
            channel_labels: vec!["Left".to_owned(), "Right".to_owned()],
            packet_time: Some(1.0),
            version: Some(1),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderConfig {
    pub id: SessionId,
    pub label: String,
    pub audio_format: AudioFormat,
    pub target: SocketAddr,
    pub packet_time: MilliSeconds,
    pub payload_type: PayloadType,
    pub channel_labels: Vec<String>,
}
