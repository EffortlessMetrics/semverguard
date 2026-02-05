//! SARIF 2.1.0 report generation for semverguard.
//!
//! This module implements the Static Analysis Results Interchange Format (SARIF)
//! output format, which is used by security and code quality tools like GitHub
//! Code Scanning, VS Code SARIF Viewer, and other analysis platforms.
//!
//! ## Honesty Policy
//!
//! SemVer engines like `cargo-semver-checks` do not reliably provide file/line
//! locations for violations. This module follows an **honesty policy**:
//!
//! - **Locations always point to `Cargo.toml`** (the manifest path), not invented
//!   source locations. This is the most accurate location we can provide.
//! - **No region data** (line/column) is ever emitted, since we cannot reliably
//!   determine the exact location of a violation.
//! - **Raw log references** are included in messages when available, allowing
//!   users to inspect the full engine output for debugging.
//! - **Distinct rules** map to the error taxonomy: SemverViolation, ToolError,
//!   BaselineError with appropriate severity levels.
//!
//! See: <https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html>

use semverguard_types::{
    FailureKind, Finding, PackageReport, PackageStatus, RequiredBump, RunReport, SensorReportV1,
};
use serde::Serialize;
use std::path::Path;

/// SARIF 2.1.0 schema version.
const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";

/// SARIF log containing all analysis results.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLog {
    /// JSON schema reference.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// SARIF format version.
    pub version: String,
    /// Analysis runs (typically one per tool invocation).
    pub runs: Vec<SarifRun>,
}

/// A single run of an analysis tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRun {
    /// Information about the tool that produced the results.
    pub tool: SarifTool,
    /// The results of the analysis.
    pub results: Vec<SarifResult>,
    /// Invocations of the tool.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<SarifInvocation>,
}

/// Information about the analysis tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifTool {
    /// The tool driver (primary component).
    pub driver: SarifToolComponent,
}

/// Information about a tool component.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifToolComponent {
    /// Tool name.
    pub name: String,
    /// Semantic version of the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_version: Option<String>,
    /// Full name with version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// Tool information URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub information_uri: Option<String>,
    /// Rules defined by this tool.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SarifRule>,
}

/// A rule (lint/check) defined by the tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    /// Stable identifier for the rule.
    pub id: String,
    /// Short description of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<SarifMessage>,
    /// Full description of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_description: Option<SarifMessage>,
    /// Help text for the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<SarifMessage>,
    /// Help URI for the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
    /// Default configuration for the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_configuration: Option<SarifRuleConfiguration>,
}

/// Default configuration for a rule.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRuleConfiguration {
    /// Severity level.
    pub level: SarifLevel,
}

/// Severity level for a result.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SarifLevel {
    /// Indicates a serious problem.
    Error,
    /// Indicates a potential problem.
    Warning,
    /// Informational message.
    Note,
    /// No level specified (part of SARIF spec).
    #[allow(dead_code)]
    None,
}

/// A message in SARIF format.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifMessage {
    /// The message text.
    pub text: String,
    /// Markdown version of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

impl SarifMessage {
    /// Create a simple text message.
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            markdown: None,
        }
    }

    /// Create a message with optional raw log reference appended.
    ///
    /// If a raw log path is provided, appends a reference to the message
    /// to help users find detailed engine output.
    pub fn with_raw_log_ref(message: impl Into<String>, raw_log: Option<&str>) -> Self {
        let base = message.into();
        let text = match raw_log {
            Some(log_path) => format!("{} (see raw log: {})", base, log_path),
            None => base,
        };
        Self {
            text,
            markdown: None,
        }
    }
}

/// A single result from the analysis.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    /// The rule that was violated.
    pub rule_id: String,
    /// Severity level.
    pub level: SarifLevel,
    /// Human-readable message.
    pub message: SarifMessage,
    /// Locations where the issue was found.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<SarifLocation>,
    /// Additional properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<SarifResultProperties>,
}

/// Location information for a result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    /// Physical location in a file.
    pub physical_location: SarifPhysicalLocation,
}

