use crate::comment;
use crate::sarif;
use semverguard_types::{
    ArtifactIndex, BaselineConfig, FailureKind, Finding, FindingLevel, FindingLocation,
    PackageReport, PackageStatus, RawLogRef, RunReport, SemverguardData, SensorReportV1, ToolInfo,
    Verdict, VerdictStatus,
};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Tool error finding for receipt generation.
#[derive(Debug, Clone)]
pub struct ToolErrorFinding {
    /// Error message.
    pub message: String,
}

impl ToolErrorFinding {
    /// Create a new tool error finding.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Resolve the artifacts directory against the workspace root.
pub fn resolve_artifacts_dir(workspace_root: &Path, artifacts_dir: &Path) -> PathBuf {
    if artifacts_dir.is_absolute() {
        artifacts_dir.to_path_buf()
    } else {
        workspace_root.join(artifacts_dir)
    }
}

/// Build the artifact index for a receipt.
pub fn build_artifact_index(
    workspace_root: &Path,
    artifacts_dir: &Path,
    report: Option<&RunReport>,
    sarif_requested: bool,
) -> ArtifactIndex {
    let report_json = artifacts_dir.join("report.json");
    let comment_md = artifacts_dir.join("comment.md");
    let sarif_json = if sarif_requested {
        Some(artifacts_dir.join("sarif.json"))
    } else {
        None
    };

    let raw_logs = match report {
        Some(report) => build_raw_log_refs(report, workspace_root, artifacts_dir),
        None => Vec::new(),
    };

    ArtifactIndex {
        report_json: normalize_receipt_path(workspace_root, &report_json),
        comment_md: normalize_receipt_path(workspace_root, &comment_md),
        sarif_json: sarif_json.map(|p| normalize_receipt_path(workspace_root, &p)),
        raw_logs,
    }
}

/// Build a receipt from a run report and tool errors.
pub fn build_receipt(
    report: Option<&RunReport>,
    errors: &[ToolErrorFinding],
    artifacts: &ArtifactIndex,
    baseline: &BaselineConfig,
    workspace_root: &Path,
) -> SensorReportV1 {
    let (run_info, findings_from_report) = match report {
        Some(report) => (build_run_info(report, baseline), build_findings(report, artifacts)),
        None => (build_run_info_from_now(baseline, workspace_root), Vec::new()),
    };

    let mut findings = findings_from_report;
    for err in errors {
        findings.push(Finding {
            check_id: "tool".to_string(),
            code: "error".to_string(),
            level: FindingLevel::Error,
            message: err.message.clone(),
            location: None,
            data: None,
        });
    }

    findings.sort_by(|a, b| {
        (
            a.check_id.as_str(),
            a.code.as_str(),
            a.message.as_str(),
        )
            .cmp(&(
                b.check_id.as_str(),
                b.code.as_str(),
                b.message.as_str(),
            ))
    });

    let (verdict_status, verdict_reason) = derive_verdict(&findings, report);

    SensorReportV1 {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "semverguard".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            repository_url: option_env!("CARGO_PKG_REPOSITORY").map(|s| s.to_string()),
        },
        run: run_info,
        verdict: Verdict {
            status: verdict_status,
            reason: verdict_reason,
        },
        findings,
        data: report.map(|r| SemverguardData { report: r.clone() }),
        artifacts: artifacts.clone(),
    }
}

/// Returns true if any finding is a tool error.
pub fn has_tool_error(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.check_id == "tool")
}

/// Compute exit code from receipt verdict and tool error presence.
pub fn exit_code_from_receipt(
    verdict: &Verdict,
    warn_as_fail: bool,
    has_tool_error_flag: bool,
) -> i32 {
    match verdict.status {
        VerdictStatus::Fail => {
            if has_tool_error_flag {
                1
            } else {
                2
            }
        }
        VerdictStatus::Warn => {
            if warn_as_fail {
                3
            } else {
                0
            }
        }
        VerdictStatus::Pass | VerdictStatus::Skip => 0,
    }
}

