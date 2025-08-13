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