/// Physical location in a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    /// Artifact (file) location.
    pub artifact_location: SarifArtifactLocation,
    /// Region within the file (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

/// Artifact (file) location.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifArtifactLocation {
    /// URI to the file.
    pub uri: String,
    /// URI base ID for relative URIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri_base_id: Option<String>,
}

/// Region within a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    /// Starting line (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    /// Starting column (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u32>,
    /// Ending line (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// Ending column (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

/// Custom properties for a result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResultProperties {
    /// Package name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    /// Package version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    /// Required bump level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_bump: Option<String>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    /// Path to raw stderr log (for debugging).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_stderr_log: Option<String>,
    /// Path to raw stdout log (for debugging).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_stdout_log: Option<String>,
    /// Failure classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
}

/// Invocation information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifInvocation {
    /// Whether the invocation succeeded.
    pub execution_successful: bool,
    /// Start time in ISO8601 format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_utc: Option<String>,
    /// End time in ISO8601 format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_utc: Option<String>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<SarifArtifactLocation>,
}

// Rule IDs for semverguard
const RULE_SEMVER_BREAKING: &str = "semverguard/breaking-change";
const RULE_SEMVER_MAJOR_REQUIRED: &str = "semverguard/major-bump-required";
const RULE_SEMVER_MINOR_REQUIRED: &str = "semverguard/minor-bump-required";
const RULE_SEMVER_PATCH_REQUIRED: &str = "semverguard/patch-bump-required";
const RULE_TOOL_ERROR: &str = "semverguard/tool-error";
const RULE_BASELINE_ERROR: &str = "semverguard/baseline-error";

/// Generate SARIF rules for semverguard.
fn generate_rules() -> Vec<SarifRule> {
    vec![
        SarifRule {
            id: RULE_SEMVER_BREAKING.to_string(),
            short_description: Some(SarifMessage::text("Breaking API change detected")),
            full_description: Some(SarifMessage::text(
                "A breaking change to the public API was detected that violates semantic versioning rules.",
            )),
            help: Some(SarifMessage::text(
                "Review the changes and either revert the breaking change or bump the major version.",
            )),
            help_uri: Some("https://semver.org/".to_string()),
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Error,
            }),
        },
        SarifRule {
            id: RULE_SEMVER_MAJOR_REQUIRED.to_string(),
            short_description: Some(SarifMessage::text("Major version bump required")),
            full_description: Some(SarifMessage::text(
                "Breaking changes require a major version bump according to semantic versioning.",
            )),
            help: Some(SarifMessage::text(
                "Increment the major version number (e.g., 1.0.0 -> 2.0.0).",
            )),
            help_uri: Some("https://semver.org/#spec-item-8".to_string()),
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Error,
            }),
        },
        SarifRule {
            id: RULE_SEMVER_MINOR_REQUIRED.to_string(),
            short_description: Some(SarifMessage::text("Minor version bump required")),
            full_description: Some(SarifMessage::text(
                "New functionality requires a minor version bump according to semantic versioning.",
            )),
            help: Some(SarifMessage::text(
                "Increment the minor version number (e.g., 1.0.0 -> 1.1.0).",
            )),
            help_uri: Some("https://semver.org/#spec-item-7".to_string()),
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Warning,
            }),
        },
        SarifRule {
            id: RULE_SEMVER_PATCH_REQUIRED.to_string(),
            short_description: Some(SarifMessage::text("Patch version bump required")),
            full_description: Some(SarifMessage::text(
                "Bug fixes require a patch version bump according to semantic versioning.",
            )),
            help: Some(SarifMessage::text(
                "Increment the patch version number (e.g., 1.0.0 -> 1.0.1).",
            )),
            help_uri: Some("https://semver.org/#spec-item-6".to_string()),
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Note,
            }),
        },
        SarifRule {
            id: RULE_TOOL_ERROR.to_string(),
            short_description: Some(SarifMessage::text("Tool error during semver checks")),
            full_description: Some(SarifMessage::text(
                "Semverguard encountered a tool or execution error while running checks.",
            )),
            help: Some(SarifMessage::text(
                "Review engine logs and configuration, then retry the check.",
            )),
            help_uri: None,
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Error,
            }),
        },
        SarifRule {
            id: RULE_BASELINE_ERROR.to_string(),
            short_description: Some(SarifMessage::text("Baseline error detected")),
            full_description: Some(SarifMessage::text(
                "Semverguard could not resolve or access the requested baseline.",
            )),
            help: Some(SarifMessage::text(
                "Verify the baseline reference and ensure required history is available.",
            )),
            help_uri: None,
            default_configuration: Some(SarifRuleConfiguration {
                level: SarifLevel::Warning,
            }),
        },
    ]
}

