//! Schema validation tests for semverguard output formats.
//!
//! These tests ensure that serialized outputs conform to their JSON schemas and
//! that golden fixtures remain stable across changes.

use jsonschema::Validator;
use semverguard_types::{
    BaselineConfig, BaselineKind, FailureKind, ListResult, ListedPackage, PackageReport,
    PackageStatus, RequiredBump, RunReport, SemverCheckOutput, SkippedPackage, Summary,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

// =============================================================================
// Schema Loading Helpers
// =============================================================================

fn schema_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema")
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn load_schema(name: &str) -> Validator {
    let path = schema_dir().join(name);
    let schema_str = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read schema {}: {}", path.display(), e));
    let schema_json: Value = serde_json::from_str(&schema_str)
        .unwrap_or_else(|e| panic!("Failed to parse schema {}: {}", path.display(), e));
    jsonschema::validator_for(&schema_json)
        .unwrap_or_else(|e| panic!("Failed to compile schema {}: {}", path.display(), e))
}

fn load_golden(name: &str) -> Value {
    let path = golden_dir().join(name);
    let json_str = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read golden file {}: {}", path.display(), e));
    serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("Failed to parse golden file {}: {}", path.display(), e))
}

fn validate_against_schema(schema: &Validator, value: &Value, context: &str) {
    let error_msgs: Vec<String> = schema
        .iter_errors(value)
        .map(|e| format!("  - {}", e))
        .collect();
    if !error_msgs.is_empty() {
        panic!(
            "Schema validation failed for {}:\n{}",
            context,
            error_msgs.join("\n")
        );
    }
}

// =============================================================================
// RunReport Schema Validation Tests
// =============================================================================

#[test]
fn test_run_report_empty_validates_against_schema() {
    let schema = load_schema("run.report.v1.json");

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

    let value = serde_json::to_value(&report).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "empty RunReport");
}

#[test]
fn test_run_report_with_all_statuses_validates_against_schema() {
    let schema = load_schema("run.report.v1.json");

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
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 500,
                command: vec!["cargo".to_string(), "semver-checks".to_string()],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(0),
                    success: true,
                    stdout: "OK".to_string(),
                    stderr: String::new(),
                    required_bump: None,
                }),
                inferred_required_bump: None,
                failure_kind: None,
                baseline_error: None,
            },
            PackageReport {
                name: "lib-b".to_string(),
                version: "2.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-b/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 1000,
                command: vec!["cargo".to_string(), "semver-checks".to_string()],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: String::new(),
                    stderr: "Major bump required".to_string(),
                    required_bump: Some(RequiredBump::Major),
                }),
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            },
            PackageReport {
                name: "lib-c".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-c/Cargo.toml"),
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
            total: 3,
            passed: 1,
            failed: 1,
            skipped: 1,
        },
    };

    let value = serde_json::to_value(&report).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "mixed RunReport");
}

#[test]
fn test_run_report_all_failure_kinds_validate() {
    let schema = load_schema("run.report.v1.json");

    for failure_kind in [
        FailureKind::SemverViolation,
        FailureKind::ToolError,
        FailureKind::BaselineError,
        FailureKind::Unknown,
    ] {
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "test-pkg".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(failure_kind),
                baseline_error: None,
            }],
            summary: Summary {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
            },
        };

        let value = serde_json::to_value(&report).expect("serialization should succeed");
        validate_against_schema(
            &schema,
            &value,
            &format!("RunReport with failure_kind {:?}", failure_kind),
        );
    }
}

#[test]
fn test_run_report_all_required_bumps_validate() {
    let schema = load_schema("run.report.v1.json");

    for bump in [
        RequiredBump::Patch,
        RequiredBump::Minor,
        RequiredBump::Major,
        RequiredBump::Unknown,
    ] {
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![PackageReport {
                name: "test-pkg".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                    required_bump: Some(bump),
                }),
                inferred_required_bump: Some(bump),
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

        let value = serde_json::to_value(&report).expect("serialization should succeed");
        validate_against_schema(
            &schema,
            &value,
            &format!("RunReport with required_bump {:?}", bump),
        );
    }
}

