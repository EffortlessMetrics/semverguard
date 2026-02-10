#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-git
//!
//! Adapter over the `git` CLI to support "changed crates" scoping.

use semverguard_domain::{GitProvider, Result, SemverguardError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parse the output of `git diff --name-only` into a list of paths.
///
/// This function handles:
/// - Empty output (returns empty vec)
/// - Lines with leading/trailing whitespace
/// - Blank lines in the middle of output
pub fn parse_diff_output(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .map(|line| line.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

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

    /// Execute a git command and return stdout on success.
    ///
    /// Returns an error if the command fails or returns a non-zero exit code.
    fn run_git(&self, workspace_root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.git_bin)
            .args(args)
            .current_dir(workspace_root)
            .output()
            .map_err(|e| SemverguardError::Git(format!("failed to run git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SemverguardError::Git(format!(
                "git command failed (exit {:?}): {stderr}",
                output.status.code()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if git is available.
    pub fn is_available(&self, workspace_root: &Path) -> bool {
        self.run_git(workspace_root, &["--version"]).is_ok()
    }

    /// Return the git version string, or None if git is not available.
    pub fn version(&self, workspace_root: &Path) -> Option<String> {
        self.run_git(workspace_root, &["--version"])
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Check if the repo is a shallow clone.
    pub fn is_shallow(&self, workspace_root: &Path) -> Result<bool> {
        let output = self.run_git(workspace_root, &["rev-parse", "--is-shallow-repository"])?;
        Ok(output.trim() == "true")
    }

    /// Resolve HEAD to a full commit SHA.
    pub fn resolve_head(&self, workspace_root: &Path) -> Result<String> {
        self.resolve_ref(workspace_root, "HEAD")
    }

    /// Resolve a ref to a full commit SHA.
    pub fn resolve_ref(&self, workspace_root: &Path, refspec: &str) -> Result<String> {
        let output = self.run_git(workspace_root, &["rev-parse", refspec])?;
        Ok(output.trim().to_string())
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

        let stdout = self.run_git(workspace_root, &["diff", "--name-only", &range])?;
        Ok(parse_diff_output(&stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    struct FakeGit {
        _dir: TempDir,
        path: PathBuf,
    }

    fn write_fake_git() -> FakeGit {
        let dir = tempfile::tempdir().expect("tempdir");
        #[cfg(windows)]
        let path = dir.path().join("fake_git.cmd");
        #[cfg(not(windows))]
        let path = dir.path().join("fake_git.sh");

        #[cfg(windows)]
        let script = "@echo off\r\nif \"%1\"==\"--version\" (\r\n  echo git version 2.40.0\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"--is-shallow-repository\" (\r\n  echo true\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"HEAD\" (\r\n  echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"bad\" (\r\n  echo unknown ref 1>&2\r\n  exit /b 1\r\n)\r\nif \"%1\"==\"rev-parse\" (\r\n  echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"diff\" if \"%2\"==\"--name-only\" (\r\n  echo src/lib.rs\r\n  echo Cargo.toml\r\n  exit /b 0\r\n)\r\necho unknown args 1>&2\r\nexit /b 1\r\n";
        #[cfg(not(windows))]
        let script = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo \"git version 2.40.0\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"--is-shallow-repository\" ]; then\n  echo \"true\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"HEAD\" ]; then\n  echo \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"bad\" ]; then\n  echo \"unknown ref\" 1>&2\n  exit 1\nfi\nif [ \"$1\" = \"rev-parse\" ]; then\n  echo \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n  exit 0\nfi\nif [ \"$1\" = \"diff\" ] && [ \"$2\" = \"--name-only\" ]; then\n  echo \"src/lib.rs\"\n  echo \"Cargo.toml\"\n  exit 0\nfi\necho \"unknown args\" 1>&2\nexit 1\n";

        fs::write(&path, script).expect("write fake git script");

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("set permissions");
        }

        FakeGit { _dir: dir, path }
    }

    mod parse_diff_output_tests {
        use super::*;

        #[test]
        fn empty_output_returns_empty_vec() {
            let result = parse_diff_output("");
            assert!(result.is_empty());
        }

        #[test]
        fn whitespace_only_output_returns_empty_vec() {
            let result = parse_diff_output("   \n\t\n  \n");
            assert!(result.is_empty());
        }

        #[test]
        fn single_file_parsed_correctly() {
            let result = parse_diff_output("src/lib.rs\n");
            assert_eq!(result, vec![PathBuf::from("src/lib.rs")]);
        }

        #[test]
        fn multiple_files_parsed_correctly() {
            let input = "src/lib.rs\nCargo.toml\ntests/integration.rs\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![
                    PathBuf::from("src/lib.rs"),
                    PathBuf::from("Cargo.toml"),
                    PathBuf::from("tests/integration.rs"),
                ]
            );
        }

        #[test]
        fn handles_paths_with_spaces() {
            let input = "path with spaces/file.rs\n";
            let result = parse_diff_output(input);
            assert_eq!(result, vec![PathBuf::from("path with spaces/file.rs")]);
        }

        #[test]
        fn handles_nested_paths() {
            let input = "crates/semverguard-git/src/lib.rs\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![PathBuf::from("crates/semverguard-git/src/lib.rs")]
            );
        }

        #[test]
        fn trims_leading_and_trailing_whitespace() {
            let input = "  src/lib.rs  \n  Cargo.toml\t\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![PathBuf::from("src/lib.rs"), PathBuf::from("Cargo.toml"),]
            );
        }

        #[test]
        fn skips_blank_lines_in_middle() {
            let input = "src/lib.rs\n\n\nCargo.toml\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![PathBuf::from("src/lib.rs"), PathBuf::from("Cargo.toml"),]
            );
        }

        #[test]
        fn handles_no_trailing_newline() {
            let input = "src/lib.rs";
            let result = parse_diff_output(input);
            assert_eq!(result, vec![PathBuf::from("src/lib.rs")]);
        }

        #[test]
        fn handles_windows_line_endings() {
            let input = "src/lib.rs\r\nCargo.toml\r\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![PathBuf::from("src/lib.rs"), PathBuf::from("Cargo.toml"),]
            );
        }
    }

    mod git_cli_tests {
        use super::*;

        #[test]
        fn new_with_custom_path() {
            let cli = GitCli::new(Some(PathBuf::from("/custom/git")));
            assert_eq!(cli.git_bin, PathBuf::from("/custom/git"));
        }

        #[test]
        fn new_with_none_uses_default() {
            let cli = GitCli::new(None);
            assert_eq!(cli.git_bin, PathBuf::from("git"));
        }

        #[test]
        fn default_uses_git() {
            let cli = GitCli::default();
            assert_eq!(cli.git_bin, PathBuf::from("git"));
        }

        #[test]
        fn is_available_and_version_use_git() {
            let fake = write_fake_git();
            let cli = GitCli::new(Some(fake.path.clone()));
            let root = fake._dir.path();

            assert!(cli.is_available(root));
            assert_eq!(cli.version(root), Some("git version 2.40.0".to_string()));
        }

        #[test]
        fn is_available_false_when_missing() {
            let cli = GitCli::new(Some(PathBuf::from("/nonexistent/git")));
            let root = Path::new(".");
            assert!(!cli.is_available(root));
            assert!(cli.version(root).is_none());
        }

        #[test]
        fn resolves_and_parses_git_outputs() {
            let fake = write_fake_git();
            let cli = GitCli::new(Some(fake.path.clone()));
            let root = fake._dir.path();

            assert_eq!(cli.is_shallow(root).unwrap(), true);
            assert_eq!(
                cli.resolve_head(root).unwrap(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            );
            assert_eq!(
                cli.resolve_ref(root, "v1.0.0").unwrap(),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            );

            let changed = cli.changed_paths(root, "base", "head").unwrap();
            assert_eq!(
                changed,
                vec![PathBuf::from("src/lib.rs"), PathBuf::from("Cargo.toml")]
            );
        }

        #[test]
        fn resolve_ref_returns_error_on_failure() {
            let fake = write_fake_git();
            let cli = GitCli::new(Some(fake.path.clone()));
            let root = fake._dir.path();

            let err = cli.resolve_ref(root, "bad").unwrap_err();
            assert!(err.to_string().contains("git command failed"));
        }
    }
}
