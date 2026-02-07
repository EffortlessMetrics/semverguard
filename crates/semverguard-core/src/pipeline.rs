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
