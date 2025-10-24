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
use rtp_rs::{RtpPacketBuildError, RtpReaderError};
use std::{
    fmt::{Debug, Display},
    io,
    net::AddrParseError,
};
use thiserror::Error;
use tokio::sync::{oneshot, watch};
use tracing::error;
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

#[derive(Error, Debug)]
#[error("Error in child app {0}: {1}")]
pub struct ChildAppError(pub String, pub String);

pub type ChildAppResult<T> = Result<T, ChildAppError>;

#[derive(Error, Debug, Diagnostic)]
pub enum VscApiError {
    #[error("Internal error: {0}")]
    Internal(#[from] Box<VscInternalError>),
    #[error("Sender error: {0}")]
    Sender(#[from] Box<SenderApiError>),
    #[error("Receiver error: {0}")]
    Receiver(#[from] Box<ReceiverApiError>),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum SenderApiError {
    #[error("Internal error: {0}")]
    Internal(#[from] Box<SenderInternalError>),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum ReceiverApiError {
    #[error("Internal error: {0}")]
    Internal(#[from] Box<ReceiverInternalError>),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum VscInternalError {
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
    #[error("Error in VSC child app: {0}")]
    ChildAppError(#[from] ChildAppError),
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] ConnectionError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum SenderInternalError {
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),
    #[error("Clock Error: {0}.")]
    ClockError(#[from] ClockError),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("RTP packet builder error: {0:?}")]
    RtpPacketBuildError(#[from] WrappedRtpPacketBuildError),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
    #[error(
        "Channel count mismatch: {configured} channels configured, but data for {provided} channels was provided."
    )]
    ChannelCountMismatch { configured: usize, provided: usize },
    #[error("RTP packet is too large: {0}. MTU is 1500.")]
    MaxMTUExceeded(usize),
    #[error("Producer closed.")]
    ProducerClosed,
    #[error("Shutdown triggered.")]
    ShutdownTriggered,
    #[error("Error in sender: {0}")]
    ChildAppError(#[from] ChildAppError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum ReceiverInternalError {
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),
    #[error("Clock Error: {0}.")]
    ClockError(#[from] ClockError),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("Channel error.")]
    ChannelError(#[from] oneshot::error::RecvError),
    #[error("Watch error.")]
    WatchError(#[from] watch::error::RecvError),
    #[error("Error in receiver: {0}")]
    ChildAppError(#[from] ChildAppError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum JackError {}

#[derive(Error, Debug, Diagnostic)]
pub enum AlsaError {}

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("YAML parse error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("Invalid SDP: {0}")]
    InvalidSdp(String),
    #[error("Invalid localIP: {0}")]
    InvalidLocalIP(String),
    #[error("Invalid IP address: {0}")]
    InvalidIp(#[from] AddrParseError),
    #[error("Unsupported sample format: {0}")]
    UnsupportedSampleFormat(String),
    #[error("NIC with specified IP not found: {0}")]
    NoSuchNIC(String),
    #[error("Receiver not configured")]
    MissingReceiverConfig,
    #[error("Clock error: {0}")]
    ClockError(#[from] ClockError),
    #[error("Backend not configured correctly: {0}")]
    BackendMisconfigured(String),
}

#[derive(Error, Debug, Diagnostic)]
#[error("PHC clock error: {0}")]
pub enum ClockError {
    #[error("Clock error: {0}")]
    ClockError(Box<dyn std::error::Error + 'static + Sync + Send>),
    #[error("I/O Error: {0}")]
    IoError(#[from] io::Error),
    #[error("NIC {0} does not support PTP")]
    PtpNotSupported(String),
}

impl ClockError {
    pub fn other<E>(e: E) -> Self
    where
        E: std::error::Error + Sync + Send + 'static,
    {
        ClockError::ClockError(Box::new(e))
    }
}

#[derive(Error, Debug, Diagnostic)]
pub enum Aes67Vsc2Error {
    #[error("I/O error: {0}")]
    IoError(#[from] Box<io::Error>),
    #[error("Config error: {0}")]
    ConfigError(#[from] Box<ConfigError>),
    #[error(transparent)]
    VscApiError(#[from] Box<VscApiError>),
    #[error("Sender API error: {0}")]
    SenderApiError(#[from] Box<SenderApiError>),
    #[error("Receiver API error: {0}")]
    ReceiverApiError(#[from] Box<ReceiverApiError>),
    #[error("Internal VSC error: {0}")]
    VscInternalError(#[from] Box<VscInternalError>),
    #[error("Internal Sender error: {0}")]
    SenderInternalError(#[from] Box<SenderInternalError>),
    #[error("Internal Receiver error: {0}")]
    ReceiverInternalError(#[from] Box<ReceiverInternalError>),
    #[error("Jack error: {0}")]
    JackError(#[from] Box<JackError>),
    #[error("Alsa error: {0}")]
    AlsaError(#[from] Box<AlsaError>),
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] Box<worterbuch_client::ConnectionError>),
    #[error("Error in child app{0}: {1}")]
    ChildAppError(String, Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Error, Debug, Diagnostic)]
pub enum DiscoveryError {
    #[error("SAP error: {0}")]
    SapError(#[from] sap_rs::error::Error),
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] ConnectionError),
}

pub type Aes67Vsc2Result<T> = Result<T, Aes67Vsc2Error>;
pub type VscApiResult<T> = Result<T, VscApiError>;
pub type SenderApiResult<T> = Result<T, SenderApiError>;
pub type ReceiverApiResult<T> = Result<T, ReceiverApiError>;
pub type VscInternalResult<T> = Result<T, VscInternalError>;
pub type SenderInternalResult<T> = Result<T, SenderInternalError>;
pub type ReceiverInternalResult<T> = Result<T, ReceiverInternalError>;
pub type JackResult<T> = Result<T, JackError>;
pub type AlsaResult<T> = Result<T, AlsaError>;
pub type ConfigResult<T> = Result<T, ConfigError>;
pub type ClockResult<T> = Result<T, ClockError>;
pub type DiscoveryResult<T> = Result<T, DiscoveryError>;

pub trait ToBoxed {
    fn boxed(self) -> Box<Self>;
}

impl<T: std::error::Error> ToBoxed for T {
    fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

pub trait ToBoxedResult<T, E: ToBoxed> {
    fn boxed(self) -> Result<T, Box<E>>;
}

impl<T, E: ToBoxed + std::error::Error> ToBoxedResult<T, E> for std::result::Result<T, E> {
    fn boxed(self) -> Result<T, Box<E>> {
        match self {
            Ok(it) => Ok(it),
            Err(err) => Err(err.boxed()),
        }
    }
}

impl From<SenderInternalError> for VscApiError {
    fn from(value: SenderInternalError) -> Self {
        VscApiError::Sender(SenderApiError::Internal(value.boxed()).boxed())
    }
}

impl From<ReceiverInternalError> for VscApiError {
    fn from(value: ReceiverInternalError) -> Self {
        VscApiError::Receiver(ReceiverApiError::Internal(value.boxed()).boxed())
    }
}

// #[derive(Error, Debug, Diagnostic)]
// pub enum Aes67Vsc2Error {
//     #[error("Worterbuch error: {0}")]
//     WorterbuchError(#[from] ConnectionError),
//     #[error("YAML parse error: {0}")]
//     YamlError(#[from] serde_yaml::Error),
//     #[error("Tracing init error: {0}")]
//     TryInitError(#[from] TryInitError),
//     #[error("Tracing init error: {0}")]
//     TraceError(#[from] ExporterBuildError),
//     #[error("Tracing config parse error: {0}")]
//     ParseError(#[from] ParseError),
//     #[error("API error.")]
//     ApiError(#[from] oneshot::error::RecvError),
//     #[error("Invalid SDP: {0}")]
//     InvalidSdp(String),
//     #[error("Invalid IP address: {0}")]
//     InvalidIp(#[from] AddrParseError),
//     #[error("JSON serde error: {0}")]
//     JsonSerdeError(#[from] serde_json::Error),
//     #[error("Shared memory error: {0}")]
//     SharedMemoryError(#[from] ShmemError),
//     #[error("Received invalid RTP data: {0:?}")]
//     InvalidRtpData(#[from] WrappedRtpError),
//     #[error("RTP packet builder error: {0:?}")]
//     RtpPacketBuildError(#[from] WrappedRtpPacketBuildError),
//     #[error("Jack error: {0:?}")]
//     JackError(#[from] jack::Error),
//     #[error("General error: {0}")]
//     Other(String),
// }

// impl Aes67Vsc2Error {
//     pub fn error_code(&self) -> u8 {
//         match self {
//             Aes67Vsc2Error::WorterbuchError(_) => ErrorCode::WorterbuchError as u8,
//             Aes67Vsc2Error::IoError(_) => ErrorCode::IoError as u8,
//             Aes67Vsc2Error::YamlError(_) => ErrorCode::YamlError as u8,
//             Aes67Vsc2Error::TryInitError(_) => ErrorCode::TryInitError as u8,
//             Aes67Vsc2Error::TraceError(_) => ErrorCode::TraceError as u8,
//             Aes67Vsc2Error::ParseError(_) => ErrorCode::ParseError as u8,
//             Aes67Vsc2Error::ApiError(_) => ErrorCode::ApiError as u8,
//             Aes67Vsc2Error::UnsupportedSampleFormat(_) => ErrorCode::UnknownSampleFormat as u8,
//             Aes67Vsc2Error::InvalidSdp(_) => ErrorCode::InvalidSdp as u8,
//             Aes67Vsc2Error::InvalidIp(_) => ErrorCode::InvalidIp as u8,
//             Aes67Vsc2Error::JsonSerdeError(_) => ErrorCode::JsonSerdeError as u8,
//             Aes67Vsc2Error::SharedMemoryError(_) => ErrorCode::SharedMemoryError as u8,
//             Aes67Vsc2Error::InvalidRtpData(_) => ErrorCode::InvalidRtpData as u8,
//             Aes67Vsc2Error::RtpPacketBuildError(_) => ErrorCode::RtpPacketBuildError as u8,
//             Aes67Vsc2Error::JackError(_) => ErrorCode::JackError as u8,
//             Aes67Vsc2Error::Other(_) => ErrorCode::Other as u8,
//         }
//     }
// }

pub trait GetErrorCode {
    fn error_code(&self) -> u8;
}

impl GetErrorCode for ConfigError {
    fn error_code(&self) -> u8 {
        todo!()
    }
}

impl GetErrorCode for VscApiError {
    fn error_code(&self) -> u8 {
        match self {
            VscApiError::Internal(e) => error!("{:?}", e),
            VscApiError::Sender(e) => error!("{:?}", e),
            VscApiError::Receiver(e) => error!("{:?}", e),
            VscApiError::ChannelError(e) => error!("{:?}", e),
        }
        // TODO
        3
    }
}

impl GetErrorCode for ReceiverApiError {
    fn error_code(&self) -> u8 {
        todo!()
    }
}

impl GetErrorCode for ReceiverInternalError {
    fn error_code(&self) -> u8 {
        todo!()
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
