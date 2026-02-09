//! Receipt building, fingerprinting, and writing for cockpit ingestion.
//!
//! This module builds `sensor.report.v1` receipts from run reports,
//! computing verdicts, fingerprints, and artifact indices.

use crate::comment;
use crate::sarif;
use semverguard_types::{
    ArtifactIndex, BaselineConfig, BaselineKind, CapabilityInfo, CapabilityStatus, FailureKind,
    Finding, FindingLevel, FindingLocation, PackageReport, PackageStatus, RawLogRef, RequiredBump,
    RunCapabilities, RunReport, SemverguardData, SensorReportV1, SummaryData, ToolInfo, Verdict,
    VerdictStatus, WaiverEntry, WaiverInfo,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

// ─────────────────────────────────────────────────────────────────────────────
// Stable code token constants (2D)
// ─────────────────────────────────────────────────────────────────────────────

/// Check ID for SemVer policy violations.
pub const CHECK_SEMVER: &str = "semver";
/// Code for SemVer violations.
pub const CODE_VIOLATION: &str = "violation";
/// Check ID for baseline resolution errors.
pub const CHECK_BASELINE: &str = "baseline";
/// Code for missing baseline.
pub const CODE_MISSING: &str = "missing";
/// Check ID for tool/runtime errors.
pub const CHECK_TOOL: &str = "tool.runtime";
/// Code for tool errors.
pub const CODE_ERROR: &str = "runtime_error";
/// Check ID for unclassifiable engine failures.
pub const CHECK_ENGINE: &str = "engine";
/// Code for unknown failures.
pub const CODE_UNKNOWN: &str = "unknown";

// ─────────────────────────────────────────────────────────────────────────────
// Verdict reason tokens
// ─────────────────────────────────────────────────────────────────────────────

/// Verdict reason: SemVer violation detected.
pub const REASON_SEMVER_VIOLATION: &str = "semver_violation";
/// Verdict reason: baseline was unavailable.
pub const REASON_BASELINE_UNAVAILABLE: &str = "baseline_unavailable";
/// Verdict reason: tool/runtime error.
pub const REASON_TOOL_ERROR: &str = "tool_error";
/// Verdict reason: engine error.
pub const REASON_ENGINE_ERROR: &str = "engine_error";
/// Verdict reason: findings were truncated.
pub const REASON_TRUNCATED: &str = "truncated";
/// Verdict reason: all packages were skipped.
pub const REASON_ALL_PACKAGES_SKIPPED: &str = "all_packages_skipped";
/// Verdict reason: finding was waived.
pub const REASON_WAIVED: &str = "waived";

// ─────────────────────────────────────────────────────────────────────────────
// Capability reason tokens
// ─────────────────────────────────────────────────────────────────────────────

/// Capability reason: git binary not found.
pub const CAP_REASON_GIT_UNAVAILABLE: &str = "git_unavailable";
/// Capability reason: shallow clone detected.
pub const CAP_REASON_SHALLOW_CLONE: &str = "shallow_clone";
/// Capability reason: baseline resolution failed.
pub const CAP_REASON_RESOLUTION_FAILED: &str = "resolution_failed";
/// Capability reason: capability not required for this configuration.
pub const CAP_REASON_NOT_REQUIRED: &str = "not_required";

/// Maximum number of findings in a receipt before truncation.
const MAX_FINDINGS: usize = 500;

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

/// Context for building capability status in receipts.
///
/// This provides the "No Green By Omission" capability reporting,
/// ensuring that silent passes when prerequisites are missing are detected.
#[derive(Debug, Clone, Default)]
pub struct CapabilityContext {
    /// Whether git was available and used successfully.
    pub git_available: bool,
    /// Optional detail about git status.
    pub git_detail: Option<String>,
    /// Whether baseline was resolved successfully.
    pub baseline_available: bool,
    /// Optional detail about baseline status.
    pub baseline_detail: Option<String>,
    /// Whether the repository is a shallow clone.
    pub shallow_clone: bool,
    /// Git binary version string (e.g. "git version 2.39.0").
    pub git_version: Option<String>,
    /// Whether git capability was not required (e.g. crates-io baseline).
    pub git_skipped: bool,
    /// Whether baseline capability was not required.
    pub baseline_skipped: bool,
}

impl CapabilityContext {
    /// Create a new capability context with default values (all unavailable).
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark git as available.
    pub fn with_git_available(mut self, available: bool) -> Self {
        self.git_available = available;
        self
    }

    /// Set git detail message.
    pub fn with_git_detail(mut self, detail: impl Into<String>) -> Self {
        self.git_detail = Some(detail.into());
        self
    }

    /// Mark baseline as available.
    pub fn with_baseline_available(mut self, available: bool) -> Self {
        self.baseline_available = available;
        self
    }

    /// Set baseline detail message.
    pub fn with_baseline_detail(mut self, detail: impl Into<String>) -> Self {
        self.baseline_detail = Some(detail.into());
        self
    }

    /// Mark as shallow clone.
    pub fn with_shallow_clone(mut self, shallow: bool) -> Self {
        self.shallow_clone = shallow;
        self
    }

    /// Set git binary version string.
    pub fn with_git_version(mut self, version: impl Into<String>) -> Self {
        self.git_version = Some(version.into());
        self
    }

    /// Mark git capability as not required (skipped).
    pub fn with_git_skipped(mut self, skipped: bool) -> Self {
        self.git_skipped = skipped;
        self
    }

    /// Mark baseline capability as not required (skipped).
    pub fn with_baseline_skipped(mut self, skipped: bool) -> Self {
        self.baseline_skipped = skipped;
        self
    }

    /// Build the RunCapabilities from this context.
    pub fn build(&self) -> RunCapabilities {
        // Prefer git_version over config-based detail for truthfulness
        let mut git_detail = if let Some(ver) = &self.git_version {
            match &self.git_detail {
                Some(cfg_detail) => Some(format!("{ver}; rev={cfg_detail}")),
                None => Some(ver.clone()),
            }
        } else {
            self.git_detail.clone()
        };
        if self.shallow_clone {
            let existing = git_detail.unwrap_or_default();
            git_detail = Some(if existing.is_empty() {
                "shallow clone".to_string()
            } else {
                format!("{existing}; shallow clone")
            });
        }

        let (git_status, git_reason) = if self.git_skipped {
            (
                CapabilityStatus::Skipped,
                Some(CAP_REASON_NOT_REQUIRED.to_string()),
            )
        } else if !self.git_available {
            (
                CapabilityStatus::Unavailable,
                Some(CAP_REASON_GIT_UNAVAILABLE.to_string()),
            )
        } else if self.shallow_clone {
            (
                CapabilityStatus::Available,
                Some(CAP_REASON_SHALLOW_CLONE.to_string()),
            )
        } else {
            (CapabilityStatus::Available, None)
        };

        let (baseline_status, baseline_reason) = if self.baseline_skipped {
            (
                CapabilityStatus::Skipped,
                Some(CAP_REASON_NOT_REQUIRED.to_string()),
            )
        } else if !self.baseline_available {
            (
                CapabilityStatus::Unavailable,
                Some(CAP_REASON_RESOLUTION_FAILED.to_string()),
            )
        } else {
            (CapabilityStatus::Available, None)
        };

        RunCapabilities {
            git: CapabilityInfo {
                status: git_status,
                detail: git_detail,
                reason: git_reason,
            },
            baseline: CapabilityInfo {
                status: baseline_status,
                detail: self.baseline_detail.clone(),
                reason: baseline_reason,
            },
        }
    }
}

/// Compute a stable semantic fingerprint for a finding.
///
/// The fingerprint is a 64-character hex string derived from the SHA-256 hash
/// of the check_id, code, package name, and package version. This enables
/// deterministic deduplication across runs.
pub fn compute_fingerprint(
    check_id: &str,
    code: &str,
    pkg_name: &str,
    pkg_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"semverguard\0");
    hasher.update(check_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(code.as_bytes());
    hasher.update(b"\0");
    hasher.update(pkg_name.as_bytes());
    hasher.update(b"\0");
    hasher.update(pkg_version.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
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
///
/// This is a convenience wrapper that calls `build_receipt_with_capabilities` without capabilities.
#[allow(dead_code)]
pub fn build_receipt(
    report: Option<&RunReport>,
    errors: &[ToolErrorFinding],
    artifacts: &ArtifactIndex,
    baseline: &BaselineConfig,
    workspace_root: &Path,
) -> SensorReportV1 {
    build_receipt_with_capabilities(report, errors, artifacts, baseline, workspace_root, None)
}

/// Build a receipt from a run report, tool errors, and capability context.
pub fn build_receipt_with_capabilities(
    report: Option<&RunReport>,
    errors: &[ToolErrorFinding],
    artifacts: &ArtifactIndex,
    baseline: &BaselineConfig,
    workspace_root: &Path,
    capabilities: Option<&CapabilityContext>,
) -> SensorReportV1 {
    build_receipt_with_capabilities_versioned(
        report,
        errors,
        artifacts,
        baseline,
        workspace_root,
        capabilities,
        None,
        &[],
    )
}

/// Build a receipt with an optional tool version override and waivers.
pub fn build_receipt_with_capabilities_versioned(
    report: Option<&RunReport>,
    errors: &[ToolErrorFinding],
    artifacts: &ArtifactIndex,
    baseline: &BaselineConfig,
    workspace_root: &Path,
    capabilities: Option<&CapabilityContext>,
    tool_version: Option<&str>,
    waivers: &[WaiverEntry],
) -> SensorReportV1 {
    let (run_info, findings_from_report) = match report {
        Some(report) => (
            build_run_info(report, baseline, capabilities),
            build_findings(report, artifacts, workspace_root),
        ),
        None => (
            build_run_info_from_now(baseline, workspace_root, capabilities),
            Vec::new(),
        ),
    };

    let mut findings = findings_from_report;
    for err in errors {
        // Tool errors don't have package context, so no fingerprint
        findings.push(Finding {
            check_id: CHECK_TOOL.to_string(),
            code: CODE_ERROR.to_string(),
            level: FindingLevel::Error,
            message: err.message.clone(),
            location: None,
            data: None,
            fingerprint: None,
            waived: None,
        });
    }

    findings.sort_by(|a, b| finding_sort_key(a).cmp(&finding_sort_key(b)));

    apply_waivers(&mut findings, waivers);

    // Truncation signaling (2B)
    let (was_truncated, findings_total, findings_emitted) = if findings.len() > MAX_FINDINGS {
        let original = findings.len();
        findings.truncate(MAX_FINDINGS);
        (true, Some(original), Some(MAX_FINDINGS))
    } else {
        (false, None, None)
    };

    let (verdict_status, verdict_reasons) = derive_verdict(&findings, report, was_truncated);

    let version = tool_version.unwrap_or(env!("CARGO_PKG_VERSION"));

    SensorReportV1 {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "semverguard".to_string(),
            version: version.to_string(),
            repository_url: option_env!("CARGO_PKG_REPOSITORY").map(|s| s.to_string()),
        },
        run: run_info,
        verdict: Verdict {
            status: verdict_status,
            reasons: verdict_reasons,
        },
        findings,
        data: report.map(|r| SemverguardData {
            report: r.clone(),
            findings_total,
            findings_emitted,
            summary: Some(build_summary_data(r, baseline)),
        }),
        artifacts: artifacts.clone(),
    }
}

/// Returns true if any finding is a tool error.
pub fn has_tool_error(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.check_id == CHECK_TOOL)
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

fn build_run_info(
    report: &RunReport,
    baseline: &BaselineConfig,
    capabilities: Option<&CapabilityContext>,
) -> semverguard_types::RunInfo {
    let duration_ms =
        duration_ms_from_strings(&report.started_at, &report.finished_at).unwrap_or(0);
    semverguard_types::RunInfo {
        started_at: report.started_at.clone(),
        finished_at: report.finished_at.clone(),
        duration_ms,
        workspace_root: report.workspace_root.clone(),
        baseline: baseline.clone(),
        capabilities: capabilities.map(|c| c.build()),
    }
}

fn build_run_info_from_now(
    baseline: &BaselineConfig,
    workspace_root: &Path,
    capabilities: Option<&CapabilityContext>,
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
        capabilities: capabilities.map(|c| c.build()),
    }
}

fn duration_ms_from_strings(start: &str, end: &str) -> Option<u128> {
    let s = OffsetDateTime::parse(start, &Rfc3339).ok()?;
    let e = OffsetDateTime::parse(end, &Rfc3339).ok()?;
    let diff = e - s;
    let ms = diff.whole_milliseconds();
    if ms < 0 { None } else { Some(ms as u128) }
}

fn build_findings(
    report: &RunReport,
    artifacts: &ArtifactIndex,
    workspace_root: &Path,
) -> Vec<Finding> {
    let raw_log_map = build_raw_log_map(artifacts);
    let mut findings = Vec::new();

    let mut packages = report.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|a, b| {
        (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str()))
    });

    for pkg in packages {
        if pkg.status != PackageStatus::Failed {
            continue;
        }

        let failure_kind = pkg.failure_kind.unwrap_or(FailureKind::Unknown);
        let (check_id, code, level) = match failure_kind {
            FailureKind::SemverViolation => (CHECK_SEMVER, CODE_VIOLATION, FindingLevel::Error),
            FailureKind::BaselineError => (CHECK_BASELINE, CODE_MISSING, FindingLevel::Warning),
            FailureKind::ToolError => (CHECK_TOOL, CODE_ERROR, FindingLevel::Error),
            FailureKind::Unknown => (CHECK_ENGINE, CODE_UNKNOWN, FindingLevel::Error),
        };

        let message = build_failure_message(pkg, failure_kind);

        let raw_log = raw_log_map
            .get(&(pkg.name.clone(), pkg.version.clone()))
            .and_then(|(stdout, stderr)| stderr.clone().or_else(|| stdout.clone()));

        let location = Some(FindingLocation {
            path: Some(normalize_receipt_path(
                &report.workspace_root,
                &pkg.manifest_path,
            )),
            line: None,
            column: None,
            raw_log,
        });

        // 2A: Path hygiene - use normalize_receipt_path for manifest_path in data
        let normalized_manifest =
            normalize_receipt_path(&report.workspace_root, &pkg.manifest_path);

        // 2C: Compute suggested version from current + required_bump
        let suggested_version = pkg
            .inferred_required_bump
            .and_then(|bump| compute_suggested_version(&pkg.version, bump));

        // 2C: Serialize baseline_error when failure_kind is BaselineError
        let baseline_error_cause = if failure_kind == FailureKind::BaselineError {
            pkg.baseline_error
                .as_ref()
                .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
        } else {
            None
        };

        let mut data_map = serde_json::Map::new();
        data_map.insert("package".to_string(), json!(pkg.name));
        data_map.insert("version".to_string(), json!(pkg.version));
        data_map.insert(
            "required_bump".to_string(),
            json!(pkg.inferred_required_bump.map(required_bump_str)),
        );
        data_map.insert(
            "failure_kind".to_string(),
            json!(failure_kind_str(failure_kind)),
        );
        data_map.insert("manifest_path".to_string(), json!(normalized_manifest));
        if let Some(sv) = &suggested_version {
            data_map.insert("suggested_version".to_string(), json!(sv));
        }
        if let Some(cause) = baseline_error_cause {
            data_map.insert("baseline_error_cause".to_string(), cause);
        }

        let data = Some(serde_json::Value::Object(data_map));

        // Use workspace_root for fingerprint (not needed for uniqueness, just for context)
        let fingerprint = Some(compute_fingerprint(check_id, code, &pkg.name, &pkg.version));

        findings.push(Finding {
            check_id: check_id.to_string(),
            code: code.to_string(),
            level,
            message,
            location,
            data,
            fingerprint,
            waived: None,
        });
    }

    // Also build findings for packages with baseline errors that need the
    // workspace_root for path normalization
    let _ = workspace_root;

    findings
}

