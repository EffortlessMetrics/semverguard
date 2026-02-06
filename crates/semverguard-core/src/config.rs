//! Configuration loading helpers.
//!
//! Loads and validates `semverguard.toml` configuration files.

use anyhow::{Context, Result};
use semverguard_types::SemverguardConfig;
use std::fs;
use std::path::Path;

/// Ensure the workspace root exists and is a directory.
pub fn ensure_workspace_root(workspace_root: &Path) -> Result<()> {
    let md = fs::metadata(workspace_root).with_context(|| {
        format!(
            "workspace root does not exist: {}",
            workspace_root.display()
        )
    })?;
    if !md.is_dir() {
        anyhow::bail!(
            "workspace root is not a directory: {}",
            workspace_root.display()
        );
    }
    Ok(())
}

/// Load config from a TOML file, returning defaults if the file doesn't exist.
pub fn load_config(path: &Path) -> Result<SemverguardConfig> {
    if !path.exists() {
        // Use defaults; missing config is not an error.
        return Ok(SemverguardConfig::default());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let cfg: SemverguardConfig =
        toml::from_str(&raw).with_context(|| format!("invalid TOML in {}", path.display()))?;
    Ok(cfg)
}
