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
}

/// Result type alias for dupfinder operations.
pub type Result<T> = std::result::Result<T, DupfinderError>;
