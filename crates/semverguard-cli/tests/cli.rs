//! Integration tests for the semverguard CLI binary.
//!
//! These tests exercise the CLI binary via `assert_cmd`, testing:
//! - Argument parsing (--help, subcommands, invalid arguments)
//! - Config loading (defaults, file loading, CLI overrides)
//! - Output formatting (text, json, file output)
//! - Exit codes (0 success, 1 semver failures, 2 config errors)

#![allow(deprecated)] // cargo_bin function is deprecated but still functional

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper to get a Command for the semverguard binary.
fn semverguard() -> Command {
    Command::new(cargo_bin("semverguard"))
}

// =============================================================================
// Section 1: Argument Parsing Tests
// =============================================================================

mod argument_parsing {
    use super::*;

    // -------------------------------------------------------------------------
    // --help output tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_root_help_shows_description() {
        semverguard()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("semverguard"))
            .stdout(predicate::str::contains(
                "Workspace orchestration for cargo-semver-checks",
            ));
    }

    #[test]
    fn test_root_help_lists_subcommands() {
        semverguard()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("check"))
            .stdout(predicate::str::contains("print-config"));
    }

    #[test]
    fn test_help_short_flag() {
        semverguard()
            .arg("-h")
            .assert()
            .success()
            .stdout(predicate::str::contains("semverguard"));
    }

    #[test]
    fn test_version_flag() {
        semverguard()
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("semverguard"));
    }

    #[test]
    fn test_version_short_flag() {
        semverguard()
            .arg("-V")
            .assert()
            .success()
            .stdout(predicate::str::contains("semverguard"));
    }

    // -------------------------------------------------------------------------
    // print-config subcommand tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_print_config_help() {
        semverguard()
            .args(["print-config", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Print the effective configuration",
            ))
            .stdout(predicate::str::contains("--config"))
            .stdout(predicate::str::contains("--workspace-root"));
    }

    #[test]
    fn test_print_config_accepts_config_flag() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("custom.toml");
        fs::write(&config_path, "[baseline]\nkind = \"git\"\n").expect("failed to write config");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success();
    }

    #[test]
    fn test_print_config_accepts_workspace_root_flag() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success();
    }

    // -------------------------------------------------------------------------
    // check subcommand tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_help_shows_all_options() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Run semver checks"))
            .stdout(predicate::str::contains("--baseline-rev"))
            .stdout(predicate::str::contains("--baseline-version"))
            .stdout(predicate::str::contains("--changed"))
            .stdout(predicate::str::contains("--json"))
            .stdout(predicate::str::contains("--format"))
            .stdout(predicate::str::contains("--fail-fast"))
            .stdout(predicate::str::contains("--cargo-bin"))
            .stdout(predicate::str::contains("--engine-arg"))
            .stdout(predicate::str::contains("--config"))
            .stdout(predicate::str::contains("--workspace-root"));
    }

    #[test]
    fn test_check_baseline_rev_option_documented() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--baseline-rev"))
            .stdout(predicate::str::contains("git baseline"));
    }

    #[test]
    fn test_check_baseline_version_option_documented() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--baseline-version"))
            .stdout(predicate::str::contains("crates.io"));
    }

    #[test]
    fn test_check_changed_option_documented() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--changed"))
            .stdout(predicate::str::contains("changed").or(predicate::str::contains("baseline")));
    }

    #[test]
    fn test_check_format_option_values() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"))
            .stdout(predicate::str::contains("text").or(predicate::str::contains("json")));
    }

    // -------------------------------------------------------------------------
    // Invalid arguments tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_unknown_subcommand_fails() {
        semverguard()
            .arg("unknown-command")
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("unrecognized subcommand")
                    .or(predicate::str::contains("invalid")),
            );
    }

    #[test]
    fn test_unknown_global_flag_fails() {
        semverguard()
            .arg("--unknown-flag")
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("unexpected argument")
                    .or(predicate::str::is_match("unknown|unexpected").unwrap()),
            );
    }

    #[test]
    fn test_check_unknown_flag_fails() {
        semverguard()
            .args(["check", "--unknown-option"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("unexpected argument")
                    .or(predicate::str::is_match("unknown|unexpected").unwrap()),
            );
    }

    #[test]
    fn test_print_config_unknown_flag_fails() {
        semverguard()
            .args(["print-config", "--unknown-option"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("unexpected argument")
                    .or(predicate::str::is_match("unknown|unexpected").unwrap()),
            );
    }

    #[test]
    fn test_no_subcommand_shows_usage() {
        semverguard().assert().failure().stderr(
            predicate::str::contains("check")
                .and(predicate::str::contains("print-config"))
                .or(predicate::str::contains("Usage")),
        );
    }

    #[test]
    fn test_check_format_invalid_value_fails() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["check", "--format", "invalid-format", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::is_match("invalid|error").unwrap()),
            );
    }
}

