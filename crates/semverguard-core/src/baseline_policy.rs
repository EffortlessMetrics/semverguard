//! Baseline error policy application.
//!
//! This module rewrites package statuses for baseline errors according to:
//! - configured `baseline.on_error.*` actions
//! - mode defaults (`pr`/`cockpit` downgrade fail -> warn)
//!
//! It runs after engine execution and before rendering/exit-code logic.

use semverguard_types::{
    BaselineErrorCause, ErrorAction, FailureKind, PackageReport, PackageStatus, RunMode, RunReport,
    SemverguardConfig, Summary,
};

/// Apply baseline error policy to a run report in-place.
pub(crate) fn apply_baseline_error_policy(report: &mut RunReport, config: &SemverguardConfig) {
    let resolved_mode = config.mode.resolve();

    for pkg in &mut report.packages {
        if pkg.failure_kind != Some(FailureKind::BaselineError) {
            continue;
        }

        let action = effective_action(pkg.baseline_error.as_ref(), config, resolved_mode);
        match action {
            ErrorAction::Fail => {
                pkg.status = PackageStatus::Failed;
            }
            ErrorAction::Warn => {
                pkg.status = PackageStatus::Skipped;
                pkg.skip_reason = Some(format!("baseline warning: {}", baseline_detail(pkg)));
            }
            ErrorAction::Skip => {
                pkg.status = PackageStatus::Skipped;
                pkg.skip_reason = Some(format!("baseline skipped: {}", baseline_detail(pkg)));
                pkg.failure_kind = None;
                pkg.baseline_error = None;
            }
        }
    }

    report.summary = summarize(&report.packages);
}

fn effective_action(
    cause: Option<&BaselineErrorCause>,
    config: &SemverguardConfig,
    resolved_mode: RunMode,
) -> ErrorAction {
    let mut action = configured_action(cause, config);

    // Release lanes are strict: warnings become failures unless explicitly skipped.
    if resolved_mode == RunMode::Release && action == ErrorAction::Warn {
        action = ErrorAction::Fail;
    }

    // PR/Cockpit lanes are lenient by default: baseline failures become warnings.
    if action == ErrorAction::Fail && resolved_mode.baseline_errors_are_warnings() {
        action = ErrorAction::Warn;
    }

    action
}

fn configured_action(
    cause: Option<&BaselineErrorCause>,
    config: &SemverguardConfig,
) -> ErrorAction {
    match cause {
        Some(BaselineErrorCause::CrateAbsentFromBaseline { .. }) => {
            config.baseline.on_error.new_crate
        }
        Some(BaselineErrorCause::RevisionNotFound { .. }) => {
            config.baseline.on_error.missing_revision
        }
        Some(BaselineErrorCause::ShallowClone { .. }) => config.baseline.on_error.shallow_clone,
        Some(BaselineErrorCause::MergeBaseNotFound { .. }) => {
            config.baseline.on_error.missing_revision
        }
        Some(BaselineErrorCause::RustdocGenerationFailed { .. }) => {
            config.baseline.on_error.rustdoc_failure
        }
        Some(BaselineErrorCause::NotPublished { .. }) => config.baseline.on_error.not_published,
        Some(BaselineErrorCause::Other { .. }) | None => config.baseline.on_error.missing_revision,
    }
}

fn baseline_detail(pkg: &PackageReport) -> String {
    if let Some(cause) = &pkg.baseline_error {
        return cause.to_string();
    }

    if let Some(reason) = &pkg.skip_reason {
        if !reason.trim().is_empty() {
            return reason.clone();
        }
    }

    if let Some(engine) = &pkg.engine {
        let stderr = engine.stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_string();
        }
    }

    "baseline comparison failed".to_string()
}

fn summarize(packages: &[PackageReport]) -> Summary {
    let total = packages.len();
    let passed = packages
        .iter()
        .filter(|p| p.status == PackageStatus::Passed)
        .count();
    let failed = packages
        .iter()
        .filter(|p| p.status == PackageStatus::Failed)
        .count();
    let skipped = packages
        .iter()
        .filter(|p| p.status == PackageStatus::Skipped)
        .count();

    Summary {
        total,
        passed,
        failed,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{BaselineKind, PackageStatus, Summary};
    use std::path::PathBuf;

    fn report_with_cause(cause: BaselineErrorCause) -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "pkg".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/pkg/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 10,
                command: vec!["cargo".to_string()],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::BaselineError),
                baseline_error: Some(cause),
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        }
    }

    fn base_config(mode: RunMode) -> SemverguardConfig {
        let mut cfg = SemverguardConfig::default();
        cfg.mode = mode;
        cfg.baseline.kind = BaselineKind::Git;
        cfg.baseline.rev = Some("origin/main".to_string());
        cfg
    }

    #[test]
    fn test_pr_mode_downgrades_baseline_fail_to_warn_skip() {
        let mut report = report_with_cause(BaselineErrorCause::RevisionNotFound {
            rev: "origin/missing".to_string(),
        });
        let cfg = base_config(RunMode::Pr);

        apply_baseline_error_policy(&mut report, &cfg);

        assert_eq!(report.packages[0].status, PackageStatus::Skipped);
        assert_eq!(
            report.packages[0].failure_kind,
            Some(FailureKind::BaselineError)
        );
        assert!(
            report.packages[0]
                .skip_reason
                .as_deref()
                .unwrap_or_default()
                .starts_with("baseline warning:")
        );
        assert_eq!(report.summary.failed, 0);
        assert_eq!(report.summary.skipped, 1);
    }

    #[test]
    fn test_release_mode_keeps_baseline_fail() {
        let mut report = report_with_cause(BaselineErrorCause::RevisionNotFound {
            rev: "origin/missing".to_string(),
        });
        let cfg = base_config(RunMode::Release);

        apply_baseline_error_policy(&mut report, &cfg);

        assert_eq!(report.packages[0].status, PackageStatus::Failed);
        assert_eq!(report.summary.failed, 1);
        assert_eq!(report.summary.skipped, 0);
    }

    #[test]
    fn test_release_mode_upgrades_baseline_warn_to_fail() {
        let mut report = report_with_cause(BaselineErrorCause::NotPublished {
            crate_name: "internal".to_string(),
            version: None,
        });
        let cfg = base_config(RunMode::Release);

        apply_baseline_error_policy(&mut report, &cfg);

        assert_eq!(report.packages[0].status, PackageStatus::Failed);
        assert_eq!(report.summary.failed, 1);
    }

    #[test]
    fn test_configured_skip_clears_baseline_failure() {
        let mut report = report_with_cause(BaselineErrorCause::CrateAbsentFromBaseline {
            crate_name: "new-crate".to_string(),
        });
        let mut cfg = base_config(RunMode::Release);
        cfg.baseline.on_error.new_crate = ErrorAction::Skip;

        apply_baseline_error_policy(&mut report, &cfg);

        assert_eq!(report.packages[0].status, PackageStatus::Skipped);
        assert_eq!(report.packages[0].failure_kind, None);
        assert_eq!(report.packages[0].baseline_error, None);
        assert!(
            report.packages[0]
                .skip_reason
                .as_deref()
                .unwrap_or_default()
                .starts_with("baseline skipped:")
        );
    }
}
