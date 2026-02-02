use crate::engine::{RequiredBump, SemverCheckOutput};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level report for a semverguard run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    /// Semverguard version (from CLI crate).
    pub semverguard_version: String,
    /// ISO8601 start timestamp (UTC).
    pub started_at: String,
    /// ISO8601 finish timestamp (UTC).
    pub finished_at: String,
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// One entry per selected package.
    pub packages: Vec<PackageReport>,
    /// Summary counts and overall status.
    pub summary: Summary,
}

/// Summary of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Total number of package entries in the report (including skipped).
    pub total: usize,
    /// Number of packages that passed.
    pub passed: usize,
    /// Number of packages that failed.
    pub failed: usize,
    /// Number of packages that were skipped.
    pub skipped: usize,
}

impl Summary {
    /// True if no packages failed.
    pub fn overall_success(&self) -> bool {
        self.failed == 0
    }
}

/// Per-package result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReport {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: String,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Status classification.
    pub status: PackageStatus,
    /// Why this package was skipped (when status is `skipped`).
    pub skip_reason: Option<String>,
    /// How long the package check took (milliseconds).
    pub duration_ms: u128,
    /// Command executed by the engine (argv form).
    pub command: Vec<String>,
    /// Captured engine output (when executed).
    pub engine: Option<SemverCheckOutput>,
    /// Best-effort required bump inference.
    pub inferred_required_bump: Option<RequiredBump>,
}

/// Result classification for a package.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageStatus {
    /// Passed the SemVer check.
    Passed,
    /// Failed the SemVer check.
    Failed,
    /// Skipped by semverguard policy/scoping.
    Skipped,
}
