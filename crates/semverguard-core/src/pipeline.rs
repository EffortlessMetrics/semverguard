//! Pipeline orchestration for semverguard.
//!
//! Provides `run_with_adapters()` for full pipeline execution (run checks,
//! build receipt, compute exit code) and a feature-gated `run()` convenience
//! that wires up default adapters.

use crate::capability::build_capability_context;
use crate::receipt::{
    CapabilityContext, ToolErrorFinding, build_artifact_index,
    build_receipt_with_capabilities_versioned, exit_code_from_receipt, has_tool_error,
    resolve_artifacts_dir, write_receipt_bundle,
};
use anyhow::{Context, Result};
use semverguard_domain::{
    GitProvider, ProgressCallback, SemverEngine, SemverguardRunner, WorkspaceProvider,
};
use semverguard_types::{BaselineKind, OutputFormat, RunReport, SemverguardConfig, SensorReportV1};
use std::path::PathBuf;
use std::sync::Arc;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Options for running the semverguard pipeline.
pub struct PipelineOptions {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Resolved configuration.
    pub config: SemverguardConfig,
    /// Whether to emit SARIF output.
    pub sarif: bool,
    /// Optional tool version override (defaults to core's CARGO_PKG_VERSION).
    pub tool_version: Option<String>,
    /// Optional progress callback.
    pub progress: Option<Arc<dyn ProgressCallback>>,
}

/// Result of a pipeline run.
pub struct PipelineResult {
    /// The receipt (sensor.report.v1).
    pub receipt: SensorReportV1,
    /// The underlying run report (if check completed successfully).
    pub report: Option<RunReport>,
    /// Computed exit code.
    pub exit_code: i32,
}

/// Run the full pipeline with explicit adapter injection.
///
/// This does NOT write files. The caller decides output.
pub fn run_with_adapters(
    options: &PipelineOptions,
    workspace: &dyn WorkspaceProvider,
    git: Option<&dyn GitProvider>,
    engine: &dyn SemverEngine,
) -> Result<PipelineResult> {
    let receipt_requested = matches!(options.config.output.format, OutputFormat::Receipt);
    let artifacts_root = resolve_artifacts_dir(
        &options.workspace_root,
        &options.config.output.artifacts_dir,
    );

    crate::config::ensure_workspace_root(&options.workspace_root)?;

    // Probe git availability
    let git_available = git.is_some() && matches!(options.config.baseline.kind, BaselineKind::Git);

    // Probe shallow clone if git adapter supports it
    #[cfg(feature = "default-adapters")]
    let shallow_clone = {
        // Try to detect shallow clone via the git adapter
        false // Will be overridden in run() convenience
    };
    #[cfg(not(feature = "default-adapters"))]
    let shallow_clone = false;

    let runner = if let Some(progress) = &options.progress {
        SemverguardRunner::with_progress(
            workspace,
            git,
            engine,
            ProgressCallbackWrapper(Arc::clone(progress)),
        )
    } else {
        SemverguardRunner::new(workspace, git, engine)
    };

    let started = OffsetDateTime::now_utc();
    let artifacts = match runner.run(&options.workspace_root, &options.config) {
        Ok(artifacts) => artifacts,
        Err(e) => {
            if !receipt_requested {
                return Err(anyhow::Error::new(e));
            }

            // Build error receipt
            let errors = vec![ToolErrorFinding::new(e.to_string())];
            let artifact_index = build_artifact_index(
                &options.workspace_root,
                &artifacts_root,
                None,
                options.sarif,
            );

            let git_skipped = !matches!(options.config.baseline.kind, BaselineKind::Git);

            let capability_ctx = CapabilityContext::new()
                .with_git_available(git_available)
                .with_baseline_available(false)
                .with_baseline_detail(format!("error: {e}"))
                .with_git_skipped(git_skipped);

            let receipt = build_receipt_with_capabilities_versioned(
                None,
                &errors,
                &artifact_index,
                &options.config.baseline,
                &options.workspace_root,
                Some(&capability_ctx),
                options.tool_version.as_deref(),
                &[],
            );

            let code = exit_code_from_receipt(
                &receipt.verdict,
                options.config.output.warn_as_fail,
                has_tool_error(&receipt.findings),
            );

            return Ok(PipelineResult {
                receipt,
                report: None,
                exit_code: code,
            });
        }
    };
    let finished = OffsetDateTime::now_utc();

    let tool_version = options
        .tool_version
        .as_deref()
        .unwrap_or(env!("CARGO_PKG_VERSION"));

    let report = RunReport {
        semverguard_version: tool_version.to_string(),
        started_at: started
            .format(&Rfc3339)
            .unwrap_or_else(|_| started.unix_timestamp().to_string()),
        finished_at: finished
            .format(&Rfc3339)
            .unwrap_or_else(|_| finished.unix_timestamp().to_string()),
        workspace_root: artifacts.workspace_root,
        packages: artifacts.packages,
        summary: artifacts.summary,
    };

    let artifact_index = build_artifact_index(
        &options.workspace_root,
        &artifacts_root,
        Some(&report),
        options.sarif,
    );

    let capability_ctx =
        build_capability_context(&options.config, &report, git_available, shallow_clone, None);

    let receipt = build_receipt_with_capabilities_versioned(
        Some(&report),
        &[],
        &artifact_index,
        &options.config.baseline,
        &options.workspace_root,
        Some(&capability_ctx),
        options.tool_version.as_deref(),
        &options.config.waivers,
    );

    let code = if receipt_requested {
        exit_code_from_receipt(
            &receipt.verdict,
            options.config.output.warn_as_fail,
            has_tool_error(&receipt.findings),
        )
    } else {
        let resolved_mode = options.config.mode.resolve();
        crate::exit_code::exit_code_from_report(
            &report,
            resolved_mode,
            options.config.output.warn_as_fail,
        )
    };

    Ok(PipelineResult {
        receipt,
        report: Some(report),
        exit_code: code,
    })
}

