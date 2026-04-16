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

use crate::error::{ManagementAgentError, ManagementAgentResult};
use aes67_rs::config::TelemetryConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};
use tokio::fs;

pub const DEFAULT_PORT: u16 = 43567;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, value_name = "FILE", env = "AES67_VSC_CONFIG")]
    pub config: PathBuf,

    #[arg(short, long, value_name = "PATH", env = "AES67_VSC_DATA_DIR")]
    pub data_dir: PathBuf,
}

impl Args {
    pub fn get() -> Self {
        Self::parse()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub web_ui: WebUiConfig,
    pub telemetry: Option<TelemetryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiConfig {
    pub bind_address: Option<IpAddr>,
    pub port: u16,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            bind_address: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            port: DEFAULT_PORT,
        }
    }
}

impl AppConfig {
    pub async fn load(path: impl AsRef<Path>) -> ManagementAgentResult<Self> {
        match fs::read(path.as_ref()).await {
            Ok(contents) => {
                let config = serde_yaml::from_slice(&contents).map_err(|e| {
                    ManagementAgentError::YamlError(path.as_ref().to_string_lossy().to_string(), e)
                })?;
                Ok(config)
            }
            Err(e) => {
                eprintln!(
                    "Could not load config file {}: {}; using default config",
                    path.as_ref().display(),
                    e
                );
                let default = AppConfig {
                    web_ui: Default::default(),
                    telemetry: Default::default(),
                };
                Ok(default)
            }
        }
    }
}
