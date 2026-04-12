//! Core operational helpers.
//!
//! This module provides default-adapter helpers for package listing, git
//! capability probing, and PR-mode gate checks.

use anyhow::Result;
use semverguard_domain::{GitProvider, SemverEngine, SemverguardRunner, WorkspaceProvider};
#[cfg(feature = "default-adapters")]
use semverguard_types::{BaselineKind, RunMode, ScopeMode};
use semverguard_types::{
    ListResult, PackageReport, PackageStatus, RunReport, SemverguardConfig, Summary,
};
#[cfg(feature = "default-adapters")]
use std::fs;
#[cfg(feature = "default-adapters")]
use std::path::Path;
use std::path::PathBuf;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Options for list operations.
#[derive(Debug, Clone)]
pub struct ListOptions {
    /// Workspace root to evaluate.
    pub workspace_root: PathBuf,
    /// Effective semverguard configuration.
    pub config: SemverguardConfig,
}

/// Run package listing with explicit adapter injection.
pub fn list_with_adapters(
    options: &ListOptions,
    workspace: &dyn WorkspaceProvider,
    git: Option<&dyn GitProvider>,
    engine: &dyn SemverEngine,
) -> Result<ListResult> {
    crate::config::ensure_workspace_root(&options.workspace_root)?;
    let runner = SemverguardRunner::new(workspace, git, engine);
    runner
        .list_packages(&options.workspace_root, &options.config)
        .map_err(anyhow::Error::new)
}

/// Build a synthetic run report for PR-mode gate skips.
///
/// Every package is emitted as skipped so downstream rendering and receipt
/// generation remain deterministic.
pub fn build_pr_mode_skipped_report(
    list_result: ListResult,
    gate_reason: &str,
    started: OffsetDateTime,
    finished: OffsetDateTime,
    semverguard_version: &str,
) -> RunReport {
    let mut packages: Vec<PackageReport> = list_result
        .would_check
        .into_iter()
        .map(|pkg| PackageReport {
            name: pkg.name,
            version: pkg.version,
            manifest_path: pkg.manifest_path,
            status: PackageStatus::Skipped,
            skip_reason: Some(gate_reason.to_string()),
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: None,
            baseline_error: None,
        })
        .collect();

    packages.extend(list_result.would_skip.into_iter().map(|pkg| PackageReport {
        name: pkg.name,
        version: pkg.version,
        manifest_path: pkg.manifest_path,
        status: PackageStatus::Skipped,
        skip_reason: Some(pkg.reason),
        duration_ms: 0,
        command: vec![],
        engine: None,
        inferred_required_bump: None,
        failure_kind: None,
        baseline_error: None,
    }));

    packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));

    let total = packages.len();
    RunReport {
        semverguard_version: semverguard_version.to_string(),
        started_at: started
            .format(&Rfc3339)
            .expect("failed to format start timestamp"),
        finished_at: finished
            .format(&Rfc3339)
            .expect("failed to format finish timestamp"),
        workspace_root: list_result.workspace_root,
        packages,
        summary: Summary {
            total,
            passed: 0,
            failed: 0,
            skipped: total,
        },
    }
}

/// Run package listing with default adapters.
#[cfg(feature = "default-adapters")]
pub fn list(options: &ListOptions) -> Result<ListResult> {
    let workspace = semverguard_workspace::CargoMetadataWorkspace::default();
    let git = semverguard_git::GitCli::default();
    let engine = semverguard_engine::CargoSemverChecksEngine::default();
    list_with_adapters(options, &workspace, Some(&git), &engine)
}

/// Result of probing local git capabilities.
#[cfg(feature = "default-adapters")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitProbe {
    /// Whether git is callable from the workspace root.
    pub available: bool,
    /// Whether the current repository is shallow.
    pub shallow_clone: bool,
    /// `git --version` output, if available.
    pub version: Option<String>,
}

/// Probe git capability data for receipt generation.
#[cfg(feature = "default-adapters")]
pub fn probe_git(workspace_root: &Path) -> GitProbe {
    let git = semverguard_git::GitCli::default();
    GitProbe {
        available: git.is_available(workspace_root),
        shallow_clone: git.is_shallow(workspace_root).unwrap_or(false),
        version: git.version(workspace_root),
    }
}

