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
            audio_format: value.audio_format.clone(),
            payload_type: value.payload_type,
            channel_labels: labels,
        })
    }
}