/// Convert a `RunReport` to SARIF format.
pub fn report_to_sarif(report: &RunReport) -> SarifLog {
    let results = report
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Failed)
        .map(package_to_sarif_result)
        .collect();

    let invocation = SarifInvocation {
        execution_successful: report.summary.overall_success(),
        start_time_utc: Some(report.started_at.clone()),
        end_time_utc: Some(report.finished_at.clone()),
        working_directory: Some(SarifArtifactLocation {
            uri: path_to_uri(&report.workspace_root),
            uri_base_id: None,
        }),
    };

    SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifToolComponent {
                    name: "semverguard".to_string(),
                    semantic_version: Some(report.semverguard_version.clone()),
                    full_name: Some(format!("semverguard {}", report.semverguard_version)),
                    information_uri: option_env!("CARGO_PKG_REPOSITORY").map(|s| s.to_string()),
                    rules: generate_rules(),
                },
            },
            results,
            invocations: vec![invocation],
        }],
    }
}

/// Convert a package report to a SARIF result.
///
/// HONESTY POLICY:
/// - Location always points to `Cargo.toml` (manifest path), not invented source locations
/// - No region data (line/column) is emitted since we cannot determine exact violation locations
/// - Raw log references are included in properties for debugging
fn package_to_sarif_result(pkg: &PackageReport) -> SarifResult {
    let (rule_id, level) = match pkg.failure_kind.unwrap_or(FailureKind::Unknown) {
        FailureKind::BaselineError => (RULE_BASELINE_ERROR, SarifLevel::Warning),
        FailureKind::ToolError => (RULE_TOOL_ERROR, SarifLevel::Error),
        FailureKind::SemverViolation | FailureKind::Unknown => match pkg.inferred_required_bump {
            Some(RequiredBump::Major) => (RULE_SEMVER_MAJOR_REQUIRED, SarifLevel::Error),
            Some(RequiredBump::Minor) => (RULE_SEMVER_MINOR_REQUIRED, SarifLevel::Warning),
            Some(RequiredBump::Patch) => (RULE_SEMVER_PATCH_REQUIRED, SarifLevel::Note),
            Some(RequiredBump::Unknown) | None => (RULE_SEMVER_BREAKING, SarifLevel::Error),
        },
    };

    let failure_kind = pkg.failure_kind.unwrap_or(FailureKind::Unknown);

    let message_text = match failure_kind {
        FailureKind::BaselineError => format!(
            "Baseline error while checking `{}` (v{})",
            pkg.name, pkg.version
        ),
        FailureKind::ToolError => format!(
            "Tool error while checking `{}` (v{})",
            pkg.name, pkg.version
        ),
        FailureKind::SemverViolation | FailureKind::Unknown => match pkg.inferred_required_bump {
            Some(RequiredBump::Major) => {
                format!(
                    "Package `{}` (v{}) has breaking changes requiring a major version bump",
                    pkg.name, pkg.version
                )
            }
            Some(RequiredBump::Minor) => {
                format!(
                    "Package `{}` (v{}) has new features requiring a minor version bump",
                    pkg.name, pkg.version
                )
            }
            Some(RequiredBump::Patch) => {
                format!(
                    "Package `{}` (v{}) has changes requiring a patch version bump",
                    pkg.name, pkg.version
                )
            }
            Some(RequiredBump::Unknown) | None => {
                format!(
                    "Package `{}` (v{}) failed semantic versioning check",
                    pkg.name, pkg.version
                )
            }
        },
    };

    // Extract stderr snippet for message enrichment (first 200 chars for context)
    let stderr_snippet = pkg
        .engine
        .as_ref()
        .map(|e| e.stderr.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.len() > 200 {
                format!("{}...", &s[..200])
            } else {
                s.to_string()
            }
        });

    // Build the message with stderr context when available
    let message = if let Some(snippet) = &stderr_snippet {
        SarifMessage::text(format!("{}\n\nEngine output: {}", message_text, snippet))
    } else {
        SarifMessage::text(message_text)
    };

    SarifResult {
        rule_id: rule_id.to_string(),
        level,
        message,
        // HONESTY: Location points to Cargo.toml (manifest path), not invented source locations
        locations: vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: path_to_uri(&pkg.manifest_path),
                    uri_base_id: None,
                },
                // HONESTY: Never invent region data - we don't know exact line/column
                region: None,
            },
        }],
        properties: Some(SarifResultProperties {
            package_name: Some(pkg.name.clone()),
            package_version: Some(pkg.version.clone()),
            required_bump: pkg.inferred_required_bump.map(|b| format!("{:?}", b)),
            duration_ms: Some(pkg.duration_ms),
            // Include raw log references for debugging
            raw_stderr_log: pkg.engine.as_ref().map(|_| {
                format!(
                    "Run with --format receipt to generate raw logs in artifacts/semverguard/raw/"
                )
            }),
            raw_stdout_log: None,
            failure_kind: Some(failure_kind_str(failure_kind).to_string()),
        }),
    }
}

