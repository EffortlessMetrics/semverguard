use std::path::PathBuf;
use thiserror::Error;

/// A semverguard error.
#[derive(Debug, Error)]
pub enum SemverguardError {
    /// Error from workspace provider.
    #[error("workspace error: {0}")]
    Workspace(String),

    /// Error from git provider.
    #[error("git error: {0}")]
    Git(String),

    /// Error from engine.
    #[error("engine error: {0}")]
    Engine(String),

    /// Configuration is invalid or incomplete.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// IO error wrapper.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// UTF-8 decoding error wrapper.
    #[error("utf-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// A path we expected to exist did not.
    #[error("missing path: {0}")]
    MissingPath(PathBuf),
}

/// Domain result alias.
pub type Result<T> = std::result::Result<T, SemverguardError>;
