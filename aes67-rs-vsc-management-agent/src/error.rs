use std::io;

use aes67_rs::error::{ClockError, ConfigError, VscApiError};
use aes67_rs_discovery::error::DiscoveryError;
use axum::{http::StatusCode, response::IntoResponse};
use miette::{Diagnostic, Report};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use worterbuch::error::WorterbuchAppError;
use worterbuch_client::ConnectionError;

#[derive(Error, Debug, Diagnostic)]
pub enum ManagementAgentError {
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("Worterbuch client error: {0}")]
    WorterbuchClientError(#[from] ConnectionError),
    #[error("Worterbuch error: {0}")]
    WorterbuchError(#[from] WorterbuchAppError),
    #[error("Worterbuch config error: {0}")]
    WbConfigError(#[from] worterbuch::common::error::ConfigError),
    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("SDP error: {0}")]
    SdpError(#[from] sdp::Error),
    #[error("VSC failed to start: {0}")]
    FailedToStart(Report),
    #[error("VSC is already running")]
    AlreadyRunning,
    #[error("Internal communication error")]
    ChannelError,
    #[error("VSC api error: {0}")]
    VscApiError(#[from] VscApiError),
    #[error("Clock error: {0}")]
    ClockError(#[from] ClockError),
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),
    #[error("I/O Handler error: {0}")]
    IoHandlerError(#[from] IoHandlerError),
    #[error("Discovery error: {0}")]
    DiscoveryError(#[from] DiscoveryError),
}

impl From<oneshot::error::RecvError> for ManagementAgentError {
    fn from(_: oneshot::error::RecvError) -> Self {
        ManagementAgentError::ChannelError
    }
}

impl<T> From<mpsc::error::SendError<T>> for ManagementAgentError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        ManagementAgentError::ChannelError
    }
}

pub type ManagementAgentResult<T> = Result<T, ManagementAgentError>;

impl IntoResponse for ManagementAgentError {
    fn into_response(self) -> axum::response::Response {
        let err: (StatusCode, String) = self.into();
        err.into_response()
    }
}

impl From<ManagementAgentError> for (StatusCode, String) {
    fn from(e: ManagementAgentError) -> Self {
        match &e {
            ManagementAgentError::IoError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ManagementAgentError::WorterbuchClientError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::WorterbuchError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::WbConfigError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::YamlError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::SdpError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ManagementAgentError::FailedToStart(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::AlreadyRunning => (StatusCode::BAD_REQUEST, e.to_string()),
            ManagementAgentError::ChannelError => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::VscApiError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::ClockError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::ConfigError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::IoHandlerError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            ManagementAgentError::DiscoveryError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        }
    }
}

pub trait LogError<T, E> {
    fn log_error(self, context: &'static str) -> Result<T, E>;
}

impl<T, E> LogError<T, E> for Result<T, E>
where
    E: std::fmt::Debug + std::fmt::Display,
{
    fn log_error(self, context: &'static str) -> Result<T, E> {
        match &self {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("{}: {}", context, e);
            }
        }
        self
    }
}

#[derive(Error, Debug, Diagnostic)]
#[error("{0}")]
pub struct IoHandlerError(Report);

impl From<Report> for IoHandlerError {
    fn from(report: Report) -> Self {
        IoHandlerError(report)
    }
}

pub type IoHandlerResult<T> = Result<T, IoHandlerError>;
