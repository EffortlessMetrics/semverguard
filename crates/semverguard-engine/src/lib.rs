#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-engine
//!
//! Runs `cargo semver-checks check-release` as a subprocess.

use semverguard_domain::{Result, SemverEngine, SemverguardError};
use semverguard_types::{RequiredBump, SemverCheckOutput, SemverCheckRequest};
use std::path::PathBuf;
use std::process::Command;

/// Engine implementation that shells out to `cargo semver-checks`.
#[derive(Debug, Default)]
pub struct CargoSemverChecksEngine;

impl CargoSemverChecksEngine {
    /// Build the cargo command and arguments for a semver check request.
    ///
    /// Returns `(cargo_binary_path, arguments)` tuple. This is extracted as a
    /// separate function to make command-building testable without executing.
    pub fn build_command(req: &SemverCheckRequest) -> (PathBuf, Vec<String>) {
        // NOTE: The request currently does not carry a configurable cargo binary; we invoke `cargo`
        // from PATH. If you need a toolchain-specific cargo, add a field and plumb it through.
        let cargo = req
            .cargo_bin
            .clone()
            .unwrap_or_else(|| PathBuf::from("cargo"));

        let mut args: Vec<String> = Vec::new();
        args.push("semver-checks".into());
        args.push("check-release".into());

        // Strongly prefer explicit manifest path: makes workspace selection deterministic.
        args.push("--manifest-path".into());
        args.push(req.manifest_path.to_string_lossy().to_string());

        // Baseline selection
        match req.baseline.kind {
            semverguard_types::BaselineKind::CratesIo => {
                if let Some(v) = &req.baseline.version {
                    args.push("--baseline-version".into());
                    args.push(v.clone());
                }
            }
            semverguard_types::BaselineKind::Git => {
                if let Some(r) = &req.baseline.rev {
                    args.push("--baseline-rev".into());
                    args.push(r.clone());
                }
            }
        }

        if let Some(root) = &req.baseline.root {
            args.push("--baseline-root".into());
            args.push(root.to_string_lossy().to_string());
        }
        if let Some(rustdoc) = &req.baseline.rustdoc {
            args.push("--baseline-rustdoc".into());
            args.push(rustdoc.to_string_lossy().to_string());
        }

        // Features
        if req.features.all_features {
            args.push("--all-features".into());
        }
        if req.features.default_features {
            args.push("--default-features".into());
        }
        if req.features.only_explicit_features {
            args.push("--only-explicit-features".into());
        }
        if !req.features.features.is_empty() {
            args.push("--features".into());
            args.push(req.features.features.join(","));
        }
        if !req.features.baseline_features.is_empty() {
            args.push("--baseline-features".into());
            args.push(req.features.baseline_features.join(","));
        }
        if !req.features.current_features.is_empty() {
            args.push("--current-features".into());
            args.push(req.features.current_features.join(","));
        }

        // Extra args
        args.extend(req.extra_args.iter().cloned());

        (cargo, args)
    }
}

impl SemverEngine for CargoSemverChecksEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)> {
        let (bin, args) = Self::build_command(&request);

        let mut cmd = Command::new(&bin);
        cmd.args(&args).current_dir(&request.workspace_root);

