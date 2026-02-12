//! Exit code computation from run reports.
//!
//! Exit code mapping:
//! - `0` - All checks passed (or baseline warnings in lenient mode)
//! - `1` - Tool/runtime error
//! - `2` - SemVer violation detected
//! - `3` - Baseline error (in strict mode or when warn_as_fail is set)

use semverguard_types::{FailureKind, PackageStatus, RunMode, RunReport};

/// Compute exit code from a run report.
///
/// This is used for non-receipt output modes (text, json, sarif).
/// For receipt mode, use `exit_code_from_receipt` instead.
pub fn exit_code_from_report(
    report: &RunReport,
    resolved_mode: RunMode,
    warn_as_fail: bool,
) -> i32 {
    let mut has_tool_error_flag = false;
    let mut has_semver_violation = false;
    let mut has_baseline_error = false;

    for pkg in &report.packages {
        if pkg.status != PackageStatus::Failed {
            continue;
        }
        match pkg.failure_kind.unwrap_or(FailureKind::Unknown) {
            FailureKind::ToolError => has_tool_error_flag = true,
            FailureKind::BaselineError => has_baseline_error = true,
            FailureKind::SemverViolation | FailureKind::Unknown => has_semver_violation = true,
        }
    }

    if has_tool_error_flag {
        1
    } else if has_semver_violation {
        2
    } else if has_baseline_error {
        // In Pr mode, baseline errors are warnings (exit 0 unless warn_as_fail is set)
        // In Release mode, baseline errors are failures (exit 3)
        let baseline_errors_are_warnings = resolved_mode.baseline_errors_are_warnings();
        if baseline_errors_are_warnings && !warn_as_fail {
            0
        } else {
            3
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{PackageReport, PackageStatus, Summary};
    use std::path::PathBuf;

    fn make_report(packages: Vec<PackageReport>) -> RunReport {
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
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages,
            summary: Summary {
                total,
                passed,
                failed,
                skipped,
            },
        }
    }

    fn failed_pkg(kind: FailureKind) -> PackageReport {
        PackageReport {
            name: "pkg".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: Some(kind),
            baseline_error: None,
        }
    }

    #[test]
    fn test_exit_0_on_pass() {
        let report = make_report(vec![]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, false), 0);
    }

    #[test]
    fn test_exit_code_ignores_non_failed_packages() {
        let report = make_report(vec![PackageReport {
            name: "pkg-pass".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            status: PackageStatus::Passed,
            skip_reason: None,
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: None,
            baseline_error: None,
        }]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, false), 0);
    }

    #[test]
    fn test_exit_1_on_tool_error() {
        let report = make_report(vec![failed_pkg(FailureKind::ToolError)]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, false), 1);
    }

    #[test]
    fn test_exit_2_on_semver_violation() {
        let report = make_report(vec![failed_pkg(FailureKind::SemverViolation)]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, false), 2);
    }

    #[test]
    fn test_exit_0_on_baseline_error_pr_mode() {
        let report = make_report(vec![failed_pkg(FailureKind::BaselineError)]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, false), 0);
    }

    #[test]
    fn test_exit_3_on_baseline_error_release_mode() {
        let report = make_report(vec![failed_pkg(FailureKind::BaselineError)]);
        assert_eq!(exit_code_from_report(&report, RunMode::Release, false), 3);
    }

    #[test]
    fn test_exit_3_on_baseline_error_warn_as_fail() {
        let report = make_report(vec![failed_pkg(FailureKind::BaselineError)]);
        assert_eq!(exit_code_from_report(&report, RunMode::Pr, true), 3);
    }
}