/// Compute the suggested next version after applying a bump.
fn compute_suggested_version(version: &str, bump: RequiredBump) -> Option<String> {
    let v = semver::Version::parse(version).ok()?;
    let next = match bump {
        RequiredBump::Major => semver::Version::new(v.major + 1, 0, 0),
        RequiredBump::Minor => semver::Version::new(v.major, v.minor + 1, 0),
        RequiredBump::Patch => semver::Version::new(v.major, v.minor, v.patch + 1),
        RequiredBump::Unknown => return None,
    };
    Some(next.to_string())
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
            format!(
                "Baseline error for `{}` (v{}): {}",
                pkg.name, pkg.version, detail
            )
        }
        FailureKind::ToolError => {
            let detail = pkg
                .engine
                .as_ref()
                .map(|e| e.stderr.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| pkg.skip_reason.clone())
                .unwrap_or_else(|| "tool error".to_string());
            format!(
                "Tool error for `{}` (v{}): {}",
                pkg.name, pkg.version, detail
            )
        }
        FailureKind::Unknown => format!(
            "Package `{}` (v{}) failed with an unknown error",
            pkg.name, pkg.version
        ),
    }
}

/// Apply waivers to findings by matching fingerprints.
///
/// Non-expired waivers with matching fingerprints set the `waived` field on findings.
fn apply_waivers(findings: &mut [Finding], waivers: &[WaiverEntry]) {
    if waivers.is_empty() {
        return;
    }

    for finding in findings.iter_mut() {
        if let Some(fp) = &finding.fingerprint {
            for waiver in waivers {
                if waiver.fingerprint == *fp && !is_waiver_expired(waiver) {
                    finding.waived = Some(WaiverInfo {
                        reason: waiver.reason.clone(),
                        ticket: waiver.ticket.clone(),
                        expires: waiver.expires.clone(),
                    });
                    break;
                }
            }
        }
    }
}