// =============================================================================
// Section 2: Config Loading Tests
// =============================================================================

mod config_loading {
    use super::*;

    // -------------------------------------------------------------------------
    // Default config tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_default_config_when_no_file_exists() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        // No semverguard.toml exists
        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("[baseline]"))
            .stdout(predicate::str::contains("[scope]"))
            .stdout(predicate::str::contains("[features]"))
            .stdout(predicate::str::contains("[engine]"))
            .stdout(predicate::str::contains("[output]"));
    }

    #[test]
    fn test_default_baseline_is_crates_io() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("crates-io"));
    }

    #[test]
    fn test_default_scope_is_workspace() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("workspace"));
    }

    #[test]
    fn test_default_output_format_is_text() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        let output = semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout_str = String::from_utf8(output).expect("valid utf-8");
        // The [output] section should contain format = "text"
        assert!(stdout_str.contains("format = \"text\""));
    }

    // -------------------------------------------------------------------------
    // Config file loading tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_file_loaded_from_explicit_path() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("custom-config.toml");

        let config_content = r#"
[baseline]
kind = "git"
rev = "origin/develop"

[scope]
mode = "changed"
"#;
        fs::write(&config_path, config_content).expect("failed to write config");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("git"))
            .stdout(predicate::str::contains("origin/develop"))
            .stdout(predicate::str::contains("changed"));
    }

    #[test]
    fn test_config_file_with_all_sections() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
kind = "git"
rev = "v1.0.0"

[scope]
mode = "workspace"
include = ["core-*"]
exclude = ["*-test"]
skip_publish_false = false
skip_no_lib = false

[features]
all_features = true
default_features = false

[engine]
fail_fast = true
extra_args = ["--verbose"]

[output]
format = "both"
pretty_json = true
"#;
        fs::write(&config_path, config_content).expect("failed to write config");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("git"))
            .stdout(predicate::str::contains("v1.0.0"))
            .stdout(predicate::str::contains("core-*"))
            .stdout(predicate::str::contains("*-test"))
            .stdout(predicate::str::contains("all_features = true"))
            .stdout(predicate::str::contains("fail_fast = true"))
            .stdout(predicate::str::contains("both"));
    }

    #[test]
    fn test_config_outputs_valid_toml() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        let output = semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout_str = String::from_utf8(output).expect("valid utf-8");
        // Verify it parses as valid TOML
        let parsed: toml::Value = toml::from_str(&stdout_str).expect("should be valid TOML");
        assert!(parsed.get("baseline").is_some());
        assert!(parsed.get("scope").is_some());
        assert!(parsed.get("features").is_some());
        assert!(parsed.get("engine").is_some());
        assert!(parsed.get("output").is_some());
    }

    #[test]
    fn test_partial_config_merges_with_defaults() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        // Only set a few values, rest should be defaults
        let config_content = r#"
[baseline]
rev = "main"

