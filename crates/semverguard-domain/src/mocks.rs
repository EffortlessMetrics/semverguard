//! Mock/fake implementations of port traits for testing.
//!
//! This module provides configurable mock implementations of:
//! - [`MockWorkspaceProvider`] - Returns preset [`WorkspaceMetadata`] or errors
//! - [`MockGitProvider`] - Returns preset changed paths or errors
//! - [`MockSemverEngine`] - Returns preset [`SemverCheckOutput`] or errors
//!
//! Each mock supports:
//! - Configuring expected inputs and outputs
//! - Call counting and verification
//! - Interior mutability via [`std::sync::Mutex`]

use crate::{GitProvider, Result, SemverEngine, SemverguardError, WorkspaceProvider};
use semverguard_types::{SemverCheckOutput, SemverCheckRequest, WorkspaceMetadata};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Records a call to [`MockWorkspaceProvider::load`].
#[derive(Debug, Clone)]
pub struct WorkspaceLoadCall {
    /// The workspace root passed to the call.
    pub workspace_root: PathBuf,
}

/// A mock implementation of [`WorkspaceProvider`] for testing.
///
/// # Example
///
/// ```
/// use semverguard_domain::mocks::MockWorkspaceProvider;
/// use semverguard_types::WorkspaceMetadata;
/// use std::path::PathBuf;
///
/// let metadata = WorkspaceMetadata {
///     workspace_root: PathBuf::from("/test"),
///     packages: vec![],
/// };
/// let provider = MockWorkspaceProvider::with_result(Ok(metadata));
/// ```
#[derive(Debug)]
pub struct MockWorkspaceProvider {
    /// The result to return from `load`.
    result: Mutex<Option<Result<WorkspaceMetadata>>>,
    /// Recorded calls for verification.
    calls: Mutex<Vec<WorkspaceLoadCall>>,
}

impl MockWorkspaceProvider {
    /// Create a new mock that returns the given result.
    pub fn with_result(result: Result<WorkspaceMetadata>) -> Self {
        Self {
            result: Mutex::new(Some(result)),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns an error.
    pub fn with_error(message: impl Into<String>) -> Self {
        Self::with_result(Err(SemverguardError::Workspace(message.into())))
    }

    /// Create a new mock that returns empty workspace metadata.
    pub fn empty(workspace_root: impl Into<PathBuf>) -> Self {
        Self::with_result(Ok(WorkspaceMetadata {
            workspace_root: workspace_root.into(),
            packages: vec![],
        }))
    }

    /// Get the number of times `load` was called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Get all recorded calls for verification.
    pub fn calls(&self) -> Vec<WorkspaceLoadCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Assert that `load` was called exactly once with the expected root.
    pub fn assert_called_once_with(&self, expected_root: &Path) {
        let calls = self.calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "Expected exactly 1 call, got {}",
            calls.len()
        );
        assert_eq!(
            calls[0].workspace_root, expected_root,
            "Expected workspace_root {:?}, got {:?}",
            expected_root, calls[0].workspace_root
        );
    }

    /// Set a new result to return on the next call.
    pub fn set_result(&self, result: Result<WorkspaceMetadata>) {
        *self.result.lock().unwrap() = Some(result);
    }
}

impl Default for MockWorkspaceProvider {
    fn default() -> Self {
        Self::empty("/workspace")
    }
}

impl WorkspaceProvider for MockWorkspaceProvider {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata> {
        self.calls.lock().unwrap().push(WorkspaceLoadCall {
            workspace_root: workspace_root.to_path_buf(),
        });

        self.result
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Err(SemverguardError::Workspace("no result configured".into())))
    }
}

/// Records a call to [`MockGitProvider::changed_paths`].
#[derive(Debug, Clone)]
pub struct GitChangedPathsCall {
    /// The workspace root passed to the call.
    pub workspace_root: PathBuf,
    /// The base revision passed to the call.
    pub base: String,
    /// The head revision passed to the call.
    pub head: String,
}

/// A mock implementation of [`GitProvider`] for testing.
///
/// # Example
///
/// ```
/// use semverguard_domain::mocks::MockGitProvider;
/// use std::path::PathBuf;
///
/// let provider = MockGitProvider::with_changed_paths(vec![
///     PathBuf::from("crates/foo/src/lib.rs"),
///     PathBuf::from("crates/bar/Cargo.toml"),
/// ]);
/// ```
#[derive(Debug)]
pub struct MockGitProvider {
    /// The result to return from `changed_paths`.
    result: Mutex<Option<Result<Vec<PathBuf>>>>,
    /// Recorded calls for verification.
    calls: Mutex<Vec<GitChangedPathsCall>>,
}