/// Check if a waiver has expired.
fn is_waiver_expired(waiver: &WaiverEntry) -> bool {
    match &waiver.expires {
        None => false,
        Some(date_str) => {
            let today = time::OffsetDateTime::now_utc().date();
            // Try simple YYYY-MM-DD parsing
            let parts: Vec<&str> = date_str.split('-').collect();
            if parts.len() == 3 {
                if let (Ok(y), Ok(m), Ok(d)) = (
                    parts[0].parse::<i32>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    if let Ok(month) = time::Month::try_from(m) {
                        if let Ok(expires_date) = time::Date::from_calendar_date(y, month, d) {
                            return today > expires_date;
                        }
                    }
                }
            }
            false // Can't parse = not expired
        }
    }
}

fn derive_verdict(
    findings: &[Finding],
    report: Option<&RunReport>,
    was_truncated: bool,
) -> (VerdictStatus, Vec<String>) {
    let mut has_tool = false;
    let mut has_semver = false;
    let mut has_engine = false;
    let mut has_baseline = false;

    for f in findings {
        // Skip waived findings for verdict computation
        if f.waived.is_some() {
            continue;
        }
        match f.check_id.as_str() {
            CHECK_TOOL => has_tool = true,
            CHECK_BASELINE => has_baseline = true,
            CHECK_SEMVER => has_semver = true,
            CHECK_ENGINE => has_engine = true,
            _ => {}
        }
    }

    let has_waived = findings.iter().any(|f| f.waived.is_some());

    let mut reasons = Vec::new();
    if has_semver {
        reasons.push(REASON_SEMVER_VIOLATION.to_string());
    }
    if has_baseline {
        reasons.push(REASON_BASELINE_UNAVAILABLE.to_string());
    }
    if has_tool {
        reasons.push(REASON_TOOL_ERROR.to_string());
    }
    if has_engine {
        reasons.push(REASON_ENGINE_ERROR.to_string());
    }
    if was_truncated {
        reasons.push(REASON_TRUNCATED.to_string());
    }

    let all_skipped = report
        .map(|r| r.summary.total > 0 && r.summary.total == r.summary.skipped)
        .unwrap_or(false);
    if all_skipped {
        reasons.push(REASON_ALL_PACKAGES_SKIPPED.to_string());
    }
    if has_waived {
        reasons.push(REASON_WAIVED.to_string());
    }

    // Status determined by priority
    let status = if has_tool || has_semver || has_engine {
        VerdictStatus::Fail
    } else if has_baseline {
        VerdictStatus::Warn
    } else if all_skipped {
        VerdictStatus::Skip
    } else {
        VerdictStatus::Pass
    };

    (status, reasons)
}

