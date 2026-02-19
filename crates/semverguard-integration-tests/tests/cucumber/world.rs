//! Test world state for cucumber BDD tests.
//!
//! The `TestWorld` struct holds all state needed across step definitions,
//! including mock configurations, workspace packages, and results.

use cucumber::World;
use semver::Version;
use semverguard_core::pipeline::{
    PipelineOptions, PipelineResult, run_with_adapters, write_pipeline_receipt,
};
use semverguard_core::resolve_artifacts_dir;
use semverguard_domain::{
    MockGitProvider, MockSemverEngine, MockWorkspaceProvider, RunArtifacts, SemverguardError,
    SemverguardRunner,
};
use semverguard_types::{
    BaselineKind, ListResult, RunReport, ScopeConfig, ScopeMode, SemverCheckOutput,
    SemverguardConfig, SensorReportV1, WorkspaceMetadata, WorkspacePackage,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test world containing all state for BDD scenarios.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct TestWorld {
    /// Temporary workspace directory for pipeline-level checks that require an existing root.
    pub _temp_workspace: TempDir,

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

    /// Result of running the full core pipeline.
    pub pipeline_result: Option<Result<PipelineRunResult, String>>,

    /// Result of writing the pipeline receipt bundle.
    pub receipt_write_result: Option<Result<(), String>>,

    /// Whether baseline rev should be unset (for testing validation).
    pub baseline_rev_unset: bool,

    /// Whether to request SARIF in pipeline runs.
    pub pipeline_sarif: bool,
}

/// Minimal snapshot of pipeline output for cucumber assertions.
#[derive(Debug, Clone)]
pub struct PipelineRunResult {
    /// The final sensor report.
    pub receipt: SensorReportV1,
    /// The run report if execution completed.
    pub report: Option<RunReport>,
    /// Computed exit code.
    pub exit_code: i32,
}

/// Result configuration for the mock engine.
#[derive(Debug, Clone)]
pub enum EngineResult {
    /// Package passes semver check.
    Pass,
    /// Package fails semver check.
    Fail,
    /// Package fails due to baseline lookup/comparison errors.
    BaselineFail,
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
        let temp_workspace = TempDir::new().expect("failed to create temporary workspace");
        Self {
            workspace_root: temp_workspace.path().to_path_buf(),
            _temp_workspace: temp_workspace,
            packages: Vec::new(),
            config: default_config(),
            changed_paths: Vec::new(),
            engine_results: HashMap::new(),
            default_engine_result: EngineResult::Pass,
            run_result: None,
            list_result: None,
            pipeline_result: None,
            receipt_write_result: None,
            baseline_rev_unset: false,
            pipeline_sarif: false,
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
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    self.workspace_root.join(path)
                }
            })
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

        let runner = if config.scope.mode == ScopeMode::Changed {
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

        let runner = if config.scope.mode == ScopeMode::Changed {
            SemverguardRunner::new(&workspace, Some(&git), &engine)
        } else {
            SemverguardRunner::new(&workspace, None, &engine)
        };

        self.list_result = Some(runner.list_packages(Path::new(&self.workspace_root), &config));
    }

    /// Execute the core pipeline and capture exit-code semantics.
    pub fn execute_pipeline_run(&mut self) {
        let workspace = self.build_workspace_provider();
        let git = self.build_git_provider();
        let engine = self.build_engine();
        let options = self.pipeline_options();

        // Always pass git adapter; behavior still depends on config/baseline mode.
        self.pipeline_result = Some(
            run_with_adapters(&options, &workspace, Some(&git), &engine)
                .map(|result| PipelineRunResult {
                    receipt: result.receipt,
                    report: result.report,
                    exit_code: result.exit_code,
                })
                .map_err(|e| e.to_string()),
        );
    }

    /// Write pipeline receipt artifacts for the last successful pipeline run.
    pub fn execute_write_pipeline_receipt(&mut self) {
        let options = self.pipeline_options();
        let result = match self.pipeline_result.as_ref() {
            Some(Ok(result)) => {
                let pipeline = PipelineResult {
                    receipt: result.receipt.clone(),
                    report: result.report.clone(),
                    exit_code: result.exit_code,
                };
                write_pipeline_receipt(&pipeline, &options).map_err(|e| e.to_string())
            }
            Some(Err(err)) => Err(format!(
                "cannot write receipt because pipeline run failed: {err}"
            )),
            None => Err("cannot write receipt before running pipeline".to_string()),
        };
        self.receipt_write_result = Some(result);
    }

    /// Resolve the absolute artifacts directory for receipt assertions.
    pub fn artifacts_dir(&self) -> PathBuf {
        resolve_artifacts_dir(&self.workspace_root, &self.config.output.artifacts_dir)
    }

    fn pipeline_options(&self) -> PipelineOptions {
        let mut config = self.config.clone();
        if self.baseline_rev_unset {
            config.baseline.rev = None;
        }

        // Use a fixed test version to keep assertions deterministic.
        PipelineOptions {
            workspace_root: self.workspace_root.clone(),
            config,
            sarif: self.pipeline_sarif,
            tool_version: Some("0.1.0-test".to_string()),
            progress: None,
        }
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
        EngineResult::BaselineFail => Ok((
            vec!["cargo".to_string(), "semver-checks".to_string()],
            SemverCheckOutput {
                exit_code: Some(2),
                success: false,
                stdout: String::new(),
                stderr: "error: unknown revision 'origin/missing' for baseline".to_string(),
                required_bump: None,
            },
        )),
        EngineResult::Error(msg) => Err(SemverguardError::Engine(msg.clone())),
    }
}

/// Create default configuration for testing.
fn default_config() -> SemverguardConfig {
    let mut config = SemverguardConfig {
        scope: ScopeConfig {
            mode: ScopeMode::Workspace,
            include: vec![],
            exclude: vec![],
            explicit_packages: vec![],
            skip_publish_false: false,
            skip_no_lib: false,
        },
        ..Default::default()
    };
    config.baseline.kind = BaselineKind::Git;
    config.baseline.rev = Some("origin/main".to_string());
    config
}
