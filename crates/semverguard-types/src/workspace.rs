use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A simplified view of `cargo metadata` for semverguard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Workspace packages eligible for selection.
    pub packages: Vec<WorkspacePackage>,
}

/// A package inside the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePackage {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: Version,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Directory containing the package's `Cargo.toml`.
    pub package_root: PathBuf,
    /// Whether the package is publishable (`publish != false`).
    pub publishable: bool,
    /// Whether the package has a library target.
    pub has_lib: bool,
}