fn build_raw_log_refs(
    report: &RunReport,
    workspace_root: &Path,
    artifacts_dir: &Path,
) -> Vec<RawLogRef> {
    let mut packages = report.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|a, b| {
        (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str()))
    });

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
    packages.sort_by(|a, b| {
        (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str()))
    });

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

/// Normalize a path to be workspace-root-relative with forward slashes.
///
/// If `strip_prefix` fails and the resulting path contains `..`, falls back
/// to the filename only to avoid path traversal.
pub fn normalize_receipt_path(workspace_root: &Path, path: &Path) -> String {
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    let normalized = full_path
        .strip_prefix(workspace_root)
        .unwrap_or(full_path.as_path());

    let result = normalized.to_string_lossy().replace('\\', "/");

    // Guard against path traversal
    if result.contains("..") {
        eprintln!(
            "warning: path could not be normalized to workspace root: {}",
            path.display()
        );
        // Fall back to filename only
        path.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| result)
    } else {
        result
    }
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

fn required_bump_str(bump: RequiredBump) -> &'static str {
    match bump {
        RequiredBump::Major => "major",
        RequiredBump::Minor => "minor",
        RequiredBump::Patch => "patch",
        RequiredBump::Unknown => "unknown",
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

/// Extract sort key for findings.
///
/// Sorts by severity (errors first), then package name, check_id, code,
/// fingerprint, and message as tiebreakers.
fn finding_sort_key(f: &Finding) -> (u8, String, String, String, String, String) {
    let package = f
        .data
        .as_ref()
        .and_then(|d| d.get("package"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let fingerprint = f.fingerprint.clone().unwrap_or_default();
    (
        f.level.severity_rank(),
        package,
        f.check_id.clone(),
        f.code.clone(),
        fingerprint,
        f.message.clone(),
    )
}

/// Build summary data from a run report and baseline config.
fn build_summary_data(report: &RunReport, baseline: &BaselineConfig) -> SummaryData {
    let packages_total = report.packages.len() as u32;

    let packages_checked = report
        .packages
        .iter()
        .filter(|p| p.status != PackageStatus::Skipped)
        .count() as u32;

    let packages_skipped = report
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Skipped)
        .count() as u32;

    let violations = report
        .packages
        .iter()
        .filter(|p| p.failure_kind == Some(FailureKind::SemverViolation))
        .count() as u32;

    let baseline_issues = report
        .packages
        .iter()
        .filter(|p| p.failure_kind == Some(FailureKind::BaselineError))
        .count() as u32;

    let max_bump = report
        .packages
        .iter()
        .filter_map(|p| p.inferred_required_bump)
        .max_by_key(|b| match b {
            RequiredBump::Major => 3,
            RequiredBump::Minor => 2,
            RequiredBump::Patch => 1,
            RequiredBump::Unknown => 0,
        })
        .map(|b| match b {
            RequiredBump::Major => "major".to_string(),
            RequiredBump::Minor => "minor".to_string(),
            RequiredBump::Patch => "patch".to_string(),
            RequiredBump::Unknown => "unknown".to_string(),
        });

    let baseline_kind = match baseline.kind {
        BaselineKind::Git => "git".to_string(),
        BaselineKind::CratesIo => "crates-io".to_string(),
    };

    let baseline_ref = match baseline.kind {
        BaselineKind::Git => baseline.rev.clone(),
        BaselineKind::CratesIo => baseline.version.clone(),
    };

    SummaryData {
        packages_total,
        packages_checked,
        packages_skipped,
        violations,
        baseline_issues,
        max_required_bump: max_bump,
        baseline_kind,
        baseline_ref,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{
        BaselineConfig, BaselineErrorCause, BaselineKind, CapabilityStatus, FailureKind, Finding,
        FindingLevel, PackageStatus, RequiredBump, SemverCheckOutput, Summary, WaiverEntry,
    };
    use std::fs;
    use tempfile::tempdir;

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
                    baseline_error: None,
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
                    baseline_error: None,
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

    fn report_with_required_bumps(bumps: &[RequiredBump]) -> RunReport {
        let mut packages = Vec::new();
        for (idx, bump) in bumps.iter().enumerate() {
            packages.push(PackageReport {
                name: format!("pkg-{idx}"),
                version: format!("1.0.{idx}"),
                manifest_path: PathBuf::from(format!("/workspace/pkg-{idx}/Cargo.toml")),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 1,
                command: vec![],
                engine: None,
                inferred_required_bump: Some(*bump),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            });
        }

        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages,
            summary: Summary {
                total: bumps.len(),
                passed: 0,
                failed: bumps.len(),
                skipped: 0,
            },
        }
    }

    fn report_with_semver_and_baseline_failures() -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "alpha".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 10,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Minor),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
                PackageReport {
                    name: "beta".to_string(),
                    version: "0.9.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 5,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: Some(FailureKind::BaselineError),
                    baseline_error: Some(BaselineErrorCause::RevisionNotFound {
                        rev: "deadbeef".to_string(),
                    }),
                },
            ],
            summary: Summary {
                total: 2,
                passed: 0,
                failed: 2,
                skipped: 0,
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
        let report = report_with_semver_and_baseline_failures();
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
                finding_sort_key(a) <= finding_sort_key(b),
                "Findings should be sorted by severity, then package, then fingerprint"
            );
        }
    }

    #[test]
    fn test_finding_level_severity_rank() {
        assert_eq!(FindingLevel::Error.severity_rank(), 0);
        assert_eq!(FindingLevel::Warning.severity_rank(), 1);
        assert_eq!(FindingLevel::Info.severity_rank(), 2);
    }

    #[test]
    fn test_findings_sorted_severity_then_package() {
        // Create a report with both semver violation (error) and baseline error (warning)
        let report = report_with_semver_and_baseline_failures();
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
        // With severity-first sort, errors come before warnings
        assert!(receipt.findings.len() >= 2);
        assert!(
            receipt.findings[0].level.severity_rank() <= receipt.findings[1].level.severity_rank()
        );
    }

    #[test]
    fn test_summary_data_populated() {
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
        let data = receipt.data.expect("data should be present");
        let summary = data.summary.expect("summary should be present");
        assert_eq!(summary.packages_total, 2);
        assert_eq!(summary.packages_checked, 1); // b-lib is failed (checked), a-lib is skipped
        assert_eq!(summary.packages_skipped, 1);
        assert_eq!(summary.violations, 1); // b-lib has SemverViolation
        assert_eq!(summary.baseline_issues, 0);
        assert_eq!(summary.max_required_bump, Some("major".to_string()));
        assert_eq!(summary.baseline_kind, "crates-io");
        assert!(summary.baseline_ref.is_none()); // default baseline has no version
    }

    #[test]
    fn test_summary_data_required_bump_variants_and_git_baseline() {
        let report = report_with_required_bumps(&[
            RequiredBump::Minor,
            RequiredBump::Patch,
            RequiredBump::Unknown,
        ]);
        let baseline_git = BaselineConfig {
            kind: BaselineKind::Git,
            rev: Some("origin/main".to_string()),
            ..BaselineConfig::default()
        };
        let summary = build_summary_data(&report, &baseline_git);
        assert_eq!(summary.max_required_bump, Some("minor".to_string()));
        assert_eq!(summary.baseline_kind, "git");
        assert_eq!(summary.baseline_ref, Some("origin/main".to_string()));

        let report_patch =
            report_with_required_bumps(&[RequiredBump::Patch, RequiredBump::Unknown]);
        let summary_patch = build_summary_data(&report_patch, &BaselineConfig::default());
        assert_eq!(summary_patch.max_required_bump, Some("patch".to_string()));

        let report_unknown = report_with_required_bumps(&[RequiredBump::Unknown]);
        let summary_unknown = build_summary_data(&report_unknown, &BaselineConfig::default());
        assert_eq!(
            summary_unknown.max_required_bump,
            Some("unknown".to_string())
        );
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

    #[test]
    fn test_fingerprint_computation_deterministic() {
        let fp1 = compute_fingerprint("semver", "violation", "my-crate", "1.0.0");
        let fp2 = compute_fingerprint("semver", "violation", "my-crate", "1.0.0");
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_fingerprint_different_for_different_inputs() {
        let fp1 = compute_fingerprint("semver", "violation", "crate-a", "1.0.0");
        let fp2 = compute_fingerprint("semver", "violation", "crate-b", "1.0.0");
        let fp3 = compute_fingerprint("semver", "violation", "crate-a", "2.0.0");
        let fp4 = compute_fingerprint("baseline", "missing", "crate-a", "1.0.0");
        assert_ne!(fp1, fp2);
        assert_ne!(fp1, fp3);
        assert_ne!(fp1, fp4);
    }

    #[test]
    fn test_capability_context_default() {
        let ctx = CapabilityContext::new();
        assert!(!ctx.git_available);
        assert!(!ctx.baseline_available);
        assert!(ctx.git_detail.is_none());
        assert!(ctx.baseline_detail.is_none());
    }

    #[test]
    fn test_capability_context_builder() {
        let ctx = CapabilityContext::new()
            .with_git_available(true)
            .with_git_detail("git 2.39.0")
            .with_baseline_available(true)
            .with_baseline_detail("baseline resolved from origin/main");

        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Available);
        assert_eq!(caps.git.detail, Some("git 2.39.0".to_string()));
        assert!(caps.git.reason.is_none());
        assert_eq!(caps.baseline.status, CapabilityStatus::Available);
        assert_eq!(
            caps.baseline.detail,
            Some("baseline resolved from origin/main".to_string())
        );
        assert!(caps.baseline.reason.is_none());
    }

    #[test]
    fn test_capability_context_unavailable() {
        let ctx = CapabilityContext::new()
            .with_git_available(false)
            .with_git_detail("git not found")
            .with_baseline_available(false)
            .with_baseline_detail("no baseline configured");

        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Unavailable);
        assert_eq!(caps.git.reason, Some("git_unavailable".to_string()));
        assert_eq!(caps.baseline.status, CapabilityStatus::Unavailable);
        assert_eq!(caps.baseline.reason, Some("resolution_failed".to_string()));
    }

    #[test]
    fn test_build_receipt_with_capabilities() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );
        let ctx = CapabilityContext::new()
            .with_git_available(true)
            .with_baseline_available(true);

        let receipt = build_receipt_with_capabilities(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
            Some(&ctx),
        );

        assert!(receipt.run.capabilities.is_some());
        let caps = receipt.run.capabilities.unwrap();
        assert_eq!(caps.git.status, CapabilityStatus::Available);
        assert_eq!(caps.baseline.status, CapabilityStatus::Available);
    }

    #[test]
    fn test_findings_have_fingerprints() {
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

        // Findings from packages should have fingerprints
        assert!(!receipt.findings.is_empty());
        let non_tool: Vec<_> = receipt
            .findings
            .iter()
            .filter(|f| f.check_id != CHECK_TOOL)
            .collect();
        assert!(!non_tool.is_empty());
        for finding in non_tool {
            assert!(finding.fingerprint.is_some());
        }
    }

    #[test]
    fn test_code_token_format() {
        // 2D: Validate all tokens match ^[a-z0-9_]+(\.[a-z0-9_]+)*$
        let tokens = [
            CHECK_SEMVER,
            CODE_VIOLATION,
            CHECK_BASELINE,
            CODE_MISSING,
            CHECK_TOOL,
            CODE_ERROR,
            CHECK_ENGINE,
            CODE_UNKNOWN,
        ];
        let re = regex_lite::Regex::new(r"^[a-z0-9_]+(\.[a-z0-9_]+)*$").unwrap();
        for token in &tokens {
            assert!(
                re.is_match(token),
                "Token '{}' does not match expected format",
                token
            );
        }
    }

    #[test]
    fn test_reason_token_format() {
        let tokens = [
            REASON_SEMVER_VIOLATION,
            REASON_BASELINE_UNAVAILABLE,
            REASON_TOOL_ERROR,
            REASON_ENGINE_ERROR,
            REASON_TRUNCATED,
            REASON_ALL_PACKAGES_SKIPPED,
            REASON_WAIVED,
            CAP_REASON_GIT_UNAVAILABLE,
            CAP_REASON_SHALLOW_CLONE,
            CAP_REASON_RESOLUTION_FAILED,
            CAP_REASON_NOT_REQUIRED,
        ];
        let re = regex_lite::Regex::new(r"^[a-z][a-z0-9_]*$").unwrap();
        for token in &tokens {
            assert!(
                re.is_match(token),
                "Reason token '{}' does not match expected format",
                token
            );
        }
    }

    #[test]
    fn test_required_bump_str_variants() {
        assert_eq!(required_bump_str(RequiredBump::Minor), "minor");
        assert_eq!(required_bump_str(RequiredBump::Patch), "patch");
        assert_eq!(required_bump_str(RequiredBump::Unknown), "unknown");
    }

    #[test]
    fn test_compute_suggested_version() {
        assert_eq!(
            compute_suggested_version("1.0.0", RequiredBump::Major),
            Some("2.0.0".to_string())
        );
        assert_eq!(
            compute_suggested_version("1.2.3", RequiredBump::Minor),
            Some("1.3.0".to_string())
        );
        assert_eq!(
            compute_suggested_version("1.2.3", RequiredBump::Patch),
            Some("1.2.4".to_string())
        );
        assert_eq!(
            compute_suggested_version("1.2.3", RequiredBump::Unknown),
            None
        );
        assert_eq!(
            compute_suggested_version("not-semver", RequiredBump::Major),
            None
        );
    }

    #[test]
    fn test_normalize_receipt_path_dotdot_guard() {
        let workspace = Path::new("/workspace");
        // Path outside workspace should fall back to filename
        let result = normalize_receipt_path(workspace, Path::new("/other/dir/Cargo.toml"));
        // On the same filesystem, strip_prefix fails, path contains ..
        // The implementation falls back to filename
        assert!(
            !result.contains(".."),
            "Result should not contain '..' path traversal: {}",
            result
        );
    }

    #[test]
    fn test_normalize_receipt_path_forward_slashes() {
        let workspace = Path::new("/workspace");
        let result = normalize_receipt_path(workspace, Path::new("/workspace/foo/bar/Cargo.toml"));
        assert!(
            !result.contains('\\'),
            "Normalized path should not contain backslashes: {}",
            result
        );
    }

    #[test]
    fn test_normalize_receipt_path_no_absolute_in_output() {
        let workspace = Path::new("/workspace");
        let result =
            normalize_receipt_path(workspace, Path::new("/workspace/crates/lib/Cargo.toml"));
        assert!(
            !result.starts_with('/'),
            "Normalized path should be relative, not absolute: {}",
            result
        );
    }

    #[test]
    fn test_normalize_receipt_path_relative_input() {
        let workspace = Path::new("/workspace");
        let result =
            normalize_receipt_path(workspace, Path::new("artifacts/semverguard/report.json"));
        assert!(
            !result.contains('\\'),
            "Relative path should use forward slashes: {}",
            result
        );
        assert!(
            !result.contains(".."),
            "Relative path should not contain path traversal: {}",
            result
        );
    }

    #[test]
    fn test_truncation_not_applied_under_limit() {
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
        // Under the limit: no truncation totals in data, no "truncated" reason
        let data = receipt.data.as_ref().expect("data should be present");
        assert!(data.findings_total.is_none());
        assert!(data.findings_emitted.is_none());
        assert!(
            !receipt.verdict.reasons.contains(&"truncated".to_string()),
            "Verdict should not contain 'truncated' reason when under limit"
        );
    }

    #[test]
    fn test_shallow_clone_capability() {
        let ctx = CapabilityContext::new()
            .with_git_available(true)
            .with_shallow_clone(true);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Available);
        assert!(caps.git.detail.as_ref().unwrap().contains("shallow clone"));
        assert_eq!(caps.git.reason, Some("shallow_clone".to_string()));
    }

    #[test]
    fn test_git_skipped_overrides_available() {
        // Even when git is available, skipped takes priority
        let ctx = CapabilityContext::new()
            .with_git_available(true)
            .with_git_skipped(true);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Skipped);
        assert_eq!(caps.git.reason, Some("not_required".to_string()));
    }

    #[test]
    fn test_git_skipped_overrides_unavailable() {
        // Even when git is unavailable, skipped takes priority
        let ctx = CapabilityContext::new()
            .with_git_available(false)
            .with_git_skipped(true);
        let caps = ctx.build();
        assert_eq!(caps.git.status, CapabilityStatus::Skipped);
        assert_eq!(caps.git.reason, Some("not_required".to_string()));
    }

    #[test]
    fn test_baseline_skipped() {
        let ctx = CapabilityContext::new()
            .with_baseline_available(true)
            .with_baseline_skipped(true);
        let caps = ctx.build();
        assert_eq!(caps.baseline.status, CapabilityStatus::Skipped);
        assert_eq!(caps.baseline.reason, Some("not_required".to_string()));
    }

    #[test]
    fn test_baseline_skipped_overrides_unavailable() {
        let ctx = CapabilityContext::new()
            .with_baseline_available(false)
            .with_baseline_skipped(true);
        let caps = ctx.build();
        assert_eq!(caps.baseline.status, CapabilityStatus::Skipped);
        assert_eq!(caps.baseline.reason, Some("not_required".to_string()));
    }

    #[test]
    fn test_waiver_applied_by_fingerprint() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );
        // The sample report has b-lib with SemverViolation
        let fp = compute_fingerprint("semver", "violation", "b-lib", "2.0.0");
        let waivers = vec![WaiverEntry {
            fingerprint: fp.clone(),
            reason: "Intentional break".to_string(),
            ticket: Some("GH#1".to_string()),
            expires: None,
        }];
        let receipt = build_receipt_with_capabilities_versioned(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
            None,
            None,
            &waivers,
        );
        // The finding should be waived
        let waived_finding = receipt
            .findings
            .iter()
            .find(|f| f.fingerprint.as_deref() == Some(&fp));
        assert!(waived_finding.is_some());
        let waived = waived_finding.unwrap().waived.as_ref().unwrap();
        assert_eq!(waived.reason, "Intentional break");
        assert_eq!(waived.ticket, Some("GH#1".to_string()));
    }

    #[test]
    fn test_capability_context_git_version_combines_with_detail() {
        let ctx = CapabilityContext::new()
            .with_git_available(true)
            .with_git_detail("origin/main")
            .with_git_version("git 2.42.0");
        let caps = ctx.build();
        assert_eq!(
            caps.git.detail,
            Some("git 2.42.0; rev=origin/main".to_string())
        );
    }

    #[test]
    fn test_resolve_artifacts_dir_absolute() {
        let dir = tempdir().unwrap();
        let artifacts = resolve_artifacts_dir(Path::new("/workspace"), dir.path());
        assert_eq!(artifacts, dir.path());
    }

    #[test]
    fn test_build_findings_failure_kinds_and_data() {
        let packages = vec![
            PackageReport {
                name: "alpha".to_string(),
                version: "1.2.3".to_string(),
                manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 10,
                command: vec![],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: String::new(),
                    stderr: "breaking".to_string(),
                    required_bump: Some(RequiredBump::Major),
                }),
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            },
            PackageReport {
                name: "beta".to_string(),
                version: "0.9.0".to_string(),
                manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 5,
                command: vec![],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(2),
                    success: false,
                    stdout: String::new(),
                    stderr: "baseline missing".to_string(),
                    required_bump: None,
                }),
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::BaselineError),
                baseline_error: Some(BaselineErrorCause::RevisionNotFound {
                    rev: "origin/missing".to_string(),
                }),
            },
            PackageReport {
                name: "gamma".to_string(),
                version: "2.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/gamma/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: Some("tool crashed".to_string()),
                duration_ms: 1,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::ToolError),
                baseline_error: None,
            },
            PackageReport {
                name: "delta".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/delta/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 1,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::Unknown),
                baseline_error: None,
            },
            PackageReport {
                name: "skip".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/skip/Cargo.toml"),
                status: PackageStatus::Skipped,
                skip_reason: Some("filtered".to_string()),
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: None,
                baseline_error: None,
            },
        ];
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages,
            summary: Summary {
                total: 5,
                passed: 0,
                failed: 4,
                skipped: 1,
            },
        };
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );

        let findings = build_findings(&report, &artifacts, report.workspace_root.as_path());
        assert_eq!(findings.len(), 4);

        let semver = findings
            .iter()
            .find(|f| f.check_id == CHECK_SEMVER)
            .unwrap();
        let semver_data = semver.data.as_ref().unwrap();
        assert_eq!(
            semver_data
                .get("suggested_version")
                .and_then(|v| v.as_str()),
            Some("2.0.0")
        );

        let baseline = findings
            .iter()
            .find(|f| f.check_id == CHECK_BASELINE)
            .unwrap();
        assert!(baseline.message.contains("baseline missing"));
        let baseline_data = baseline.data.as_ref().unwrap();
        assert!(baseline_data.get("baseline_error_cause").is_some());

        let tool = findings.iter().find(|f| f.check_id == CHECK_TOOL).unwrap();
        assert!(tool.message.contains("tool crashed"));

        let unknown = findings
            .iter()
            .find(|f| f.check_id == CHECK_ENGINE)
            .unwrap();
        assert!(unknown.message.contains("unknown error"));
    }

    #[test]
    fn test_build_failure_message_without_bump() {
        let pkg = PackageReport {
            name: "alpha".to_string(),
            version: "1.2.3".to_string(),
            manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 1,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: Some(FailureKind::SemverViolation),
            baseline_error: None,
        };

        let message = build_failure_message(&pkg, FailureKind::SemverViolation);
        assert!(message.contains("failed semantic versioning checks"));
    }

    #[test]
    fn test_apply_waivers_with_expired_and_invalid_dates() {
        let mut findings = vec![Finding {
            check_id: CHECK_SEMVER.to_string(),
            code: CODE_VIOLATION.to_string(),
            level: FindingLevel::Error,
            message: "boom".to_string(),
            location: None,
            data: None,
            fingerprint: Some("fp".to_string()),
            waived: None,
        }];
        let waivers = vec![
            WaiverEntry {
                fingerprint: "fp".to_string(),
                reason: "expired".to_string(),
                ticket: None,
                expires: Some("2000-01-01".to_string()),
            },
            WaiverEntry {
                fingerprint: "fp".to_string(),
                reason: "invalid-date".to_string(),
                ticket: None,
                expires: Some("not-a-date".to_string()),
            },
        ];

        apply_waivers(&mut findings, &waivers);
        let waived = findings[0].waived.as_ref().unwrap();
        assert_eq!(waived.reason, "invalid-date");
    }

    #[test]
    fn test_apply_waivers_non_expired_applies() {
        let mut findings = vec![Finding {
            check_id: CHECK_SEMVER.to_string(),
            code: CODE_VIOLATION.to_string(),
            level: FindingLevel::Error,
            message: "boom".to_string(),
            location: None,
            data: None,
            fingerprint: Some("fp-ok".to_string()),
            waived: None,
        }];
        let waivers = vec![WaiverEntry {
            fingerprint: "fp-ok".to_string(),
            reason: "ok".to_string(),
            ticket: None,
            expires: Some("2099-01-01".to_string()),
        }];

        apply_waivers(&mut findings, &waivers);
        assert!(findings[0].waived.is_some());
    }

    #[test]
    fn test_apply_waivers_skips_missing_fingerprint() {
        let mut findings = vec![Finding {
            check_id: CHECK_SEMVER.to_string(),
            code: CODE_VIOLATION.to_string(),
            level: FindingLevel::Error,
            message: "boom".to_string(),
            location: None,
            data: None,
            fingerprint: None,
            waived: None,
        }];
        let waivers = vec![WaiverEntry {
            fingerprint: "fp-ok".to_string(),
            reason: "ok".to_string(),
            ticket: None,
            expires: Some("2099-01-01".to_string()),
        }];

        apply_waivers(&mut findings, &waivers);
        assert!(findings[0].waived.is_none());
    }

    #[test]
    fn test_is_waiver_expired_with_valid_date() {
        let waiver = WaiverEntry {
            fingerprint: "fp".to_string(),
            reason: "expired".to_string(),
            ticket: None,
            expires: Some("2000-01-01".to_string()),
        };
        assert!(is_waiver_expired(&waiver));
    }

    #[test]
    fn test_is_waiver_expired_invalid_month_returns_false() {
        let waiver = WaiverEntry {
            fingerprint: "fp".to_string(),
            reason: "invalid".to_string(),
            ticket: None,
            expires: Some("2024-13-01".to_string()),
        };
        assert!(!is_waiver_expired(&waiver));
    }

    #[test]
    fn test_is_waiver_expired_invalid_day_returns_false() {
        let waiver = WaiverEntry {
            fingerprint: "fp".to_string(),
            reason: "invalid".to_string(),
            ticket: None,
            expires: Some("2024-02-30".to_string()),
        };
        assert!(!is_waiver_expired(&waiver));
    }

    #[test]
    fn test_is_waiver_expired_invalid_format_returns_false() {
        let waiver = WaiverEntry {
            fingerprint: "fp".to_string(),
            reason: "invalid".to_string(),
            ticket: None,
            expires: Some("bad".to_string()),
        };
        assert!(!is_waiver_expired(&waiver));
    }

    #[test]
    fn test_derive_verdict_includes_engine_waived_truncated_and_skipped() {
        let findings = vec![
            Finding {
                check_id: CHECK_ENGINE.to_string(),
                code: CODE_UNKNOWN.to_string(),
                level: FindingLevel::Error,
                message: "engine failed".to_string(),
                location: None,
                data: None,
                fingerprint: None,
                waived: None,
            },
            Finding {
                check_id: CHECK_BASELINE.to_string(),
                code: CODE_MISSING.to_string(),
                level: FindingLevel::Warning,
                message: "baseline missing".to_string(),
                location: None,
                data: None,
                fingerprint: None,
                waived: Some(WaiverInfo {
                    reason: "waived".to_string(),
                    ticket: None,
                    expires: None,
                }),
            },
        ];
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 0,
                skipped: 1,
            },
        };

        let (status, reasons) = derive_verdict(&findings, Some(&report), true);
        assert_eq!(status, VerdictStatus::Fail);
        assert!(reasons.contains(&REASON_ENGINE_ERROR.to_string()));
        assert!(reasons.contains(&REASON_ALL_PACKAGES_SKIPPED.to_string()));
        assert!(reasons.contains(&REASON_TRUNCATED.to_string()));
        assert!(reasons.contains(&REASON_WAIVED.to_string()));
    }

    #[test]
    fn test_derive_verdict_unknown_check_id_passes() {
        let findings = vec![Finding {
            check_id: "custom".to_string(),
            code: CODE_UNKNOWN.to_string(),
            level: FindingLevel::Error,
            message: "custom".to_string(),
            location: None,
            data: None,
            fingerprint: None,
            waived: None,
        }];

        let (status, reasons) = derive_verdict(&findings, None, false);
        assert_eq!(status, VerdictStatus::Pass);
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_derive_verdict_baseline_warn() {
        let findings = vec![Finding {
            check_id: CHECK_BASELINE.to_string(),
            code: CODE_MISSING.to_string(),
            level: FindingLevel::Warning,
            message: "baseline missing".to_string(),
            location: None,
            data: None,
            fingerprint: None,
            waived: None,
        }];

        let (status, reasons) = derive_verdict(&findings, None, false);
        assert_eq!(status, VerdictStatus::Warn);
        assert!(reasons.contains(&REASON_BASELINE_UNAVAILABLE.to_string()));
    }

    #[test]
    fn test_derive_verdict_all_skipped() {
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
            summary: Summary {
                total: 2,
                passed: 0,
                failed: 0,
                skipped: 2,
            },
        };

        let (status, reasons) = derive_verdict(&[], Some(&report), false);
        assert_eq!(status, VerdictStatus::Skip);
        assert!(reasons.contains(&REASON_ALL_PACKAGES_SKIPPED.to_string()));
    }

    #[test]
    fn test_write_raw_logs_handles_missing_engine_and_skipped() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("raw")).unwrap();

        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "skip".to_string(),
                    version: "0.1.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/skip/Cargo.toml"),
                    status: PackageStatus::Skipped,
                    skip_reason: Some("filtered".to_string()),
                    duration_ms: 0,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
                PackageReport {
                    name: "beta".to_string(),
                    version: "0.2.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: Some("engine missing".to_string()),
                    duration_ms: 10,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: Some(FailureKind::ToolError),
                    baseline_error: None,
                },
                PackageReport {
                    name: "alpha".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
                    status: PackageStatus::Passed,
                    skip_reason: None,
                    duration_ms: 5,
                    command: vec![],
                    engine: Some(SemverCheckOutput {
                        exit_code: Some(0),
                        success: true,
                        stdout: "ok".to_string(),
                        stderr: String::new(),
                        required_bump: None,
                    }),
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
            ],
            summary: Summary {
                total: 3,
                passed: 1,
                failed: 1,
                skipped: 1,
            },
        };

        write_raw_logs(dir.path(), &report).unwrap();
        let (stdout_path, stderr_path) = raw_log_paths(dir.path(), &report.packages[1]);
        let stdout = fs::read_to_string(stdout_path).unwrap();
        let stderr = fs::read_to_string(stderr_path).unwrap();
        assert_eq!(stdout, "no output");
        assert_eq!(stderr, "engine missing");
    }

    #[test]
    fn test_write_receipt_bundle_compact_json() {
        let dir = tempdir().unwrap();
        let report = sample_report();
        let artifacts =
            build_artifact_index(Path::new("workspace"), dir.path(), Some(&report), false);
        let receipt = build_receipt(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
        );

        write_receipt_bundle(dir.path(), &receipt, Some(&report), false, false).unwrap();

        let contents = fs::read_to_string(dir.path().join("report.json")).unwrap();
        assert!(contents.contains("\"schema\""));
    }

    #[test]
    fn test_normalize_receipt_path_traversal_fallback() {
        let workspace = Path::new("/workspace");
        let result = normalize_receipt_path(workspace, Path::new("/workspace/../secret.txt"));
        assert_eq!(result, "secret.txt");
    }

    #[test]
    fn test_sanitize_filename_component_replaces_special_chars() {
        let sanitized = sanitize_filename_component("a/b:c");
        assert_eq!(sanitized, "a_b_c");
    }

    #[test]
    fn test_exit_code_from_receipt_tool_error_and_warn_as_fail() {
        let verdict = Verdict {
            status: VerdictStatus::Fail,
            reasons: vec![],
        };
        assert_eq!(exit_code_from_receipt(&verdict, false, true), 1);

        let warn_verdict = Verdict {
            status: VerdictStatus::Warn,
            reasons: vec![],
        };
        assert_eq!(exit_code_from_receipt(&warn_verdict, true, false), 3);
    }

    #[test]
    fn test_exit_code_from_receipt_warn_without_fail() {
        let verdict = Verdict {
            status: VerdictStatus::Warn,
            reasons: vec![],
        };
        assert_eq!(exit_code_from_receipt(&verdict, false, false), 0);
    }

    #[test]
    fn test_truncation_applied_over_limit() {
        let errors: Vec<ToolErrorFinding> = (0..(MAX_FINDINGS + 1))
            .map(|_| ToolErrorFinding::new("boom".to_string()))
            .collect();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            None,
            false,
        );
        let receipt = build_receipt_with_capabilities(
            None,
            &errors,
            &artifacts,
            &BaselineConfig::default(),
            Path::new("/workspace"),
            None,
        );
        assert_eq!(receipt.findings.len(), MAX_FINDINGS);
        assert!(
            receipt
                .verdict
                .reasons
                .contains(&REASON_TRUNCATED.to_string())
        );
    }

    #[test]
    fn test_waived_findings_skip_verdict() {
        let report = sample_report();
        let artifacts = build_artifact_index(
            Path::new("workspace"),
            Path::new("artifacts/semverguard"),
            Some(&report),
            false,
        );
        let fp = compute_fingerprint("semver", "violation", "b-lib", "2.0.0");
        let waivers = vec![WaiverEntry {
            fingerprint: fp,
            reason: "Intentional".to_string(),
            ticket: None,
            expires: None,
        }];
        let receipt = build_receipt_with_capabilities_versioned(
            Some(&report),
            &[],
            &artifacts,
            &BaselineConfig::default(),
            report.workspace_root.as_path(),
            None,
            None,
            &waivers,
        );
        // Verdict should NOT be Fail since the semver violation is waived
        assert_ne!(receipt.verdict.status, VerdictStatus::Fail);
        assert!(receipt.verdict.reasons.contains(&"waived".to_string()));
    }
}
