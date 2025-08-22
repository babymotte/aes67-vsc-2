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
use rtp_rs::{RtpPacketBuildError, RtpReaderError};
use shared_memory::ShmemError;
use std::{fmt::Display, io, net::AddrParseError};
use thiserror::Error;
use tokio::sync::oneshot;
use tracing_subscriber::{filter::ParseError, util::TryInitError};
use worterbuch_client::ConnectionError;

pub enum ErrorCode {
    WorterbuchError = 0x10,
    IoError = 0x11,
    YamlError = 0x12,
    TryInitError = 0x13,
    TraceError = 0x14,
    ParseError = 0x15,
    ApiError = 0x16,
    UnknownSampleFormat = 0x17,
    InvalidSdp = 0x18,
    InvalidIp = 0x19,
    JsonSerdeError = 0x1A,
    SharedMemoryError = 0x1B,
    InvalidRtpData = 0x1C,
    RtpPacketBuildError = 0x1D,
    JackError = 0x1E,
    Other = 0x1F,
}

// TODO split into receiver error, sender error, etc and remove any unused error variants
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
    UnsupportedSampleFormat(String),
    #[error("Invalid SDP: {0}")]
    InvalidSdp(String),
    #[error("Invalid IP address: {0}")]
    InvalidIp(#[from] AddrParseError),
    #[error("JSON serde error: {0}")]
    JsonSerdeError(#[from] serde_json::Error),
    #[error("Shared memory error: {0}")]
    SharedMemoryError(#[from] ShmemError),
    #[error("Received invalid RTP data: {0:?}")]
    InvalidRtpData(#[from] WrappedRtpError),
    #[error("RTP packet builder error: {0:?}")]
    RtpPacketBuildError(#[from] WrappedRtpPacketBuildError),
    #[error("Jack error: {0:?}")]
    JackError(#[from] jack::Error),
    #[error("General error: {0}")]
    Other(String),
}

impl Aes67Vsc2Error {
    pub fn error_code(&self) -> u8 {
        match self {
            Aes67Vsc2Error::WorterbuchError(_) => ErrorCode::WorterbuchError as u8,
            Aes67Vsc2Error::IoError(_) => ErrorCode::IoError as u8,
            Aes67Vsc2Error::YamlError(_) => ErrorCode::YamlError as u8,
            Aes67Vsc2Error::TryInitError(_) => ErrorCode::TryInitError as u8,
            Aes67Vsc2Error::TraceError(_) => ErrorCode::TraceError as u8,
            Aes67Vsc2Error::ParseError(_) => ErrorCode::ParseError as u8,
            Aes67Vsc2Error::ApiError(_) => ErrorCode::ApiError as u8,
            Aes67Vsc2Error::UnsupportedSampleFormat(_) => ErrorCode::UnknownSampleFormat as u8,
            Aes67Vsc2Error::InvalidSdp(_) => ErrorCode::InvalidSdp as u8,
            Aes67Vsc2Error::InvalidIp(_) => ErrorCode::InvalidIp as u8,
            Aes67Vsc2Error::JsonSerdeError(_) => ErrorCode::JsonSerdeError as u8,
            Aes67Vsc2Error::SharedMemoryError(_) => ErrorCode::SharedMemoryError as u8,
            Aes67Vsc2Error::InvalidRtpData(_) => ErrorCode::InvalidRtpData as u8,
            Aes67Vsc2Error::RtpPacketBuildError(_) => ErrorCode::RtpPacketBuildError as u8,
            Aes67Vsc2Error::JackError(_) => ErrorCode::JackError as u8,
            Aes67Vsc2Error::Other(_) => ErrorCode::Other as u8,
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
pub struct WrappedRtpError(pub RtpReaderError);

impl Display for WrappedRtpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

#[derive(Error, Debug, Diagnostic)]
pub struct WrappedRtpPacketBuildError(pub RtpPacketBuildError);

impl Display for WrappedRtpPacketBuildError {
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
