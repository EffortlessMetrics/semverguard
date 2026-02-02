use crate::{BaselineConfig, FeaturesConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Request to run a SemVer check for a package.
#[derive(Debug, Clone)]
pub struct SemverCheckRequest {
    /// Workspace root where cargo should be executed.
    pub workspace_root: PathBuf,

    /// Which `cargo` binary to run. If unset, uses `"cargo"` from PATH.
    pub cargo_bin: Option<PathBuf>,

    /// Path to the package manifest to check.
    pub manifest_path: PathBuf,

    /// Baseline selection options.
    pub baseline: BaselineConfig,

    /// Feature-selection options.
    pub features: FeaturesConfig,

    /// Extra args appended to the engine invocation.
    pub extra_args: Vec<String>,

    /// Optional timeout.
    pub timeout: Option<Duration>,
}

/// Output from running a SemVer check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemverCheckOutput {
    /// Exit code from the process.
    pub exit_code: Option<i32>,
    /// True if the process exited successfully.
    pub success: bool,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Best-effort required bump inference from output.
    pub required_bump: Option<RequiredBump>,
}

/// Required version bump inferred from the tool output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequiredBump {
    /// Patch bump required.
    Patch,
    /// Minor bump required.
    Minor,
    /// Major bump required.
    Major,
    /// Could not infer required bump.
    Unknown,
}

impl RequiredBump {
    /// Infer a bump from cargo-semver-checks output (best-effort heuristic).
    pub fn infer(text: &str) -> Option<Self> {
        let hay = text.to_ascii_lowercase();
        // cargo-semver-checks output tends to include phrases like:
        // "major version bump required" / "minor version bump required"
        if hay.contains("major") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Major);
        }
        if hay.contains("minor") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Minor);
        }
        if hay.contains("patch") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Patch);
        }
        if hay.contains("bump required") {
            return Some(Self::Unknown);
        }
        None
    }
}
