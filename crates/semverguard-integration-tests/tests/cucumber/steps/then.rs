//! Then step definitions for assertions.

use crate::world::TestWorld;
use cucumber::then;
use semverguard_types::{FailureKind, PackageStatus, VerdictStatus};

// =============================================================================
// Package Count Assertions
// =============================================================================

#[then(expr = "{int} packages are checked")]
fn packages_are_checked(world: &mut TestWorld, count: usize) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let checked_count = artifacts
                .packages
                .iter()
                .filter(|p| p.status != PackageStatus::Skipped)
                .count();
            assert_eq!(
                checked_count, count,
                "Expected {} packages to be checked, but {} were checked",
                count, checked_count
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "{int} packages are in results")]
fn packages_in_results(world: &mut TestWorld, count: usize) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            assert_eq!(
                artifacts.packages.len(),
                count,
                "Expected {} packages in results, got {}",
                count,
                artifacts.packages.len()
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

// =============================================================================
// Package Status Assertions
// =============================================================================

#[then(expr = "package {string} is checked")]
fn package_is_checked(world: &mut TestWorld, name: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let pkg = artifacts
                .packages
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in results", name));
            assert_ne!(
                pkg.status,
                PackageStatus::Skipped,
                "Package '{}' was skipped but expected to be checked",
                name
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "package {string} is skipped")]
fn package_is_skipped(world: &mut TestWorld, name: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let pkg = artifacts
                .packages
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in results", name));
            assert_eq!(
                pkg.status,
                PackageStatus::Skipped,
                "Package '{}' was not skipped (status: {:?})",
                name,
                pkg.status
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "package {string} is skipped with reason {string}")]
fn package_is_skipped_with_reason(world: &mut TestWorld, name: String, reason: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let pkg = artifacts
                .packages
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in results", name));
            assert_eq!(
                pkg.status,
                PackageStatus::Skipped,
                "Package '{}' was not skipped",
                name
            );
            let skip_reason = pkg
                .skip_reason
                .as_ref()
                .expect("Skipped package should have a reason");
            assert!(
                skip_reason.contains(&reason),
                "Skip reason '{}' does not contain '{}'",
                skip_reason,
                reason
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "package {string} passed")]
fn package_passed(world: &mut TestWorld, name: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let pkg = artifacts
                .packages
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in results", name));
            assert_eq!(
                pkg.status,
                PackageStatus::Passed,
                "Package '{}' did not pass (status: {:?})",
                name,
                pkg.status
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "package {string} failed")]
fn package_failed(world: &mut TestWorld, name: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            let pkg = artifacts
                .packages
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in results", name));
            assert_eq!(
                pkg.status,
                PackageStatus::Failed,
                "Package '{}' did not fail (status: {:?})",
                name,
                pkg.status
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

// =============================================================================
// Summary Assertions
// =============================================================================

#[then(expr = "summary shows {int} passed, {int} failed, {int} skipped")]
fn summary_shows(world: &mut TestWorld, passed: usize, failed: usize, skipped: usize) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            assert_eq!(
                artifacts.summary.passed, passed,
                "Expected {} passed, got {}",
                passed, artifacts.summary.passed
            );
            assert_eq!(
                artifacts.summary.failed, failed,
                "Expected {} failed, got {}",
                failed, artifacts.summary.failed
            );
            assert_eq!(
                artifacts.summary.skipped, skipped,
                "Expected {} skipped, got {}",
                skipped, artifacts.summary.skipped
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

#[then(expr = "summary total is {int}")]
fn summary_total(world: &mut TestWorld, total: usize) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(artifacts) => {
            assert_eq!(
                artifacts.summary.total, total,
                "Expected total {}, got {}",
                total, artifacts.summary.total
            );
        }
        Err(e) => panic!("Expected successful run, got error: {}", e),
    }
}

// =============================================================================
// Error Assertions
// =============================================================================

#[then(expr = "error contains {string}")]
fn error_contains(world: &mut TestWorld, message: String) {
    let result = world.run_result.as_ref().expect("Run result not set");
    match result {
        Ok(_) => panic!("Expected error, but run succeeded"),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(&message),
                "Error '{}' does not contain '{}'",
                error_msg,
                message
            );
        }
    }
}

#[then("run succeeds")]
fn run_succeeds(world: &mut TestWorld) {
    let result = world.run_result.as_ref().expect("Run result not set");
    assert!(
        result.is_ok(),
        "Expected run to succeed, got error: {:?}",
        result
    );
}

