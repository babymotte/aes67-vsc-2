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

use crate::{error::ConfigError, formats::AudioFormat};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderConfig {
    pub id: String,
    pub audio_format: AudioFormat,
    pub interface_ip: IpAddr,
    pub target: SocketAddr,
    pub payload_type: u8,
    pub channel_labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxDescriptor {
    pub id: String,
    pub audio_format: AudioFormat,
    pub payload_type: u8,
    pub channel_labels: Vec<Option<String>>,
}

impl TryFrom<&SenderConfig> for TxDescriptor {
    type Error = ConfigError;

    fn try_from(value: &SenderConfig) -> Result<Self, Self::Error> {
        let labels = (0..value.audio_format.frame_format.channels)
            .map(|i| value.channel_labels.as_ref().and_then(|it| it.get(i)))
            .map(|it| it.map(|it| it.to_owned()))
            .collect::<Vec<Option<String>>>();
        Ok(Self {
            id: value.id.clone(),
            audio_format: value.audio_format,
            payload_type: value.payload_type,
            channel_labels: labels,
        })
    }
}
