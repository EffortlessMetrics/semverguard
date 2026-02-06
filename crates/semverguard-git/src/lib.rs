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
            let stderr = String::from_utf8(output.stderr)?;
            return Err(SemverguardError::Git(format!(
                "git command failed (exit {:?}): {stderr}",
                output.status.code()
            )));
        }

        Ok(String::from_utf8(output.stdout)?)
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
    }
}
