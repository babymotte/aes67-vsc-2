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

use crate::formats::FramesPerSecond;
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, time::Duration};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioConfig {
    pub nic: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: FramesPerSecond,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub ptp: Option<PtpMode>,
    pub audio: AudioConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ptp: None,
            audio: AudioConfig::default(),
        }
    }
}

fn default_sample_rate() -> FramesPerSecond {
    48_000
}