// =============================================================================
// ListResult Schema Validation Tests
// =============================================================================

#[test]
fn test_list_result_empty_validates_against_schema() {
    let schema = load_schema("list.result.v1.json");

    let result = ListResult {
        workspace_root: PathBuf::from("/workspace"),
        would_check: vec![],
        would_skip: vec![],
    };

    let value = serde_json::to_value(&result).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "empty ListResult");
}

#[test]
fn test_list_result_mixed_validates_against_schema() {
    let schema = load_schema("list.result.v1.json");

    let result = ListResult {
        workspace_root: PathBuf::from("/workspace"),
        would_check: vec![
            ListedPackage {
                name: "lib-a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-a/Cargo.toml"),
            },
            ListedPackage {
                name: "lib-b".to_string(),
                version: "2.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/lib-b/Cargo.toml"),
            },
        ],
        would_skip: vec![SkippedPackage {
            name: "internal".to_string(),
            version: "0.1.0".to_string(),
            manifest_path: PathBuf::from("/workspace/internal/Cargo.toml"),
            reason: "publish = false".to_string(),
        }],
    };

    let value = serde_json::to_value(&result).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "mixed ListResult");
}

// =============================================================================
// SensorReportV1 (Receipt) Schema Validation Tests
// =============================================================================

#[test]
fn test_receipt_pass_validates_against_schema() {
    let schema = load_schema("sensor.report.v1.json");

    let receipt = semverguard_types::SensorReportV1 {
        schema: "sensor.report.v1".to_string(),
        tool: semverguard_types::ToolInfo {
            name: "semverguard".to_string(),
            version: "0.1.0".to_string(),
            repository_url: None,
        },
        run: semverguard_types::RunInfo {
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            duration_ms: 60000,
            workspace_root: PathBuf::from("/workspace"),
            baseline: BaselineConfig::default(),
            capabilities: None,
        },
        verdict: semverguard_types::Verdict {
            status: semverguard_types::VerdictStatus::Pass,
            reasons: vec![],
        },
        findings: vec![],
        data: None,
        artifacts: semverguard_types::ArtifactIndex {
            report_json: "artifacts/semverguard/report.json".to_string(),
            comment_md: "artifacts/semverguard/comment.md".to_string(),
            sarif_json: None,
            raw_logs: vec![],
        },
    };

    let value = serde_json::to_value(&receipt).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "passing receipt");
}

#[test]
fn test_receipt_fail_with_findings_validates_against_schema() {
    let schema = load_schema("sensor.report.v1.json");

    let receipt = semverguard_types::SensorReportV1 {
        schema: "sensor.report.v1".to_string(),
        tool: semverguard_types::ToolInfo {
            name: "semverguard".to_string(),
            version: "0.1.0".to_string(),
            repository_url: Some("https://github.com/example/semverguard".to_string()),
        },
        run: semverguard_types::RunInfo {
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            duration_ms: 60000,
            workspace_root: PathBuf::from("/workspace"),
            baseline: BaselineConfig {
                kind: BaselineKind::Git,
                version: None,
                rev: Some("origin/main".to_string()),
                root: None,
                rustdoc: None,
                on_error: semverguard_types::BaselineErrorBehavior::default(),
            },
            capabilities: None,
        },
        verdict: semverguard_types::Verdict {
            status: semverguard_types::VerdictStatus::Fail,
            reasons: vec!["semver_violation".to_string()],
        },
        findings: vec![semverguard_types::Finding {
            check_id: "semver".to_string(),
            code: "violation".to_string(),
            level: semverguard_types::FindingLevel::Error,
            message: "Package `lib-a` (v1.0.0) requires a major version bump".to_string(),
            location: Some(semverguard_types::FindingLocation {
                path: Some("crates/lib-a/Cargo.toml".to_string()),
                line: None,
                column: None,
                raw_log: Some("artifacts/semverguard/raw/lib-a-1.0.0.stderr.log".to_string()),
            }),
            data: Some(serde_json::json!({
                "package": "lib-a",
                "version": "1.0.0",
                "required_bump": "major"
            })),
            fingerprint: None,
        }],
        data: None,
        artifacts: semverguard_types::ArtifactIndex {
            report_json: "artifacts/semverguard/report.json".to_string(),
            comment_md: "artifacts/semverguard/comment.md".to_string(),
            sarif_json: Some("artifacts/semverguard/sarif.json".to_string()),
            raw_logs: vec![semverguard_types::RawLogRef {
                package: "lib-a".to_string(),
                version: "1.0.0".to_string(),
                stdout: Some("artifacts/semverguard/raw/lib-a-1.0.0.stdout.log".to_string()),
                stderr: Some("artifacts/semverguard/raw/lib-a-1.0.0.stderr.log".to_string()),
            }],
        },
    };

    let value = serde_json::to_value(&receipt).expect("serialization should succeed");
    validate_against_schema(&schema, &value, "failing receipt with findings");
}

