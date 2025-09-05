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
    error::{ConfigError, ConfigResult},
    formats::{self, AudioFormat, FrameFormat, Frames, MilliSeconds, Seconds},
};
use lazy_static::lazy_static;
use regex::Regex;
use sdp::SessionDescription;
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, time::Duration};

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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverConfig {
    pub id: Option<String>,
    #[serde(deserialize_with = "crate::serde::deserialize_sdp")]
    pub session: SessionDescription,
    pub link_offset: MilliSeconds,
    #[serde(default)]
    pub delay_calculation_interval: Option<Seconds>,
    pub interface_ip: IpAddr,
}

impl ReceiverConfig {
    pub fn id(&self) -> &str {
        self.id.as_ref().unwrap_or(&self.session.session_name)
    }

    pub fn buffer_time(&self) -> MilliSeconds {
        (self.link_offset * 20.0).max(20.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RxDescriptor {
    pub id: String,
    pub session_name: String,
    pub session_id: u64,
    pub session_version: u64,
    pub packet_time: MilliSeconds,
    pub link_offset: MilliSeconds,
    pub origin_ip: IpAddr,
    pub rtp_offset: u32,
    pub audio_format: AudioFormat,
    pub channel_labels: Vec<Option<String>>,
}

impl TryFrom<&ReceiverConfig> for RxDescriptor {
    type Error = ConfigError;
    fn try_from(rx_config: &ReceiverConfig) -> ConfigResult<Self> {
        let id = rx_config.id().to_owned();
        let descriptor = RxDescriptor::new(id, &rx_config.session, rx_config.link_offset)?;
        Ok(descriptor)
    }
}

impl RxDescriptor {
    pub fn new(
        receiver_id: String,
        sd: &SessionDescription,
        link_offset: MilliSeconds,
    ) -> ConfigResult<Self> {
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

        let mediaclk = if let Some(it) = media.attribute("mediaclk").and_then(|it| it) {
            it
        } else {
            return Err(ConfigError::InvalidSdp("mediaclk".to_owned()));
        };

        let rtp_offset = if let Some(caps) = MEDIACLK_REGEX.captures(mediaclk) {
            caps[1].parse().expect("regex guarantees this is a number")
        } else {
            return Err(ConfigError::InvalidSdp("malformed mediaclk".to_owned()));
        };

        let session_name = sd.session_name.clone();
        let session_id = sd.origin.session_id;
        let session_version = sd.origin.session_version;
        let origin_ip = sd.origin.unicast_address.parse()?;

        let frame_format = FrameFormat {
            channels,
            sample_format,
        };
        let audio_format = AudioFormat {
            frame_format,
            sample_rate,
        };
        // let channel_labels = sd.session_information.
        Ok(RxDescriptor {
            id: receiver_id,
            session_name,
            session_id,
            session_version,
            audio_format,
            packet_time,
            origin_ip,
            rtp_offset,
            channel_labels,
            link_offset,
        })
    }

    pub fn session_id_from_sdp(sdp: &SessionDescription) -> String {
        format!("{} {}", sdp.origin.session_id, sdp.origin.session_version)
    }

    pub fn session_id(&self) -> String {
        format!("{} {}", self.session_id, self.session_version)
    }

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
    pub(crate) fn frames_in_link_offset(&self) -> usize {
        formats::duration_to_frames(
            Duration::from_micros((self.link_offset * 1_000.0).round() as u64),
            self.audio_format.sample_rate,
        )
        .round() as usize
    }

    pub(crate) fn frames_in_buffer(&self, buffer_len: usize) -> u64 {
        buffer_len as u64 / self.bytes_per_frame() as u64
    }

    pub fn to_link_offset(&self, samples: usize) -> usize {
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