/// Determine whether PR mode should skip checks due to missing manifest version
/// changes and no forcing labels.
#[cfg(feature = "default-adapters")]
pub fn pr_mode_skip_reason(config: &SemverguardConfig, workspace_root: &Path) -> Option<String> {
    if config.mode.resolve() != RunMode::Pr {
        return None;
    }
    if config.scope.mode != ScopeMode::Changed {
        return None;
    }
    if has_pr_force_run_label() {
        return None;
    }
    if config.baseline.kind != BaselineKind::Git {
        return None;
    }
    let baseline_rev = config.baseline.rev.as_deref()?;

    let git = semverguard_git::GitCli::default();
    match git.has_manifest_version_change(workspace_root, baseline_rev, "HEAD") {
        Ok(true) => None,
        Ok(false) => Some(format!(
            "pr mode gate: skipped because no Cargo.toml version change was detected relative \
             to {baseline_rev}, and no forcing label is present"
        )),
        Err(e) => Some(format!(
            "pr mode gate: skipped because version-change detection failed relative to \
             {baseline_rev}: {e}"
        )),
    }
}

#[cfg(feature = "default-adapters")]
fn normalize_label(label: &str) -> String {
    label.trim().to_ascii_lowercase()
}

#[cfg(feature = "default-adapters")]
fn parse_label_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(normalize_label)
        .filter(|v| !v.is_empty())
        .collect()
}

#[cfg(feature = "default-adapters")]
fn read_github_labels_from_event_path(event_path: &Path) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(event_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let Some(labels) = value
        .get("pull_request")
        .and_then(|pr| pr.get("labels"))
        .and_then(|labels| labels.as_array())
    else {
        return Vec::new();
    };

    labels
        .iter()
        .filter_map(|entry| entry.get("name").and_then(|v| v.as_str()))
        .map(normalize_label)
        .collect()
}

#[cfg(feature = "default-adapters")]
fn has_pr_force_run_label() -> bool {
    const FORCE_LABELS: &[&str] = &[
        "semver:run",
        "semverguard:run",
        "semverguard/run",
        "semver-check",
        "run-semverguard",
    ];

    let mut labels: Vec<String> = Vec::new();

    if let Ok(raw) = std::env::var("SEMVERGUARD_PR_LABELS") {
        labels.extend(parse_label_list(&raw));
    }
    if let Ok(raw) = std::env::var("CI_MERGE_REQUEST_LABELS") {
        labels.extend(parse_label_list(&raw));
    }
    if let Ok(event_path) = std::env::var("GITHUB_EVENT_PATH") {
        labels.extend(read_github_labels_from_event_path(Path::new(&event_path)));
    }

    labels
        .iter()
        .any(|label| FORCE_LABELS.contains(&label.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{ListedPackage, SkippedPackage};
    use std::path::PathBuf;

    #[test]
    fn test_build_pr_mode_skipped_report_marks_all_as_skipped_and_sorted() {
        let list_result = ListResult {
            workspace_root: PathBuf::from("."),
            would_check: vec![
                ListedPackage {
                    name: "zeta".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("crates/zeta/Cargo.toml"),
                },
                ListedPackage {
                    name: "alpha".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("crates/alpha/Cargo.toml"),
                },
            ],
            would_skip: vec![SkippedPackage {
                name: "beta".to_string(),
                version: "0.5.0".to_string(),
                manifest_path: PathBuf::from("crates/beta/Cargo.toml"),
                reason: "excluded by pattern".to_string(),
            }],
        };

        let started = OffsetDateTime::UNIX_EPOCH;
        let finished = started + time::Duration::seconds(1);
        let report = build_pr_mode_skipped_report(
            list_result,
            "pr mode gate skip",
            started,
            finished,
            "0.1.0",
        );

        assert_eq!(report.summary.total, 3);
        assert_eq!(report.summary.passed, 0);
        assert_eq!(report.summary.failed, 0);
        assert_eq!(report.summary.skipped, 3);
        assert_eq!(report.packages[0].name, "alpha");
        assert_eq!(report.packages[1].name, "beta");
        assert_eq!(report.packages[2].name, "zeta");
        assert_eq!(report.packages[0].status, PackageStatus::Skipped);
        assert_eq!(report.packages[1].status, PackageStatus::Skipped);
        assert_eq!(report.packages[2].status, PackageStatus::Skipped);
        assert_eq!(
            report.packages[0].skip_reason.as_deref(),
            Some("pr mode gate skip")
        );
        assert_eq!(
            report.packages[1].skip_reason.as_deref(),
            Some("excluded by pattern")
        );
    }

    #[cfg(feature = "default-adapters")]
    #[test]
    fn test_parse_label_list_normalizes_and_filters_empty() {
        let labels = parse_label_list(" semver:run, ,Run-SemverGuard ,");
        assert_eq!(
            labels,
            vec!["semver:run".to_string(), "run-semverguard".to_string()]
        );
    }
}
