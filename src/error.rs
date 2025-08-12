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

use axum::{http::StatusCode, response::IntoResponse};
use miette::Diagnostic;
use opentelemetry_otlp::ExporterBuildError;
use rtp_rs::RtpReaderError;
use shared_memory::ShmemError;
use std::{fmt::Display, io};
use thiserror::Error;
use tokio::sync::oneshot;
use tracing_subscriber::{filter::ParseError, util::TryInitError};
use worterbuch_client::ConnectionError;

#[derive(Error, Debug, Diagnostic)]
pub enum Aes67Vsc2Error {
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] ConnectionError),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("YAML parse error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Tracing init error: {0}")]
    TryInitError(#[from] TryInitError),
    #[error("Tracing init error: {0}")]
    TraceError(#[from] ExporterBuildError),
    #[error("Tracing config parse error: {0}")]
    ParseError(#[from] ParseError),
    #[error("API error.")]
    ApiError(#[from] oneshot::error::RecvError),
    #[error("Unknown smaple format: {0}")]
    UnknownSampleFormat(String),
    #[error("Invalid SDP: {0}")]
    InvalidSdp(String),
    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),
    #[error("JSON serde error: {0}")]
    JsonSerdeError(#[from] serde_json::Error),
    #[error("Shared memory error: {0}")]
    SharedMemoryError(#[from] ShmemError),
    #[error("Received invalid RTP data: {0:?}")]
    InvalidRtpData(#[from] WrappedRtpError),
    #[error("General error: {0}")]
    Other(String),
}

#[derive(Error, Debug, Diagnostic)]
pub struct WrappedRtpError(pub RtpReaderError);

impl Display for WrappedRtpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl IntoResponse for Aes67Vsc2Error {
    // TODO differentiate between error causes
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{self}")).into_response()
    }
}

pub type Aes67Vsc2Result<T> = Result<T, Aes67Vsc2Error>;
