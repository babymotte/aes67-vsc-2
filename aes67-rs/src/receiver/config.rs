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
    error::ConfigError,
    formats::{
        self, AudioFormat, FrameFormat, Frames, FramesPerSecond, MilliSeconds, SampleFormat,
        Seconds,
    },
    time::MICROS_PER_MILLI_F,
};
use lazy_static::lazy_static;
use regex::Regex;
use sdp::SessionDescription;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

lazy_static! {
    static ref MEDIA_REGEX: Regex =
        Regex::new(r"audio (.+) (.+) (.+)").expect("no dynammic input, can't fail");
    static ref RTPMAP_REGEX: Regex = Regex::new(r"([0-9]+) (.+?[0-9]+)\/([0-9]+)\/([0-9]+)")
        .expect("no dynammic input, can't fail");
    static ref TS_REFCLK_REGEX: Regex =
        Regex::new(r"ptp=(.+):(.+):(.+)").expect("no dynammic input, can't fail");
    static ref MEDIACLK_REGEX: Regex =
        Regex::new(r"direct=([0-9]+)").expect("no dynammic input, can't fail");
    static ref CHANNELS_REGEX: Regex =
        Regex::new(r"([0-9]+) channels: (.+)").expect("no dynammic input, can't fail");
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverConfig {
    pub id: u32,
    pub label: String,
    pub audio_format: AudioFormat,
    pub source: SocketAddr,
    pub origin_ip: IpAddr,
    pub payload_type: u8,
    pub rtp_offset: u32,
    pub channel_labels: Option<Vec<String>>,
    pub link_offset: MilliSeconds,
    #[serde(default)]
    pub delay_calculation_interval: Option<Seconds>,
}

impl ReceiverConfig {
    pub fn buffer_time(&self) -> MilliSeconds {
        (self.link_offset * 20.0).max(20.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId {
    pub id: u64,
    pub version: u64,
}

impl<T: AsRef<(u64, u64)>> From<T> for SessionId {
    fn from(value: T) -> Self {
        let r = value.as_ref();
        let id = r.0;
        let version = r.1;
        SessionId { id, version }
    }
}

impl ReceiverConfig {
    pub fn bytes_per_sample(&self) -> usize {
        self.audio_format
            .frame_format
            .sample_format
            .bytes_per_sample()
    }

    pub fn bytes_per_frame(&self) -> usize {
        formats::bytes_per_frame(
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    #[deprecated = "link offset is a configuration that may change during playout, it is not acceptable to read this from a static object"]
    pub fn frames_in_link_offset(&self) -> usize {
        formats::duration_to_frames(
            Duration::from_micros((self.link_offset * MICROS_PER_MILLI_F).round() as u64),
            self.audio_format.sample_rate,
        )
        .round() as usize
    }

    pub(crate) fn frames_in_buffer(&self, buffer_len: usize) -> u64 {
        buffer_len as u64 / self.bytes_per_frame() as u64
    }

    pub fn to_link_offset(&self, samples: usize) -> MilliSeconds {
        formats::to_link_offset(samples, self.audio_format.sample_rate)
    }

    pub fn duration_to_frames(&self, duration: Duration) -> f64 {
        formats::duration_to_frames(duration, self.audio_format.sample_rate)
    }

    pub fn frames_to_duration(&self, frames: Frames) -> Duration {
        formats::frames_to_duration(frames, self.audio_format.sample_rate)
    }

    pub fn frames_to_duration_float(&self, frames: f64) -> Duration {
        formats::frames_to_duration_float(frames, self.audio_format.sample_rate)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub destination_ip: IpAddr,
    pub destination_port: u16,
    pub channels: usize,
    pub sample_format: SampleFormat,
    pub sample_rate: FramesPerSecond,
    pub packet_time: MilliSeconds,
    pub origin_ip: IpAddr,
}

impl TryFrom<&SessionDescription> for SessionInfo {
    type Error = ConfigError;

    fn try_from(sd: &SessionDescription) -> Result<Self, Self::Error> {
        let origin_ip = sd.origin.unicast_address.parse()?;

        let media = if let Some(it) = sd.media_descriptions.first() {
            it
        } else {
            return Err(ConfigError::InvalidSdp(
                "no media description found".to_owned(),
            ));
        };

        let fmt = if let Some(format) = media.media_name.formats.first() {
            format
        } else {
            return Err(ConfigError::InvalidSdp("no media format found".to_owned()));
        };

        // TODO make sure the right rtpmap is picked in case there is more than one
        let rtpmap = if let Some(Some(it)) = media.attribute("rtpmap") {
            it
        } else {
            return Err(ConfigError::InvalidSdp("no rtpmap found".to_owned()));
        };

        let (payload_type, sample_format, sample_rate, channels) =
            if let Some(caps) = RTPMAP_REGEX.captures(rtpmap) {
                (
                    caps[1].to_owned(),
                    caps[2].parse()?,
                    caps[3].parse().expect("regex guarantees this is a number"),
                    caps[4].parse().expect("regex guarantees this is a number"),
                )
            } else {
                return Err(ConfigError::InvalidSdp("malformed rtpmap".to_owned()));
            };

        let no_labels = || {
            let mut v = vec![];
            for _ in 0..channels {
                v.push(None);
            }
            v
        };

        let channel_labels = if let Some(i) = &sd.session_information {
            if let Some(caps) = CHANNELS_REGEX.captures(i) {
                caps[2].split(", ").map(|it| Some(it.to_owned())).collect()
            } else {
                no_labels()
            }
        } else {
            no_labels()
        };

        if &payload_type != fmt {
            return Err(ConfigError::InvalidSdp(
                "rtpmap and media description payload types do not match".to_owned(),
            ));
        }

        let packet_time = if let Some(ptime) = media
            .attribute("ptime")
            .and_then(|it| it)
            .and_then(|p| p.parse().ok())
        {
            ptime
        } else {
            return Err(ConfigError::InvalidSdp("no ptime".to_owned()));
        };

        let name = sd.session_name.clone();
        let session_id = sd.origin.session_id;
        let session_version = sd.origin.session_version;

        let frame_format = FrameFormat {
            channels,
            sample_format,
        };
        let audio_format = AudioFormat {
            frame_format,
            sample_rate,
        };
        let session_id = SessionId {
            id: session_id,
            version: session_version,
        };

        let global_c = sd.connection_information.as_ref();
        let destination_address = media
            .connection_information
            .as_ref()
            .or(global_c)
            .ok_or_else(|| {
                ConfigError::InvalidSdp(format!("no connection information for media {media:?}"))
            })?
            .address
            .as_ref()
            .ok_or_else(|| ConfigError::InvalidSdp("no address for media".to_owned()))?
            .address
            .to_owned();
        let mut split = destination_address.split('/');
        let ip = split.next();
        let prefix = split.next();
        let destination_ip: IpAddr = if let (Some(ip), Some(_prefix)) = (ip, prefix) {
            ip.parse()?
        } else {
            return Err(ConfigError::InvalidSdp(format!(
                "invalid ip address: {destination_address}"
            )));
        };

        let destination_port = media.media_name.port.value.to_owned() as u16;

        Ok(SessionInfo {
            id: session_id,
            name,
            channels,
            destination_ip,
            destination_port,
            packet_time,
            sample_format: audio_format.frame_format.sample_format,
            sample_rate: audio_format.sample_rate,
            origin_ip,
        })
    }
}
