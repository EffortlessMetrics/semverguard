use crate::{BaselineConfig, RunReport};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Status of a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityStatus {
    /// Capability is available and was used successfully.
    Available,
    /// Capability is not available (e.g., git not found, baseline not resolved).
    Unavailable,
    /// Capability was explicitly skipped.
    Skipped,
}

/// Capability status with optional detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInfo {
    /// Status of this capability.
    pub status: CapabilityStatus,
    /// Optional detail explaining the status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Machine-readable reason token explaining the status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Capabilities block for "No Green By Omission".
///
/// This block explicitly reports the status of key capabilities
/// to prevent silent passes when prerequisites are missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCapabilities {
    /// Git provider availability.
    pub git: CapabilityInfo,
    /// Baseline resolution status.
    pub baseline: CapabilityInfo,
}

/// Sensor report envelope for cockpit ingestion (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReportV1 {
    /// Schema identifier for this receipt.
    pub schema: String,
    /// Tool metadata.
    pub tool: ToolInfo,
    /// Run metadata.
    pub run: RunInfo,
    /// Final verdict for this run.
    pub verdict: Verdict,
    /// Findings emitted by the tool.
    pub findings: Vec<Finding>,
    /// Tool-specific payload (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SemverguardData>,
    /// Artifact index for the run.
    pub artifacts: ArtifactIndex,
}

/// Tool information for the receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name.
    pub name: String,
    /// Tool version.
    pub version: String,
    /// Repository URL (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// Run metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    /// ISO8601 start timestamp (UTC).
    pub started_at: String,
    /// ISO8601 finish timestamp (UTC).
    pub finished_at: String,
    /// Duration in milliseconds.
    pub duration_ms: u128,
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Baseline configuration used.
    pub baseline: BaselineConfig,
    /// Capabilities status for "No Green By Omission".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<RunCapabilities>,
}

/// Final verdict for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// Verdict status.
    pub status: VerdictStatus,
    /// Machine-readable reason tokens explaining the verdict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// Verdict status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerdictStatus {
    /// All checks passed.
    Pass,
    /// Warnings present, no hard failures.
    Warn,
    /// One or more failures.
    Fail,
    /// No checks were run.
    Skip,
}

/// A single finding emitted by the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Producer identifier (e.g., semver, engine, baseline).
    pub check_id: String,
    /// Classification code (e.g., violation, missing).
    pub code: String,
    /// Severity level.
    pub level: FindingLevel,
    /// Human-readable message.
    pub message: String,
    /// Optional location data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<FindingLocation>,
    /// Optional structured data payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Stable semantic fingerprint for deduplication.
    ///
    /// Computed from check_id, code, package name, and version.
    /// Enables deterministic deduplication across runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Waiver info if this finding was waived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waived: Option<WaiverInfo>,
}

/// Severity levels for findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingLevel {
    /// Error-level finding.
    Error,
    /// Warning-level finding.
    Warning,
    /// Informational finding.
    Info,
}

impl FindingLevel {
    /// Returns a numeric rank for sorting: Error=0 (highest severity), Warning=1, Info=2.
    pub fn severity_rank(&self) -> u8 {
        match self {
            FindingLevel::Error => 0,
            FindingLevel::Warning => 1,
            FindingLevel::Info => 2,
        }
    }
}

/// Waiver information attached to a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaiverInfo {
    /// Human-readable reason for the waiver.
    pub reason: String,
    /// Optional ticket reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// Optional expiration date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Best-effort location information for a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingLocation {
    /// Path to the relevant file (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Line number (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// Path to raw log file (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_log: Option<String>,
}

/// Summary data promoted for cockpit dashboard display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryData {
    /// Total number of packages in the report.
    pub packages_total: u32,
    /// Number of packages checked (non-skipped).
    pub packages_checked: u32,
    /// Number of skipped packages.
    pub packages_skipped: u32,
    /// Number of packages with SemVer violations.
    pub violations: u32,
    /// Number of packages with baseline errors.
    pub baseline_issues: u32,
    /// Maximum required bump across all violations (major/minor/patch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_required_bump: Option<String>,
    /// Baseline kind used for comparison.
    pub baseline_kind: String,
    /// Baseline reference (rev or version).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_ref: Option<String>,
}