/// Convert failure kind to string representation.
fn failure_kind_str(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::SemverViolation => "semver-violation",
        FailureKind::ToolError => "tool-error",
        FailureKind::BaselineError => "baseline-error",
        FailureKind::Unknown => "unknown",
    }
}

/// Convert a receipt to SARIF format.
pub fn receipt_to_sarif(receipt: &SensorReportV1) -> SarifLog {
    let results = receipt
        .findings
        .iter()
        .map(finding_to_sarif_result)
        .collect();

    let invocation = SarifInvocation {
        execution_successful: matches!(
            receipt.verdict.status,
            semverguard_types::VerdictStatus::Pass
        ),
        start_time_utc: Some(receipt.run.started_at.clone()),
        end_time_utc: Some(receipt.run.finished_at.clone()),
        working_directory: Some(SarifArtifactLocation {
            uri: normalize_uri_str(&receipt.run.workspace_root.to_string_lossy()),
            uri_base_id: None,
        }),
    };

    SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifToolComponent {
                    name: receipt.tool.name.clone(),
                    semantic_version: Some(receipt.tool.version.clone()),
                    full_name: Some(format!("{} {}", receipt.tool.name, receipt.tool.version)),
                    information_uri: receipt.tool.repository_url.clone(),
                    rules: generate_rules(),
                },
            },
            results,
            invocations: vec![invocation],
        }],
    }
}