/// Convenience: run with default adapters (feature-gated).
#[cfg(feature = "default-adapters")]
pub fn run(options: &PipelineOptions) -> Result<PipelineResult> {
    let workspace = semverguard_workspace::CargoMetadataWorkspace::default();
    let git = semverguard_git::GitCli::default();
    let engine = semverguard_engine::CargoSemverChecksEngine::default();

    // Probe shallow clone
    let shallow_clone = git.is_shallow(&options.workspace_root).unwrap_or(false);
    let git_available = git.is_available(&options.workspace_root);
    let git_version = git.version(&options.workspace_root);

    let receipt_requested = matches!(options.config.output.format, OutputFormat::Receipt);
    let artifacts_root = resolve_artifacts_dir(
        &options.workspace_root,
        &options.config.output.artifacts_dir,
    );

    crate::config::ensure_workspace_root(&options.workspace_root)?;

    let runner = if let Some(progress) = &options.progress {
        SemverguardRunner::with_progress(
            &workspace,
            Some(&git),
            &engine,
            ProgressCallbackWrapper(Arc::clone(progress)),
        )
    } else {
        SemverguardRunner::new(&workspace, Some(&git), &engine)
    };

    let started = OffsetDateTime::now_utc();
    let artifacts = match runner.run(&options.workspace_root, &options.config) {
        Ok(artifacts) => artifacts,
        Err(e) => {
            if !receipt_requested {
                return Err(anyhow::Error::new(e));
            }

            let errors = vec![ToolErrorFinding::new(e.to_string())];
            let artifact_index = build_artifact_index(
                &options.workspace_root,
                &artifacts_root,
                None,
                options.sarif,
            );

            let git_skipped = !matches!(options.config.baseline.kind, BaselineKind::Git);

            let capability_ctx = CapabilityContext::new()
                .with_git_available(git_available)
                .with_baseline_available(false)
                .with_baseline_detail(format!("error: {e}"))
                .with_shallow_clone(shallow_clone)
                .with_git_skipped(git_skipped);

            let receipt = build_receipt_with_capabilities_versioned(
                None,
                &errors,
                &artifact_index,
                &options.config.baseline,
                &options.workspace_root,
                Some(&capability_ctx),
                options.tool_version.as_deref(),
                &[],
            );

            let code = exit_code_from_receipt(
                &receipt.verdict,
                options.config.output.warn_as_fail,
                has_tool_error(&receipt.findings),
            );

            return Ok(PipelineResult {
                receipt,
                report: None,
                exit_code: code,
            });
        }
    };
    let finished = OffsetDateTime::now_utc();

    let tool_version = options
        .tool_version
        .as_deref()
        .unwrap_or(env!("CARGO_PKG_VERSION"));

    let report = RunReport {
        semverguard_version: tool_version.to_string(),
        started_at: started
            .format(&Rfc3339)
            .unwrap_or_else(|_| started.unix_timestamp().to_string()),
        finished_at: finished
            .format(&Rfc3339)
            .unwrap_or_else(|_| finished.unix_timestamp().to_string()),
        workspace_root: artifacts.workspace_root,
        packages: artifacts.packages,
        summary: artifacts.summary,
    };

    let artifact_index = build_artifact_index(
        &options.workspace_root,
        &artifacts_root,
        Some(&report),
        options.sarif,
    );

    let capability_ctx = build_capability_context(
        &options.config,
        &report,
        git_available,
        shallow_clone,
        git_version,
    );

    let receipt = build_receipt_with_capabilities_versioned(
        Some(&report),
        &[],
        &artifact_index,
        &options.config.baseline,
        &options.workspace_root,
        Some(&capability_ctx),
        options.tool_version.as_deref(),
        &options.config.waivers,
    );

    let code = if receipt_requested {
        exit_code_from_receipt(
            &receipt.verdict,
            options.config.output.warn_as_fail,
            has_tool_error(&receipt.findings),
        )
    } else {
        let resolved_mode = options.config.mode.resolve();
        crate::exit_code::exit_code_from_report(
            &report,
            resolved_mode,
            options.config.output.warn_as_fail,
        )
    };

    Ok(PipelineResult {
        receipt,
        report: Some(report),
        exit_code: code,
    })
}