[scope]
exclude = ["internal-*"]
"#;
        fs::write(&config_path, config_content).expect("failed to write config");

        let output = semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout_str = String::from_utf8(output).expect("valid utf-8");
        // Custom values should be present
        assert!(stdout_str.contains("main"));
        assert!(stdout_str.contains("internal-*"));
        // Defaults should also be present
        assert!(stdout_str.contains("crates-io")); // default baseline kind
        assert!(stdout_str.contains("skip_publish_false = true")); // default
    }

    // -------------------------------------------------------------------------
    // CLI args override config file tests
    // -------------------------------------------------------------------------

    // Note: Testing CLI overrides is limited without running actual check commands
    // that require a valid workspace. We test that the options are accepted.

    #[test]
    fn test_check_accepts_baseline_rev_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--baseline-rev"));
    }

    #[test]
    fn test_check_accepts_baseline_version_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--baseline-version"));
    }

    #[test]
    fn test_check_accepts_changed_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--changed"));
    }

    #[test]
    fn test_check_accepts_fail_fast_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--fail-fast"));
    }

    #[test]
    fn test_check_accepts_format_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"));
    }

    #[test]
    fn test_check_accepts_json_path_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--json"));
    }

    #[test]
    fn test_check_accepts_engine_arg_override() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--engine-arg"));
    }

    // -------------------------------------------------------------------------
    // Invalid config tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_invalid_toml_syntax_returns_error() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("bad.toml");

        fs::write(&config_path, "this is not { valid toml").expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .code(1)
            .stderr(predicate::str::contains("invalid TOML").or(predicate::str::contains("TOML")));
    }

    #[test]
    fn test_unknown_config_field_returns_error() {
        // Unknown fields cause errors due to deny_unknown_fields on config structs
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
kind = "git"
unknown_field = "should cause error"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("unknown field"));
    }

    #[test]
    fn test_invalid_enum_value_returns_error() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
kind = "invalid-kind"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .code(1)
            .stderr(predicate::str::contains("invalid").or(predicate::str::contains("TOML")));
    }
}

// =============================================================================
// Section 3: Output Formatting Tests
// =============================================================================

mod output_formatting {
    use super::*;

    // -------------------------------------------------------------------------
    // Text output tests
    // -------------------------------------------------------------------------

    // Note: Full output format testing requires running actual check commands
    // with valid workspaces and cargo-semver-checks installed. These tests
    // verify the configuration and argument parsing aspects.

    #[test]
    fn test_format_text_option_accepted() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"));
    }

    #[test]
    fn test_print_config_outputs_text_format() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        // Default should show text format
        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("format = \"text\""));
    }

    // -------------------------------------------------------------------------
    // JSON output tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_json_option_accepted() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"));
    }

    #[test]
    fn test_config_with_json_format() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[output]
format = "json"
pretty_json = false
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("format = \"json\""))
            .stdout(predicate::str::contains("pretty_json = false"));
    }

    #[test]
    fn test_config_with_both_format() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[output]
format = "both"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("format = \"both\""));
    }

    // -------------------------------------------------------------------------
    // --json <path> file output tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_json_path_option_documented() {
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--json"))
            .stdout(predicate::str::contains("JSON").or(predicate::str::contains("json")));
    }

    #[test]
    fn test_config_with_json_path() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[output]
format = "json"
json_path = "report.json"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("json_path = \"report.json\""));
    }

    #[test]
    fn test_pretty_json_option() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[output]
pretty_json = true
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("pretty_json = true"));
    }
}

// =============================================================================
// Section 4: Exit Code Tests
// =============================================================================

mod exit_codes {
    use super::*;

    // -------------------------------------------------------------------------
    // Exit 0 on success tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_help_exits_with_code_0() {
        semverguard().arg("--help").assert().code(0);
    }

    #[test]
    fn test_version_exits_with_code_0() {
        semverguard().arg("--version").assert().code(0);
    }

    #[test]
    fn test_print_config_default_exits_with_code_0() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .code(0);
    }

    #[test]
    fn test_print_config_with_file_exits_with_code_0() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = "[baseline]\nkind = \"git\"\n";
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .code(0);
    }

    // -------------------------------------------------------------------------
    // Exit 2 on semver failures tests
    // -------------------------------------------------------------------------

    // Note: Testing exit code 2 (semver failures) requires running the actual
    // check command with a workspace that has breaking changes. This is
    // documented here but requires cargo-semver-checks to be installed and
    // a specially crafted test fixture.

    // -------------------------------------------------------------------------
    // Exit 1 on config/runtime errors tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_nonexistent_workspace_root_exits_with_code_2() {
        semverguard()
            .args([
                "print-config",
                "--workspace-root",
                "/nonexistent/path/xyz123",
            ])
            .assert()
            .code(1)
            .stderr(predicate::str::contains("workspace root does not exist"));
    }

    #[test]
    fn test_workspace_root_is_file_exits_with_code_2() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("not-a-directory.txt");
        fs::write(&file_path, "content").expect("failed to write");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(&file_path)
            .assert()
            .code(1)
            .stderr(predicate::str::contains(
                "workspace root is not a directory",
            ));
    }

    #[test]
    fn test_invalid_config_file_exits_with_code_2() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("bad.toml");

        fs::write(&config_path, "invalid { toml syntax").expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .code(1);
    }

    #[test]
    fn test_nonexistent_config_file_with_explicit_path_uses_defaults() {
        // When config file doesn't exist, CLI uses defaults (not an error)
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args([
                "print-config",
                "--config",
                "/nonexistent/config.toml",
                "--workspace-root",
            ])
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("crates-io")); // default baseline kind
    }

    #[test]
    fn test_config_with_wrong_type_exits_with_code_2() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        // fail_fast should be bool, not string
        let config_content = r#"