fn finding_to_sarif_result(finding: &Finding) -> SarifResult {
    let (rule_id, level) = match finding.check_id.as_str() {
        "baseline" => (RULE_BASELINE_ERROR, SarifLevel::Warning),
        "tool" => (RULE_TOOL_ERROR, SarifLevel::Error),
        "semver" => {
            if let Some(bump) = finding
                .data
                .as_ref()
                .and_then(|v| v.get("required_bump"))
                .and_then(|v| v.as_str())
            {
                match bump {
                    "major" => (RULE_SEMVER_MAJOR_REQUIRED, SarifLevel::Error),
                    "minor" => (RULE_SEMVER_MINOR_REQUIRED, SarifLevel::Warning),
                    "patch" => (RULE_SEMVER_PATCH_REQUIRED, SarifLevel::Note),
                    _ => (RULE_SEMVER_BREAKING, SarifLevel::Error),
                }
            } else {
                (RULE_SEMVER_BREAKING, SarifLevel::Error)
            }
        }
        _ => (RULE_SEMVER_BREAKING, SarifLevel::Error),
    };

    // Extract raw log reference for message enrichment
    let raw_log_ref = finding
        .location
        .as_ref()
        .and_then(|loc| loc.raw_log.as_deref());

    // HONESTY POLICY: Location should point to the manifest path (Cargo.toml),
    // not invented source locations. We prefer the manifest path as the primary
    // location, and include raw log reference in the message for debugging.
    // If no manifest path is available, we may use the raw log path as a fallback.
    let locations = if let Some(loc) = &finding.location {
        // Prefer manifest path (Cargo.toml) over raw log for the location
        let uri = loc.path.as_ref().or(loc.raw_log.as_ref());
        match uri {
            Some(path) => vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: normalize_uri_str(path),
                        uri_base_id: None,
                    },
                    // HONESTY: Never invent region data - we don't know the exact line/column
                    region: None,
                },
            }],
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Extract additional properties for debugging
    let properties = finding.data.as_ref().map(|data| {
        SarifResultProperties {
            package_name: data.get("package").and_then(|v| v.as_str()).map(String::from),
            package_version: data.get("version").and_then(|v| v.as_str()).map(String::from),
            required_bump: data.get("required_bump").and_then(|v| v.as_str()).map(String::from),
            duration_ms: None,
            raw_stderr_log: raw_log_ref.map(String::from),
            raw_stdout_log: None,
            failure_kind: data.get("failure_kind").and_then(|v| v.as_str()).map(String::from),
        }
    });

    SarifResult {
        rule_id: rule_id.to_string(),
        level,
        // Include raw log reference in message for transparency
        message: SarifMessage::with_raw_log_ref(finding.message.clone(), raw_log_ref),
        locations,
        properties,
    }
}

/// Convert a path to a file URI.
fn path_to_uri(path: &Path) -> String {
    // Try to convert to absolute path and then to URI
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    // Convert to forward slashes for URI format
    let path_str = abs_path.to_string_lossy().replace('\\', "/");

    // Handle Windows drive letters (C: -> /C:)
    if path_str.len() >= 2 && path_str.chars().nth(1) == Some(':') {
        format!("file:///{}", path_str)
    } else if path_str.starts_with('/') {
        format!("file://{}", path_str)
    } else {
        format!("file:///{}", path_str)
    }
}

fn normalize_uri_str(s: &str) -> String {
    s.replace('\\', "/")
}

