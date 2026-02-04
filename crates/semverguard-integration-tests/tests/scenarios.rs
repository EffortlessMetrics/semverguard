//! BDD-style integration tests for semverguard.
//!
//! These tests verify end-to-end scenarios by exercising the domain layer's
//! `SemverguardRunner` with mock adapters. Each scenario follows the Given-When-Then
//! pattern to clearly communicate intent and expected behavior.
//!
//! # Scenarios Covered
//!
//! 1. **Check entire workspace** - All packages in workspace mode are checked
//! 2. **Check only changed packages** - Only changed packages are checked in changed mode
//! 3. **Exclude packages by glob** - Packages matching exclude patterns are skipped
//! 4. **Skip non-publishable packages** - Packages with `publish = false` are skipped
//! 5. **JSON report generation** - JSON report contains expected structure and data
//!
//! # Test Fixtures
//!
//! Tests use mock providers from `semverguard-domain` to simulate workspaces,
//! git changes, and semver check results without requiring actual Cargo projects.

use semver::Version;
use semverguard_domain::{
    MockGitProvider, MockSemverEngine, MockWorkspaceProvider, SemverguardRunner,
};
use semverguard_cli::receipt::{
    build_artifact_index, build_receipt, exit_code_from_receipt, has_tool_error,
    write_receipt_bundle, ToolErrorFinding,
};
use semverguard_types::{
    BaselineConfig, BaselineKind, EngineConfig, FeaturesConfig, OutputConfig, PackageStatus,
    RunReport, ScopeConfig, ScopeMode, SemverCheckOutput, SemverguardConfig, WorkspaceMetadata,
    WorkspacePackage,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// =============================================================================
// Test Fixtures and Helpers
// =============================================================================

/// Creates a basic package fixture for testing.
fn make_package(
    name: &str,
    version: &str,
    root: &str,
    publishable: bool,
    has_lib: bool,
) -> WorkspacePackage {
    WorkspacePackage {
        name: name.to_string(),
        version: Version::parse(version).unwrap(),
        manifest_path: PathBuf::from(format!("{root}/Cargo.toml")),
        package_root: PathBuf::from(root),
        publishable,
        has_lib,
    }
}

/// Creates workspace metadata with the given packages.
fn make_workspace(workspace_root: &str, packages: Vec<WorkspacePackage>) -> WorkspaceMetadata {
    WorkspaceMetadata {
        workspace_root: PathBuf::from(workspace_root),
        packages,
    }
}

/// Creates a default config for workspace-mode checks.
fn default_workspace_config() -> SemverguardConfig {
    SemverguardConfig {
        baseline: BaselineConfig::default(),
        scope: ScopeConfig {
            mode: ScopeMode::Workspace,
            include: vec![],
            exclude: vec![],
            explicit_packages: vec![],
            skip_publish_false: false,
            skip_no_lib: false,
        },
        features: FeaturesConfig::default(),
        engine: EngineConfig::default(),
        output: OutputConfig::default(),
    }
}

/// Creates a config for changed-mode checks with git baseline.
fn changed_mode_config(baseline_rev: &str) -> SemverguardConfig {
    SemverguardConfig {
        baseline: BaselineConfig {
            kind: BaselineKind::Git,
            rev: Some(baseline_rev.to_string()),
            ..Default::default()
        },
        scope: ScopeConfig {
            mode: ScopeMode::Changed,
            include: vec![],
            exclude: vec![],
            explicit_packages: vec![],
            skip_publish_false: false,
            skip_no_lib: false,
        },
        features: FeaturesConfig::default(),
        engine: EngineConfig::default(),
        output: OutputConfig::default(),
    }
}

/// Creates a successful semver check output.
fn success_output() -> SemverCheckOutput {
    SemverCheckOutput {
        exit_code: Some(0),
        success: true,
        stdout: "No breaking changes detected".to_string(),
        stderr: String::new(),
        required_bump: None,
    }
}

/// Converts RunArtifacts to a RunReport for JSON serialization tests.
fn artifacts_to_report(artifacts: &semverguard_domain::RunArtifacts) -> RunReport {
    RunReport {
        semverguard_version: "0.1.0".to_string(),
        started_at: "2024-01-15T10:00:00Z".to_string(),
        finished_at: "2024-01-15T10:00:05Z".to_string(),
        workspace_root: artifacts.workspace_root.clone(),
        packages: artifacts.packages.clone(),
        summary: artifacts.summary.clone(),
    }
}

// =============================================================================
// Scenario 1: Check Entire Workspace
// =============================================================================

/// Scenario: Check entire workspace
///
/// Given a workspace with multiple packages
/// When running `semverguard check` in workspace mode
/// Then all packages are checked
#[test]
fn scenario_check_entire_workspace_checks_all_packages() {
    // Given: A workspace with three packages
    let packages = vec![
        make_package(
            "core-lib",
            "1.0.0",
            "/workspace/crates/core-lib",
            true,
            true,
        ),
        make_package("utils", "0.5.0", "/workspace/crates/utils", true, true),
        make_package("api", "2.0.0", "/workspace/crates/api", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((
        vec!["cargo".into(), "semver-checks".into()],
        success_output(),
    )));

    // When: Running the check in workspace mode
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let config = default_workspace_config();
    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: All packages are checked
    assert_eq!(
        result.packages.len(),
        3,
        "Expected all 3 packages to be in results"
    );
    assert_eq!(
        result.summary.total, 3,
        "Summary total should equal package count"
    );
    assert_eq!(
        engine.call_count(),
        3,
        "Engine should be called once per package"
    );

    // Verify each package was processed
    let package_names: Vec<&str> = result.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(package_names.contains(&"core-lib"));
    assert!(package_names.contains(&"utils"));
    assert!(package_names.contains(&"api"));

    // Verify all passed
    assert_eq!(result.summary.passed, 3);
    assert_eq!(result.summary.failed, 0);
    assert_eq!(result.summary.skipped, 0);
}

/// Scenario: Check workspace with mixed results
///
/// Given a workspace with packages that have different check outcomes
/// When running checks
/// Then results correctly reflect pass/fail status for each package
#[test]
fn scenario_workspace_check_captures_mixed_results() {
    // Given: A workspace with packages that will have different outcomes
    let packages = vec![
        make_package("passing-lib", "1.0.0", "/workspace/passing-lib", true, true),
        make_package("failing-lib", "1.0.0", "/workspace/failing-lib", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::with_results(vec![
        Ok((
            vec!["cargo".into()],
            SemverCheckOutput {
                exit_code: Some(0),
                success: true,
                stdout: "OK".into(),
                stderr: String::new(),
                required_bump: None,
            },
        )),
        Ok((
            vec!["cargo".into()],
            SemverCheckOutput {
                exit_code: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "Breaking change detected".into(),
                required_bump: Some(semverguard_types::RequiredBump::Major),
            },
        )),
    ]);

    // When: Running the check
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let result = runner
        .run(Path::new("/workspace"), &default_workspace_config())
        .unwrap();

    // Then: Results correctly capture both pass and fail
    assert_eq!(result.summary.passed, 1);
    assert_eq!(result.summary.failed, 1);

    // Verify specific package statuses
    let passing = result
        .packages
        .iter()
        .find(|p| p.name == "passing-lib")
        .unwrap();
    let failing = result
        .packages
        .iter()
        .find(|p| p.name == "failing-lib")
        .unwrap();

    assert_eq!(passing.status, PackageStatus::Passed);
    assert_eq!(failing.status, PackageStatus::Failed);
    assert!(failing
        .engine
        .as_ref()
        .unwrap()
        .stderr
        .contains("Breaking change"));
}

// =============================================================================
// Scenario 2: Check Only Changed Packages
// =============================================================================

/// Scenario: Check only changed packages
///
/// Given a workspace with multiple packages
/// And changes only in one package
/// When running `semverguard check --changed`
/// Then only the changed package is checked
#[test]
fn scenario_changed_mode_checks_only_modified_packages() {
    // Given: A workspace with three packages
    let packages = vec![
        make_package(
            "changed-lib",
            "1.0.0",
            "/workspace/crates/changed-lib",
            true,
            true,
        ),
        make_package(
            "unchanged-lib",
            "1.0.0",
            "/workspace/crates/unchanged-lib",
            true,
            true,
        ),
        make_package(
            "another-unchanged",
            "1.0.0",
            "/workspace/crates/another-unchanged",
            true,
            true,
        ),
    ];
    let metadata = make_workspace("/workspace", packages);

    // And: Only changed-lib has modifications
    let changed_paths = vec![
        PathBuf::from("crates/changed-lib/src/lib.rs"),
        PathBuf::from("crates/changed-lib/src/utils.rs"),
    ];

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let git = MockGitProvider::with_changed_paths(changed_paths);
    let engine = MockSemverEngine::success();

    // When: Running in changed mode
    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let config = changed_mode_config("origin/main");
    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Only the changed package is checked, others are skipped
    assert_eq!(
        engine.call_count(),
        1,
        "Engine should only be called for the changed package"
    );
    assert_eq!(result.summary.passed, 1);
    assert_eq!(
        result.summary.skipped, 2,
        "Unchanged packages should be skipped"
    );

    // Verify the changed package was checked
    let changed = result
        .packages
        .iter()
        .find(|p| p.name == "changed-lib")
        .unwrap();
    assert_eq!(changed.status, PackageStatus::Passed);

    // Verify unchanged packages are skipped with correct reason
    let unchanged = result
        .packages
        .iter()
        .find(|p| p.name == "unchanged-lib")
        .unwrap();
    assert_eq!(unchanged.status, PackageStatus::Skipped);
    assert!(
        unchanged
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("unchanged"),
        "Skip reason should indicate unchanged status"
    );
}

/// Scenario: Workspace-level changes trigger all packages
///
/// Given a workspace with multiple packages
/// And changes to workspace-level files (e.g., root Cargo.toml)
/// When running in changed mode
/// Then all packages are checked (conservative behavior)
#[test]
fn scenario_workspace_level_changes_check_all_packages() {
    // Given: A workspace with two packages
    let packages = vec![
        make_package("lib-a", "1.0.0", "/workspace/crates/lib-a", true, true),
        make_package("lib-b", "1.0.0", "/workspace/crates/lib-b", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    // And: A workspace-level file changed (not within any package)
    let changed_paths = vec![PathBuf::from("Cargo.toml")];

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let git = MockGitProvider::with_changed_paths(changed_paths);
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Running in changed mode
    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let config = changed_mode_config("origin/main");
    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: All packages are checked due to conservative behavior
    assert_eq!(
        engine.call_count(),
        2,
        "All packages should be checked when workspace-level files change"
    );
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.summary.skipped, 0);
}

/// Scenario: No changes result in no checks
///
/// Given a workspace with packages
/// And no files have changed
/// When running in changed mode
/// Then no packages are checked
#[test]
fn scenario_no_changes_skips_all_packages() {
    // Given: A workspace with packages
    let packages = vec![
        make_package("lib-a", "1.0.0", "/workspace/lib-a", true, true),
        make_package("lib-b", "1.0.0", "/workspace/lib-b", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    // And: No changes in git
    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let git = MockGitProvider::no_changes();
    let engine = MockSemverEngine::success();

    // When: Running in changed mode
    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let config = changed_mode_config("origin/main");
    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: No packages are checked, all are skipped
    engine.assert_not_called();
    assert_eq!(result.summary.skipped, 2);
    assert_eq!(result.summary.passed, 0);
}

// =============================================================================
// Scenario 3: Exclude Packages by Glob
// =============================================================================

/// Scenario: Exclude packages by glob
///
/// Given a workspace with packages matching exclude patterns
/// When running with exclude config
/// Then matching packages are skipped
#[test]
fn scenario_exclude_glob_skips_matching_packages() {
    // Given: A workspace with packages, some matching exclude patterns
    let packages = vec![
        make_package("core-lib", "1.0.0", "/workspace/core-lib", true, true),
        make_package("core-test", "0.1.0", "/workspace/core-test", true, true),
        make_package("utils-test", "0.1.0", "/workspace/utils-test", true, true),
        make_package("helpers", "1.0.0", "/workspace/helpers", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Running with exclude pattern for "*-test" packages
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.exclude = vec!["*-test".to_string()];

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Test packages are skipped, others are checked
    assert_eq!(
        engine.call_count(),
        2,
        "Only non-test packages should be checked"
    );
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.summary.skipped, 2);

    // Verify excluded packages have correct skip reason
    let test_pkg = result
        .packages
        .iter()
        .find(|p| p.name == "core-test")
        .unwrap();
    assert_eq!(test_pkg.status, PackageStatus::Skipped);
    assert!(
        test_pkg.skip_reason.as_ref().unwrap().contains("filtered"),
        "Skip reason should mention filtering"
    );

    // Verify non-excluded packages were checked
    let core = result
        .packages
        .iter()
        .find(|p| p.name == "core-lib")
        .unwrap();
    let helpers = result
        .packages
        .iter()
        .find(|p| p.name == "helpers")
        .unwrap();
    assert_eq!(core.status, PackageStatus::Passed);
    assert_eq!(helpers.status, PackageStatus::Passed);
}

/// Scenario: Include and exclude globs work together
///
/// Given a workspace with various packages
/// When running with both include and exclude patterns
/// Then only packages matching include AND not matching exclude are checked
#[test]
fn scenario_include_and_exclude_combined() {
    // Given: A workspace with various packages
    let packages = vec![
        make_package("myapp-core", "1.0.0", "/workspace/myapp-core", true, true),
        make_package("myapp-test", "0.1.0", "/workspace/myapp-test", true, true),
        make_package("myapp-utils", "1.0.0", "/workspace/myapp-utils", true, true),
        make_package("other-lib", "1.0.0", "/workspace/other-lib", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Including "myapp-*" but excluding "*-test"
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.include = vec!["myapp-*".to_string()];
    config.scope.exclude = vec!["*-test".to_string()];

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Only myapp-core and myapp-utils are checked
    assert_eq!(engine.call_count(), 2);
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.summary.skipped, 2);

    // Verify correct packages were checked
    let checked: Vec<&str> = result
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Passed)
        .map(|p| p.name.as_str())
        .collect();
    assert!(checked.contains(&"myapp-core"));
    assert!(checked.contains(&"myapp-utils"));
    assert!(!checked.contains(&"myapp-test"));
    assert!(!checked.contains(&"other-lib"));
}

/// Scenario: Multiple exclude patterns
///
/// Given a workspace with various packages
/// When running with multiple exclude patterns
/// Then packages matching any pattern are skipped
#[test]
fn scenario_multiple_exclude_patterns() {
    // Given: A workspace with packages
    let packages = vec![
        make_package("main-lib", "1.0.0", "/workspace/main-lib", true, true),
        make_package("test-utils", "0.1.0", "/workspace/test-utils", true, true),
        make_package(
            "internal-helpers",
            "0.1.0",
            "/workspace/internal-helpers",
            true,
            true,
        ),
        make_package("api-client", "1.0.0", "/workspace/api-client", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Excluding both "test-*" and "internal-*" patterns
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.exclude = vec!["test-*".to_string(), "internal-*".to_string()];

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Both test and internal packages are skipped
    assert_eq!(engine.call_count(), 2);
    assert_eq!(result.summary.skipped, 2);

    let skipped_names: Vec<&str> = result
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Skipped)
        .map(|p| p.name.as_str())
        .collect();
    assert!(skipped_names.contains(&"test-utils"));
    assert!(skipped_names.contains(&"internal-helpers"));
}

// =============================================================================
// Scenario 4: Skip Non-Publishable Packages
// =============================================================================

/// Scenario: Skip non-publishable packages
///
/// Given a package with `publish = false`
/// When running check
/// Then the package is skipped with reason
#[test]
fn scenario_skip_publish_false_packages() {
    // Given: A workspace with both publishable and non-publishable packages
    let packages = vec![
        make_package("public-api", "1.0.0", "/workspace/public-api", true, true),
        make_package(
            "internal-only",
            "0.1.0",
            "/workspace/internal-only",
            false,
            true,
        ),
        make_package(
            "another-public",
            "2.0.0",
            "/workspace/another-public",
            true,
            true,
        ),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Running with skip_publish_false enabled
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.skip_publish_false = true;

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Non-publishable package is skipped
    assert_eq!(
        engine.call_count(),
        2,
        "Only publishable packages should be checked"
    );
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.summary.skipped, 1);

    // Verify the internal package was skipped with correct reason
    let internal = result
        .packages
        .iter()
        .find(|p| p.name == "internal-only")
        .unwrap();
    assert_eq!(internal.status, PackageStatus::Skipped);
    assert!(
        internal
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("publish = false"),
        "Skip reason should mention publish = false"
    );

    // Verify skipped package has zero duration and no engine output
    assert_eq!(internal.duration_ms, 0);
    assert!(internal.engine.is_none());
}

/// Scenario: Skip binary-only packages (no library target)
///
/// Given a package without a library target
/// When running check with skip_no_lib enabled
/// Then the package is skipped
#[test]
fn scenario_skip_binary_only_packages() {
    // Given: A workspace with library and binary-only packages
    let packages = vec![
        make_package("my-library", "1.0.0", "/workspace/my-library", true, true),
        make_package("cli-tool", "1.0.0", "/workspace/cli-tool", true, false), // No lib
        make_package("another-lib", "1.0.0", "/workspace/another-lib", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Running with skip_no_lib enabled
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.skip_no_lib = true;

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Binary-only package is skipped
    assert_eq!(engine.call_count(), 2);
    assert_eq!(result.summary.skipped, 1);

    let cli = result
        .packages
        .iter()
        .find(|p| p.name == "cli-tool")
        .unwrap();
    assert_eq!(cli.status, PackageStatus::Skipped);
    assert!(
        cli.skip_reason
            .as_ref()
            .unwrap()
            .contains("no library target"),
        "Skip reason should mention no library target"
    );
}

/// Scenario: Combine multiple skip filters
///
/// Given packages with various characteristics
/// When running with multiple filters enabled
/// Then packages matching any filter are skipped
#[test]
fn scenario_multiple_skip_filters_combined() {
    // Given: Packages with various characteristics
    let packages = vec![
        // Should be checked: publishable, has lib, matches include, not excluded
        make_package("core-lib", "1.0.0", "/workspace/core-lib", true, true),
        // Should skip: not publishable
        make_package(
            "internal-lib",
            "0.1.0",
            "/workspace/internal-lib",
            false,
            true,
        ),
        // Should skip: no library target
        make_package("core-cli", "1.0.0", "/workspace/core-cli", true, false),
        // Should skip: matches exclude pattern
        make_package("core-test", "0.1.0", "/workspace/core-test", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();

    // When: Running with multiple filters
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.skip_publish_false = true;
    config.scope.skip_no_lib = true;
    config.scope.exclude = vec!["*-test".to_string()];

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Only core-lib should be checked
    assert_eq!(engine.call_count(), 1);
    assert_eq!(result.summary.passed, 1);
    assert_eq!(result.summary.skipped, 3);

    let checked = result
        .packages
        .iter()
        .find(|p| p.status == PackageStatus::Passed)
        .unwrap();
    assert_eq!(checked.name, "core-lib");
}

// =============================================================================
// Scenario 5: JSON Report Generation
// =============================================================================

/// Scenario: JSON report generation
///
/// Given a successful check run
/// When `--json report.json` is specified
/// Then a valid JSON report is written
#[test]
fn scenario_json_report_generation() {
    // Given: A workspace with packages
    let packages = vec![
        make_package("lib-a", "1.0.0", "/workspace/lib-a", true, true),
        make_package("lib-b", "2.0.0", "/workspace/lib-b", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((
        vec![
            "cargo".into(),
            "semver-checks".into(),
            "check-release".into(),
        ],
        success_output(),
    )));

    // When: Running the check
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let config = default_workspace_config();
    let artifacts = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: The artifacts can be serialized to valid JSON
    let report = artifacts_to_report(&artifacts);
    let json_result = serde_json::to_string_pretty(&report);
    assert!(json_result.is_ok(), "Report should serialize to JSON");

    let json = json_result.unwrap();

    // Verify JSON contains expected structure
    assert!(json.contains("\"semverguard_version\""));
    assert!(json.contains("\"workspace_root\""));
    assert!(json.contains("\"packages\""));
    assert!(json.contains("\"summary\""));
    assert!(json.contains("\"lib-a\""));
    assert!(json.contains("\"lib-b\""));
}

/// Scenario: JSON report contains correct summary counts
///
/// Given a check with mixed results (pass, fail, skip)
/// When generating the JSON report
/// Then the summary accurately reflects all outcomes
#[test]
fn scenario_json_report_summary_accuracy() {
    // Given: A workspace with packages that will have different outcomes
    let packages = vec![
        make_package("passing", "1.0.0", "/workspace/passing", true, true),
        make_package("failing", "1.0.0", "/workspace/failing", true, true),
        make_package("skipped", "0.1.0", "/workspace/skipped", false, true), // Not publishable
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::with_results(vec![
        Ok((vec![], success_output())),
        Ok((
            vec![],
            SemverCheckOutput {
                exit_code: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "API breakage".into(),
                required_bump: Some(semverguard_types::RequiredBump::Major),
            },
        )),
    ]);

    // When: Running with skip_publish_false enabled
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.skip_publish_false = true;

    let artifacts = runner.run(Path::new("/workspace"), &config).unwrap();
    let report = artifacts_to_report(&artifacts);

    // Then: Summary correctly reflects all outcomes
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let summary = &parsed["summary"];
    assert_eq!(summary["total"], 3);
    assert_eq!(summary["passed"], 1);
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["skipped"], 1);

    // Verify overall_success reflects the failure
    assert!(!report.summary.overall_success());
}

/// Scenario: JSON report includes package details
///
/// Given a completed check run
/// When generating the JSON report
/// Then each package entry contains expected fields
#[test]
fn scenario_json_report_package_details() {
    // Given: A workspace with a package
    let packages = vec![make_package(
        "detailed-lib",
        "1.2.3",
        "/workspace/detailed-lib",
        true,
        true,
    )];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::with_result(Ok((
        vec!["cargo".into(), "semver-checks".into()],
        SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "Check completed successfully".into(),
            stderr: String::new(),
            required_bump: None,
        },
    )));

    // When: Running the check
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let artifacts = runner
        .run(Path::new("/workspace"), &default_workspace_config())
        .unwrap();
    let report = artifacts_to_report(&artifacts);

    // Then: Package entry contains all expected fields
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let pkg = &parsed["packages"][0];
    assert_eq!(pkg["name"], "detailed-lib");
    assert_eq!(pkg["version"], "1.2.3");
    assert!(pkg["manifest_path"]
        .as_str()
        .unwrap()
        .contains("detailed-lib"));
    assert_eq!(pkg["status"], "passed");
    assert!(pkg["skip_reason"].is_null());
    assert!(pkg["duration_ms"].is_number());
    assert!(pkg["command"].is_array());
    assert!(pkg["engine"].is_object());
}

/// Scenario: JSON report for skipped packages
///
/// Given packages that are skipped
/// When generating the JSON report
/// Then skipped packages have correct status and reason
#[test]
fn scenario_json_report_skipped_packages() {
    // Given: A package that will be skipped
    let packages = vec![make_package(
        "internal-pkg",
        "0.1.0",
        "/workspace/internal-pkg",
        false, // Not publishable
        true,
    )];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();

    // When: Running with skip_publish_false
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.scope.skip_publish_false = true;

    let artifacts = runner.run(Path::new("/workspace"), &config).unwrap();
    let report = artifacts_to_report(&artifacts);

    // Then: Skipped package has correct status and reason in JSON
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let pkg = &parsed["packages"][0];
    assert_eq!(pkg["status"], "skipped");
    assert!(
        pkg["skip_reason"]
            .as_str()
            .unwrap()
            .contains("publish = false"),
        "Skip reason should be present in JSON"
    );
    assert_eq!(pkg["duration_ms"], 0);
    assert!(pkg["engine"].is_null());
}

// =============================================================================
// Additional Edge Cases
// =============================================================================

/// Scenario: Empty workspace
///
/// Given a workspace with no packages
/// When running check
/// Then the run succeeds with zero packages
#[test]
fn scenario_empty_workspace() {
    // Given: An empty workspace
    let metadata = make_workspace("/workspace", vec![]);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::success();

    // When: Running check
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let result = runner
        .run(Path::new("/workspace"), &default_workspace_config())
        .unwrap();

    // Then: Run succeeds with zero packages
    assert_eq!(result.packages.len(), 0);
    assert_eq!(result.summary.total, 0);
    engine.assert_not_called();
}

/// Scenario: Fail-fast stops on first failure
///
/// Given multiple packages with a failing package
/// When running with fail_fast enabled
/// Then execution stops after the first failure
#[test]
fn scenario_fail_fast_stops_early() {
    // Given: Three packages, second will fail
    let packages = vec![
        make_package("first", "1.0.0", "/workspace/first", true, true),
        make_package(
            "second-fails",
            "1.0.0",
            "/workspace/second-fails",
            true,
            true,
        ),
        make_package("third", "1.0.0", "/workspace/third", true, true),
    ];
    let metadata = make_workspace("/workspace", packages);

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let engine = MockSemverEngine::with_results(vec![
        Ok((vec![], success_output())),
        Ok((
            vec![],
            SemverCheckOutput {
                exit_code: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "Breaking change".into(),
                required_bump: None,
            },
        )),
        // Third result would be used but shouldn't be reached
        Ok((vec![], success_output())),
    ]);

    // When: Running with fail_fast
    let runner = SemverguardRunner::new(&workspace, None, &engine);
    let mut config = default_workspace_config();
    config.engine.fail_fast = true;

    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Only first two packages are processed
    assert_eq!(engine.call_count(), 2, "Should stop after first failure");
    assert_eq!(result.packages.len(), 2);
    assert_eq!(result.summary.passed, 1);
    assert_eq!(result.summary.failed, 1);
}

/// Scenario: Changed mode with multiple packages changed
///
/// Given changes in multiple packages
/// When running in changed mode
/// Then all changed packages are checked
#[test]
fn scenario_multiple_changed_packages() {
    // Given: A workspace with packages
    let packages = vec![
        make_package(
            "changed-a",
            "1.0.0",
            "/workspace/crates/changed-a",
            true,
            true,
        ),
        make_package(
            "changed-b",
            "1.0.0",
            "/workspace/crates/changed-b",
            true,
            true,
        ),
        make_package(
            "unchanged",
            "1.0.0",
            "/workspace/crates/unchanged",
            true,
            true,
        ),
    ];
    let metadata = make_workspace("/workspace", packages);

    // And: Multiple packages have changes
    let changed_paths = vec![
        PathBuf::from("crates/changed-a/src/lib.rs"),
        PathBuf::from("crates/changed-b/src/mod.rs"),
    ];

    let workspace = MockWorkspaceProvider::with_result(Ok(metadata));
    let git = MockGitProvider::with_changed_paths(changed_paths);
    let engine = MockSemverEngine::success();
    engine.set_default(Ok((vec![], success_output())));

    // When: Running in changed mode
    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let config = changed_mode_config("origin/main");
    let result = runner.run(Path::new("/workspace"), &config).unwrap();

    // Then: Both changed packages are checked
    assert_eq!(engine.call_count(), 2);
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.summary.skipped, 1);

    let checked_names: Vec<&str> = result
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Passed)
        .map(|p| p.name.as_str())
        .collect();
    assert!(checked_names.contains(&"changed-a"));
    assert!(checked_names.contains(&"changed-b"));
}

// =============================================================================
// Scenario 6: Receipt + Exit Code Integration
// =============================================================================

#[test]
fn scenario_tool_error_emits_receipt_bundle() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let artifacts = build_artifact_index(temp_dir.path(), None, false);
    let receipt = build_receipt(
        None,
        &[ToolErrorFinding::new("simulated tool failure")],
        &artifacts,
        &BaselineConfig::default(),
    );

    write_receipt_bundle(temp_dir.path(), &receipt, None, false, true)
        .expect("failed to write receipt bundle");

    assert!(temp_dir.path().join("report.json").exists());
    assert!(temp_dir.path().join("comment.md").exists());
}

#[test]
fn scenario_receipt_exit_codes_and_required_fields() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let report = RunReport {
        semverguard_version: "0.1.0".to_string(),
        started_at: "2024-01-15T10:00:00Z".to_string(),
        finished_at: "2024-01-15T10:01:00Z".to_string(),
        workspace_root: temp_dir.path().to_path_buf(),
        packages: vec![semverguard_types::PackageReport {
            name: "lib-a".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: temp_dir.path().join("Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 10,
            command: vec![],
            engine: None,
            inferred_required_bump: Some(semverguard_types::RequiredBump::Major),
            failure_kind: Some(semverguard_types::FailureKind::SemverViolation),
        }],
        summary: semverguard_types::Summary {
            total: 1,
            passed: 0,
            failed: 1,
            skipped: 0,
        },
    };

    let artifacts = build_artifact_index(temp_dir.path(), Some(&report), false);
    let receipt = build_receipt(Some(&report), &[], &artifacts, &BaselineConfig::default());
    let code = exit_code_from_receipt(
        &receipt.verdict,
        false,
        has_tool_error(&receipt.findings),
    );

    assert_eq!(code, 2);

    let json = serde_json::to_value(&receipt).expect("receipt should serialize");
    assert!(json.get("schema").is_some());
    assert!(json.get("tool").is_some());
    assert!(json.get("run").and_then(|r| r.get("started_at")).is_some());
    assert!(json.get("verdict").is_some());
    assert!(json.get("findings").is_some());
}
