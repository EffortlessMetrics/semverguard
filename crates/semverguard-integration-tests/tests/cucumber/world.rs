//! Test world state for cucumber BDD tests.
//!
//! The `TestWorld` struct holds all state needed across step definitions,
//! including mock configurations, workspace packages, and results.

use cucumber::World;
use semver::Version;
use semverguard_domain::{
    MockGitProvider, MockSemverEngine, MockWorkspaceProvider, RunArtifacts, SemverguardError,
    SemverguardRunner,
};
use semverguard_types::{
    BaselineConfig, EngineConfig, FeaturesConfig, ListResult, OutputConfig, ScopeConfig, ScopeMode,
    SemverCheckOutput, SemverguardConfig, WorkspaceMetadata, WorkspacePackage,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Test world containing all state for BDD scenarios.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct TestWorld {
    /// Packages in the workspace.
    pub packages: Vec<WorkspacePackage>,

    /// Workspace root path.
    pub workspace_root: PathBuf,

    /// Configuration for the run.
    pub config: SemverguardConfig,

    /// Changed file paths (for git provider).
    pub changed_paths: Vec<PathBuf>,

    /// Engine results keyed by package name.
    pub engine_results: HashMap<String, EngineResult>,

    /// Default engine result for packages without specific configuration.
    pub default_engine_result: EngineResult,

    /// Result of running the check.
    pub run_result: Option<Result<RunArtifacts, SemverguardError>>,

    /// Result of listing packages.
    pub list_result: Option<Result<ListResult, SemverguardError>>,

    /// Whether baseline rev should be unset (for testing validation).
    pub baseline_rev_unset: bool,
}

/// Result configuration for the mock engine.
#[derive(Debug, Clone)]
pub enum EngineResult {
    /// Package passes semver check.
    Pass,
    /// Package fails semver check.
    Fail,
    /// Engine returns an error.
    Error(String),
}

impl Default for EngineResult {
    fn default() -> Self {
        EngineResult::Pass
    }
}

impl TestWorld {
    /// Create a new test world with default values.
    pub fn new() -> Self {
        Self {
            packages: Vec::new(),
            workspace_root: PathBuf::from("/workspace"),
            config: default_config(),
            changed_paths: Vec::new(),
            engine_results: HashMap::new(),
            default_engine_result: EngineResult::Pass,
            run_result: None,
            list_result: None,
            baseline_rev_unset: false,
        }
    }

    /// Add a package to the workspace.
    pub fn add_package(
        &mut self,
        name: &str,
        version: &str,
        publishable: bool,
        has_lib: bool,
        path: Option<&str>,
    ) {
        let package_path = path
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_root.join(name));
        let manifest_path = package_path.join("Cargo.toml");

        self.packages.push(WorkspacePackage {
            name: name.to_string(),
            version: Version::parse(version).unwrap_or_else(|_| Version::new(1, 0, 0)),
            manifest_path,
            package_root: package_path,
            publishable,
            has_lib,
        });
    }

    /// Build the workspace metadata from configured packages.
    pub fn build_workspace_metadata(&self) -> WorkspaceMetadata {
        WorkspaceMetadata {
            workspace_root: self.workspace_root.clone(),
            packages: self.packages.clone(),
        }
    }

    /// Build mock workspace provider.
    pub fn build_workspace_provider(&self) -> MockWorkspaceProvider {
        MockWorkspaceProvider::with_result(Ok(self.build_workspace_metadata()))
    }

    /// Build mock git provider.
    pub fn build_git_provider(&self) -> MockGitProvider {
        MockGitProvider::with_changed_paths(self.changed_paths.clone())
    }

    /// Build mock semver engine.
    pub fn build_engine(&self) -> MockSemverEngine {
        let engine = MockSemverEngine::success();

        // Set up results for specific packages
        let results: Vec<_> = self
            .packages
            .iter()
            .filter_map(|pkg| {
                let result = self
                    .engine_results
                    .get(&pkg.name)
                    .cloned()
                    .unwrap_or_else(|| self.default_engine_result.clone());
                Some(engine_result_to_output(&result))
            })
            .collect();

        if !results.is_empty() {
            let engine = MockSemverEngine::with_results(results);
            return engine;
        }

        // Set default result
        engine.set_default(engine_result_to_output(&self.default_engine_result));
        engine
    }

    /// Execute the semverguard run.
    pub fn execute_run(&mut self) {
        let workspace = self.build_workspace_provider();
        let git = self.build_git_provider();
        let engine = self.build_engine();

        // Handle baseline_rev_unset for testing
        let mut config = self.config.clone();
        if self.baseline_rev_unset {
            config.baseline.rev = None;
        }

        let runner = if matches!(config.scope.mode, ScopeMode::Changed) {
            SemverguardRunner::new(&workspace, Some(&git), &engine)
        } else {
            SemverguardRunner::new(&workspace, None, &engine)
        };

        self.run_result = Some(runner.run(Path::new(&self.workspace_root), &config));
    }

    /// Execute the list packages command.
    pub fn execute_list(&mut self) {
        let workspace = self.build_workspace_provider();
        let git = self.build_git_provider();
        let engine = self.build_engine();

        let mut config = self.config.clone();
        if self.baseline_rev_unset {
            config.baseline.rev = None;
        }

        let runner = if matches!(config.scope.mode, ScopeMode::Changed) {
            SemverguardRunner::new(&workspace, Some(&git), &engine)
        } else {
            SemverguardRunner::new(&workspace, None, &engine)
        };

        self.list_result = Some(runner.list_packages(Path::new(&self.workspace_root), &config));
    }
}

/// Convert an EngineResult to the format expected by MockSemverEngine.
fn engine_result_to_output(
    result: &EngineResult,
) -> Result<(Vec<String>, SemverCheckOutput), SemverguardError> {
    match result {
        EngineResult::Pass => Ok((
            vec!["cargo".to_string(), "semver-checks".to_string()],
            SemverCheckOutput {
                exit_code: Some(0),
                success: true,
                stdout: "No breaking changes detected".to_string(),
                stderr: String::new(),
                required_bump: None,
            },
        )),
        EngineResult::Fail => Ok((
            vec!["cargo".to_string(), "semver-checks".to_string()],
            SemverCheckOutput {
                exit_code: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "Breaking change detected".to_string(),
                required_bump: Some(semverguard_types::RequiredBump::Major),
            },
        )),
        EngineResult::Error(msg) => Err(SemverguardError::Engine(msg.clone())),
    }
}

/// Create default configuration for testing.
fn default_config() -> SemverguardConfig {
    SemverguardConfig {
        baseline: BaselineConfig::default(),
        scope: ScopeConfig {
            mode: ScopeMode::Workspace,
            include: vec![],
            exclude: vec![],
            explicit_packages: vec![],
            skip_publish_false: false,
            skip_no_lib: false,
        },
        features: FeaturesConfig::default(),
        engine: EngineConfig::default(),
        output: OutputConfig::default(),
    }
}
