use std::io;

use miette::Diagnostic;
use thiserror::Error;
use worterbuch::error::WorterbuchAppError;
use worterbuch_client::ConnectionError;

#[derive(Error, Debug, Diagnostic)]
pub enum WebUIError {
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

pub type WebUIResult<T> = Result<T, WebUIError>;
