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

use crate::error::ManagementAgentResult;
use aes67_rs::config::TelemetryConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    pub port: u16,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT }
    }
}

impl AppConfig {
    pub async fn load(path: impl AsRef<Path>) -> ManagementAgentResult<Self> {
        match fs::read(path).await {
            Ok(contents) => {
                let config = serde_yaml::from_slice(&contents)?;
                Ok(config)
            }
            Err(e) => {
                eprintln!("Could not load config: {e}; using default config");
                let default = AppConfig {
                    web_ui: Default::default(),
                    telemetry: Default::default(),
                };
                Ok(default)
            }
        }
    }
}
