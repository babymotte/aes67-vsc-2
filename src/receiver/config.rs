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
    config::{Config, WebServerConfig},
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    formats::{self, AudioFormat, FrameFormat, MilliSeconds},
};
use lazy_static::lazy_static;
use regex::Regex;
use sdp::SessionDescription;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

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
    #[serde(default = "WebServerConfig::default")]
    pub webserver: WebServerConfig,
    #[serde(
        deserialize_with = "crate::serde::deserialize_sdp",
        serialize_with = "crate::serde::serialize_sdp"
    )]
    pub session: SessionDescription,
    pub link_offset: MilliSeconds,
    pub buffer_time: MilliSeconds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RxDescriptor {
    pub id: String,
    pub session_name: String,
    pub session_id: u64,
    pub session_version: u64,
    #[deprecated = "packet time should not be assumed based on SDP but taken from actual packet size"]
    pub packet_time: MilliSeconds,
    pub link_offset: MilliSeconds,
    pub origin_ip: IpAddr,
    pub rtp_offset: u32,
    pub audio_format: AudioFormat,
    pub channel_labels: Vec<Option<String>>,
}

impl TryFrom<&Config> for RxDescriptor {
    type Error = Aes67Vsc2Error;
    fn try_from(cfg: &Config) -> Aes67Vsc2Result<Self> {
        let rx_config = cfg
            .receiver_config
            .as_ref()
            .ok_or_else(|| Aes67Vsc2Error::Other("no receiver config".to_owned()))?;
        let id = cfg.app.instance.name.clone();
        let descriptor = RxDescriptor::new(id, &rx_config.session, rx_config.link_offset)?;
        Ok(descriptor)
    }
}

impl RxDescriptor {
    pub fn new(
        receiver_id: String,
        sd: &SessionDescription,
        link_offset: MilliSeconds,
    ) -> Aes67Vsc2Result<Self> {
        let media = if let Some(it) = sd.media_descriptions.first() {
            it
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp(
                "no media description found".to_owned(),
            ));
        };

        let fmt = if let Some(format) = media.media_name.formats.first() {
            format
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp(
                "no media format found".to_owned(),
            ));
        };

        // TODO make sure the right rtpmap is picked in case there is more than one
        let rtpmap = if let Some(Some(it)) = media.attribute("rtpmap") {
            it
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp("no rtpmap found".to_owned()));
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
                return Err(Aes67Vsc2Error::InvalidSdp("malformed rtpmap".to_owned()));
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
            return Err(Aes67Vsc2Error::InvalidSdp(
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
            return Err(Aes67Vsc2Error::InvalidSdp("no ptime".to_owned()));
        };

        let mediaclk = if let Some(it) = media.attribute("mediaclk").and_then(|it| it) {
            it
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp("mediaclk".to_owned()));
        };

        let rtp_offset = if let Some(caps) = MEDIACLK_REGEX.captures(mediaclk) {
            caps[1].parse().expect("regex guarantees this is a number")
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp("malformed mediaclk".to_owned()));
        };

        let session_name = sd.session_name.clone();
        let session_id = sd.origin.session_id;
        let session_version = sd.origin.session_version;
        let origin_ip = sd
            .origin
            .unicast_address
            .parse()
            .map_err(|e| Aes67Vsc2Error::Other(format!("error parsing origin IP: {e}")))?;

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

    #[deprecated = "packet time should not be assumed based on SDP but taken from actual packet size"]
    pub fn frames_per_packet(&self) -> usize {
        formats::frames_per_packet(self.audio_format.sample_rate, self.packet_time)
    }

    pub(crate) fn frames_per_ms(&self) -> usize {
        formats::frames_per_packet(self.audio_format.sample_rate, 1.0)
    }

    pub(crate) fn frames_in_link_offset(&self) -> usize {
        formats::frames_per_packet(self.audio_format.sample_rate, self.link_offset)
    }

    pub(crate) fn frames_per_link_offset(&self, link_offset: MilliSeconds) -> usize {
        formats::frames_per_packet(self.audio_format.sample_rate, link_offset)
    }

    #[deprecated = "packet time should not be assumed based on SDP but taken from actual packet size"]
    pub fn samples_per_packet(&self) -> usize {
        formats::samples_per_packet(
            self.audio_format.frame_format.channels,
            self.audio_format.sample_rate,
            self.packet_time,
        )
    }

    #[deprecated = "packet time should not be assumed based on SDP but taken from actual packet size"]
    pub fn rtp_payload_size(&self) -> usize {
        formats::rtp_payload_size(
            self.audio_format.sample_rate,
            self.packet_time,
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    #[deprecated = "packet time should not be assumed based on SDP but taken from actual packet size"]
    pub fn rtp_packet_size(&self) -> usize {
        formats::rtp_packet_size(
            self.audio_format.sample_rate,
            self.packet_time,
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    pub fn to_link_offset(&self, samples: usize) -> usize {
        formats::to_link_offset(samples, self.audio_format.sample_rate)
    }
}
