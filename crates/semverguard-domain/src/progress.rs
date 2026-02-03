//! Progress tracking types for semverguard operations.
//!
//! This module provides the types and traits needed for progress reporting
//! during semver checks. The CLI can implement these traits to display
//! progress information to users.

use semverguard_types::PackageStatus;

/// Events that can be reported during a semverguard run.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// The total number of packages to check is known.
    TotalPackages {
        /// Total number of packages that will be checked (after filtering).
        total: usize,
    },
    /// A package check is starting.
    PackageStarted {
        /// Package name.
        name: String,
        /// Current index (0-based).
        index: usize,
        /// Total packages to check.
        total: usize,
    },
    /// A package check completed.
    PackageCompleted {
        /// Package name.
        name: String,
        /// Result status.
        status: PackageStatus,
        /// Duration in milliseconds.
        duration_ms: u128,
    },
    /// A package was skipped (no check needed).
    PackageSkipped {
        /// Package name.
        name: String,
        /// Reason for skipping.
        reason: String,
    },
    /// All checks completed.
    Finished {
        /// Total passed.
        passed: usize,
        /// Total failed.
        failed: usize,
        /// Total skipped.
        skipped: usize,
    },
}

/// Trait for progress callbacks.
///
/// Implementations receive progress events during a semverguard run and can
/// display them to the user (e.g., with a progress bar or spinner).
pub trait ProgressCallback: Send + Sync {
    /// Called when a progress event occurs.
    fn on_progress(&self, event: ProgressEvent);
}

/// A no-op progress callback that does nothing.
///
/// Used as the default when no progress reporting is needed.
pub struct NoopProgressCallback;

impl ProgressCallback for NoopProgressCallback {
    fn on_progress(&self, _event: ProgressEvent) {
        // Do nothing
    }
}

/// A progress callback implemented as a boxed closure.
pub struct FnProgressCallback<F>(pub F)
where
    F: Fn(ProgressEvent) + Send + Sync;

impl<F> ProgressCallback for FnProgressCallback<F>
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn on_progress(&self, event: ProgressEvent) {
        (self.0)(event)
    }
}