/// Write the receipt bundle to disk.
pub fn write_receipt_bundle(
    artifacts_dir: &Path,
    receipt: &SensorReportV1,
    report: Option<&RunReport>,
    sarif_requested: bool,
    pretty_json: bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(artifacts_dir)?;
    fs::create_dir_all(artifacts_dir.join("raw"))?;

    let json = if pretty_json {
        serde_json::to_string_pretty(receipt)?
    } else {
        serde_json::to_string(receipt)?
    };
    fs::write(artifacts_dir.join("report.json"), json)?;

    let comment_md = comment::render_comment(receipt);
    fs::write(artifacts_dir.join("comment.md"), comment_md)?;

    if let Some(report) = report {
        write_raw_logs(artifacts_dir, report)?;
    }

    if sarif_requested {
        let sarif_log = sarif::receipt_to_sarif(receipt);
        let sarif_json = sarif::sarif_to_json(&sarif_log, pretty_json)
            .map_err(|e| anyhow::anyhow!("failed to serialize SARIF report: {e}"))?;
        fs::write(artifacts_dir.join("sarif.json"), sarif_json)?;
    }

    Ok(())
}

fn build_run_info(report: &RunReport, baseline: &BaselineConfig) -> semverguard_types::RunInfo {
    let duration_ms = duration_ms_from_strings(&report.started_at, &report.finished_at)
        .unwrap_or(0);
    semverguard_types::RunInfo {
        started_at: report.started_at.clone(),
        finished_at: report.finished_at.clone(),
        duration_ms,
        workspace_root: report.workspace_root.clone(),
        baseline: baseline.clone(),
    }
}

fn build_run_info_from_now(
    baseline: &BaselineConfig,
    workspace_root: &Path,
) -> semverguard_types::RunInfo {
    let now = OffsetDateTime::now_utc();
    let ts = now
        .format(&Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string());
    semverguard_types::RunInfo {
        started_at: ts.clone(),
        finished_at: ts,
        duration_ms: 0,
        workspace_root: workspace_root.to_path_buf(),
        baseline: baseline.clone(),
    }
}

fn duration_ms_from_strings(start: &str, end: &str) -> Option<u128> {
    let s = OffsetDateTime::parse(start, &Rfc3339).ok()?;
    let e = OffsetDateTime::parse(end, &Rfc3339).ok()?;
    let diff = e - s;
    let ms = diff.whole_milliseconds();
    if ms < 0 {
        None
    } else {
        Some(ms as u128)
    }
}

fn build_findings(report: &RunReport, artifacts: &ArtifactIndex) -> Vec<Finding> {
    let raw_log_map = build_raw_log_map(artifacts);
    let mut findings = Vec::new();

    let mut packages = report.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str())));

    for pkg in packages {
        if pkg.status != PackageStatus::Failed {
            continue;
        }

        let failure_kind = pkg.failure_kind.unwrap_or(FailureKind::Unknown);
        let (check_id, code, level) = match failure_kind {
            FailureKind::SemverViolation => ("semver", "violation", FindingLevel::Error),
            FailureKind::BaselineError => ("baseline", "missing", FindingLevel::Warning),
            FailureKind::ToolError => ("tool", "error", FindingLevel::Error),
            FailureKind::Unknown => ("engine", "unknown", FindingLevel::Error),
        };

        let message = build_failure_message(pkg, failure_kind);

        let raw_log = raw_log_map
            .get(&(pkg.name.clone(), pkg.version.clone()))
            .and_then(|(stdout, stderr)| stderr.clone().or_else(|| stdout.clone()));

        let location = Some(FindingLocation {
            path: Some(normalize_receipt_path(&report.workspace_root, &pkg.manifest_path)),
            line: None,
            column: None,
            raw_log,
        });

        let data = Some(json!({
            "package": pkg.name,
            "version": pkg.version,
            "required_bump": pkg.inferred_required_bump.map(required_bump_str),
            "failure_kind": failure_kind_str(failure_kind),
            "manifest_path": pkg.manifest_path.display().to_string(),
        }));

        findings.push(Finding {
            check_id: check_id.to_string(),
            code: code.to_string(),
            level,
            message,
            location,
            data,
        });
    }

    findings
}

