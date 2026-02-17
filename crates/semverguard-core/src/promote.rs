//! Baseline promotion logic.
//!
//! Provides functionality to update the baseline revision in `semverguard.toml`
//! while preserving comments and formatting.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

/// Result of a baseline promotion operation.
#[derive(Debug, Clone)]
pub struct PromotionResult {
    /// Previous baseline revision value (if any).
    pub previous: Option<String>,
    /// New baseline revision value.
    pub new_value: String,
    /// Config key that was modified.
    pub config_key: String,
    /// Whether the config file was actually written.
    pub written: bool,
}

/// Promote the git baseline revision in a config file.
///
/// Reads the config file, updates `baseline.rev` to `new_rev`, and optionally
/// writes it back. Uses `toml_edit` to preserve comments and formatting.
///
/// If `write` is false, performs a dry run and returns what would change.
pub fn promote_git_baseline(
    config_path: &Path,
    new_rev: &str,
    write: bool,
) -> Result<PromotionResult> {
    if !config_path.exists() {
        bail!(
            "Config file not found: {}\nCreate a semverguard.toml first, or use --config to specify the path.",
            config_path.display()
        );
    }

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;

    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse {}", config_path.display()))?;

    // Get or create [baseline] section
    if doc.get("baseline").is_none() {
        doc["baseline"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let baseline = doc["baseline"]
        .as_table_mut()
        .context("baseline is not a table")?;

    // Read previous value
    let previous = baseline
        .get("rev")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Also ensure kind is "git" if not set
    if baseline.get("kind").is_none() {
        baseline["kind"] = toml_edit::value("git");
    }

    // Update rev
    baseline["rev"] = toml_edit::value(new_rev);

    let result = PromotionResult {
        previous,
        new_value: new_rev.to_string(),
        config_key: "baseline.rev".to_string(),
        written: write,
    };

    if write {
        fs::write(config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", config_path.display()))?;
    }

    Ok(result)
}

/// Resolve a git ref to a full commit SHA using the default git adapter.
#[cfg(feature = "default-adapters")]
pub fn resolve_git_revision(workspace_root: &Path, rev: &str) -> Result<String> {
    let git = semverguard_git::GitCli::default();
    if rev == "HEAD" {
        git.resolve_head(workspace_root)
            .context("failed to resolve HEAD - is this a git repository?")
    } else {
        git.resolve_ref(workspace_root, rev)
            .with_context(|| format!("failed to resolve ref: {rev}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_promote_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"
[baseline]
kind = "git"
rev = "abc123"
"#,
        )
        .unwrap();

        let result = promote_git_baseline(&config_path, "def456", false).unwrap();
        assert_eq!(result.previous, Some("abc123".to_string()));
        assert_eq!(result.new_value, "def456");
        assert!(!result.written);

        // File should be unchanged
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("abc123"));
    }

    #[test]
    fn test_promote_write() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"# My config
[baseline]
kind = "git"
rev = "abc123"

[scope]
mode = "changed"
"#,
        )
        .unwrap();

        let result = promote_git_baseline(&config_path, "def456", true).unwrap();
        assert_eq!(result.previous, Some("abc123".to_string()));
        assert_eq!(result.new_value, "def456");
        assert!(result.written);

        // File should be updated
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("def456"));
        assert!(!content.contains("abc123"));
        // Comments and other sections preserved
        assert!(content.contains("# My config"));
        assert!(content.contains("[scope]"));
    }

    #[test]
    fn test_promote_no_baseline_section() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"
[scope]
mode = "workspace"
"#,
        )
        .unwrap();

        let result = promote_git_baseline(&config_path, "newrev", true).unwrap();
        assert_eq!(result.previous, None);
        assert_eq!(result.new_value, "newrev");
        assert!(result.written);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("newrev"));
        assert!(content.contains("baseline"));
    }

    #[test]
    fn test_promote_no_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        let result = promote_git_baseline(&config_path, "rev", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_promote_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = promote_git_baseline(dir.path(), "def456", false).unwrap_err();
        assert!(err.to_string().contains("failed to read"));
    }

    #[test]
    fn test_promote_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(&config_path, "not = [valid").unwrap();
        let err = promote_git_baseline(&config_path, "def456", false).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn test_promote_baseline_not_table() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(&config_path, "baseline = \"oops\"").unwrap();
        let err = promote_git_baseline(&config_path, "def456", false).unwrap_err();
        assert!(err.to_string().contains("baseline is not a table"));
    }

    #[test]
    fn test_promote_write_error() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"
[baseline]
kind = "git"
rev = "abc123"
"#,
        )
        .unwrap();

        let mut perms = fs::metadata(&config_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&config_path, perms).unwrap();

        let err = promote_git_baseline(&config_path, "def456", true).unwrap_err();
        assert!(err.to_string().contains("failed to write"));
    }
}
