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
    config::adjust_labels_for_channel_count,
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
pub struct PartialReceiverConfig {
    pub label: Option<String>,
    pub audio_format: Option<AudioFormat>,
    pub source: Option<SocketAddr>,
    pub origin_ip: Option<IpAddr>,
    pub link_offset: Option<MilliSeconds>,
    pub rtp_offset: Option<u32>,
    pub channel_labels: Vec<String>,
}

impl PartialReceiverConfig {
    pub fn with_sample_rate(sample_rate: FramesPerSecond) -> Self {
        let mut it = Self::default();
        it.audio_format
            .as_mut()
            .map(|af| af.sample_rate = sample_rate);
        it
    }

    pub fn from_sdp_content(sdp_content: &str) -> Result<Self, ConfigError> {
        todo!()
    }

    pub async fn from_sdp_url(sdp_url: &str) -> Result<Self, ConfigError> {
        let response = reqwest::get(sdp_url)
            .await
            .map_err(|e| ConfigError::InvalidSdp(e.to_string()))?;
        let sdp_content = response
            .text()
            .await
            .map_err(|e| ConfigError::InvalidSdp(e.to_string()))?;
        Self::from_sdp_content(&sdp_content)
    }

    pub fn from_session_info(session_info: &SessionInfo) -> Self {
        let label = Some(session_info.name.clone());
        let audio_format = Some(AudioFormat {
            sample_rate: session_info.sample_rate,
            frame_format: FrameFormat {
                channels: session_info.channels,
                sample_format: session_info.sample_format,
            },
        });
        let source = Some(SocketAddr::from((
            session_info.destination_ip,
            session_info.destination_port,
        )));
        let origin_ip = Some(session_info.origin_ip);
        let link_offset = Some(4.0);
        let rtp_offset = Some(session_info.rtp_offset);
        let channel_labels = session_info.channel_labels.clone();

        Self {
            label,
            audio_format,
            source,
            origin_ip,
            link_offset,
            rtp_offset,
            channel_labels,
        }
    }
}

impl Default for PartialReceiverConfig {
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
            source: None,
            origin_ip: None,
            link_offset: Some(4.0),
            rtp_offset: Some(0),
            channel_labels: vec!["Left".to_owned(), "Right".to_owned()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverConfig {
    pub id: u32,
    pub label: String,
    pub audio_format: AudioFormat,
    pub source: SocketAddr,
    pub origin_ip: IpAddr,
    pub rtp_offset: u32,
    pub channel_labels: Vec<String>,
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
    pub channel_labels: Vec<String>,
    pub rtp_offset: u32,
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

        let no_labels = |count| Vec::with_capacity(count);

        let mut channel_labels = if let Some(i) = &sd.session_information {
            if let Some(caps) = CHANNELS_REGEX.captures(i) {
                caps[2].split(", ").map(|it| it.to_owned()).collect()
            } else {
                no_labels(channels)
            }
        } else {
            no_labels(channels)
        };
        adjust_labels_for_channel_count(channels, &mut channel_labels);

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
        let rtp_offset = media
            .attribute("mediaclk")
            .and_then(|it| it)
            .and_then(|clk| {
                if let Some(caps) = MEDIACLK_REGEX.captures(&clk) {
                    caps[1].parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

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
            channel_labels,
            rtp_offset,
        })
    }
}