impl MockGitProvider {
    /// Create a new mock that returns the given result.
    pub fn with_result(result: Result<Vec<PathBuf>>) -> Self {
        Self {
            result: Mutex::new(Some(result)),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns the given changed paths.
    pub fn with_changed_paths(paths: Vec<PathBuf>) -> Self {
        Self::with_result(Ok(paths))
    }

    /// Create a new mock that returns an error.
    pub fn with_error(message: impl Into<String>) -> Self {
        Self::with_result(Err(SemverguardError::Git(message.into())))
    }

    /// Create a new mock that returns no changed paths.
    pub fn no_changes() -> Self {
        Self::with_changed_paths(vec![])
    }

    /// Get the number of times `changed_paths` was called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Get all recorded calls for verification.
    pub fn calls(&self) -> Vec<GitChangedPathsCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Assert that `changed_paths` was called exactly once with expected args.
    pub fn assert_called_once_with(
        &self,
        expected_root: &Path,
        expected_base: &str,
        expected_head: &str,
    ) {
        let calls = self.calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "Expected exactly 1 call, got {}",
            calls.len()
        );
        assert_eq!(
            calls[0].workspace_root, expected_root,
            "Expected workspace_root {:?}, got {:?}",
            expected_root, calls[0].workspace_root
        );
        assert_eq!(
            calls[0].base, expected_base,
            "Expected base {:?}, got {:?}",
            expected_base, calls[0].base
        );
        assert_eq!(
            calls[0].head, expected_head,
            "Expected head {:?}, got {:?}",
            expected_head, calls[0].head
        );
    }

    /// Set a new result to return on the next call.
    pub fn set_result(&self, result: Result<Vec<PathBuf>>) {
        *self.result.lock().unwrap() = Some(result);
    }
}

impl Default for MockGitProvider {
    fn default() -> Self {
        Self::no_changes()
    }
}

impl GitProvider for MockGitProvider {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>> {
        self.calls.lock().unwrap().push(GitChangedPathsCall {
            workspace_root: workspace_root.to_path_buf(),
            base: base.to_string(),
            head: head.to_string(),
        });

        self.result
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Err(SemverguardError::Git("no result configured".into())))
    }
}

/// Records a call to [`MockSemverEngine::check`].
#[derive(Debug, Clone)]
pub struct SemverCheckCall {
    /// The request passed to the call.
    pub request: SemverCheckRequest,
}

/// A mock implementation of [`SemverEngine`] for testing.
///
/// This mock can be configured to return different results for successive calls,
/// which is useful when testing multi-package scenarios.
///
/// # Example
///
/// ```
/// use semverguard_domain::mocks::MockSemverEngine;
/// use semverguard_types::SemverCheckOutput;
///
/// // Single result for all calls
/// let engine = MockSemverEngine::with_output(SemverCheckOutput {
///     exit_code: Some(0),
///     success: true,
///     stdout: String::new(),
///     stderr: String::new(),
///     required_bump: None,
/// });
///
/// // Different results for each call
/// let engine = MockSemverEngine::with_outputs(vec![
///     (vec![], SemverCheckOutput { exit_code: Some(0), success: true, stdout: String::new(), stderr: String::new(), required_bump: None }),
///     (vec!["--verbose".into()], SemverCheckOutput { exit_code: Some(1), success: false, stdout: String::new(), stderr: "breaking change".into(), required_bump: None }),
/// ]);
/// ```
#[derive(Debug)]
pub struct MockSemverEngine {
    /// Queue of results to return from successive `check` calls.
    results: Mutex<Vec<Result<(Vec<String>, SemverCheckOutput)>>>,
    /// Default result when queue is empty.
    default_result: Mutex<Option<Result<(Vec<String>, SemverCheckOutput)>>>,
    /// Recorded calls for verification.
    calls: Mutex<Vec<SemverCheckCall>>,
}