fn build_failure_message(pkg: &PackageReport, kind: FailureKind) -> String {
    match kind {
        FailureKind::SemverViolation => match pkg.inferred_required_bump {
            Some(bump) => format!(
                "Package `{}` (v{}) requires a {} version bump",
                pkg.name,
                pkg.version,
                required_bump_str(bump)
            ),
            None => format!(
                "Package `{}` (v{}) failed semantic versioning checks",
                pkg.name, pkg.version
            ),
        },
        FailureKind::BaselineError => {
            let detail = pkg
                .engine
                .as_ref()
                .map(|e| e.stderr.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| pkg.skip_reason.clone())
                .unwrap_or_else(|| "baseline error".to_string());
            format!("Baseline error for `{}` (v{}): {}", pkg.name, pkg.version, detail)
        }
        FailureKind::ToolError => {
            let detail = pkg
                .engine
                .as_ref()
                .map(|e| e.stderr.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| pkg.skip_reason.clone())
                .unwrap_or_else(|| "tool error".to_string());
            format!("Tool error for `{}` (v{}): {}", pkg.name, pkg.version, detail)
        }
        FailureKind::Unknown => format!(
            "Package `{}` (v{}) failed with an unknown error",
            pkg.name, pkg.version
        ),
    }
}

fn derive_verdict(
    findings: &[Finding],
    report: Option<&RunReport>,
) -> (VerdictStatus, Option<String>) {
    let mut has_tool = false;
    let mut has_semver = false;
    let mut has_baseline = false;

    for f in findings {
        match f.check_id.as_str() {
            "tool" => has_tool = true,
            "baseline" => has_baseline = true,
            "semver" | "engine" => has_semver = true,
            _ => {}
        }
    }

    if has_tool {
        return (
            VerdictStatus::Fail,
            Some("tool error".to_string()),
        );
    }
    if has_semver {
        return (
            VerdictStatus::Fail,
            Some("semver violation".to_string()),
        );
    }
    if has_baseline {
        return (
            VerdictStatus::Warn,
            Some("baseline issue".to_string()),
        );
    }

    if let Some(report) = report {
        if report.summary.total > 0 && report.summary.total == report.summary.skipped {
            return (VerdictStatus::Skip, Some("all packages skipped".to_string()));
        }
    }

    (VerdictStatus::Pass, None)
}

fn build_raw_log_refs(
    report: &RunReport,
    workspace_root: &Path,
    artifacts_dir: &Path,
) -> Vec<RawLogRef> {
    let mut packages = report.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str())));

    let mut raw_logs = Vec::new();
    for pkg in packages {
        if pkg.status == PackageStatus::Skipped {
            continue;
        }
        let (stdout_path, stderr_path) = raw_log_paths(artifacts_dir, pkg);
        raw_logs.push(RawLogRef {
            package: pkg.name.clone(),
            version: pkg.version.clone(),
            stdout: Some(normalize_receipt_path(workspace_root, &stdout_path)),
            stderr: Some(normalize_receipt_path(workspace_root, &stderr_path)),
        });
    }

    raw_logs
}

fn build_raw_log_map(
    artifacts: &ArtifactIndex,
) -> HashMap<(String, String), (Option<String>, Option<String>)> {
    let mut map = HashMap::new();
    for raw in &artifacts.raw_logs {
        map.insert(
            (raw.package.clone(), raw.version.clone()),
            (raw.stdout.clone(), raw.stderr.clone()),
        );
    }
    map
}

