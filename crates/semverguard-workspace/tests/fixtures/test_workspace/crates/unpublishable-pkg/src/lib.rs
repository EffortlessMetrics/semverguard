//! An unpublishable library package for testing.

/// A simple function.
pub fn internal_only() -> &'static str {
    "This package is not published"
}
