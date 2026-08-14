//! Error types for dupfinder-core.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during dupfinder operations.
#[derive(Error, Debug)]
pub enum DupfinderError {
    /// An I/O error occurred while accessing a file.
    #[error("I/O error at {path}: {source}")]
    IoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// An error occurred with the cache.
    #[error("Cache error: {0}")]
    CacheError(String),

    /// An error occurred with glob pattern compilation.
    #[error("Invalid glob pattern: {0}")]
    GlobError(#[from] globset::Error),

    /// An error occurred during serialization/deserialization.
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    /// A configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// A safety protection error preventing operations on protected/system paths.
    #[error("Safety protection error at '{path}': {reason}")]
    ProtectedPathError { path: PathBuf, reason: String },

    /// An original preservation invariant error.
    #[error("Original preservation violation: {0}")]
    OriginalPreservationError(String),

    /// A TOCTOU / file-state-changed error.
    #[error("File state changed since scan at '{path}': {reason}")]
    StateChangedError { path: PathBuf, reason: String },

    /// A general remediation or cleanup error.
    #[error("Remediation error: {0}")]
    RemediationError(String),
}

/// Result type alias for dupfinder operations.
pub type Result<T> = std::result::Result<T, DupfinderError>;