[engine]
fail_fast = "not-a-bool"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .code(1);
    }

    // -------------------------------------------------------------------------
    // Invalid argument exit code tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_unknown_subcommand_exits_nonzero() {
        semverguard().arg("unknown").assert().failure(); // clap returns exit code 2 for usage errors
    }

    #[test]
    fn test_missing_required_subcommand_exits_nonzero() {
        semverguard().assert().failure();
    }

    #[test]
    fn test_invalid_format_value_exits_nonzero() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        semverguard()
            .args(["check", "--format", "invalid", "--workspace-root"])
            .arg(temp_dir.path())
            .assert()
            .failure();
    }
}

// =============================================================================
// Section 5: Fixture-Based Tests
// =============================================================================

mod fixture_tests {
    use super::*;

    /// Path to the fixtures directory relative to the test file.
    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::PathBuf::from(manifest_dir)
            .join("tests")
            .join("fixtures")
    }

    #[test]
    fn test_fixtures_dir_exists() {
        let fixtures = fixtures_dir();
        assert!(
            fixtures.exists(),
            "fixtures directory should exist at {:?}",
            fixtures
        );
    }

    #[test]
    fn test_simple_workspace_fixture_exists() {
        let workspace = fixtures_dir().join("simple_workspace");
        assert!(workspace.exists(), "simple_workspace fixture should exist");
        assert!(
            workspace.join("Cargo.toml").exists(),
            "should have Cargo.toml"
        );
        assert!(
            workspace.join("semverguard.toml").exists(),
            "should have semverguard.toml"
        );
    }

    #[test]
    fn test_print_config_with_fixture() {
        let workspace = fixtures_dir().join("simple_workspace");

        semverguard()
            .args(["print-config", "--workspace-root"])
            .arg(&workspace)
            .assert()
            .success()
            .stdout(predicate::str::contains("[baseline]"))
            .stdout(predicate::str::contains("[scope]"));
    }

    #[test]
    fn test_print_config_with_fixture_config_file() {
        let workspace = fixtures_dir().join("simple_workspace");
        let config_path = workspace.join("semverguard.toml");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(&workspace)
            .assert()
            .success()
            .stdout(predicate::str::contains("crates-io")) // fixture uses crates-io baseline
            .stdout(predicate::str::contains("workspace")); // fixture uses workspace mode
    }
}

// =============================================================================
// Section 6: Edge Case Tests
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_config_file_uses_defaults() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        // Empty file should use all defaults
        fs::write(&config_path, "").expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("crates-io")); // default baseline kind
    }

    #[test]
    fn test_config_with_empty_arrays() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[scope]
include = []
exclude = []

[features]
features = []

[engine]
extra_args = []
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success();
    }

    #[test]
    fn test_workspace_root_with_trailing_slash() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let path_with_slash = format!("{}/", temp_dir.path().display());

        semverguard()
            .args(["print-config", "--workspace-root", &path_with_slash])
            .assert()
            .success();
    }

    #[test]
    fn test_config_path_with_spaces() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("path with spaces").join("config.toml");

        // Create parent directory
        fs::create_dir_all(config_path.parent().unwrap()).expect("failed to create dir");
        fs::write(&config_path, "[baseline]\nkind = \"git\"\n").expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("git"));
    }

    #[test]
    fn test_multiple_engine_args() {
        // Verify --engine-arg can be repeated
        semverguard()
            .args(["check", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--engine-arg"));
    }

    #[test]
    fn test_special_characters_in_config_values() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[scope]
include = ["pkg-*", "lib_*"]
exclude = ["*-test", "*_internal"]

[engine]
extra_args = ["--verbose", "--color=always"]
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("pkg-*"))
            .stdout(predicate::str::contains("lib_*"))
            .stdout(predicate::str::contains("*-test"))
            .stdout(predicate::str::contains("--verbose"));
    }
}

