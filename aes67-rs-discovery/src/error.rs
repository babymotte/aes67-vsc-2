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

use std::time::SystemTimeError;

use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::oneshot;
use worterbuch_client::ConnectionError;

#[derive(Error, Debug, Diagnostic)]
pub enum DiscoveryError {
    #[error("SAP error: {0}")]
    SapError(#[from] sap_rs::error::Error),
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] ConnectionError),
    #[error("System time error: {0}")]
    SystemTimeError(#[from] SystemTimeError),
    #[error("No session with ID '{0}' found")]
    NoSuchSession(String),
    #[error("Channel closed")]
    ChannelError(#[from] oneshot::error::RecvError),
}

pub type DiscoveryResult<T> = Result<T, DiscoveryError>;
