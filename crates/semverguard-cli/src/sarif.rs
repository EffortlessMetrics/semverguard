//! SARIF 2.1.0 report generation for semverguard.
//!
//! This module implements the Static Analysis Results Interchange Format (SARIF)
//! output format, which is used by security and code quality tools like GitHub
//! Code Scanning, VS Code SARIF Viewer, and other analysis platforms.
//!
//! See: https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html

use semverguard_types::{PackageReport, PackageStatus, RequiredBump, RunReport};
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
    /// No level specified.
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
                    information_uri: Some("https://github.com/your-org/semverguard".to_string()),
                    rules: generate_rules(),
                },
            },
            results,
            invocations: vec![invocation],
        }],
    }
}

/// Convert a package report to a SARIF result.
fn package_to_sarif_result(pkg: &PackageReport) -> SarifResult {
    let (rule_id, level) = match pkg.inferred_required_bump {
        Some(RequiredBump::Major) => (RULE_SEMVER_MAJOR_REQUIRED, SarifLevel::Error),
        Some(RequiredBump::Minor) => (RULE_SEMVER_MINOR_REQUIRED, SarifLevel::Warning),
        Some(RequiredBump::Patch) => (RULE_SEMVER_PATCH_REQUIRED, SarifLevel::Note),
        Some(RequiredBump::Unknown) | None => (RULE_SEMVER_BREAKING, SarifLevel::Error),
    };

    let message_text = match pkg.inferred_required_bump {
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
    };

    SarifResult {
        rule_id: rule_id.to_string(),
        level,
        message: SarifMessage::text(message_text),
        locations: vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: path_to_uri(&pkg.manifest_path),
                    uri_base_id: None,
                },
                region: Some(SarifRegion {
                    start_line: Some(1),
                    start_column: Some(1),
                    end_line: None,
                    end_column: None,
                }),
            },
        }],
        properties: Some(SarifResultProperties {
            package_name: Some(pkg.name.clone()),
            package_version: Some(pkg.version.clone()),
            required_bump: pkg.inferred_required_bump.map(|b| format!("{:?}", b)),
            duration_ms: Some(pkg.duration_ms),
        }),
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
    use semverguard_types::{PackageStatus, Summary};
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
        assert_eq!(rules.len(), 4);
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_BREAKING));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_MAJOR_REQUIRED));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_MINOR_REQUIRED));
        assert!(rules.iter().any(|r| r.id == RULE_SEMVER_PATCH_REQUIRED));
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
    }
}
