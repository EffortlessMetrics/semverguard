//! Given step definitions for setting up test state.

use crate::world::{EngineResult, TestWorld};
use cucumber::given;
use semverguard_types::{BaselineKind, ScopeMode};
use std::path::PathBuf;

// =============================================================================
// Package Setup
// =============================================================================

#[given(expr = "a workspace with packages:")]
fn workspace_with_packages(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    if let Some(table) = step.table.as_ref() {
        for row in table.rows.iter().skip(1) {
            // Skip header row
            let name = row.get(0).map(|s| s.as_str()).unwrap_or("");
            let version = row.get(1).map(|s| s.as_str()).unwrap_or("1.0.0");
            let publishable = row.get(2).map(|s| s == "true").unwrap_or(true);
            let has_lib = row.get(3).map(|s| s == "true").unwrap_or(true);
            let path = row.get(4).map(|s| s.as_str());

            world.add_package(name, version, publishable, has_lib, path);
        }
    }
}

#[given(expr = "an empty workspace")]
fn empty_workspace(_world: &mut TestWorld) {
    // World starts with empty packages by default
}

// =============================================================================
// Config Setup - Include/Exclude Patterns (using regex to match brackets)
// =============================================================================

#[given(regex = r#"^config has include patterns \[(.+)\]$"#)]
fn config_include_patterns(world: &mut TestWorld, patterns: String) {
    let patterns: Vec<String> = parse_quoted_list(&patterns);
    world.config.scope.include = patterns;
}

#[given(regex = r#"^config has exclude patterns \[(.+)\]$"#)]
fn config_exclude_patterns(world: &mut TestWorld, patterns: String) {
    let patterns: Vec<String> = parse_quoted_list(&patterns);
    world.config.scope.exclude = patterns;
}

#[given(regex = r#"^explicit packages \[(.+)\]$"#)]
fn explicit_packages(world: &mut TestWorld, packages: String) {
    let packages: Vec<String> = parse_quoted_list(&packages);
    world.config.scope.explicit_packages = packages;
}

// =============================================================================
// Config Setup - Skip Filters
// =============================================================================

#[given(expr = "config has skip_publish_false enabled")]
fn skip_publish_false_enabled(world: &mut TestWorld) {
    world.config.scope.skip_publish_false = true;
}

#[given(expr = "config has skip_publish_false disabled")]
fn skip_publish_false_disabled(world: &mut TestWorld) {
    world.config.scope.skip_publish_false = false;
}

#[given(expr = "config has skip_no_lib enabled")]
fn skip_no_lib_enabled(world: &mut TestWorld) {
    world.config.scope.skip_no_lib = true;
}

#[given(expr = "config has skip_no_lib disabled")]
fn skip_no_lib_disabled(world: &mut TestWorld) {
    world.config.scope.skip_no_lib = false;
}

// =============================================================================
// Config Setup - Scope Mode
// =============================================================================

#[given(expr = "scope mode is {string}")]
fn scope_mode(world: &mut TestWorld, mode: String) {
    world.config.scope.mode = match mode.as_str() {
        "workspace" => ScopeMode::Workspace,
        "changed" => ScopeMode::Changed,
        _ => panic!("Unknown scope mode: {}", mode),
    };
}

#[given(expr = "baseline rev is {string}")]
fn baseline_rev(world: &mut TestWorld, rev: String) {
    world.config.baseline.rev = Some(rev);
    world.config.baseline.kind = BaselineKind::Git;
}

#[given(expr = "baseline rev is not set")]
fn baseline_rev_not_set(world: &mut TestWorld) {
    world.baseline_rev_unset = true;
    world.config.baseline.kind = BaselineKind::Git;
}

// =============================================================================
// Config Setup - Changed Files (using regex to match brackets)
// =============================================================================

#[given(regex = r#"^changed files \[(.+)\]$"#)]
fn changed_files(world: &mut TestWorld, files: String) {
    let files: Vec<String> = parse_quoted_list(&files);
    world.changed_paths = files.into_iter().map(PathBuf::from).collect();
}

#[given(expr = "no changed files")]
fn no_changed_files(world: &mut TestWorld) {
    world.changed_paths.clear();
}

// =============================================================================
// Config Setup - Fail Fast
// =============================================================================

#[given(expr = "fail_fast is enabled")]
fn fail_fast_enabled(world: &mut TestWorld) {
    world.config.engine.fail_fast = true;
}

#[given(expr = "fail_fast is disabled")]
fn fail_fast_disabled(world: &mut TestWorld) {
    world.config.engine.fail_fast = false;
}

// =============================================================================
// Engine Result Configuration
// =============================================================================

#[given(expr = "engine will return {string} for {string}")]
fn engine_will_return(world: &mut TestWorld, result: String, package: String) {
    let engine_result = match result.as_str() {
        "pass" => EngineResult::Pass,
        "fail" => EngineResult::Fail,
        _ => EngineResult::Error(format!("Unknown result: {}", result)),
    };
    world.engine_results.insert(package, engine_result);
}

#[given(expr = "engine will return error {string} for {string}")]
fn engine_will_return_error(world: &mut TestWorld, error: String, package: String) {
    world
        .engine_results
        .insert(package, EngineResult::Error(error));
}

#[given(expr = "all engines return {string}")]
fn all_engines_return(world: &mut TestWorld, result: String) {
    world.default_engine_result = match result.as_str() {
        "pass" => EngineResult::Pass,
        "fail" => EngineResult::Fail,
        _ => EngineResult::Error(format!("Unknown result: {}", result)),
    };
}

// =============================================================================
// Helpers
// =============================================================================

/// Parse a quoted list like `"a", "b"` into a Vec<String>
fn parse_quoted_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
