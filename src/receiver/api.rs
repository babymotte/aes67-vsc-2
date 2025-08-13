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
