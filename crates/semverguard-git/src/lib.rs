#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-git
//!
//! Adapter over the `git` CLI to support "changed crates" scoping.

use semverguard_domain::{GitProvider, Result, SemverguardError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git client backed by the `git` executable on PATH (or a configured path).
#[derive(Debug, Clone)]
pub struct GitCli {
    git_bin: PathBuf,
}

impl GitCli {
    /// Create a new Git CLI adapter.
    pub fn new(git_bin: Option<PathBuf>) -> Self {
        Self {
            git_bin: git_bin.unwrap_or_else(|| PathBuf::from("git")),
        }
    }
}

impl Default for GitCli {
    fn default() -> Self {
        Self::new(None)
    }
}

impl GitProvider for GitCli {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>> {
        // Use three-dot syntax to compare merge-base..HEAD changes.
        // This matches common PR semantics.
        let range = format!("{base}...{head}");

        let output = Command::new(&self.git_bin)
            .arg("diff")
            .arg("--name-only")
            .arg(range)
            .current_dir(workspace_root)
            .output()
            .map_err(|e| SemverguardError::Git(format!("failed to run git diff: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8(output.stderr)?;
            return Err(SemverguardError::Git(format!(
                "git diff failed (exit {:?}): {stderr}",
                output.status.code()
            )));
        }

        let stdout = String::from_utf8(output.stdout)?;
        let mut paths = Vec::new();
        for line in stdout.lines() {
            let s = line.trim();
            if s.is_empty() {
                continue;
            }
            paths.push(PathBuf::from(s));
        }
        Ok(paths)
    }
}