#[then("run fails")]
fn run_fails(world: &mut TestWorld) {
    let result = world.run_result.as_ref().expect("Run result not set");
    assert!(result.is_err(), "Expected run to fail, but it succeeded");
}

// =============================================================================
// List Command Assertions
// =============================================================================

#[then(expr = "would_check contains {string}")]
fn would_check_contains(world: &mut TestWorld, name: String) {
    let result = world.list_result.as_ref().expect("List result not set");
    match result {
        Ok(list) => {
            let found = list.would_check.iter().any(|p| p.name == name);
            assert!(
                found,
                "Package '{}' not found in would_check list. Available: {:?}",
                name,
                list.would_check.iter().map(|p| &p.name).collect::<Vec<_>>()
            );
        }
        Err(e) => panic!("Expected successful list, got error: {}", e),
    }
}

#[then(expr = "would_skip contains {string}")]
fn would_skip_contains(world: &mut TestWorld, name: String) {
    let result = world.list_result.as_ref().expect("List result not set");
    match result {
        Ok(list) => {
            let found = list.would_skip.iter().any(|p| p.name == name);
            assert!(
                found,
                "Package '{}' not found in would_skip list. Available: {:?}",
                name,
                list.would_skip.iter().map(|p| &p.name).collect::<Vec<_>>()
            );
        }
        Err(e) => panic!("Expected successful list, got error: {}", e),
    }
}

#[then(expr = "would_skip contains {string} with reason {string}")]
fn would_skip_contains_with_reason(world: &mut TestWorld, name: String, reason: String) {
    let result = world.list_result.as_ref().expect("List result not set");
    match result {
        Ok(list) => {
            let pkg = list
                .would_skip
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("Package '{}' not found in would_skip list", name));
            assert!(
                pkg.reason.contains(&reason),
                "Skip reason '{}' does not contain '{}'",
                pkg.reason,
                reason
            );
        }
        Err(e) => panic!("Expected successful list, got error: {}", e),
    }
}

#[then(expr = "would_check has {int} packages")]
fn would_check_count(world: &mut TestWorld, count: usize) {
    let result = world.list_result.as_ref().expect("List result not set");
    match result {
        Ok(list) => {
            assert_eq!(
                list.would_check.len(),
                count,
                "Expected {} packages in would_check, got {}",
                count,
                list.would_check.len()
            );
        }
        Err(e) => panic!("Expected successful list, got error: {}", e),
    }
}

#[then(expr = "would_skip has {int} packages")]
fn would_skip_count(world: &mut TestWorld, count: usize) {
    let result = world.list_result.as_ref().expect("List result not set");
    match result {
        Ok(list) => {
            assert_eq!(
                list.would_skip.len(),
                count,
                "Expected {} packages in would_skip, got {}",
                count,
                list.would_skip.len()
            );
        }
        Err(e) => panic!("Expected successful list, got error: {}", e),
    }
}

// =============================================================================
// Pipeline Assertions
// =============================================================================

#[then(expr = "pipeline exit code is {int}")]
fn pipeline_exit_code_is(world: &mut TestWorld, code: i32) {
    let result = world
        .pipeline_result
        .as_ref()
        .expect("Pipeline result not set");
    match result {
        Ok(pipeline) => {
            assert_eq!(
                pipeline.exit_code, code,
                "Expected pipeline exit code {}, got {}",
                code, pipeline.exit_code
            );
        }
        Err(e) => panic!("Expected successful pipeline run, got error: {}", e),
    }
}

#[then(expr = "pipeline report summary shows {int} passed, {int} failed, {int} skipped")]
fn pipeline_report_summary(world: &mut TestWorld, passed: usize, failed: usize, skipped: usize) {
    let result = world
        .pipeline_result
        .as_ref()
        .expect("Pipeline result not set");
    match result {
        Ok(pipeline) => {
            let report = pipeline
                .report
                .as_ref()
                .expect("Expected pipeline report to be present");
            assert_eq!(
                report.summary.passed, passed,
                "Expected {} passed, got {}",
                passed, report.summary.passed
            );
            assert_eq!(
                report.summary.failed, failed,
                "Expected {} failed, got {}",
                failed, report.summary.failed
            );
            assert_eq!(
                report.summary.skipped, skipped,
                "Expected {} skipped, got {}",
                skipped, report.summary.skipped
            );
        }
        Err(e) => panic!("Expected successful pipeline run, got error: {}", e),
    }
}

#[then(expr = "package {string} in pipeline report passed")]
fn package_in_pipeline_report_passed(world: &mut TestWorld, name: String) {
    assert_pipeline_package_status(world, &name, PackageStatus::Passed);
}

