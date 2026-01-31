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
