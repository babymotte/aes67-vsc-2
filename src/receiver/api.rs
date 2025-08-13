use crate::{error::Aes67Vsc2Result, receiver::config::RxDescriptor};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, net::SocketAddr};
use tokio::sync::oneshot;
use tracing::instrument;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverInfo {
    pub shmem_address: String,
    pub descriptor: RxDescriptor,
}

impl Display for ReceiverInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_yaml::to_string(self).unwrap_or_else(|_| "<invalid yaml>".to_owned())
        )
    }
}
#[derive(Debug)]
pub enum ReceiverApiMessage {
    Stop,
    GetInfo(oneshot::Sender<ReceiverInfo>),
}

#[derive(Debug, Clone)]
pub struct ReceiverApi {
    url: String,
    reqwest_client: Client,
}

impl ReceiverApi {
    pub fn with_socket_addr(addr: SocketAddr, use_tls: bool) -> Self {
        let schema = if use_tls { "https" } else { "http" };
        let url = format!("{}://{}", schema, addr);
        ReceiverApi {
            url,
            reqwest_client: Client::new(),
        }
    }

    pub fn with_url(url: String) -> Self {
        ReceiverApi {
            url,
            reqwest_client: Client::new(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    #[instrument(ret, err)]
    pub async fn stop(&self) -> Aes67Vsc2Result<bool> {
        let body = self
            .reqwest_client
            .post(format!("{}/stop", self.url))
            .send()
            .await?
            .text()
            .await?;
        Ok(serde_json::from_str(&body)?)
    }

    #[instrument(ret, err)]
    pub async fn info(&self) -> Aes67Vsc2Result<ReceiverInfo> {
        let body = self
            .reqwest_client
            .get(format!("{}/info", self.url))
            .send()
            .await?
            .text()
            .await?;
        Ok(serde_json::from_str(&body)?)
    }
}