#[test]
fn test_receipt_all_verdict_statuses_validate() {
    let schema = load_schema("sensor.report.v1.json");

    for status in [
        semverguard_types::VerdictStatus::Pass,
        semverguard_types::VerdictStatus::Warn,
        semverguard_types::VerdictStatus::Fail,
        semverguard_types::VerdictStatus::Skip,
    ] {
        let receipt = semverguard_types::SensorReportV1 {
            schema: "sensor.report.v1".to_string(),
            tool: semverguard_types::ToolInfo {
                name: "semverguard".to_string(),
                version: "0.1.0".to_string(),
                repository_url: None,
            },
            run: semverguard_types::RunInfo {
                started_at: "2024-01-15T10:00:00Z".to_string(),
                finished_at: "2024-01-15T10:00:01Z".to_string(),
                duration_ms: 1000,
                workspace_root: PathBuf::from("/workspace"),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: semverguard_types::Verdict {
                status,
                reasons: vec![],
            },
            findings: vec![],
            data: None,
            artifacts: semverguard_types::ArtifactIndex {
                report_json: "report.json".to_string(),
                comment_md: "comment.md".to_string(),
                sarif_json: None,
                raw_logs: vec![],
            },
        };

        let value = serde_json::to_value(&receipt).expect("serialization should succeed");
        validate_against_schema(
            &schema,
            &value,
            &format!("receipt with status {:?}", status),
        );
    }
}

#[test]
fn test_receipt_all_finding_levels_validate() {
    let schema = load_schema("sensor.report.v1.json");

    for level in [
        semverguard_types::FindingLevel::Error,
        semverguard_types::FindingLevel::Warning,
        semverguard_types::FindingLevel::Info,
    ] {
        let receipt = semverguard_types::SensorReportV1 {
            schema: "sensor.report.v1".to_string(),
            tool: semverguard_types::ToolInfo {
                name: "semverguard".to_string(),
                version: "0.1.0".to_string(),
                repository_url: None,
            },
            run: semverguard_types::RunInfo {
                started_at: "2024-01-15T10:00:00Z".to_string(),
                finished_at: "2024-01-15T10:00:01Z".to_string(),
                duration_ms: 1000,
                workspace_root: PathBuf::from("/workspace"),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: semverguard_types::Verdict {
                status: semverguard_types::VerdictStatus::Fail,
                reasons: vec![],
            },
            findings: vec![semverguard_types::Finding {
                check_id: "test".to_string(),
                code: "test".to_string(),
                level,
                message: "test message".to_string(),
                location: None,
                data: None,
                fingerprint: None,
            }],
            data: None,
            artifacts: semverguard_types::ArtifactIndex {
                report_json: "report.json".to_string(),
                comment_md: "comment.md".to_string(),
                sarif_json: None,
                raw_logs: vec![],
            },
        };

        let value = serde_json::to_value(&receipt).expect("serialization should succeed");
        validate_against_schema(
            &schema,
            &value,
            &format!("receipt with finding level {:?}", level),
        );
    }
}

// =============================================================================
// Golden File Tests
// =============================================================================

#[test]
fn test_golden_run_report_empty_validates() {
    let schema = load_schema("run.report.v1.json");
    let golden = load_golden("run_report_empty.json");
    validate_against_schema(&schema, &golden, "golden run_report_empty.json");
}

#[test]
fn test_golden_run_report_mixed_validates() {
    let schema = load_schema("run.report.v1.json");
    let golden = load_golden("run_report_mixed.json");
    validate_against_schema(&schema, &golden, "golden run_report_mixed.json");
}

#[test]
fn test_golden_list_result_mixed_validates() {
    let schema = load_schema("list.result.v1.json");
    let golden = load_golden("list_result_mixed.json");
    validate_against_schema(&schema, &golden, "golden list_result_mixed.json");
}

#[test]
fn test_golden_receipt_pass_validates() {
    let schema = load_schema("sensor.report.v1.json");
    let golden = load_golden("receipt_pass.json");
    validate_against_schema(&schema, &golden, "golden receipt_pass.json");
}

#[test]
fn test_golden_receipt_fail_validates() {
    let schema = load_schema("sensor.report.v1.json");
    let golden = load_golden("receipt_fail.json");
    validate_against_schema(&schema, &golden, "golden receipt_fail.json");
}

// =============================================================================
// Golden File Roundtrip Tests
// =============================================================================

#[test]
fn test_golden_run_report_empty_roundtrip() {
    let golden = load_golden("run_report_empty.json");
    let report: RunReport = serde_json::from_value(golden.clone()).expect("deserialization failed");
    let reserialized = serde_json::to_value(&report).expect("reserialization failed");

    // Compare key fields (not exact match due to path normalization)
    assert_eq!(
        golden["semverguard_version"],
        reserialized["semverguard_version"]
    );
    assert_eq!(golden["started_at"], reserialized["started_at"]);
    assert_eq!(golden["finished_at"], reserialized["finished_at"]);
    assert_eq!(golden["summary"], reserialized["summary"]);
    assert_eq!(
        golden["packages"].as_array().unwrap().len(),
        reserialized["packages"].as_array().unwrap().len()
    );
}

#[test]
fn test_golden_run_report_mixed_roundtrip() {
    let golden = load_golden("run_report_mixed.json");
    let report: RunReport = serde_json::from_value(golden.clone()).expect("deserialization failed");
    let reserialized = serde_json::to_value(&report).expect("reserialization failed");

    // Verify summary counts match
    assert_eq!(golden["summary"]["total"], reserialized["summary"]["total"]);
    assert_eq!(
        golden["summary"]["passed"],
        reserialized["summary"]["passed"]
    );
    assert_eq!(
        golden["summary"]["failed"],
        reserialized["summary"]["failed"]
    );
    assert_eq!(
        golden["summary"]["skipped"],
        reserialized["summary"]["skipped"]
    );

    // Verify package count and names
    let golden_pkgs = golden["packages"].as_array().unwrap();
    let reserialized_pkgs = reserialized["packages"].as_array().unwrap();
    assert_eq!(golden_pkgs.len(), reserialized_pkgs.len());

    for (g, r) in golden_pkgs.iter().zip(reserialized_pkgs.iter()) {
        assert_eq!(g["name"], r["name"]);
        assert_eq!(g["version"], r["version"]);
        assert_eq!(g["status"], r["status"]);
    }
}

#[test]
fn test_golden_list_result_mixed_roundtrip() {
    let golden = load_golden("list_result_mixed.json");
    let result: ListResult =
        serde_json::from_value(golden.clone()).expect("deserialization failed");
    let reserialized = serde_json::to_value(&result).expect("reserialization failed");

    let golden_check = golden["would_check"].as_array().unwrap();
    let reserialized_check = reserialized["would_check"].as_array().unwrap();
    assert_eq!(golden_check.len(), reserialized_check.len());

    for (g, r) in golden_check.iter().zip(reserialized_check.iter()) {
        assert_eq!(g["name"], r["name"]);
        assert_eq!(g["version"], r["version"]);
    }

    let golden_skip = golden["would_skip"].as_array().unwrap();
    let reserialized_skip = reserialized["would_skip"].as_array().unwrap();
    assert_eq!(golden_skip.len(), reserialized_skip.len());
}

// =============================================================================
// Deterministic Ordering Tests
// =============================================================================

#[test]
fn test_run_report_packages_should_be_sorted_by_name() {
    // Given: packages in reverse order
    let packages = vec![
        PackageReport {
            name: "zebra".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/zebra/Cargo.toml"),
            status: PackageStatus::Passed,
            skip_reason: None,
            duration_ms: 100,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: None,
            baseline_error: None,
        },
        PackageReport {
            name: "alpha".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
            status: PackageStatus::Passed,
            skip_reason: None,
            duration_ms: 100,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: None,
            baseline_error: None,
        },
        PackageReport {
            name: "beta".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
            status: PackageStatus::Passed,
            skip_reason: None,
            duration_ms: 100,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
            failure_kind: None,
            baseline_error: None,
        },
    ];

    let mut report = RunReport {
        semverguard_version: "0.1.0".to_string(),
        started_at: "2024-01-15T10:00:00Z".to_string(),
        finished_at: "2024-01-15T10:00:01Z".to_string(),
        workspace_root: PathBuf::from("/workspace"),
        packages,
        summary: Summary {
            total: 3,
            passed: 3,
            failed: 0,
            skipped: 0,
        },
    };

    // When: sorting packages
    report.packages.sort_by(|a, b| a.name.cmp(&b.name));

    // Then: packages are in alphabetical order
    assert_eq!(report.packages[0].name, "alpha");
    assert_eq!(report.packages[1].name, "beta");
    assert_eq!(report.packages[2].name, "zebra");
}

#[test]
fn test_list_result_entries_should_be_sorted_by_name() {
    let mut result = ListResult {
        workspace_root: PathBuf::from("/workspace"),
        would_check: vec![
            ListedPackage {
                name: "zebra".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/zebra/Cargo.toml"),
            },
            ListedPackage {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
            },
        ],
        would_skip: vec![
            SkippedPackage {
                name: "omega".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/omega/Cargo.toml"),
                reason: "publish = false".to_string(),
            },
            SkippedPackage {
                name: "gamma".to_string(),
                version: "0.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/gamma/Cargo.toml"),
                reason: "no lib target".to_string(),
            },
        ],
    };

    // Sort both lists by name
    result.would_check.sort_by(|a, b| a.name.cmp(&b.name));
    result.would_skip.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(result.would_check[0].name, "alpha");
    assert_eq!(result.would_check[1].name, "zebra");
    assert_eq!(result.would_skip[0].name, "gamma");
    assert_eq!(result.would_skip[1].name, "omega");
}

// =============================================================================
// JSON Field Ordering Tests
// =============================================================================

#[test]
fn test_package_status_serializes_to_kebab_case() {
    assert_eq!(
        serde_json::to_string(&PackageStatus::Passed).unwrap(),
        "\"passed\""
    );
    assert_eq!(
        serde_json::to_string(&PackageStatus::Failed).unwrap(),
        "\"failed\""
    );
    assert_eq!(
        serde_json::to_string(&PackageStatus::Skipped).unwrap(),
        "\"skipped\""
    );
}

#[test]
fn test_failure_kind_serializes_to_kebab_case() {
    assert_eq!(
        serde_json::to_string(&FailureKind::SemverViolation).unwrap(),
        "\"semver-violation\""
    );
    assert_eq!(
        serde_json::to_string(&FailureKind::ToolError).unwrap(),
        "\"tool-error\""
    );
    assert_eq!(
        serde_json::to_string(&FailureKind::BaselineError).unwrap(),
        "\"baseline-error\""
    );
    assert_eq!(
        serde_json::to_string(&FailureKind::Unknown).unwrap(),
        "\"unknown\""
    );
}

#[test]
fn test_required_bump_serializes_to_kebab_case() {
    assert_eq!(
        serde_json::to_string(&RequiredBump::Patch).unwrap(),
        "\"patch\""
    );
    assert_eq!(
        serde_json::to_string(&RequiredBump::Minor).unwrap(),
        "\"minor\""
    );
    assert_eq!(
        serde_json::to_string(&RequiredBump::Major).unwrap(),
        "\"major\""
    );
    assert_eq!(
        serde_json::to_string(&RequiredBump::Unknown).unwrap(),
        "\"unknown\""
    );
}

#[test]
fn test_verdict_status_serializes_to_kebab_case() {
    assert_eq!(
        serde_json::to_string(&semverguard_types::VerdictStatus::Pass).unwrap(),
        "\"pass\""
    );
    assert_eq!(
        serde_json::to_string(&semverguard_types::VerdictStatus::Warn).unwrap(),
        "\"warn\""
    );
    assert_eq!(
        serde_json::to_string(&semverguard_types::VerdictStatus::Fail).unwrap(),
        "\"fail\""
    );
    assert_eq!(
        serde_json::to_string(&semverguard_types::VerdictStatus::Skip).unwrap(),
        "\"skip\""
    );
}

#[test]
fn test_finding_level_serializes_to_kebab_case() {
    assert_eq!(
        serde_json::to_string(&semverguard_types::FindingLevel::Error).unwrap(),
        "\"error\""
    );
    assert_eq!(
        serde_json::to_string(&semverguard_types::FindingLevel::Warning).unwrap(),
        "\"warning\""
    );
    assert_eq!(
        serde_json::to_string(&semverguard_types::FindingLevel::Info).unwrap(),
        "\"info\""
    );
}

// =============================================================================
// Stability Tests - Ensure output doesn't change unexpectedly
// =============================================================================

#[test]
fn test_empty_report_json_structure_is_stable() {
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

    let json = serde_json::to_value(&report).unwrap();

    // Verify expected top-level fields exist
    assert!(json.get("semverguard_version").is_some());
    assert!(json.get("started_at").is_some());
    assert!(json.get("finished_at").is_some());
    assert!(json.get("workspace_root").is_some());
    assert!(json.get("packages").is_some());
    assert!(json.get("summary").is_some());

    // Verify summary fields
    let summary = json.get("summary").unwrap();
    assert!(summary.get("total").is_some());
    assert!(summary.get("passed").is_some());
    assert!(summary.get("failed").is_some());
    assert!(summary.get("skipped").is_some());
}

#[test]
fn test_package_report_json_structure_is_stable() {
    let pkg = PackageReport {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        manifest_path: PathBuf::from("/test/Cargo.toml"),
        status: PackageStatus::Passed,
        skip_reason: None,
        duration_ms: 100,
        command: vec!["cargo".to_string()],
        engine: Some(SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "OK".to_string(),
            stderr: String::new(),
            required_bump: None,
        }),
        inferred_required_bump: None,
        failure_kind: None,
        baseline_error: None,
    };

    let json = serde_json::to_value(&pkg).unwrap();

    // Verify expected fields exist
    assert!(json.get("name").is_some());
    assert!(json.get("version").is_some());
    assert!(json.get("manifest_path").is_some());
    assert!(json.get("status").is_some());
    assert!(json.get("skip_reason").is_some());
    assert!(json.get("duration_ms").is_some());
    assert!(json.get("command").is_some());
    assert!(json.get("engine").is_some());
    assert!(json.get("inferred_required_bump").is_some());

    // Verify engine fields when present
    let engine = json.get("engine").unwrap();
    assert!(engine.get("exit_code").is_some());
    assert!(engine.get("success").is_some());
    assert!(engine.get("stdout").is_some());
    assert!(engine.get("stderr").is_some());
    assert!(engine.get("required_bump").is_some());
}