#[then(expr = "package {string} in pipeline report failed")]
fn package_in_pipeline_report_failed(world: &mut TestWorld, name: String) {
    assert_pipeline_package_status(world, &name, PackageStatus::Failed);
}

#[then(expr = "package {string} in pipeline report is skipped with reason {string}")]
fn package_in_pipeline_report_skipped_with_reason(
    world: &mut TestWorld,
    name: String,
    reason: String,
) {
    let pkg = find_pipeline_package(world, &name);
    assert_eq!(
        pkg.status,
        PackageStatus::Skipped,
        "Package '{}' was not skipped (status: {:?})",
        name,
        pkg.status
    );
    let skip_reason = pkg
        .skip_reason
        .as_ref()
        .expect("Skipped package should have reason");
    assert!(
        skip_reason.contains(&reason),
        "Skip reason '{}' does not contain '{}'",
        skip_reason,
        reason
    );
}

#[then(expr = "package {string} in pipeline report has failure kind {string}")]
fn package_in_pipeline_report_has_failure_kind(world: &mut TestWorld, name: String, kind: String) {
    let expected = parse_failure_kind(&kind);
    let pkg = find_pipeline_package(world, &name);
    assert_eq!(
        pkg.failure_kind,
        Some(expected),
        "Package '{}' failure kind mismatch",
        name
    );
}

#[then(expr = "package {string} in pipeline report has no failure kind")]
fn package_in_pipeline_report_has_no_failure_kind(world: &mut TestWorld, name: String) {
    let pkg = find_pipeline_package(world, &name);
    assert!(
        pkg.failure_kind.is_none(),
        "Expected package '{}' to have no failure kind, got {:?}",
        name,
        pkg.failure_kind
    );
}

#[then(expr = "pipeline receipt verdict is {string}")]
fn pipeline_receipt_verdict_is(world: &mut TestWorld, verdict: String) {
    let expected = match verdict.as_str() {
        "pass" => VerdictStatus::Pass,
        "warn" => VerdictStatus::Warn,
        "fail" => VerdictStatus::Fail,
        "skip" => VerdictStatus::Skip,
        _ => panic!("Unknown verdict status '{}'", verdict),
    };

    let result = world
        .pipeline_result
        .as_ref()
        .expect("Pipeline result not set");
    match result {
        Ok(pipeline) => {
            assert_eq!(
                pipeline.receipt.verdict.status, expected,
                "Expected verdict {:?}, got {:?}",
                expected, pipeline.receipt.verdict.status
            );
        }
        Err(e) => panic!("Expected successful pipeline run, got error: {}", e),
    }
}

#[then("pipeline receipt write succeeds")]
fn pipeline_receipt_write_succeeds(world: &mut TestWorld) {
    let result = world
        .receipt_write_result
        .as_ref()
        .expect("Receipt write result not set");
    match result {
        Ok(()) => {}
        Err(e) => panic!("Expected receipt write success, got error: {}", e),
    }
}

#[then(expr = "receipt artifact {string} exists")]
fn receipt_artifact_exists(world: &mut TestWorld, artifact: String) {
    let path = world.artifacts_dir().join(&artifact);
    assert!(
        path.exists(),
        "Expected receipt artifact '{}' at '{}'",
        artifact,
        path.display()
    );
}

fn find_pipeline_package<'a>(
    world: &'a mut TestWorld,
    name: &str,
) -> &'a semverguard_types::PackageReport {
    let result = world
        .pipeline_result
        .as_ref()
        .expect("Pipeline result not set");
    let pipeline = result
        .as_ref()
        .unwrap_or_else(|e| panic!("Expected successful pipeline run, got error: {}", e));
    let report = pipeline
        .report
        .as_ref()
        .expect("Expected pipeline report to be present");
    report
        .packages
        .iter()
        .find(|pkg| pkg.name == name)
        .unwrap_or_else(|| panic!("Package '{}' not found in pipeline report", name))
}

fn assert_pipeline_package_status(world: &mut TestWorld, name: &str, expected: PackageStatus) {
    let pkg = find_pipeline_package(world, name);
    assert_eq!(
        pkg.status, expected,
        "Package '{}' status mismatch (expected {:?}, got {:?})",
        name, expected, pkg.status
    );
}

fn parse_failure_kind(kind: &str) -> FailureKind {
    match kind {
        "semver-violation" => FailureKind::SemverViolation,
        "tool-error" => FailureKind::ToolError,
        "baseline-error" => FailureKind::BaselineError,
        "unknown" => FailureKind::Unknown,
        _ => panic!("Unknown failure kind '{}'", kind),
    }
}
