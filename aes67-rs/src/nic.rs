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

use crate::error::{ConfigError, ConfigResult};
use pnet::datalink::{self, NetworkInterface};
use std::{io, net::IpAddr, path::PathBuf, process::Command};

/// Returns Some("/dev/ptpX") if the interface has a PHC, None if not
pub fn phc_device_for_interface_ethtool(iface: &NetworkInterface) -> io::Result<Option<PathBuf>> {
    let output = Command::new("ethtool")
        .arg("-T")
        .arg(&iface.name)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "ethtool failed for {}",
            iface.name
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(idx_str) = line.strip_prefix("Hardware timestamp provider index:") {
            let idx_str = idx_str.trim();
            if let Ok(idx) = idx_str.parse::<u32>() {
                return Ok(Some(PathBuf::from(format!("/dev/ptp{}", idx))));
            }
        }
    }

    // No PHC index found
    Ok(None)
}

pub fn find_ptp_interfaces() -> Vec<NetworkInterface> {
    let mut out = Vec::new();
    for iface in datalink::interfaces() {
        if let Ok(Some(_)) = phc_device_for_interface_ethtool(&iface) {
            out.push(iface);
        }
    }
    out
}

pub fn find_nic_with_name(name: &String) -> ConfigResult<NetworkInterface> {
    for iface in datalink::interfaces() {
        if &iface.name == name {
            return Ok(iface);
        }
    }

    Err(ConfigError::NoSuchNIC(name.to_owned()))
}

pub fn find_nic_for_ip(ip: IpAddr) -> ConfigResult<NetworkInterface> {
    for iface in datalink::interfaces() {
        for ipn in &iface.ips {
            if ipn.ip() == ip {
                return Ok(iface);
            }
        }
    }

    Err(ConfigError::NoSuchNIC(ip.to_string()))
}
