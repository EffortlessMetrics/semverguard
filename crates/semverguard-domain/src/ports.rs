use crate::Result;
use semverguard_types::{SemverCheckOutput, SemverCheckRequest, WorkspaceMetadata};
use std::path::{Path, PathBuf};

/// Provides workspace/package information.
pub trait WorkspaceProvider {
    /// Load workspace metadata rooted at `workspace_root`.
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata>;
}

/// Provides git-based operations for scoping and baseline selection.
pub trait GitProvider {
    /// List changed paths between `base` and `head` (typically HEAD).
    ///
    /// Returned paths should be relative to `workspace_root`.
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>>;
}

/// Runs semver checks for a package.
pub trait SemverEngine {
    /// Execute the check for one package.
    fn check(&self, request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)>;
}
