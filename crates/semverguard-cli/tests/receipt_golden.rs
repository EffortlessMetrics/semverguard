use semverguard_cli::{
    comment,
    receipt::{build_artifact_index, build_receipt},
    sarif,
};
use semverguard_types::{
    BaselineConfig, FailureKind, PackageReport, PackageStatus, RequiredBump, RunReport,
    SemverCheckOutput, Summary,
};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/receipt")
}

fn sample_report() -> RunReport {
    RunReport {
        semverguard_version: "0.1.0".to_string(),
        started_at: "2024-01-15T10:00:00Z".to_string(),
        finished_at: "2024-01-15T10:01:30Z".to_string(),
        workspace_root: PathBuf::from("workspace"),
        packages: vec![
            PackageReport {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("manifest-alpha.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 1200,
                command: vec![
                    "cargo".to_string(),
                    "semver-checks".to_string(),
                    "check-release".to_string(),
                    "-p".to_string(),
                    "alpha".to_string(),
                ],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: "Checking alpha v1.0.0".to_string(),
                    stderr: "breaking change detected".to_string(),
                    required_bump: Some(RequiredBump::Major),
                }),
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
            },
            PackageReport {
                name: "beta".to_string(),
                version: "0.2.0".to_string(),
                manifest_path: PathBuf::from("manifest-beta.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 800,
                command: vec![
                    "cargo".to_string(),
                    "semver-checks".to_string(),
                    "check-release".to_string(),
                    "-p".to_string(),
                    "beta".to_string(),
                ],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(2),
                    success: false,
                    stdout: String::new(),
                    stderr: "baseline not found".to_string(),
                    required_bump: None,
                }),
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::BaselineError),
            },
            PackageReport {
                name: "gamma".to_string(),
                version: "0.3.0".to_string(),
                manifest_path: PathBuf::from("manifest-gamma.toml"),
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
            total: 3,
            passed: 0,
            failed: 2,
            skipped: 1,
        },
    }
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name)).expect("fixture should exist")
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end_matches('\n').to_string()
}

#[test]
fn test_receipt_golden_files_and_schema() {
    let report = sample_report();
    let workspace_root = Path::new("workspace");
    let artifacts_dir = Path::new("artifacts/semverguard");

    let artifacts = build_artifact_index(workspace_root, artifacts_dir, Some(&report), true);
    let receipt = build_receipt(
        Some(&report),
        &[],
        &artifacts,
        &BaselineConfig::default(),
        workspace_root,
    );

    let receipt_json = serde_json::to_string_pretty(&receipt).expect("receipt should serialize");
    assert_eq!(
        normalize_newlines(&receipt_json),
        normalize_newlines(&read_fixture("receipt.json"))
    );

    let comment_md = comment::render_comment(&receipt);
    assert_eq!(
        normalize_newlines(&comment_md),
        normalize_newlines(&read_fixture("comment.md"))
    );

    let sarif_log = sarif::receipt_to_sarif(&receipt);
    let sarif_json = sarif::sarif_to_json(&sarif_log, true).expect("sarif should serialize");
    assert_eq!(
        normalize_newlines(&sarif_json),
        normalize_newlines(&read_fixture("sarif.json"))
    );

    let schema_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/sensor.report.v1.json");
    let schema_str = fs::read_to_string(schema_path).expect("schema should exist");
    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_str).expect("schema should be valid JSON");
    let compiled = jsonschema::JSONSchema::compile(&schema_json).expect("schema should compile");

    let value = serde_json::to_value(&receipt).expect("receipt should be JSON value");
    let result = compiled.validate(&value);
    assert!(result.is_ok(), "receipt should validate against schema");
}