        let output = cmd.output().map_err(|e| {
            SemverguardError::Engine(format!("failed to run cargo semver-checks: {e}"))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let exit_code = output.status.code();
        let success = output.status.success();

        // Heuristic: infer required bump from either stream.
        let required = RequiredBump::infer(&stdout).or_else(|| RequiredBump::infer(&stderr));

        let mut command = Vec::with_capacity(1 + args.len());
        command.push(bin.to_string_lossy().to_string());
        command.extend(args);

        Ok((
            command,
            SemverCheckOutput {
                exit_code,
                success,
                stdout,
                stderr,
                required_bump: required,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{BaselineConfig, BaselineKind, FeaturesConfig};
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    // =========================================================================
    // Helper to create a minimal SemverCheckRequest for testing
    // =========================================================================

    fn minimal_request() -> SemverCheckRequest {
        SemverCheckRequest {
            workspace_root: PathBuf::from("/workspace"),
            cargo_bin: None,
            manifest_path: PathBuf::from("/workspace/crates/mylib/Cargo.toml"),
            baseline: BaselineConfig::default(),
            features: FeaturesConfig {
                default_features: false,
                ..FeaturesConfig::default()
            },
            extra_args: vec![],
            timeout: None,
        }
    }

    struct FakeCargo {
        _dir: TempDir,
        path: PathBuf,
    }

    fn write_fake_cargo(exit_code: i32) -> FakeCargo {
        let dir = tempfile::tempdir().expect("tempdir");
        #[cfg(windows)]
        let path = dir.path().join("fake_cargo.cmd");
        #[cfg(not(windows))]
        let path = dir.path().join("fake_cargo.sh");

        #[cfg(windows)]
        let script = format!(
            "@echo off\r\necho Major version bump required.\r\necho stderr line 1>&2\r\nexit /b {exit_code}\r\n"
        );
        #[cfg(not(windows))]
        let script = format!(
            "#!/bin/sh\necho \"Major version bump required.\"\necho \"stderr line\" 1>&2\nexit {exit_code}\n"
        );

        fs::write(&path, script).expect("write fake cargo script");

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("set permissions");
        }

        FakeCargo { _dir: dir, path }
    }

    // =========================================================================
    // Basic command structure tests
    // =========================================================================

    #[test]
    fn test_build_command_uses_cargo_from_path_by_default() {
        let req = minimal_request();
        let (cargo, _args) = CargoSemverChecksEngine::build_command(&req);

        assert_eq!(cargo, PathBuf::from("cargo"));
    }

    #[test]
    fn test_build_command_uses_custom_cargo_bin() {
        let mut req = minimal_request();
        req.cargo_bin = Some(PathBuf::from("/custom/bin/cargo"));

        let (cargo, _args) = CargoSemverChecksEngine::build_command(&req);

        assert_eq!(cargo, PathBuf::from("/custom/bin/cargo"));
    }

    #[test]
    fn test_build_command_starts_with_semver_checks_check_release() {
        let req = minimal_request();
        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.len() >= 2);
        assert_eq!(args[0], "semver-checks");
        assert_eq!(args[1], "check-release");
    }

    #[test]
    fn test_build_command_includes_manifest_path() {
        let req = minimal_request();
        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let manifest_idx = args.iter().position(|a| a == "--manifest-path");
        assert!(manifest_idx.is_some());

        let idx = manifest_idx.unwrap();
        assert!(idx + 1 < args.len());
        assert_eq!(args[idx + 1], "/workspace/crates/mylib/Cargo.toml");
    }

    // =========================================================================
    // Baseline mode tests: CratesIo
    // =========================================================================

    #[test]
    fn test_build_command_baseline_cratesio_without_version() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Should not have --baseline-version if version is None
        assert!(!args.contains(&"--baseline-version".to_string()));
    }

    #[test]
    fn test_build_command_baseline_cratesio_with_version() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: Some("1.2.3".to_string()),
            rev: None,
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let version_idx = args.iter().position(|a| a == "--baseline-version");
        assert!(version_idx.is_some());

        let idx = version_idx.unwrap();
        assert_eq!(args[idx + 1], "1.2.3");
    }

    // =========================================================================
    // Baseline mode tests: Git
    // =========================================================================

    #[test]
    fn test_build_command_baseline_git_without_rev() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: None,
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(!args.contains(&"--baseline-rev".to_string()));
    }

    #[test]
    fn test_build_command_baseline_git_with_rev() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("origin/main".to_string()),
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let rev_idx = args.iter().position(|a| a == "--baseline-rev");
        assert!(rev_idx.is_some());

