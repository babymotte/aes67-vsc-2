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

use crate::{error::ConfigResult, receiver::config::ReceiverConfig, sender::config::SenderConfig};
use clap::Parser;
use gethostname::gethostname;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PtpMode {
    /// System mode is used when there is an external PTP daemon running on this machine that synchronizes the
    /// system TAI clock to a PTP master or the PTP master or that acts as a PTP master itself and uses the
    /// system TAI clock as source.
    /// This mode is useful if other applications on the same machine also need PTP time but there is no NIC that
    /// provides a PHC, which is often the case on laptops and consumer PCs.
    /// On desktop/general purpose devices it may not be desirable to synchronize the system time to a PTP master
    /// since the PTP master may use an arbitrary timescale.
    #[default]
    System,
    /// PHC mode is used when there is an external PTP daemon running in salve-only mode that synchronizes
    /// the PHC of the given network interface to a PTP master, potentially without synchronizing the system
    /// clock to the PHC.
    /// This mode is useful if other applications on the same machine also need PTP time but it is not acceptable
    /// to synchronize the system clock to the PTP master. Its downside is that it requires a NIC that provides a
    /// PHC, which is usually not the case on consumer hardware.
    Phc { nic: String },
    #[cfg(feature = "statime")]
    /// Internal mode is used when there is no external PTP daemon running. The application will start its own
    /// internal slave-only PTP client to provide a clock that is synchronized to a PTP master.
    /// This mode is useful if it is not acceptable to synchronize the system clock to the PTP master and none
    /// of the machine's NICs provides a PHC or if running an external PTP daemon is not desired. Its downside
    /// is that it requires exclusive access to the default PTP port, so no other applications on the same machine
    /// can use PTP at the same time.
    Internal { nic: String },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "AppConfig::default")]
    pub app: AppConfig,
    #[serde(default)]
    pub telemetry: Option<TelemetryConfig>,
    #[serde(default)]
    pub ptp: Option<PtpMode>,
    #[serde(default)]
    pub receivers: Vec<ReceiverConfig>,
    #[serde(default)]
    pub senders: Vec<SenderConfig>,
}

impl Config {
    #[instrument]
    pub fn load() -> ConfigResult<Config> {
        let args = Args::parse();

        info!("Loading config …");

        let config = Config::load_from_file(args.config.as_deref())?;

        Ok(config)
    }

    #[instrument]
    fn load_from_file(path: Option<&Path>) -> ConfigResult<Config> {
        match path {
            Some(path) => {
                let content = fs::read_to_string(path)?;
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
                match fs::read_to_string(path) {
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
