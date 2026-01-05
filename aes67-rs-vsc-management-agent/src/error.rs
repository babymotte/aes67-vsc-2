use std::io;

use axum::{http::StatusCode, response::IntoResponse};
use miette::Diagnostic;
use thiserror::Error;
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
        }
    }
}
