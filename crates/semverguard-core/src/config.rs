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

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{BaselineKind, OutputFormat};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_workspace_root_ok() {
        let dir = tempdir().unwrap();
        ensure_workspace_root(dir.path()).unwrap();
    }

    #[test]
    fn test_ensure_workspace_root_missing() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing");
        let err = ensure_workspace_root(&missing).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_ensure_workspace_root_not_dir() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "data").unwrap();
        let err = ensure_workspace_root(&file_path).unwrap_err();
        assert!(err.to_string().contains("not a directory"));
    }

    #[test]
    fn test_load_config_missing_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.baseline.kind, BaselineKind::CratesIo);
        assert_eq!(cfg.output.format, OutputFormat::Text);
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        fs::write(&path, "[baseline\nkind = \"git\"").unwrap();
        let err = load_config(&path).unwrap_err();
        assert!(err.to_string().contains("invalid TOML"));
    }

    #[test]
    fn test_load_config_read_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config_dir");
        fs::create_dir(&path).unwrap();
        let err = load_config(&path).unwrap_err();
        assert!(err.to_string().contains("failed to read"));
    }

    #[test]
    fn test_load_config_parses_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"
[baseline]
kind = "git"
rev = "origin/main"

[output]
format = "receipt"
pretty_json = false
"#,
        )
        .unwrap();

        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.baseline.kind, BaselineKind::Git);
        assert_eq!(cfg.baseline.rev.as_deref(), Some("origin/main"));
        assert_eq!(cfg.output.format, OutputFormat::Receipt);
        assert!(!cfg.output.pretty_json);
    }
}