/// Serialize a SARIF log to JSON.
pub fn sarif_to_json(log: &SarifLog, pretty: bool) -> serde_json::Result<String> {
    if pretty {
        serde_json::to_string_pretty(log)
    } else {
        serde_json::to_string(log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{FailureKind, PackageStatus, Summary};
    use std::path::PathBuf;

    fn sample_report() -> RunReport {
        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "lib-a".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/crates/lib-a/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 500,
                    command: vec!["cargo".to_string()],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Major),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-b".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/crates/lib-b/Cargo.toml"),
                    status: PackageStatus::Passed,
                    skip_reason: None,
                    duration_ms: 300,
                    command: vec!["cargo".to_string()],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
            ],
            summary: Summary {
                total: 2,
                passed: 1,
                failed: 1,
                skipped: 0,
            },
        }
    }

    #[test]
    fn test_report_to_sarif_basic() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        assert_eq!(sarif.version, "2.1.0");
        assert_eq!(sarif.runs.len(), 1);
        assert_eq!(sarif.runs[0].tool.driver.name, "semverguard");
        assert_eq!(
            sarif.runs[0].tool.driver.semantic_version,
            Some("0.1.0".to_string())
        );
    }

    #[test]
    fn test_report_to_sarif_results() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        // Only failed packages should be in results
        assert_eq!(sarif.runs[0].results.len(), 1);
        let result = &sarif.runs[0].results[0];
        assert_eq!(result.rule_id, RULE_SEMVER_MAJOR_REQUIRED);
        assert!(matches!(result.level, SarifLevel::Error));
    }

    #[test]
    fn test_report_to_sarif_rules() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        let rules = &sarif.runs[0].tool.driver.rules;
        assert_eq!(rules.len(), 6);
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_BREAKING));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_MAJOR_REQUIRED));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_MINOR_REQUIRED));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_PATCH_REQUIRED));
        assert!(rules.iter().any(|r| r.id == RULE_TOOL_ERROR));
        assert!(rules.iter().any(|r| r.id == RULE_BASELINE_ERROR));
    }

    #[test]
    fn test_sarif_to_json() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);
        let json = sarif_to_json(&sarif, false).unwrap();

        assert!(json.contains("\"$schema\""));
        assert!(json.contains("\"version\":\"2.1.0\""));
        assert!(json.contains("\"semverguard\""));
    }

    #[test]
    fn test_sarif_to_json_pretty() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);
        let json = sarif_to_json(&sarif, true).unwrap();

        // Pretty JSON should have newlines
        assert!(json.contains('\n'));
    }

    #[test]
    fn test_path_to_uri_unix() {
        let uri = path_to_uri(Path::new("/workspace/Cargo.toml"));
        assert!(uri.starts_with("file://"));
        assert!(uri.contains("workspace"));
        assert!(uri.contains("Cargo.toml"));
    }

    #[test]
    fn test_sarif_invocation() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        assert_eq!(sarif.runs[0].invocations.len(), 1);
        let invocation = &sarif.runs[0].invocations[0];
        assert!(!invocation.execution_successful); // Has failures
        assert_eq!(
            invocation.start_time_utc,
            Some("2024-01-15T10:00:00Z".to_string())
        );
        assert_eq!(
            invocation.end_time_utc,
            Some("2024-01-15T10:01:00Z".to_string())
        );
    }

    #[test]
    fn test_empty_report() {
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
            summary: Summary {
                total: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        assert!(sarif.runs[0].results.is_empty());
        assert!(sarif.runs[0].invocations[0].execution_successful);
    }

    #[test]
    fn test_result_properties() {
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        let result = &sarif.runs[0].results[0];
        let props = result.properties.as_ref().unwrap();
        assert_eq!(props.package_name, Some("lib-a".to_string()));
        assert_eq!(props.package_version, Some("1.0.0".to_string()));
        assert_eq!(props.required_bump, Some("Major".to_string()));
        assert_eq!(props.duration_ms, Some(500));
        // Verify failure_kind is included in properties
        assert_eq!(
            props.failure_kind,
            Some("semver-violation".to_string())
        );
    }

    #[test]
    fn test_sarif_no_region_data() {
        // Verify honesty policy: no region data is ever emitted
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        for result in &sarif.runs[0].results {
            for loc in &result.locations {
                assert!(
                    loc.physical_location.region.is_none(),
                    "HONESTY POLICY VIOLATION: region data should never be emitted"
                );
            }
        }
    }

    #[test]
    fn test_sarif_location_is_manifest_path() {
        // Verify honesty policy: location always points to Cargo.toml
        let report = sample_report();
        let sarif = report_to_sarif(&report);

        for result in &sarif.runs[0].results {
            for loc in &result.locations {
                assert!(
                    loc.physical_location
                        .artifact_location
                        .uri
                        .contains("Cargo.toml"),
                    "HONESTY POLICY: location should point to Cargo.toml manifest"
                );
            }
        }
    }

    #[test]
    fn test_message_with_raw_log_ref() {
        let msg = SarifMessage::with_raw_log_ref("Test message", Some("path/to/log.txt"));
        assert!(msg.text.contains("Test message"));
        assert!(msg.text.contains("path/to/log.txt"));
        assert!(msg.text.contains("see raw log:"));

        let msg_no_log = SarifMessage::with_raw_log_ref("Test message", None);
        assert_eq!(msg_no_log.text, "Test message");
        assert!(!msg_no_log.text.contains("see raw log:"));
    }

    #[test]
    fn test_failure_kind_str() {
        assert_eq!(failure_kind_str(FailureKind::SemverViolation), "semver-violation");
        assert_eq!(failure_kind_str(FailureKind::ToolError), "tool-error");
        assert_eq!(failure_kind_str(FailureKind::BaselineError), "baseline-error");
        assert_eq!(failure_kind_str(FailureKind::Unknown), "unknown");
    }

    #[test]
    fn test_sarif_rules_have_correct_severity() {
        // Verify rule severity mapping aligns with error taxonomy
        let rules = generate_rules();

        let breaking = rules.iter().find(|r| r.id == RULE_SEMVER_BREAKING).unwrap();
        assert!(matches!(
            breaking.default_configuration.as_ref().unwrap().level,
            SarifLevel::Error
        ));

        let major = rules
            .iter()
            .find(|r| r.id == RULE_SEMVER_MAJOR_REQUIRED)
            .unwrap();
        assert!(matches!(
            major.default_configuration.as_ref().unwrap().level,
            SarifLevel::Error
        ));

        let minor = rules
            .iter()
            .find(|r| r.id == RULE_SEMVER_MINOR_REQUIRED)
            .unwrap();
        assert!(matches!(
            minor.default_configuration.as_ref().unwrap().level,
            SarifLevel::Warning
        ));

        let patch = rules
            .iter()
            .find(|r| r.id == RULE_SEMVER_PATCH_REQUIRED)
            .unwrap();
        assert!(matches!(
            patch.default_configuration.as_ref().unwrap().level,
            SarifLevel::Note
        ));

        let tool_error = rules.iter().find(|r| r.id == RULE_TOOL_ERROR).unwrap();
        assert!(matches!(
            tool_error.default_configuration.as_ref().unwrap().level,
            SarifLevel::Error
        ));

        let baseline_error = rules.iter().find(|r| r.id == RULE_BASELINE_ERROR).unwrap();
        assert!(matches!(
            baseline_error.default_configuration.as_ref().unwrap().level,
            SarifLevel::Warning
        ));
    }

    #[test]
    fn test_tool_error_sarif_result() {
        // Test that tool errors map to the correct rule
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "failing-lib".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/crates/failing-lib/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec!["cargo".to_string()],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::ToolError),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        let result = &sarif.runs[0].results[0];

        assert_eq!(result.rule_id, RULE_TOOL_ERROR);
        assert!(matches!(result.level, SarifLevel::Error));
        assert!(result.message.text.contains("Tool error"));
    }

    #[test]
    fn test_baseline_error_sarif_result() {
        // Test that baseline errors map to the correct rule with warning severity
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "new-lib".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/crates/new-lib/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 50,
                command: vec!["cargo".to_string()],
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

        let sarif = report_to_sarif(&report);
        let result = &sarif.runs[0].results[0];

        assert_eq!(result.rule_id, RULE_BASELINE_ERROR);
        // Baseline errors are warnings, not errors
        assert!(matches!(result.level, SarifLevel::Warning));
        assert!(result.message.text.contains("Baseline error"));
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_all_packages_skipped_no_results() {
        // When all packages are skipped, there should be no SARIF results
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "lib-a".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-a/Cargo.toml"),
                    status: PackageStatus::Skipped,
                    skip_reason: Some("excluded by glob".to_string()),
                    duration_ms: 0,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-b".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-b/Cargo.toml"),
                    status: PackageStatus::Skipped,
                    skip_reason: Some("unchanged".to_string()),
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
                failed: 0,
                skipped: 2,
            },
        };

        let sarif = report_to_sarif(&report);
        assert!(sarif.runs[0].results.is_empty());
        assert!(sarif.runs[0].invocations[0].execution_successful);
    }

    #[test]
    fn test_multiple_failures_with_same_rule() {
        // Multiple packages failing with same rule (Major bump required)
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "lib-a".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-a/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 100,
                    command: vec!["cargo".to_string()],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Major),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-b".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-b/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 150,
                    command: vec!["cargo".to_string()],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Major),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
            ],
            summary: Summary {
                total: 2,
                passed: 0,
                failed: 2,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results.len(), 2);

        // Both should have the same rule ID
        assert_eq!(sarif.runs[0].results[0].rule_id, RULE_SEMVER_MAJOR_REQUIRED);
        assert_eq!(sarif.runs[0].results[1].rule_id, RULE_SEMVER_MAJOR_REQUIRED);

        // But properties should differ
        let props0 = sarif.runs[0].results[0].properties.as_ref().unwrap();
        let props1 = sarif.runs[0].results[1].properties.as_ref().unwrap();
        assert_ne!(props0.package_name, props1.package_name);
    }

    #[test]
    fn test_mixed_failure_kinds_in_single_report() {
        // Report with different failure kinds
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "lib-major".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-major/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 100,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Major),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-minor".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-minor/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 100,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: Some(RequiredBump::Minor),
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-tool-error".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-tool-error/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 50,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: Some(FailureKind::ToolError),
                    baseline_error: None,
                },
                PackageReport {
                    name: "lib-baseline".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/lib-baseline/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 50,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: Some(FailureKind::BaselineError),
                    baseline_error: None,
                },
            ],
            summary: Summary {
                total: 4,
                passed: 0,
                failed: 4,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results.len(), 4);

        // Verify different rules are used
        let rule_ids: Vec<_> = sarif.runs[0]
            .results
            .iter()
            .map(|r| r.rule_id.as_str())
            .collect();
        assert!(rule_ids.contains(&RULE_SEMVER_MAJOR_REQUIRED));
        assert!(rule_ids.contains(&RULE_SEMVER_MINOR_REQUIRED));
        assert!(rule_ids.contains(&RULE_TOOL_ERROR));
        assert!(rule_ids.contains(&RULE_BASELINE_ERROR));
    }

    #[test]
    fn test_special_characters_in_package_name() {
        // Package names with special characters should be properly escaped in JSON
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "my_lib-core".to_string(), // underscore and hyphen
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/my_lib-core/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        let json = sarif_to_json(&sarif, false).unwrap();

        // Should be valid JSON with package name properly escaped
        assert!(json.contains("my_lib-core"));
        // Verify it's valid JSON by parsing
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_windows_path_handling() {
        // Test that Windows paths are converted to forward slashes
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("C:\\Users\\dev\\workspace"),
            packages: vec![PackageReport {
                name: "lib-a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("C:\\Users\\dev\\workspace\\lib-a\\Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        let json = sarif_to_json(&sarif, false).unwrap();

        // URIs should not contain backslashes
        assert!(
            !json.contains("\\\\"),
            "URIs should not contain escaped backslashes"
        );
        // Should use forward slashes in the URI
        assert!(json.contains("Cargo.toml"));
    }

    #[test]
    fn test_patch_bump_level() {
        // Test that patch bump gets Note severity
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "lib-patch".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-patch/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: Some(RequiredBump::Patch),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        let result = &sarif.runs[0].results[0];

        assert_eq!(result.rule_id, RULE_SEMVER_PATCH_REQUIRED);
        assert!(matches!(result.level, SarifLevel::Note));
        assert!(result.message.text.contains("patch version bump"));
    }

    #[test]
    fn test_unknown_bump_level() {
        // Test that unknown bump gets breaking-change rule
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "lib-unknown".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-unknown/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: Some(RequiredBump::Unknown),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let sarif = report_to_sarif(&report);
        let result = &sarif.runs[0].results[0];

        assert_eq!(result.rule_id, RULE_SEMVER_BREAKING);
        assert!(matches!(result.level, SarifLevel::Error));
    }

    #[test]
    fn test_normalize_uri_str() {
        assert_eq!(normalize_uri_str("foo\\bar\\baz"), "foo/bar/baz");
        assert_eq!(normalize_uri_str("foo/bar/baz"), "foo/bar/baz");
        assert_eq!(normalize_uri_str("C:\\Users\\dev"), "C:/Users/dev");
    }
}
