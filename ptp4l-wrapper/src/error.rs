use std::{io, process::ExitStatus};

use miette::Diagnostic;

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
