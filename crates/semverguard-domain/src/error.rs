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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Display implementation tests
    // =========================================================================

    #[test]
    fn test_workspace_error_display() {
        let err = SemverguardError::Workspace("failed to load metadata".to_string());
        assert_eq!(format!("{err}"), "workspace error: failed to load metadata");
    }

    #[test]
    fn test_git_error_display() {
        let err = SemverguardError::Git("git diff failed".to_string());
        assert_eq!(format!("{err}"), "git error: git diff failed");
    }

    #[test]
    fn test_engine_error_display() {
        let err = SemverguardError::Engine("cargo-semver-checks not found".to_string());
        assert_eq!(
            format!("{err}"),
            "engine error: cargo-semver-checks not found"
        );
    }

    #[test]
    fn test_invalid_config_error_display() {
        let err = SemverguardError::InvalidConfig("missing baseline rev".to_string());
        assert_eq!(format!("{err}"), "invalid config: missing baseline rev");
    }

    #[test]
    fn test_io_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = SemverguardError::Io(io_err);
        assert!(format!("{err}").contains("io error:"));
        assert!(format!("{err}").contains("file not found"));
    }

    #[test]
    fn test_utf8_error_display() {
        let invalid_bytes = vec![0xff, 0xfe];
        let utf8_err = String::from_utf8(invalid_bytes).unwrap_err();
        let err = SemverguardError::Utf8(utf8_err);
        assert!(format!("{err}").contains("utf-8 error:"));
    }

    #[test]
    fn test_missing_path_error_display() {
        let err = SemverguardError::MissingPath(PathBuf::from("/some/missing/path"));
        assert_eq!(format!("{err}"), "missing path: /some/missing/path");
    }

    // =========================================================================
    // From conversion tests
    // =========================================================================

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: SemverguardError = io_err.into();
        assert!(matches!(err, SemverguardError::Io(_)));
        assert!(format!("{err}").contains("access denied"));
    }

    #[test]
    fn test_from_utf8_error() {
        let invalid_bytes = vec![0x80, 0x81];
        let utf8_err = String::from_utf8(invalid_bytes).unwrap_err();
        let err: SemverguardError = utf8_err.into();
        assert!(matches!(err, SemverguardError::Utf8(_)));
    }

    // =========================================================================
    // Debug trait tests
    // =========================================================================

    #[test]
    fn test_error_debug() {
        let err = SemverguardError::Workspace("test error".to_string());
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("Workspace"));
        assert!(debug_str.contains("test error"));
    }

    // =========================================================================
    // Result alias tests
    // =========================================================================

    #[test]
    fn test_result_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_err() {
        let result: Result<i32> = Err(SemverguardError::InvalidConfig("bad config".to_string()));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SemverguardError::InvalidConfig(_)
        ));
    }

    // =========================================================================
    // Error source chain tests
    // =========================================================================

    #[test]
    fn test_io_error_source() {
        use std::error::Error;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "original error");
        let err = SemverguardError::Io(io_err);
        // The source should be the underlying io::Error
        assert!(err.source().is_some());
    }

    #[test]
    fn test_utf8_error_source() {
        use std::error::Error;
        let invalid_bytes = vec![0xff];
        let utf8_err = String::from_utf8(invalid_bytes).unwrap_err();
        let err = SemverguardError::Utf8(utf8_err);
        // The source should be the underlying FromUtf8Error
        assert!(err.source().is_some());
    }

    #[test]
    fn test_simple_error_no_source() {
        use std::error::Error;
        let err = SemverguardError::Workspace("test".to_string());
        // Simple string errors have no underlying source
        assert!(err.source().is_none());
    }
}