        let idx = rev_idx.unwrap();
        assert_eq!(args[idx + 1], "origin/main");
    }

    #[test]
    fn test_build_command_baseline_git_with_tag_rev() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("v1.0.0".to_string()),
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let rev_idx = args.iter().position(|a| a == "--baseline-rev");
        assert!(rev_idx.is_some());
        assert_eq!(args[rev_idx.unwrap() + 1], "v1.0.0");
    }

    #[test]
    fn test_build_command_baseline_git_with_sha_rev() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("abc123def456".to_string()),
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let rev_idx = args.iter().position(|a| a == "--baseline-rev");
        assert!(rev_idx.is_some());
        assert_eq!(args[rev_idx.unwrap() + 1], "abc123def456");
    }

    // =========================================================================
    // Baseline mode tests: baseline-root and baseline-rustdoc
    // =========================================================================

    #[test]
    fn test_build_command_baseline_root() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: Some(PathBuf::from("/other/workspace")),
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let root_idx = args.iter().position(|a| a == "--baseline-root");
        assert!(root_idx.is_some());

        let idx = root_idx.unwrap();
        assert_eq!(args[idx + 1], "/other/workspace");
    }

    #[test]
    fn test_build_command_baseline_rustdoc() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: None,
            rustdoc: Some(PathBuf::from("/cached/rustdoc.json")),
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let rustdoc_idx = args.iter().position(|a| a == "--baseline-rustdoc");
        assert!(rustdoc_idx.is_some());

        let idx = rustdoc_idx.unwrap();
        assert_eq!(args[idx + 1], "/cached/rustdoc.json");
    }

    #[test]
    fn test_build_command_baseline_with_both_root_and_rustdoc() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("v2.0.0".to_string()),
            root: Some(PathBuf::from("/baseline/root")),
            rustdoc: Some(PathBuf::from("/baseline/rustdoc.json")),
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--baseline-rev".to_string()));
        assert!(args.contains(&"--baseline-root".to_string()));
        assert!(args.contains(&"--baseline-rustdoc".to_string()));
    }

    // =========================================================================
    // Feature flag handling tests
    // =========================================================================

    #[test]
    fn test_build_command_all_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--all-features".to_string()));
    }

    #[test]
    fn test_build_command_default_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: true,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--default-features".to_string()));
    }

    #[test]
    fn test_build_command_only_explicit_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: true,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--only-explicit-features".to_string()));
    }

    #[test]
    fn test_build_command_features_list() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["serde".to_string(), "async".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let features_idx = args.iter().position(|a| a == "--features");
        assert!(features_idx.is_some());

        let idx = features_idx.unwrap();
        assert_eq!(args[idx + 1], "serde,async");
    }

    #[test]
    fn test_build_command_baseline_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec!["feature1".to_string(), "feature2".to_string()],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let idx = args.iter().position(|a| a == "--baseline-features");
        assert!(idx.is_some());
        assert_eq!(args[idx.unwrap() + 1], "feature1,feature2");
    }

    #[test]
    fn test_build_command_current_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec!["new_feat".to_string()],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let idx = args.iter().position(|a| a == "--current-features");
        assert!(idx.is_some());
        assert_eq!(args[idx.unwrap() + 1], "new_feat");
    }

    #[test]
    fn test_build_command_empty_features_list_not_added() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(!args.contains(&"--features".to_string()));
        assert!(!args.contains(&"--baseline-features".to_string()));
        assert!(!args.contains(&"--current-features".to_string()));
    }

    #[test]
    fn test_build_command_all_feature_flags_together() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: true,
            only_explicit_features: true,
            features: vec!["feat1".to_string()],
            baseline_features: vec!["base_feat".to_string()],
            current_features: vec!["curr_feat".to_string()],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--all-features".to_string()));
        assert!(args.contains(&"--default-features".to_string()));
        assert!(args.contains(&"--only-explicit-features".to_string()));
        assert!(args.contains(&"--features".to_string()));
        assert!(args.contains(&"--baseline-features".to_string()));
        assert!(args.contains(&"--current-features".to_string()));
    }

    // =========================================================================
    // Extra args pass-through tests
    // =========================================================================

    #[test]
    fn test_build_command_extra_args_empty() {
        let mut req = minimal_request();
        req.extra_args = vec![];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Just verify no crash and basic structure is intact
        assert!(args.len() >= 4); // semver-checks, check-release, --manifest-path, path
    }

    #[test]
    fn test_build_command_extra_args_single() {
        let mut req = minimal_request();
        req.extra_args = vec!["--verbose".to_string()];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--verbose".to_string()));
        // Extra args should be at the end
        assert_eq!(args.last(), Some(&"--verbose".to_string()));
    }

    #[test]
    fn test_build_command_extra_args_multiple() {
        let mut req = minimal_request();
        req.extra_args = vec![
            "--verbose".to_string(),
            "--release-type".to_string(),
            "minor".to_string(),
        ];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // All extra args should be present and in order at the end
        let len = args.len();
        assert!(len >= 3);
        assert_eq!(args[len - 3], "--verbose");
        assert_eq!(args[len - 2], "--release-type");
        assert_eq!(args[len - 1], "minor");
    }

    #[test]
    fn test_build_command_extra_args_appear_after_feature_flags() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            ..FeaturesConfig::default()
        };
        req.extra_args = vec!["--custom-flag".to_string()];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let all_features_idx = args.iter().position(|a| a == "--all-features").unwrap();
        let custom_idx = args.iter().position(|a| a == "--custom-flag").unwrap();

        assert!(custom_idx > all_features_idx);
    }

    // =========================================================================
    // RequiredBump inference tests
    // =========================================================================

    #[test]
    fn test_required_bump_infer_major() {
        let text = "Error: major version bump required due to breaking changes";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_required_bump_infer_major_case_insensitive() {
        let text = "MAJOR VERSION BUMP REQUIRED";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_required_bump_infer_minor() {
        let text = "Warning: minor version bump required for new features";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Minor));
    }

    #[test]
    fn test_required_bump_infer_patch() {
        let text = "Note: patch version bump required for bug fixes";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Patch));
    }

    #[test]
    fn test_required_bump_infer_unknown_bump_required() {
        let text = "Some kind of bump required for changes";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Unknown));
    }

    #[test]
    fn test_required_bump_infer_none_when_no_match() {
        let text = "All checks passed successfully";
        let bump = RequiredBump::infer(text);
        assert!(bump.is_none());
    }

    #[test]
    fn test_required_bump_infer_empty_string() {
        let text = "";
        let bump = RequiredBump::infer(text);
        assert!(bump.is_none());
    }

    #[test]
    fn test_required_bump_infer_priority_major_over_minor() {
        // If output contains multiple keywords, major should match first
        // since it's checked first in the implementation
        let text = "major version bump required, minor changes also present";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    // =========================================================================
    // Complex scenario tests
    // =========================================================================

    #[test]
    fn test_build_command_full_configuration() {
        let req = SemverCheckRequest {
            workspace_root: PathBuf::from("/my/workspace"),
            cargo_bin: Some(PathBuf::from("/usr/local/bin/cargo")),
            manifest_path: PathBuf::from("/my/workspace/crates/core/Cargo.toml"),
            baseline: BaselineConfig {
                kind: BaselineKind::Git,
                version: None,
                rev: Some("v1.5.0".to_string()),
                root: Some(PathBuf::from("/baseline/workspace")),
                rustdoc: None,
                ..Default::default()
            },
            features: FeaturesConfig {
                all_features: false,
                default_features: false,
                only_explicit_features: true,
                features: vec!["serde".to_string(), "tokio".to_string()],
                baseline_features: vec![],
                current_features: vec![],
            },
            extra_args: vec!["--release-type".to_string(), "minor".to_string()],
            timeout: None,
        };

        let (cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Verify cargo binary
        assert_eq!(cargo, PathBuf::from("/usr/local/bin/cargo"));

        // Verify basic structure
        assert_eq!(args[0], "semver-checks");
        assert_eq!(args[1], "check-release");

        // Verify manifest path
        assert!(args.contains(&"--manifest-path".to_string()));
        assert!(args.contains(&"/my/workspace/crates/core/Cargo.toml".to_string()));

        // Verify baseline
        assert!(args.contains(&"--baseline-rev".to_string()));
        assert!(args.contains(&"v1.5.0".to_string()));
        assert!(args.contains(&"--baseline-root".to_string()));
        assert!(args.contains(&"/baseline/workspace".to_string()));

        // Verify features
        assert!(args.contains(&"--only-explicit-features".to_string()));
        assert!(args.contains(&"--features".to_string()));
        assert!(args.contains(&"serde,tokio".to_string()));

        // Verify extra args at end
        let len = args.len();
        assert_eq!(args[len - 2], "--release-type");
        assert_eq!(args[len - 1], "minor");
    }

    #[test]
    fn test_build_command_cratesio_with_version_ignores_rev() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: Some("2.0.0".to_string()),
            rev: Some("should-be-ignored".to_string()), // This should be ignored
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Should have --baseline-version
        assert!(args.contains(&"--baseline-version".to_string()));
        assert!(args.contains(&"2.0.0".to_string()));

        // Should NOT have --baseline-rev (because kind is CratesIo)
        assert!(!args.contains(&"--baseline-rev".to_string()));
    }

    #[test]
    fn test_build_command_git_with_rev_ignores_version() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: Some("should-be-ignored".to_string()), // This should be ignored
            rev: Some("main".to_string()),
            root: None,
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Should have --baseline-rev
        assert!(args.contains(&"--baseline-rev".to_string()));
        assert!(args.contains(&"main".to_string()));

        // Should NOT have --baseline-version (because kind is Git)
        assert!(!args.contains(&"--baseline-version".to_string()));
    }

    #[test]
    fn test_build_command_single_feature() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["single".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[idx + 1], "single"); // No comma for single feature
    }

    // =========================================================================
    // Argument order tests
    // =========================================================================

    #[test]
    fn test_build_command_argument_order() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("main".to_string()),
            root: None,
            rustdoc: None,
            ..Default::default()
        };
        req.features = FeaturesConfig {
            all_features: true,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };
        req.extra_args = vec!["--extra".to_string()];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Find positions
        let semver_checks_pos = args.iter().position(|a| a == "semver-checks").unwrap();
        let check_release_pos = args.iter().position(|a| a == "check-release").unwrap();
        let manifest_pos = args.iter().position(|a| a == "--manifest-path").unwrap();
        let baseline_rev_pos = args.iter().position(|a| a == "--baseline-rev").unwrap();
        let all_features_pos = args.iter().position(|a| a == "--all-features").unwrap();
        let extra_pos = args.iter().position(|a| a == "--extra").unwrap();

        // Verify order: semver-checks < check-release < manifest-path < baseline < features < extra
        assert!(semver_checks_pos < check_release_pos);
        assert!(check_release_pos < manifest_pos);
        assert!(manifest_pos < baseline_rev_pos);
        assert!(baseline_rev_pos < all_features_pos);
        assert!(all_features_pos < extra_pos);
    }

    // =========================================================================
    // Manifest path tests with different path styles
    // =========================================================================

    #[test]
    fn test_build_command_manifest_path_with_spaces() {
        let mut req = minimal_request();
        req.manifest_path = PathBuf::from("/path/with spaces/Cargo.toml");

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let manifest_idx = args.iter().position(|a| a == "--manifest-path").unwrap();
        assert_eq!(args[manifest_idx + 1], "/path/with spaces/Cargo.toml");
    }

    #[test]
    fn test_build_command_manifest_path_windows_style() {
        let mut req = minimal_request();
        req.manifest_path = PathBuf::from("C:\\Users\\dev\\project\\Cargo.toml");

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let manifest_idx = args.iter().position(|a| a == "--manifest-path").unwrap();
        // PathBuf::to_string_lossy will preserve the path format
        assert!(args[manifest_idx + 1].contains("Cargo.toml"));
    }

    #[test]
    fn test_build_command_manifest_path_relative() {
        let mut req = minimal_request();
        req.manifest_path = PathBuf::from("./crates/mylib/Cargo.toml");

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let manifest_idx = args.iter().position(|a| a == "--manifest-path").unwrap();
        assert_eq!(args[manifest_idx + 1], "./crates/mylib/Cargo.toml");
    }

    // =========================================================================
    // Feature combinations edge cases
    // =========================================================================

    #[test]
    fn test_build_command_features_with_special_characters() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![
                "feature-with-dash".to_string(),
                "feature_with_underscore".to_string(),
            ],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[idx + 1], "feature-with-dash,feature_with_underscore");
    }

    #[test]
    fn test_build_command_many_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
                "e".to_string(),
            ],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[idx + 1], "a,b,c,d,e");
    }

    #[test]
    fn test_build_command_different_baseline_and_current_features() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec!["old_feat".to_string()],
            current_features: vec!["new_feat".to_string()],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let baseline_idx = args
            .iter()
            .position(|a| a == "--baseline-features")
            .unwrap();
        let current_idx = args.iter().position(|a| a == "--current-features").unwrap();

        assert_eq!(args[baseline_idx + 1], "old_feat");
        assert_eq!(args[current_idx + 1], "new_feat");

        // Baseline features should come before current features
        assert!(baseline_idx < current_idx);
    }

    // =========================================================================
    // Baseline root and rustdoc path tests
    // =========================================================================

    #[test]
    fn test_build_command_baseline_root_with_spaces() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("main".to_string()),
            root: Some(PathBuf::from("/path/with spaces/baseline")),
            rustdoc: None,
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let root_idx = args.iter().position(|a| a == "--baseline-root").unwrap();
        assert_eq!(args[root_idx + 1], "/path/with spaces/baseline");
    }

    #[test]
    fn test_build_command_baseline_rustdoc_json_path() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: None,
            rustdoc: Some(PathBuf::from("/cache/crate-1.0.0-rustdoc.json")),
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let rustdoc_idx = args.iter().position(|a| a == "--baseline-rustdoc").unwrap();
        assert_eq!(args[rustdoc_idx + 1], "/cache/crate-1.0.0-rustdoc.json");
    }

    // =========================================================================
    // Extra args edge cases
    // =========================================================================

    #[test]
    fn test_build_command_extra_args_with_equals() {
        let mut req = minimal_request();
        req.extra_args = vec!["--target=x86_64-unknown-linux-gnu".to_string()];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--target=x86_64-unknown-linux-gnu".to_string()));
    }

    #[test]
    fn test_build_command_extra_args_with_quotes() {
        let mut req = minimal_request();
        req.extra_args = vec!["--message=\"Hello World\"".to_string()];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        assert!(args.contains(&"--message=\"Hello World\"".to_string()));
    }

    #[test]
    fn test_build_command_extra_args_duplicate_flags() {
        // Some tools allow duplicate flags; we should pass them through
        let mut req = minimal_request();
        req.extra_args = vec![
            "--include".to_string(),
            "path1".to_string(),
            "--include".to_string(),
            "path2".to_string(),
        ];

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let include_count = args.iter().filter(|a| *a == "--include").count();
        assert_eq!(include_count, 2);
    }

    // =========================================================================
    // SemverCheckOutput construction tests
    // =========================================================================

    #[test]
    fn test_semver_check_output_success_with_no_bump() {
        let output = SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "No breaking changes detected.".to_string(),
            stderr: String::new(),
            required_bump: None,
        };

        assert!(output.success);
        assert_eq!(output.exit_code, Some(0));
        assert!(output.required_bump.is_none());
    }

    #[test]
    fn test_semver_check_output_failure_with_major_bump() {
        let output = SemverCheckOutput {
            exit_code: Some(1),
            success: false,
            stdout: "Breaking changes detected. Major version bump required.".to_string(),
            stderr: String::new(),
            required_bump: Some(RequiredBump::Major),
        };

        assert!(!output.success);
        assert_eq!(output.exit_code, Some(1));
        assert_eq!(output.required_bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_semver_check_output_failure_with_minor_bump() {
        let output = SemverCheckOutput {
            exit_code: Some(1),
            success: false,
            stdout: "New public items detected. Minor version bump required.".to_string(),
            stderr: String::new(),
            required_bump: Some(RequiredBump::Minor),
        };

        assert!(!output.success);
        assert_eq!(output.required_bump, Some(RequiredBump::Minor));
    }

    #[test]
    fn test_semver_check_output_no_exit_code() {
        // Simulate process killed by signal (no exit code)
        let output = SemverCheckOutput {
            exit_code: None,
            success: false,
            stdout: String::new(),
            stderr: "Process terminated".to_string(),
            required_bump: None,
        };

        assert!(!output.success);
        assert!(output.exit_code.is_none());
    }

    #[test]
    fn test_semver_check_output_exit_code_2() {
        // Exit code 2 typically means config/usage error
        let output = SemverCheckOutput {
            exit_code: Some(2),
            success: false,
            stdout: String::new(),
            stderr: "Invalid configuration".to_string(),
            required_bump: None,
        };

        assert!(!output.success);
        assert_eq!(output.exit_code, Some(2));
    }

    // =========================================================================
    // RequiredBump inference additional tests
    // =========================================================================

    #[test]
    fn test_required_bump_infer_from_multiline_output() {
        let text = r#"
Checking crate `mylib`...
Found 3 breaking changes:
  - removed function `foo`
  - changed signature of `bar`
  - removed struct `Baz`

Major version bump required.
"#;
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_required_bump_infer_words_scattered() {
        // Test when the key words are scattered in the text
        let text = "The changes are major in nature and a version bump is required";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_required_bump_infer_partial_keywords() {
        // "bump required" phrase without specifying level triggers Unknown
        let text = "A version bump required for these changes";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Unknown));
    }

    #[test]
    fn test_required_bump_infer_stderr_style_message() {
        let text = "error: Breaking API changes detected. A major bump is required to proceed.";
        let bump = RequiredBump::infer(text);
        assert_eq!(bump, Some(RequiredBump::Major));
    }

    // =========================================================================
    // Command verification tests - exact argument structure
    // =========================================================================

    #[test]
    fn test_build_command_minimal_args_count() {
        let req = minimal_request();
        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Minimum args: semver-checks, check-release, --manifest-path, <path>
        assert!(args.len() >= 4);
    }

    #[test]
    fn test_build_command_no_duplicate_flags() {
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: true,
            only_explicit_features: true,
            features: vec!["feat".to_string()],
            baseline_features: vec!["base".to_string()],
            current_features: vec!["curr".to_string()],
        };
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("main".to_string()),
            root: Some(PathBuf::from("/root")),
            rustdoc: Some(PathBuf::from("/doc")),
            ..Default::default()
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Check that flags appear exactly once
        let flag_counts = |flag: &str| args.iter().filter(|a| *a == flag).count();

        assert_eq!(flag_counts("--manifest-path"), 1);
        assert_eq!(flag_counts("--baseline-rev"), 1);
        assert_eq!(flag_counts("--baseline-root"), 1);
        assert_eq!(flag_counts("--baseline-rustdoc"), 1);
        assert_eq!(flag_counts("--all-features"), 1);
        assert_eq!(flag_counts("--default-features"), 1);
        assert_eq!(flag_counts("--only-explicit-features"), 1);
        assert_eq!(flag_counts("--features"), 1);
        assert_eq!(flag_counts("--baseline-features"), 1);
        assert_eq!(flag_counts("--current-features"), 1);
    }

    #[test]
    fn test_build_command_flag_value_pairs_are_adjacent() {
        let mut req = minimal_request();
        req.baseline = BaselineConfig {
            kind: BaselineKind::Git,
            version: None,
            rev: Some("v1.0.0".to_string()),
            root: Some(PathBuf::from("/baseline")),
            rustdoc: None,
            ..Default::default()
        };
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["serde".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Verify flag-value pairs are adjacent
        let flags = [
            "--manifest-path",
            "--baseline-rev",
            "--baseline-root",
            "--features",
        ];

        for flag in flags {
            let idx = args
                .iter()
                .position(|a| a == flag)
                .expect("flag should exist");
            assert!(idx + 1 < args.len());
            // Value should not be another flag (starts with --)
            assert!(!args[idx + 1].starts_with("--"));
        }
    }

    // =========================================================================
    // CargoSemverChecksEngine struct tests
    // =========================================================================

    #[test]
    fn test_cargo_semver_checks_engine_default() {
        let engine = CargoSemverChecksEngine::default();
        // Just verify it can be created
        assert!(format!("{:?}", engine).contains("CargoSemverChecksEngine"));
    }

    #[test]
    fn test_cargo_semver_checks_engine_debug() {
        let engine = CargoSemverChecksEngine;
        let debug_str = format!("{:?}", engine);
        assert!(debug_str.contains("CargoSemverChecksEngine"));
    }

    // =========================================================================
    // Error path tests - subprocess failures
    // =========================================================================

    #[test]
    fn test_check_cargo_not_found() {
        // Test behavior when cargo binary doesn't exist
        let engine = CargoSemverChecksEngine;
        let mut req = minimal_request();
        req.cargo_bin = Some(PathBuf::from("/nonexistent/path/to/cargo"));

        let result = engine.check(req);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        // Should indicate it failed to run the command
        assert!(err_msg.contains("failed to run cargo semver-checks"));
    }

    #[test]
    fn test_check_invalid_cargo_path() {
        // Test with a path that exists but isn't executable (use a directory)
        let engine = CargoSemverChecksEngine;
        let mut req = minimal_request();
        // Use temp dir as "cargo" - it exists but isn't executable
        req.cargo_bin = Some(std::env::temp_dir());

        let result = engine.check(req);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("failed to run cargo semver-checks"));
    }

    #[test]
    fn test_check_success_with_fake_cargo() {
        let fake = write_fake_cargo(0);
        let engine = CargoSemverChecksEngine;
        let mut req = minimal_request();
        req.cargo_bin = Some(fake.path.clone());
        req.workspace_root = fake._dir.path().to_path_buf();

        let (command, output) = engine.check(req).expect("fake cargo should run");

        assert_eq!(command[0], fake.path.to_string_lossy().to_string());
        assert!(command.contains(&"semver-checks".to_string()));
        assert!(command.contains(&"check-release".to_string()));
        assert_eq!(output.exit_code, Some(0));
        assert!(output.success);
        assert!(output.stdout.contains("Major version bump required."));
        assert!(output.stderr.contains("stderr line"));
        assert_eq!(output.required_bump, Some(RequiredBump::Major));
    }

    #[test]
    fn test_check_nonexistent_manifest() {
        let fake = write_fake_cargo(1);
        let engine = CargoSemverChecksEngine;
        let mut req = minimal_request();
        req.cargo_bin = Some(fake.path.clone());
        req.workspace_root = fake._dir.path().to_path_buf();
        req.manifest_path = PathBuf::from("/definitely/does/not/exist/Cargo.toml");

        let (_command, output) = engine.check(req).expect("fake cargo should run");
        assert!(!output.success);
    }

    // =========================================================================
    // Feature flag conflict/edge case tests
    // =========================================================================

    #[test]
    fn test_build_command_all_features_with_explicit_features() {
        // When both all_features and explicit features are set
        // cargo-semver-checks allows this (all_features wins for actual features)
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: false,
            only_explicit_features: false,
            features: vec!["serde".to_string(), "tokio".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Both should be present - cargo-semver-checks handles the semantics
        assert!(args.contains(&"--all-features".to_string()));
        assert!(args.contains(&"--features".to_string()));
        let feat_idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[feat_idx + 1], "serde,tokio");
    }

    #[test]
    fn test_build_command_default_features_false_with_all_features() {
        // Edge case: both default_features=true and all_features=true
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: true,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Both flags should be present
        assert!(args.contains(&"--all-features".to_string()));
        assert!(args.contains(&"--default-features".to_string()));
    }

    #[test]
    fn test_build_command_only_explicit_with_default_features() {
        // Edge case: only_explicit and default_features both true
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: true,
            only_explicit_features: true,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Both flags should be present - cargo-semver-checks resolves conflict
        assert!(args.contains(&"--default-features".to_string()));
        assert!(args.contains(&"--only-explicit-features".to_string()));
    }

    #[test]
    fn test_build_command_empty_feature_name_in_list() {
        // Edge case: features list contains empty string
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["foo".to_string(), "".to_string(), "bar".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // Empty string is preserved in comma-separated list (cargo will handle)
        let feat_idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[feat_idx + 1], "foo,,bar");
    }

    #[test]
    fn test_build_command_feature_names_with_special_chars() {
        // Feature names with unusual but valid characters
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec![
                "feature/name".to_string(), // forward slash
                "feature.name".to_string(), // dot
                "feature:name".to_string(), // colon
                "feature+name".to_string(), // plus
            ],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let feat_idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(
            args[feat_idx + 1],
            "feature/name,feature.name,feature:name,feature+name"
        );
    }

    #[test]
    fn test_build_command_all_three_feature_modes_enabled() {
        // All feature boolean flags enabled simultaneously
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: true,
            default_features: true,
            only_explicit_features: true,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        // All three flags should be present
        assert!(args.contains(&"--all-features".to_string()));
        assert!(args.contains(&"--default-features".to_string()));
        assert!(args.contains(&"--only-explicit-features".to_string()));
    }

    #[test]
    fn test_build_command_whitespace_in_feature_name() {
        // Feature name with whitespace (unusual but we pass through)
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["feature with spaces".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let feat_idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[feat_idx + 1], "feature with spaces");
    }

    #[test]
    fn test_build_command_unicode_in_feature_name() {
        // Feature name with unicode characters
        let mut req = minimal_request();
        req.features = FeaturesConfig {
            all_features: false,
            default_features: false,
            only_explicit_features: false,
            features: vec!["功能".to_string(), "фича".to_string()],
            baseline_features: vec![],
            current_features: vec![],
        };

        let (_cargo, args) = CargoSemverChecksEngine::build_command(&req);

        let feat_idx = args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(args[feat_idx + 1], "功能,фича");
    }
}
