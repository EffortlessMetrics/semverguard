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
/// - Blank lines in the middle of output
/// - Windows line endings (\r\n)
///
/// It does NOT trim leading/trailing whitespace from lines, as those could
/// be significant in a filename.
pub fn parse_diff_output(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn is_manifest_path(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "Cargo.toml")
}

fn diff_has_version_change(diff: &str) -> bool {
    diff.lines().any(|line| {
        if !(line.starts_with('+') || line.starts_with('-')) {
            return false;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            return false;
        }

        let content = line[1..].trim_start();
        let Some(rest) = content.strip_prefix("version") else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('=') {
            return false;
        }
        let value = rest[1..].trim_start();
        value.starts_with('"') || value.starts_with('\'')
    })
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

    /// Detect whether any changed `Cargo.toml` contains a changed `version = "..."`
    /// line between `base...head`.
    pub fn has_manifest_version_change(
        &self,
        workspace_root: &Path,
        base: &str,
        head: &str,
    ) -> Result<bool> {
        let manifests: Vec<PathBuf> = self
            .changed_paths(workspace_root, base, head)?
            .into_iter()
            .filter(|p| is_manifest_path(p))
            .collect();

        if manifests.is_empty() {
            return Ok(false);
        }

        let range = format!("{base}...{head}");
        for manifest in manifests {
            let manifest_arg = manifest.to_string_lossy().into_owned();
            let diff = self.run_git(
                workspace_root,
                &["diff", "--unified=0", &range, "--", &manifest_arg],
            )?;
            if diff_has_version_change(&diff) {
                return Ok(true);
            }
        }
        Ok(false)
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
    use std::process::Command;
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
        let script = "@echo off\r\nif \"%1\"==\"--version\" (\r\n  echo git version 2.40.0\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"--is-shallow-repository\" (\r\n  echo true\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"HEAD\" (\r\n  echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"rev-parse\" if \"%2\"==\"bad\" (\r\n  echo unknown ref 1>&2\r\n  exit /b 1\r\n)\r\nif \"%1\"==\"rev-parse\" (\r\n  echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"diff\" if \"%2\"==\"--name-only\" (\r\n  echo src/lib.rs\r\n  echo Cargo.toml\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"diff\" if \"%2\"==\"--unified=0\" (\r\n  echo @@ -3 +3 @@\r\n  echo -version = '0.1.0'\r\n  echo +version = '0.2.0'\r\n  exit /b 0\r\n)\r\necho unknown args 1>&2\r\nexit /b 1\r\n";
        #[cfg(not(windows))]
        let script = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo \"git version 2.40.0\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"--is-shallow-repository\" ]; then\n  echo \"true\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"HEAD\" ]; then\n  echo \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n  exit 0\nfi\nif [ \"$1\" = \"rev-parse\" ] && [ \"$2\" = \"bad\" ]; then\n  echo \"unknown ref\" 1>&2\n  exit 1\nfi\nif [ \"$1\" = \"rev-parse\" ]; then\n  echo \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n  exit 0\nfi\nif [ \"$1\" = \"diff\" ] && [ \"$2\" = \"--name-only\" ]; then\n  echo \"src/lib.rs\"\n  echo \"Cargo.toml\"\n  exit 0\nfi\nif [ \"$1\" = \"diff\" ] && [ \"$2\" = \"--unified=0\" ]; then\n  echo \"@@ -3 +3 @@\"\n  echo \"-version = \\\"0.1.0\\\"\"\n  echo \"+version = \\\"0.2.0\\\"\"\n  exit 0\nfi\necho \"unknown args\" 1>&2\nexit 1\n";

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
        fn does_not_trim_leading_and_trailing_whitespace() {
            let input = "  src/lib.rs  \n  Cargo.toml\t\n";
            let result = parse_diff_output(input);
            assert_eq!(
                result,
                vec![
                    PathBuf::from("  src/lib.rs  "),
                    PathBuf::from("  Cargo.toml\t"),
                ]
            );
        }

        #[test]
        fn handles_filenames_with_significant_spaces() {
            let input = " file_with_space.rs\n";
            let result = parse_diff_output(input);
            assert_eq!(result, vec![PathBuf::from(" file_with_space.rs")]);
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

    mod version_change_detection_tests {
        use super::*;

        #[test]
        fn detects_version_line_addition() {
            let diff = "\
@@ -3 +3 @@
-version = \"0.1.0\"
+version = \"0.2.0\"
";
            assert!(diff_has_version_change(diff));
        }

        #[test]
        fn ignores_file_header_lines() {
            let diff = "\
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1 +1 @@
-name = \"mycrate\"
+name = \"mycrate2\"
";
            assert!(!diff_has_version_change(diff));
        }

        #[test]
        fn ignores_dependency_version_keys() {
            let diff = "\
@@ -12 +12 @@
-serde = { version = \"1\" }
+serde = { version = \"1\", features = [\"derive\"] }
";
            assert!(!diff_has_version_change(diff));
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

            assert!(cli.is_shallow(root).unwrap());
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

        fn run_git(repo: &Path, args: &[&str]) -> String {
            let output = Command::new("git")
                .args(args)
                .current_dir(repo)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }

        #[test]
        fn has_manifest_version_change_true_when_manifest_bumped_in_repo() {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path();

            fs::create_dir_all(root.join("src")).expect("create src");
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            )
            .expect("write Cargo.toml");
            fs::write(root.join("src/lib.rs"), "pub fn lib() {}\n").expect("write lib.rs");

            run_git(root, &["init", "-b", "main"]);
            run_git(root, &["config", "user.email", "test@example.com"]);
            run_git(root, &["config", "user.name", "Test User"]);
            // Defensive: some environments set commit.gpgsign globally; force
            // it off at repo scope so the test does not need host signing keys.
            run_git(root, &["config", "commit.gpgsign", "false"]);
            run_git(root, &["config", "tag.gpgsign", "false"]);
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "initial"]);
            let baseline = run_git(root, &["rev-parse", "HEAD"]);

            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"pkg\"\nversion = \"0.2.0\"\nedition = \"2021\"\n",
            )
            .expect("update Cargo.toml");
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "version bump"]);

            let cli = GitCli::default();
            let changed = cli
                .has_manifest_version_change(root, &baseline, "HEAD")
                .expect("version check");
            assert!(changed);
        }
    }
}
