//! Then step definitions for assertions.

use crate::world::TestWorld;
use cucumber::then;
use semverguard_types::PackageStatus;

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