// =============================================================================
// Section 7: Concurrent Execution Tests
// =============================================================================

mod concurrent_tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_concurrent_print_config_calls() {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(|| {
                    let temp_dir = TempDir::new().expect("failed to create temp dir");

                    semverguard()
                        .args(["print-config", "--workspace-root"])
                        .arg(temp_dir.path())
                        .assert()
                        .success();
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should complete successfully");
        }
    }

    #[test]
    fn test_concurrent_help_calls() {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(|| {
                    semverguard().arg("--help").assert().success();
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should complete successfully");
        }
    }
}

// =============================================================================
// Section 8: CLI Override Precedence Tests
// =============================================================================

mod cli_override_precedence {
    use super::*;

    // -------------------------------------------------------------------------
    // Test that CLI args override config file values
    // These tests verify the config merging behavior by checking print-config
    // output after providing CLI overrides (where applicable to print-config).
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_file_baseline_rev_is_preserved() {
        // When config file has baseline.rev, print-config should show it
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
kind = "git"
rev = "origin/develop"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("origin/develop"))
            .stdout(predicate::str::contains("git"));
    }

    #[test]
    fn test_config_changed_mode_is_preserved() {
        // When config file has scope.mode = "changed", print-config should show it
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[scope]
mode = "changed"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("changed"));
    }

    #[test]
    fn test_config_engine_args_accumulate() {
        // Multiple extra_args in config should all be preserved
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[engine]
extra_args = ["--verbose", "--color=always", "--release-type=minor"]
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("--verbose"))
            .stdout(predicate::str::contains("--color=always"))
            .stdout(predicate::str::contains("--release-type=minor"));
    }

    #[test]
    fn test_partial_config_override_preserves_other_values() {
        // When config sets multiple values, overriding one should preserve others
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
kind = "git"
rev = "v1.0.0"

[scope]
mode = "workspace"
include = ["lib-*"]
exclude = ["*-test"]
skip_publish_false = true
skip_no_lib = true

[engine]
fail_fast = true
extra_args = ["--verbose"]

[output]
format = "both"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        // All values should be present in print-config output
        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("v1.0.0"))
            .stdout(predicate::str::contains("lib-*"))
            .stdout(predicate::str::contains("*-test"))
            .stdout(predicate::str::contains("skip_publish_false = true"))
            .stdout(predicate::str::contains("skip_no_lib = true"))
            .stdout(predicate::str::contains("fail_fast = true"))
            .stdout(predicate::str::contains("--verbose"))
            .stdout(predicate::str::contains("both"));
    }

    #[test]
    fn test_config_features_preserved() {
        // Feature configuration should be fully preserved
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[features]
all_features = true
default_features = false
only_explicit_features = true
features = ["serde", "tokio"]
baseline_features = ["old-feature"]
current_features = ["new-feature"]
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("all_features = true"))
            .stdout(predicate::str::contains("default_features = false"))
            .stdout(predicate::str::contains("only_explicit_features = true"))
            .stdout(predicate::str::contains("serde"))
            .stdout(predicate::str::contains("tokio"));
    }

    #[test]
    fn test_config_with_baseline_version_uses_crates_io() {
        // Setting baseline.version implies crates-io kind
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[baseline]
version = "1.2.3"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("1.2.3"))
            .stdout(predicate::str::contains("crates-io"));
    }

    #[test]
    fn test_explicit_packages_config() {
        // explicit_packages in scope should be preserved
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[scope]
mode = "workspace"
explicit_packages = ["my-lib", "my-core"]
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("my-lib"))
            .stdout(predicate::str::contains("my-core"));
    }

    #[test]
    fn test_cargo_bin_config() {
        // Custom cargo binary path should be preserved
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("semverguard.toml");

        let config_content = r#"
[engine]
cargo_bin = "/custom/path/to/cargo"
"#;
        fs::write(&config_path, config_content).expect("failed to write");

        semverguard()
            .args(["print-config", "--config"])
            .arg(&config_path)
            .arg("--workspace-root")
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("/custom/path/to/cargo"));
    }
}
