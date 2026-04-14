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

use miette::Diagnostic;
use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum Error {
    #[error("I/O error writing config file: {0}")]
    ConfigFileWriteError(io::Error),
    #[error("Failed to spawn ptp4l: {0}")]
    SpawnError(io::Error),
    #[error("ptp4l exited unexpectedly: {0}")]
    UnexpectedExit(ExitStatus),
    #[error("Configuration directory not found")]
    ConfigDirNotFound,
    #[error("Runtime directory not found")]
    RuntimeDirNotFound,
    #[error("Failed to create configuration directory: {0}")]
    ConfigDirCreateError(io::Error),
    #[error("Failed to create UDS directory: {0}")]
    UdsDirCreateError(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