/// Semverguard-specific payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemverguardData {
    /// Legacy run report payload.
    pub report: RunReport,
    /// Total number of findings before truncation (if truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings_total: Option<usize>,
    /// Number of findings emitted after truncation (if truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings_emitted: Option<usize>,
    /// Summary data for cockpit dashboards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryData>,
}

/// Artifact index for a receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactIndex {
    /// Path to the receipt JSON (sensor.report.v1).
    pub report_json: String,
    /// Path to the PR comment markdown.
    pub comment_md: String,
    /// Path to SARIF output (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sarif_json: Option<String>,
    /// Raw logs for engine output.
    pub raw_logs: Vec<RawLogRef>,
}

/// Raw log reference for a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLogRef {
    /// Package name.
    pub package: String,
    /// Package version.
    pub version: String,
    /// Stdout log path (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Stderr log path (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageReport, PackageStatus, Summary};

    fn sample_report() -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "lib-a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/crates/lib-a/Cargo.toml"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 500,
                command: vec!["cargo".to_string()],
                engine: None,
                inferred_required_bump: None,
                failure_kind: None,
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 1,
                failed: 0,
                skipped: 0,
            },
        }
    }

    #[test]
    fn test_finding_level_severity_rank() {
        assert_eq!(FindingLevel::Error.severity_rank(), 0);
        assert_eq!(FindingLevel::Warning.severity_rank(), 1);
        assert_eq!(FindingLevel::Info.severity_rank(), 2);
    }

    #[test]
    fn test_receipt_json_roundtrip() {
        let report = sample_report();
        let receipt = SensorReportV1 {
            schema: "sensor.report.v1".to_string(),
            tool: ToolInfo {
                name: "semverguard".to_string(),
                version: "0.1.0".to_string(),
                repository_url: Some("https://example.com".to_string()),
            },
            run: RunInfo {
                started_at: report.started_at.clone(),
                finished_at: report.finished_at.clone(),
                duration_ms: 1000,
                workspace_root: report.workspace_root.clone(),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                reasons: vec![],
            },
            findings: vec![],
            data: Some(SemverguardData {
                report,
                findings_total: None,
                findings_emitted: None,
                summary: None,
            }),
            artifacts: ArtifactIndex {
                report_json: "artifacts/semverguard/report.json".to_string(),
                comment_md: "artifacts/semverguard/comment.md".to_string(),
                sarif_json: None,
                raw_logs: vec![],
            },
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let parsed: SensorReportV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema, "sensor.report.v1");
        assert!(matches!(parsed.verdict.status, VerdictStatus::Pass));
        assert!(parsed.data.is_some());
    }

    #[test]
    fn test_verdict_status_kebab_case() {
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Fail).unwrap(),
            "\"fail\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Skip).unwrap(),
            "\"skip\""
        );
    }

    #[test]
    fn test_finding_level_kebab_case() {
        assert_eq!(
            serde_json::to_string(&FindingLevel::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&FindingLevel::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&FindingLevel::Info).unwrap(),
            "\"info\""
        );
    }

    #[test]
    fn test_receipt_schema_validation() {
        use std::fs;

        let schema_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/sensor.report.v1.json");
        let schema_str = fs::read_to_string(schema_path).unwrap();
        let schema_json: serde_json::Value = serde_json::from_str(&schema_str).unwrap();
        let validator = jsonschema::validator_for(&schema_json).unwrap();

        let report = sample_report();
        let receipt = SensorReportV1 {
            schema: "sensor.report.v1".to_string(),
            tool: ToolInfo {
                name: "semverguard".to_string(),
                version: "0.1.0".to_string(),
                repository_url: None,
            },
            run: RunInfo {
                started_at: report.started_at.clone(),
                finished_at: report.finished_at.clone(),
                duration_ms: 1000,
                workspace_root: report.workspace_root.clone(),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                reasons: vec![],
            },
            findings: vec![],
            data: Some(SemverguardData {
                report,
                findings_total: None,
                findings_emitted: None,
                summary: None,
            }),
            artifacts: ArtifactIndex {
                report_json: "artifacts/semverguard/report.json".to_string(),
                comment_md: "artifacts/semverguard/comment.md".to_string(),
                sarif_json: None,
                raw_logs: vec![],
            },
        };

        let value = serde_json::to_value(receipt).unwrap();
        assert!(validator.is_valid(&value), "receipt should validate against schema");
    }
}