impl MockSemverEngine {
    /// Create a new mock that returns the given output for all calls.
    pub fn with_output(output: SemverCheckOutput) -> Self {
        Self {
            results: Mutex::new(Vec::new()),
            default_result: Mutex::new(Some(Ok((vec![], output)))),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns the given (args, output) tuple for all calls.
    pub fn with_result(result: Result<(Vec<String>, SemverCheckOutput)>) -> Self {
        Self {
            results: Mutex::new(Vec::new()),
            default_result: Mutex::new(Some(result)),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns different outputs for successive calls.
    pub fn with_outputs(outputs: Vec<(Vec<String>, SemverCheckOutput)>) -> Self {
        let results = outputs.into_iter().map(Ok).collect();
        Self {
            results: Mutex::new(results),
            default_result: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns different results for successive calls.
    pub fn with_results(results: Vec<Result<(Vec<String>, SemverCheckOutput)>>) -> Self {
        Self {
            results: Mutex::new(results),
            default_result: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that returns an error.
    pub fn with_error(message: impl Into<String>) -> Self {
        Self::with_result(Err(SemverguardError::Engine(message.into())))
    }

    /// Create a new mock that returns a successful output with no issues.
    pub fn success() -> Self {
        Self::with_output(SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            required_bump: None,
        })
    }

    /// Create a new mock that returns a failed output (semver violation).
    pub fn failure(stderr: impl Into<String>) -> Self {
        Self::with_output(SemverCheckOutput {
            exit_code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: stderr.into(),
            required_bump: None,
        })
    }

    /// Get the number of times `check` was called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Get all recorded calls for verification.
    pub fn calls(&self) -> Vec<SemverCheckCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Get the package names from all recorded calls.
    pub fn checked_packages(&self) -> Vec<PathBuf> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.request.manifest_path.clone())
            .collect()
    }

    /// Assert that `check` was called exactly N times.
    pub fn assert_call_count(&self, expected: usize) {
        let actual = self.call_count();
        assert_eq!(
            actual, expected,
            "Expected {} calls, got {}",
            expected, actual
        );
    }

    /// Assert that `check` was not called.
    pub fn assert_not_called(&self) {
        self.assert_call_count(0);
    }

    /// Push an additional result to the queue.
    pub fn push_result(&self, result: Result<(Vec<String>, SemverCheckOutput)>) {
        self.results.lock().unwrap().push(result);
    }

    /// Push an additional output to the queue.
    pub fn push_output(&self, args: Vec<String>, output: SemverCheckOutput) {
        self.push_result(Ok((args, output)));
    }

    /// Set the default result used when the queue is empty.
    pub fn set_default(&self, result: Result<(Vec<String>, SemverCheckOutput)>) {
        *self.default_result.lock().unwrap() = Some(result);
    }
}

impl Default for MockSemverEngine {
    fn default() -> Self {
        Self::success()
    }
}

impl SemverEngine for MockSemverEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)> {
        self.calls.lock().unwrap().push(SemverCheckCall {
            request: request.clone(),
        });

        // Try to pop from the queue first
        let mut results = self.results.lock().unwrap();
        if !results.is_empty() {
            return results.remove(0);
        }
        drop(results);

        // Fall back to default result - take it and put back a clone of success part
        let default = self.default_result.lock().unwrap().take();
        match default {
            Some(Ok((args, output))) => {
                // Put a copy back for next call
                *self.default_result.lock().unwrap() = Some(Ok((args.clone(), output.clone())));
                Ok((args, output))
            }
            Some(Err(_)) => {
                // Can't clone the error, so create a new one
                Err(SemverguardError::Engine("configured error".into()))
            }
            None => Err(SemverguardError::Engine("no result configured".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use semverguard_types::{BaselineConfig, FeaturesConfig, WorkspacePackage};

    #[test]
    fn test_mock_workspace_provider_returns_configured_result() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/test/workspace"),
            packages: vec![WorkspacePackage {
                name: "test-pkg".into(),
                version: Version::new(1, 0, 0),
                manifest_path: PathBuf::from("/test/workspace/Cargo.toml"),
                package_root: PathBuf::from("/test/workspace"),
                publishable: true,
                has_lib: true,
            }],
        };

        let provider = MockWorkspaceProvider::with_result(Ok(metadata.clone()));
        let result = provider.load(Path::new("/test/workspace")).unwrap();

        assert_eq!(result.workspace_root, metadata.workspace_root);
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, "test-pkg");
    }

    #[test]
    fn test_mock_workspace_provider_records_calls() {
        let provider = MockWorkspaceProvider::empty("/test");

        let _ = provider.load(Path::new("/first"));
        provider.set_result(Ok(WorkspaceMetadata {
            workspace_root: PathBuf::from("/second"),
            packages: vec![],
        }));
        let _ = provider.load(Path::new("/second"));

        assert_eq!(provider.call_count(), 2);
        let calls = provider.calls();
        assert_eq!(calls[0].workspace_root, PathBuf::from("/first"));
        assert_eq!(calls[1].workspace_root, PathBuf::from("/second"));
    }

    #[test]
    fn test_mock_workspace_provider_returns_error() {
        let provider = MockWorkspaceProvider::with_error("test error");
        let result = provider.load(Path::new("/test"));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[test]
    fn test_mock_git_provider_returns_changed_paths() {
        let paths = vec![
            PathBuf::from("crates/foo/src/lib.rs"),
            PathBuf::from("crates/bar/Cargo.toml"),
        ];
        let provider = MockGitProvider::with_changed_paths(paths.clone());

        let result = provider
            .changed_paths(Path::new("/workspace"), "origin/main", "HEAD")
            .unwrap();

        assert_eq!(result, paths);
    }

    #[test]
    fn test_mock_git_provider_records_calls() {
        let provider = MockGitProvider::no_changes();

        let _ = provider.changed_paths(Path::new("/ws"), "base1", "head1");
        provider.set_result(Ok(vec![]));
        let _ = provider.changed_paths(Path::new("/ws"), "base2", "head2");

        assert_eq!(provider.call_count(), 2);
        let calls = provider.calls();
        assert_eq!(calls[0].base, "base1");
        assert_eq!(calls[1].base, "base2");
    }

    #[test]
    fn test_mock_git_provider_assert_called_once_with() {
        let provider = MockGitProvider::no_changes();
        let _ = provider.changed_paths(Path::new("/ws"), "origin/main", "HEAD");

        provider.assert_called_once_with(Path::new("/ws"), "origin/main", "HEAD");
    }

    #[test]
    fn test_mock_semver_engine_returns_success() {
        let engine = MockSemverEngine::success();
        let request = make_test_request();

        let (_, output) = engine.check(request).unwrap();

        assert!(output.success);
        assert_eq!(output.exit_code, Some(0));
    }

    #[test]
    fn test_mock_semver_engine_returns_failure() {
        let engine = MockSemverEngine::failure("breaking change detected");
        let request = make_test_request();

        let (_, output) = engine.check(request).unwrap();

        assert!(!output.success);
        assert_eq!(output.exit_code, Some(1));
        assert!(output.stderr.contains("breaking change"));
    }

    #[test]
    fn test_mock_semver_engine_multiple_results() {
        let engine = MockSemverEngine::with_outputs(vec![
            (
                vec![],
                SemverCheckOutput {
                    exit_code: Some(0),
                    success: true,
                    stdout: "first".into(),
                    stderr: String::new(),
                    required_bump: None,
                },
            ),
            (
                vec!["--verbose".into()],
                SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: "second".into(),
                    stderr: String::new(),
                    required_bump: None,
                },
            ),
        ]);

        let (_, out1) = engine.check(make_test_request()).unwrap();
        let (args2, out2) = engine.check(make_test_request()).unwrap();

        assert!(out1.success);
        assert_eq!(out1.stdout, "first");

        assert!(!out2.success);
        assert_eq!(out2.stdout, "second");
        assert_eq!(args2, vec!["--verbose"]);
    }

    #[test]
    fn test_mock_semver_engine_records_calls() {
        let engine = MockSemverEngine::success();

        let _ = engine.check(make_test_request());
        let _ = engine.check(make_test_request());

        assert_eq!(engine.call_count(), 2);
        engine.assert_call_count(2);
    }

    #[test]
    fn test_mock_semver_engine_checked_packages() {
        let engine = MockSemverEngine::success();
        engine.set_default(Ok((
            vec![],
            SemverCheckOutput {
                exit_code: Some(0),
                success: true,
                stdout: String::new(),
                stderr: String::new(),
                required_bump: None,
            },
        )));

        let mut req1 = make_test_request();
        req1.manifest_path = PathBuf::from("/ws/crates/foo/Cargo.toml");
        let _ = engine.check(req1);

        let mut req2 = make_test_request();
        req2.manifest_path = PathBuf::from("/ws/crates/bar/Cargo.toml");
        let _ = engine.check(req2);

        let packages = engine.checked_packages();
        assert_eq!(packages.len(), 2);
        assert!(packages.contains(&PathBuf::from("/ws/crates/foo/Cargo.toml")));
        assert!(packages.contains(&PathBuf::from("/ws/crates/bar/Cargo.toml")));
    }

    fn make_test_request() -> SemverCheckRequest {
        SemverCheckRequest {
            workspace_root: PathBuf::from("/test/workspace"),
            cargo_bin: None,
            manifest_path: PathBuf::from("/test/workspace/Cargo.toml"),
            baseline: BaselineConfig::default(),
            features: FeaturesConfig::default(),
            extra_args: vec![],
            timeout: None,
        }
    }
}