/// Wrapper to implement ProgressCallback for Arc<dyn ProgressCallback>.
struct ProgressCallbackWrapper(Arc<dyn ProgressCallback>);

impl ProgressCallback for ProgressCallbackWrapper {
    fn on_progress(&self, event: semverguard_domain::ProgressEvent) {
        self.0.on_progress(event);
    }
}

/// Write a pipeline result's receipt bundle to disk.
pub fn write_pipeline_receipt(result: &PipelineResult, options: &PipelineOptions) -> Result<()> {
    let artifacts_root = resolve_artifacts_dir(
        &options.workspace_root,
        &options.config.output.artifacts_dir,
    );

    write_receipt_bundle(
        &artifacts_root,
        &result.receipt,
        result.report.as_ref(),
        options.sarif,
        options.config.output.pretty_json,
    )
    .context("failed to write receipt bundle")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use semverguard_domain::{Result as DomainResult, SemverguardError};
    use semverguard_types::{
        OutputFormat, RequiredBump, RunMode, ScopeMode, SemverCheckOutput, SemverCheckRequest,
        SemverguardConfig, VerdictStatus, WorkspaceMetadata, WorkspacePackage,
    };
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::{fs, io};
    use tempfile::tempdir;

    struct MockWorkspaceProvider {
        metadata: WorkspaceMetadata,
    }

    impl WorkspaceProvider for MockWorkspaceProvider {
        fn load(&self, _workspace_root: &Path) -> DomainResult<WorkspaceMetadata> {
            Ok(self.metadata.clone())
        }
    }

    struct FailingWorkspaceProvider {
        message: String,
    }

    impl WorkspaceProvider for FailingWorkspaceProvider {
        fn load(&self, _workspace_root: &Path) -> DomainResult<WorkspaceMetadata> {
            Err(SemverguardError::Workspace(self.message.clone()))
        }
    }

    enum EngineOutcome {
        Success,
        SemverFail,
        Error,
    }

    struct MockEngine {
        outcome: EngineOutcome,
    }

    impl SemverEngine for MockEngine {
        fn check(
            &self,
            _request: SemverCheckRequest,
        ) -> DomainResult<(Vec<String>, SemverCheckOutput)> {
            match self.outcome {
                EngineOutcome::Success => Ok((
                    vec!["cargo".to_string(), "semver-checks".to_string()],
                    SemverCheckOutput {
                        exit_code: Some(0),
                        success: true,
                        stdout: "ok".to_string(),
                        stderr: String::new(),
                        required_bump: None,
                    },
                )),
                EngineOutcome::SemverFail => Ok((
                    vec!["cargo".to_string(), "semver-checks".to_string()],
                    SemverCheckOutput {
                        exit_code: Some(1),
                        success: false,
                        stdout: String::new(),
                        stderr: "breaking change".to_string(),
                        required_bump: Some(RequiredBump::Major),
                    },
                )),
                EngineOutcome::Error => Err(SemverguardError::Engine("engine failed".to_string())),
            }
        }
    }

    #[derive(Default)]
    struct RecordingProgress {
        events: Arc<Mutex<Vec<semverguard_domain::ProgressEvent>>>,
    }

    impl ProgressCallback for RecordingProgress {
        fn on_progress(&self, event: semverguard_domain::ProgressEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn basic_metadata(workspace_root: &Path) -> WorkspaceMetadata {
        let pkg_root = workspace_root.join("crates").join("my-crate");
        let manifest_path = pkg_root.join("Cargo.toml");
        WorkspaceMetadata {
            workspace_root: workspace_root.to_path_buf(),
            packages: vec![WorkspacePackage {
                name: "my-crate".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                manifest_path,
                package_root: pkg_root,
                publishable: true,
                has_lib: true,
            }],
        }
    }

    fn base_config() -> SemverguardConfig {
        let mut config = SemverguardConfig::default();
        config.mode = RunMode::Release;
        config.scope.mode = ScopeMode::Workspace;
        config
    }

    fn write_minimal_workspace(root: &Path) -> io::Result<()> {
        let crate_root = root.join("crates").join("my-crate");
        fs::create_dir_all(crate_root.join("src"))?;
        let workspace_toml = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/my-crate\"]\n";
        fs::write(root.join("Cargo.toml"), workspace_toml)?;
        let crate_toml = "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n";
        fs::write(crate_root.join("Cargo.toml"), crate_toml)?;
        fs::write(crate_root.join("src/lib.rs"), "pub fn hello() {}\n")?;
        Ok(())
    }

    fn write_fake_cargo(dir: &Path) -> io::Result<PathBuf> {
        #[cfg(windows)]
        let (path, script) = {
            let path = dir.join("fake-cargo.cmd");
            let script = "@echo off\r\nif \"%1\"==\"semver-checks\" (\r\n  echo No breaking changes detected.\r\n  exit /b 0\r\n)\r\nexit /b 1\r\n";
            (path, script.to_string())
        };
        #[cfg(not(windows))]
        let (path, script) = {
            let path = dir.join("fake-cargo");
            let script = "#!/bin/sh\nif [ \"$1\" = \"semver-checks\" ]; then\n  echo \"No breaking changes detected.\"\n  exit 0\nfi\nexit 1\n";
            (path, script.to_string())
        };
        fs::write(&path, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }
        Ok(path)
    }

    #[test]
    fn test_run_with_adapters_non_receipt_uses_report_exit_code() {
        let dir = tempdir().unwrap();
        let metadata = basic_metadata(dir.path());
        let workspace = MockWorkspaceProvider { metadata };
        let engine = MockEngine {
            outcome: EngineOutcome::SemverFail,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Text;

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: Some("0.0.0-test".to_string()),
            progress: None,
        };

        let result = run_with_adapters(&options, &workspace, None, &engine).unwrap();
        assert_eq!(result.exit_code, 2);
        let report = result.report.as_ref().unwrap();
        assert_eq!(report.summary.failed, 1);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Fail);
    }

    #[test]
    fn test_run_with_adapters_non_receipt_tool_error_exit_code() {
        let dir = tempdir().unwrap();
        let metadata = basic_metadata(dir.path());
        let workspace = MockWorkspaceProvider { metadata };
        let engine = MockEngine {
            outcome: EngineOutcome::Error,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Text;

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: Some("0.0.0-test".to_string()),
            progress: None,
        };

        let result = run_with_adapters(&options, &workspace, None, &engine).unwrap();
        assert_eq!(result.exit_code, 1);
        let report = result.report.as_ref().unwrap();
        assert_eq!(report.summary.failed, 1);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Fail);
    }

    #[test]
    fn test_run_with_adapters_receipt_success_records_progress() {
        let dir = tempdir().unwrap();
        let metadata = basic_metadata(dir.path());
        let workspace = MockWorkspaceProvider { metadata };
        let engine = MockEngine {
            outcome: EngineOutcome::Success,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Receipt;

        let events: Arc<Mutex<Vec<semverguard_domain::ProgressEvent>>> =
            Arc::new(Mutex::new(Vec::new()));
        let recorder: Arc<dyn ProgressCallback> = Arc::new(RecordingProgress {
            events: Arc::clone(&events),
        });

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: Some("0.0.0-test".to_string()),
            progress: Some(recorder),
        };

        let result = run_with_adapters(&options, &workspace, None, &engine).unwrap();
        assert!(result.report.is_some());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Pass);

        let events = events.lock().unwrap();
        let has_total = events.iter().any(|event| {
            matches!(
                event,
                semverguard_domain::ProgressEvent::TotalPackages { total: 1 }
            )
        });
        assert!(has_total);
        let has_finished = events.iter().any(|event| {
            matches!(
                event,
                semverguard_domain::ProgressEvent::Finished {
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                }
            )
        });
        assert!(has_finished);
    }

    #[test]
    fn test_run_with_adapters_receipt_error_builds_tool_receipt() {
        let dir = tempdir().unwrap();
        let workspace = FailingWorkspaceProvider {
            message: "boom".to_string(),
        };
        let engine = MockEngine {
            outcome: EngineOutcome::Success,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Receipt;

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: None,
            progress: None,
        };

        let result = run_with_adapters(&options, &workspace, None, &engine).unwrap();
        assert!(result.report.is_none());
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Fail);
        assert!(
            result
                .receipt
                .findings
                .iter()
                .any(|f| f.check_id == crate::receipt::CHECK_TOOL)
        );
    }

    #[test]
    fn test_run_with_adapters_non_receipt_propagates_error() {
        let dir = tempdir().unwrap();
        let workspace = FailingWorkspaceProvider {
            message: "load failed".to_string(),
        };
        let engine = MockEngine {
            outcome: EngineOutcome::Success,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Text;

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: None,
            progress: None,
        };

        let err = run_with_adapters(&options, &workspace, None, &engine)
            .err()
            .unwrap();
        assert!(err.to_string().contains("workspace error"));
        assert!(err.to_string().contains("load failed"));
    }

    #[test]
    fn test_write_pipeline_receipt_writes_files() {
        let dir = tempdir().unwrap();
        let metadata = basic_metadata(dir.path());
        let workspace = MockWorkspaceProvider { metadata };
        let engine = MockEngine {
            outcome: EngineOutcome::Success,
        };
        let mut config = base_config();
        config.output.format = OutputFormat::Receipt;

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: true,
            tool_version: Some("0.0.0-test".to_string()),
            progress: None,
        };

        let result = run_with_adapters(&options, &workspace, None, &engine).unwrap();
        write_pipeline_receipt(&result, &options).unwrap();

        let artifacts_dir = crate::receipt::resolve_artifacts_dir(
            &options.workspace_root,
            &options.config.output.artifacts_dir,
        );
        assert!(artifacts_dir.join("report.json").exists());
        assert!(artifacts_dir.join("comment.md").exists());
        assert!(artifacts_dir.join("sarif.json").exists());
        assert!(artifacts_dir.join("raw").exists());
    }

    #[cfg(feature = "default-adapters")]
    #[test]
    fn test_run_default_adapters_success_with_fake_cargo() {
        let dir = tempdir().unwrap();
        write_minimal_workspace(dir.path()).unwrap();
        let fake_cargo = write_fake_cargo(dir.path()).unwrap();

        let mut config = base_config();
        config.output.format = OutputFormat::Receipt;
        config.engine.cargo_bin = Some(fake_cargo);

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: Some("0.0.0-test".to_string()),
            progress: None,
        };

        let result = run(&options).expect("run should succeed");
        assert!(result.report.is_some());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Pass);
    }

    #[cfg(feature = "default-adapters")]
    #[test]
    fn test_run_default_adapters_invalid_glob_builds_tool_receipt() {
        let dir = tempdir().unwrap();
        write_minimal_workspace(dir.path()).unwrap();

        let mut config = base_config();
        config.output.format = OutputFormat::Receipt;
        config.scope.include = vec!["[invalid".to_string()];

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: None,
            progress: None,
        };

        let result = run(&options).expect("run should return tool receipt");
        assert!(result.report.is_none());
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.receipt.verdict.status, VerdictStatus::Fail);
        assert!(
            result
                .receipt
                .findings
                .iter()
                .any(|f| f.check_id == crate::receipt::CHECK_TOOL)
        );
    }

    #[cfg(feature = "default-adapters")]
    #[test]
    fn test_run_default_adapters_text_with_progress() {
        let dir = tempdir().unwrap();
        write_minimal_workspace(dir.path()).unwrap();
        let fake_cargo = write_fake_cargo(dir.path()).unwrap();

        let mut config = base_config();
        config.output.format = OutputFormat::Text;
        config.engine.cargo_bin = Some(fake_cargo);

        let recorder = RecordingProgress::default();
        let events = Arc::clone(&recorder.events);
        let progress: Arc<dyn ProgressCallback> = Arc::new(recorder);

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: Some("0.0.0-test".to_string()),
            progress: Some(progress),
        };

        let result = run(&options).expect("run should succeed");
        assert!(result.report.is_some());
        assert_eq!(result.exit_code, 0);

        let events = events.lock().unwrap();
        assert!(!events.is_empty());
    }

    #[cfg(feature = "default-adapters")]
    #[test]
    fn test_run_default_adapters_invalid_glob_non_receipt_returns_err() {
        let dir = tempdir().unwrap();
        write_minimal_workspace(dir.path()).unwrap();

        let mut config = base_config();
        config.output.format = OutputFormat::Text;
        config.scope.include = vec!["[invalid".to_string()];

        let options = PipelineOptions {
            workspace_root: dir.path().to_path_buf(),
            config,
            sarif: false,
            tool_version: None,
            progress: None,
        };

        let result = run(&options);
        assert!(result.is_err());
    }
}
