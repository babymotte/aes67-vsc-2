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

pub mod config;
pub mod error;

use crate::config::Config;
use std::path::{Path, PathBuf};
use tosub::SubsystemHandle;
use tracing::info;

pub async fn start_ptpt4l(
    subsys: &SubsystemHandle,
    executable_path: Option<impl Into<PathBuf>>,
    config: Config,
    interface: String,
    config_dir: impl Into<PathBuf>,
) -> error::Result<SubsystemHandle> {
    let executable_path = executable_path.map(Into::into);
    let config_dir = config_dir.into();

    let subsys = subsys.spawn("ptp4l", |s| {
        run_ptp4l(s, executable_path, config, interface, config_dir)
    });

    Ok(subsys)
}

async fn run_ptp4l(
    subsys: SubsystemHandle,
    executable_path: Option<PathBuf>,
    config: Config,
    interface: String,
    config_dir: PathBuf,
) -> error::Result<()> {
    info!("Starting ptp4l …");

    let config_file = write_config(&config, &interface, &config_dir).await?;
    info!("Wrote config file to {}", config_file.display());

    let mut child = tokio::process::Command::new(executable_path.unwrap_or_else(|| "ptp4l".into()))
        .arg("-f")
        .arg(&config_file)
        .arg("-i")
        .arg(&interface)
        .spawn()
        .map_err(error::Error::SpawnError)?;

    info!("ptp4l running (pid {})", child.id().unwrap_or(0));

    tokio::select! {
        _ = subsys.shutdown_requested() => {
            info!("Shutdown requested, stopping ptp4l …");
            child.kill().await.ok();
            child.wait().await.ok();
        }
        result = child.wait() => {
            let status = result.map_err(error::Error::SpawnError)?;
            if !status.success() {
                return Err(error::Error::UnexpectedExit(status));
            }
        }
    }

    Ok(())
}

async fn write_config(
    config: &Config,
    interface: &str,
    config_dir: &Path,
) -> error::Result<PathBuf> {
    let config_file = config_dir.join(format!("{}.conf", interface));
    tokio::fs::create_dir_all(config_dir)
        .await
        .map_err(error::Error::ConfigDirCreateError)?;

    if let Some(uds_address) = &config.global.uds_address {
        if let Some(parent) = uds_address.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(error::Error::UdsDirCreateError)?;
        }
        tokio::fs::remove_file(&uds_address).await.ok();
    }

    let contents = config
        .display_with_interfaces(&[interface.to_string()])
        .to_string();

    tokio::fs::write(&config_file, contents)
        .await
        .map_err(error::Error::ConfigFileWriteError)?;

    Ok(config_file)
}
