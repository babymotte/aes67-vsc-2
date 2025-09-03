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

use crate::{error::ConfigResult, receiver::config::ReceiverConfig};
use clap::Parser;
use gethostname::gethostname;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::fs;
use tracing::{info, instrument, warn};

#[derive(Parser)]
#[command(author, version, about, long_about)]
pub struct Args {
    /// Path to config file
    #[arg(short, long, env = "AES67_VSC_2_CONFIG")]
    config: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebServerConfig {
    pub bind_address: IpAddr,
    pub port: u16,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            bind_address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 3000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryConfig {
    pub endpoint: EndpointConfig,
    pub credentials: Option<Credentials>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EndpointConfig {
    Grpc(String),
    Http(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub user: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub name: String,
    pub instance: InstanceConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "aes67-vsc-2".to_owned(),
            instance: InstanceConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub name: String,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            name: gethostname().to_string_lossy().to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketConfig {
    pub bind_address: IpAddr,
    pub port: u16,
    #[serde(default, with = "serde_millis")]
    pub keepalive_time: Option<Duration>,
    #[serde(default, with = "serde_millis")]
    pub keepalive_interval: Option<Duration>,
    pub keepalive_retries: Option<u32>,
    #[serde(default, with = "serde_millis")]
    pub user_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "AppConfig::default")]
    pub app: AppConfig,
    #[serde(default)]
    pub telemetry: Option<TelemetryConfig>,
    #[serde(default)]
    pub receiver_config: Option<ReceiverConfig>,
    pub interface_ip: IpAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: Default::default(),
            telemetry: Default::default(),
            receiver_config: Default::default(),
            interface_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }
}

impl Config {
    #[instrument]
    pub async fn load() -> ConfigResult<Config> {
        let args = Args::parse();

        info!("Loading config …");

        let config = Config::load_from_file(args.config.as_deref()).await?;

        Ok(config)
    }

    #[instrument]
    async fn load_from_file(path: Option<&Path>) -> ConfigResult<Config> {
        match path {
            Some(path) => {
                let content = fs::read_to_string(&path).await?;
                let config = serde_yaml::from_str(&content)?;
                info!("Config loaded from {}", path.to_string_lossy());
                Ok(config)
            }
            None => {
                let path = if cfg!(debug_assertions) {
                    let it = "./config-dev.yaml";
                    warn!("No config file specified, using {it}");
                    it
                } else {
                    let it = "/etc/aes67-vsc-2/config.yaml";
                    warn!("No config file specified, using {it}");
                    it
                };
                match fs::read_to_string(path).await {
                    Ok(it) => {
                        let config = serde_yaml::from_str(&it)?;
                        info!("Config loaded from {path}");
                        Ok(config)
                    }
                    Err(_) => {
                        warn!("Could not read config file {path}, using default config.");
                        Ok(Config::default())
                    }
                }
            }
        }
    }

    pub fn instance_name(&self) -> String {
        format!("{}/{}", self.app.name, self.app.instance.name)
    }
}