fn write_raw_logs(artifacts_dir: &Path, report: &RunReport) -> anyhow::Result<()> {
    let mut packages = report.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str())));

    for pkg in packages {
        if pkg.status == PackageStatus::Skipped {
            continue;
        }
        let (stdout_path, stderr_path) = raw_log_paths(artifacts_dir, pkg);

        let (stdout, stderr) = match &pkg.engine {
            Some(engine) => (engine.stdout.clone(), engine.stderr.clone()),
            None => {
                let msg = pkg
                    .skip_reason
                    .clone()
                    .unwrap_or_else(|| "no output".to_string());
                ("no output".to_string(), msg)
            }
        };

        fs::write(stdout_path, stdout)?;
        fs::write(stderr_path, stderr)?;
    }

    Ok(())
}

fn raw_log_paths(artifacts_dir: &Path, pkg: &PackageReport) -> (PathBuf, PathBuf) {
    let raw_dir = artifacts_dir.join("raw");
    let name = sanitize_filename_component(&pkg.name);
    let version = sanitize_filename_component(&pkg.version);
    let base = format!("{name}-{version}");
    (
        raw_dir.join(format!("{base}.stdout.log")),
        raw_dir.join(format!("{base}.stderr.log")),
    )
}

fn normalize_receipt_path(workspace_root: &Path, path: &Path) -> String {
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    let normalized = full_path
        .strip_prefix(workspace_root)
        .unwrap_or(full_path.as_path());

    normalized.to_string_lossy().replace('\\', "/")
}

fn sanitize_filename_component(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn required_bump_str(bump: semverguard_types::RequiredBump) -> &'static str {
    match bump {
        semverguard_types::RequiredBump::Major => "major",
        semverguard_types::RequiredBump::Minor => "minor",
        semverguard_types::RequiredBump::Patch => "patch",
        semverguard_types::RequiredBump::Unknown => "unknown",
    }
}

fn failure_kind_str(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::SemverViolation => "semver-violation",
        FailureKind::ToolError => "tool-error",
        FailureKind::BaselineError => "baseline-error",
        FailureKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{BaselineConfig, PackageStatus, RequiredBump, Summary};

    fn sample_report() -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "b-lib".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/b-lib/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 10,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Major),
                    failure_kind: Some(FailureKind::SemverViolation),
                },
                PackageReport {
                    name: "a-lib".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/a-lib/Cargo.toml"),
                    status: PackageStatus::Skipped,
                    skip_reason: Some("publish = false".to_string()),
                    duration_ms: 0,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                },
            ],
            summary: Summary {
                total: 2,
                passed: 0,
                failed: 1,
                skipped: 1,
            },
        }
    }

    #[test]
    fn test_build_artifact_index_raw_logs_sorted() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );

        assert_eq!(artifacts.raw_logs.len(), 1);
        let raw = &artifacts.raw_logs[0];
        assert_eq!(raw.package, "b-lib");
        let stdout_path = PathBuf::from(raw.stdout.as_ref().unwrap());
        let stderr_path = PathBuf::from(raw.stderr.as_ref().unwrap());
        assert_eq!(
            stdout_path.file_name().unwrap().to_string_lossy(),
            "b-lib-2.0.0.stdout.log"
        );
        assert_eq!(
            stderr_path.file_name().unwrap().to_string_lossy(),
            "b-lib-2.0.0.stderr.log"
        );
    }

    #[test]
    fn test_build_receipt_findings_ordering() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );
        let receipt = build_receipt(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
        );

        assert!(!receipt.findings.is_empty());
        for window in receipt.findings.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                (a.check_id.as_str(), a.code.as_str(), a.message.as_str())
                    <= (b.check_id.as_str(), b.code.as_str(), b.message.as_str())
            );
        }
    }

    #[test]
    fn test_comment_includes_sections() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );
        let receipt = build_receipt(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
        );

        let comment = comment::render_comment(&receipt);
        assert!(comment.contains("### Failed packages"));
        assert!(comment.contains("### Skipped packages"));
        assert!(comment.contains("### Warnings / tool errors"));
    }
}
