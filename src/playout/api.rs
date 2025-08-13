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

use crate::error::Aes67Vsc2Result;
use reqwest::Client;
use std::net::SocketAddr;
use tracing::instrument;

#[derive(Debug)]
pub enum PlayoutApiMessage {
    Stop,
}

#[derive(Debug, Clone)]
pub struct PlayoutApi {
    addr: SocketAddr,
    use_tls: bool,
    reqwest_client: Client,
}

impl PlayoutApi {
    pub fn new(addr: SocketAddr, use_tls: bool) -> Self {
        PlayoutApi {
            addr,
            use_tls,
            reqwest_client: Client::new(),
        }
    }

    pub fn url(&self) -> String {
        let schema = if self.use_tls { "https" } else { "http" };
        format!("{}://{}", schema, self.addr)
    }

    #[instrument(ret, err)]
    pub async fn stop(&self) -> Aes67Vsc2Result<bool> {
        let body = self
            .reqwest_client
            .post(format!("{}/stop", self.url()))
            .send()
            .await?
            .text()
            .await?;
        Ok(serde_json::from_str(&body)?)
    }
}
