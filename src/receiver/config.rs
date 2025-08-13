use crate::{
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    formats::{self, AudioFormat, FrameFormat},
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverConfig {
    #[serde(
        deserialize_with = "crate::serde::deserialize_sdp",
        serialize_with = "crate::serde::serialize_sdp"
    )]
    pub session: SessionDescription,
    pub link_offset: u32,
    pub buffer_overhead: u32,
    pub interface_ip: IpAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RxDescriptor {
    pub id: String,
    pub session_name: String,
    pub session_id: u64,
    pub session_version: u64,
    pub packet_time: f32,
    pub link_offset: u32,
    pub origin_ip: IpAddr,
    pub rtp_offset: u32,
    pub audio_format: AudioFormat,
}

impl RxDescriptor {
    pub fn new(
        receiver_id: String,
        sd: &SessionDescription,
        link_offset: u32,
    ) -> Aes67Vsc2Result<Self> {
        let media = if let Some(it) = sd.media_descriptions.iter().next() {
            it
        } else {
            return Err(Aes67Vsc2Error::InvalidSdp(
                "no media description found".to_owned(),
            ));
        };

        let fmt = if let Some(format) = media.media_name.formats.iter().next() {
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
        Ok(RxDescriptor {
            id: receiver_id,
            session_name,
            session_id,
            session_version,
            audio_format,
            packet_time,
            link_offset,
            origin_ip,
            rtp_offset,
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

    pub fn frames_per_packet(&self) -> usize {
        formats::frames_per_packet(self.audio_format.sample_rate, self.packet_time)
    }

    pub fn samples_per_packet(&self) -> usize {
        formats::samples_per_packet(
            self.audio_format.frame_format.channels,
            self.audio_format.sample_rate,
            self.packet_time,
        )
    }

    pub fn packets_in_link_offset(&self) -> usize {
        formats::packets_in_link_offset(self.link_offset, self.packet_time)
    }

    pub fn frames_in_link_offset(&self) -> u32 {
        formats::frames_per_link_offset_buffer(self.link_offset, self.audio_format.sample_rate)
    }

    pub fn link_offset_buffer_size(&self) -> usize {
        formats::link_offset_buffer_size(
            self.audio_format.frame_format.channels,
            self.link_offset,
            self.audio_format.sample_rate,
            self.audio_format.frame_format.sample_format,
        )
    }

    pub fn rtp_payload_size(&self) -> usize {
        formats::rtp_payload_size(
            self.audio_format.sample_rate,
            self.packet_time,
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    pub fn rtp_packet_size(&self) -> usize {
        formats::rtp_packet_size(
            self.audio_format.sample_rate,
            self.packet_time,
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    pub fn samples_per_link_offset_buffer(&self) -> usize {
        formats::samples_per_link_offset_buffer(
            self.audio_format.frame_format.channels,
            self.link_offset,
            self.audio_format.sample_rate,
        )
    }

    pub fn rtp_buffer_size(&self) -> usize {
        formats::rtp_buffer_size(
            self.link_offset,
            self.packet_time,
            self.audio_format.sample_rate,
            self.audio_format.frame_format.channels,
            self.audio_format.frame_format.sample_format,
        )
    }

    pub fn to_link_offset(&self, samples: usize) -> usize {
        formats::to_link_offset(samples, self.audio_format.sample_rate)
    }
}
