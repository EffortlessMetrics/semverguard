//! Configuration loading helpers.
//!
//! Loads and validates `semverguard.toml` configuration files.

use anyhow::{Context, Result};
use globset::Glob;
use semverguard_types::{BaselineKind, ScopeMode, SemverguardConfig};
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

/// Return true when `semverguard.toml` explicitly sets `scope.mode`.
///
/// This is used by frontends to preserve explicit user intent in config files
/// when applying mode-driven scope defaults.
pub fn config_explicitly_sets_scope_mode(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return false;
    };

    value
        .get("scope")
        .and_then(|scope| scope.get("mode"))
        .is_some()
}

/// Validation diagnostics for configuration files.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Validation errors that should fail CI.
    pub errors: Vec<String>,
    /// Validation warnings that should be reviewed.
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Return true when there are no errors or warnings.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    /// Return true when there are one or more validation errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Validate semantic configuration constraints.
pub fn validate_config(config: &SemverguardConfig) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Validate glob patterns
    for (i, pattern) in config.scope.include.iter().enumerate() {
        if let Err(e) = Glob::new(pattern) {
            result.errors.push(format!(
                "Invalid glob in scope.include[{i}] \"{pattern}\": {e}"
            ));
        }
    }

    for (i, pattern) in config.scope.exclude.iter().enumerate() {
        if let Err(e) = Glob::new(pattern) {
            result.errors.push(format!(
                "Invalid glob in scope.exclude[{i}] \"{pattern}\": {e}"
            ));
        }
    }

    // Validate baseline configuration
    match config.baseline.kind {
        BaselineKind::Git => {
            if config.baseline.rev.is_none() {
                result
                    .errors
                    .push("baseline.kind is \"git\" but baseline.rev is not set".to_string());
            }
            if config.baseline.version.is_some() {
                result.warnings.push(
                    "baseline.version is set but baseline.kind is \"git\"; version will be \
                     ignored"
                        .to_string(),
                );
            }
        }
        BaselineKind::CratesIo => {
            if config.baseline.rev.is_some() {
                result.warnings.push(
                    "baseline.rev is set but baseline.kind is \"crates-io\"; rev will be ignored"
                        .to_string(),
                );
            }
        }
    }

    // Validate scope mode consistency
    if config.scope.mode == ScopeMode::Changed {
        if config.baseline.kind != BaselineKind::Git {
            result
                .errors
                .push("scope.mode is \"changed\" requires baseline.kind = \"git\"".to_string());
        }
        if config.baseline.rev.is_none() {
            result
                .errors
                .push("scope.mode is \"changed\" requires baseline.rev to be set".to_string());
        }
    }

    // Validate file paths
    if let Some(ref root) = config.baseline.root {
        if !root.exists() {
            result
                .errors
                .push(format!("baseline.root does not exist: {}", root.display()));
        } else if !root.is_dir() {
            result.errors.push(format!(
                "baseline.root is not a directory: {}",
                root.display()
            ));
        }
    }

    if let Some(ref rustdoc) = config.baseline.rustdoc {
        if !rustdoc.exists() {
            result.errors.push(format!(
                "baseline.rustdoc does not exist: {}",
                rustdoc.display()
            ));
        } else if !rustdoc.is_file() {
            result.errors.push(format!(
                "baseline.rustdoc is not a file: {}",
                rustdoc.display()
            ));
        }
    }

    if let Some(ref cargo_bin) = config.engine.cargo_bin
        && !cargo_bin.exists()
    {
        result.warnings.push(format!(
            "engine.cargo_bin does not exist: {}",
            cargo_bin.display()
        ));
    }

    // Validate features configuration
    if config.features.all_features && config.features.only_explicit_features {
        result.warnings.push(
            "all_features and only_explicit_features both true; all_features takes precedence"
                .to_string(),
        );
    }

    if config.features.only_explicit_features && config.features.features.is_empty() {
        result
            .warnings
            .push("only_explicit_features is true but features list is empty".to_string());
    }

    // Validate waivers
    for (i, waiver) in config.waivers.iter().enumerate() {
        // Check fingerprint format (64-char hex)
        if waiver.fingerprint.len() != 64
            || !waiver.fingerprint.chars().all(|c| c.is_ascii_hexdigit())
        {
            result.errors.push(format!(
                "waivers[{i}].fingerprint must be a 64-character hex string, got: {}",
                waiver.fingerprint
            ));
        }
        // Check reason is not empty
        if waiver.reason.trim().is_empty() {
            result
                .errors
                .push(format!("waivers[{i}].reason must not be empty"));
        }
        // Warn on expired waivers
        if let Some(expires) = &waiver.expires {
            let parts: Vec<&str> = expires.split('-').collect();
            if parts.len() == 3
                && let (Ok(y), Ok(m), Ok(d)) = (
                    parts[0].parse::<i32>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
                && let Ok(month) = time::Month::try_from(m)
                && let Ok(date) = time::Date::from_calendar_date(y, month, d)
            {
                let today = time::OffsetDateTime::now_utc().date();
                if today > date {
                    result.warnings.push(format!(
                        "waivers[{i}].expires ({expires}) has already passed"
                    ));
                }
            }
        }
    }

    result
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

    #[test]
    fn test_config_explicitly_sets_scope_mode_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        assert!(!config_explicitly_sets_scope_mode(&path));
    }

    #[test]
    fn test_config_explicitly_sets_scope_mode_true() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"
[scope]
mode = "changed"
"#,
        )
        .unwrap();

        assert!(config_explicitly_sets_scope_mode(&path));
    }

    #[test]
    fn test_config_explicitly_sets_scope_mode_false_when_scope_missing_mode() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"
[scope]
include = ["*"]
"#,
        )
        .unwrap();

        assert!(!config_explicitly_sets_scope_mode(&path));
    }

    #[test]
    fn test_config_explicitly_sets_scope_mode_false_for_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("semverguard.toml");
        fs::write(&path, "[scope").unwrap();
        assert!(!config_explicitly_sets_scope_mode(&path));
    }
}
