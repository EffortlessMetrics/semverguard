//! Capability context building for receipt generation.
//!
//! Determines capability status based on configuration, run results,
//! and proactive git probing.

use crate::receipt::CapabilityContext;
use semverguard_types::{BaselineKind, FailureKind, PackageStatus, RunReport, SemverguardConfig};

/// Build capability context for receipt generation.
///
/// Determines capability status based on configuration, run results,
/// and actual git probe results.
pub fn build_capability_context(
    config: &SemverguardConfig,
    report: &RunReport,
    git_available: bool,
    shallow_clone: bool,
    git_version: Option<String>,
) -> CapabilityContext {
    let git_detail = if git_available {
        config.baseline.rev.clone()
    } else {
        None
    };

    // Baseline is available if we didn't have any baseline errors
    let baseline_available = !report.packages.iter().any(|p| {
        p.status == PackageStatus::Failed && p.failure_kind == Some(FailureKind::BaselineError)
    });
    let baseline_detail = if baseline_available {
        match config.baseline.kind {
            BaselineKind::Git => config.baseline.rev.clone().map(|r| format!("git:{r}")),
            BaselineKind::CratesIo => config
                .baseline
                .version
                .clone()
                .map(|v| format!("crates-io:{v}")),
        }
    } else {
        Some("baseline resolution failed".to_string())
    };

    let git_skipped = !matches!(config.baseline.kind, BaselineKind::Git);

    let mut ctx = CapabilityContext::new()
        .with_git_available(git_available)
        .with_baseline_available(baseline_available)
        .with_shallow_clone(shallow_clone)
        .with_git_skipped(git_skipped);

    if let Some(ver) = git_version {
        ctx = ctx.with_git_version(ver);
    }
    if let Some(detail) = git_detail {
        ctx = ctx.with_git_detail(detail);
    }
    if let Some(detail) = baseline_detail {
        ctx = ctx.with_baseline_detail(detail);
    }

    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{CapabilityStatus, PackageReport, PackageStatus, Summary};
    use std::path::PathBuf;

    fn empty_report() -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
            summary: Summary {
                total: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
            },
        }
    }

    #[test]
    fn test_git_available_with_rev() {
        let mut config = SemverguardConfig::default();
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let ctx = build_capability_context(&config, &empty_report(), true, false, None);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Available);
        assert_eq!(caps.git.detail, Some("origin/main".to_string()));
    }

    #[test]
    fn test_git_unavailable() {
        let mut config = SemverguardConfig::default();
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());
        let ctx = build_capability_context(&config, &empty_report(), false, false, None);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Unavailable);
        assert_eq!(caps.git.reason, Some("git_unavailable".to_string()));
    }

    #[test]
    fn test_shallow_clone_appended_to_git_detail() {
        let mut config = SemverguardConfig::default();
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let ctx = build_capability_context(&config, &empty_report(), true, true, None);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Available);
        assert!(caps.git.detail.as_ref().unwrap().contains("shallow clone"));
        assert!(caps.git.detail.as_ref().unwrap().contains("origin/main"));
    }

    #[test]
    fn test_crates_io_baseline_skips_git() {
        let mut config = SemverguardConfig::default();
        config.baseline.kind = BaselineKind::CratesIo;

        let ctx = build_capability_context(&config, &empty_report(), true, false, None);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Skipped);
        assert_eq!(caps.git.reason, Some("not_required".to_string()));
        // Baseline should still be available
        assert_eq!(caps.baseline.status, CapabilityStatus::Available);
    }

    #[test]
    fn test_baseline_error_detected() {
        let mut config = SemverguardConfig::default();
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "lib".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::BaselineError),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let ctx = build_capability_context(&config, &report, true, false, None);
        let caps = ctx.build();
        assert_eq!(caps.baseline.status, CapabilityStatus::Unavailable);
        assert_eq!(
            caps.baseline.detail,
            Some("baseline resolution failed".to_string())
        );
    }
}
