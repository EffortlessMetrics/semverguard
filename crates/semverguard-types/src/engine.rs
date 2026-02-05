use crate::{BaselineConfig, FeaturesConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Request to run a SemVer check for a package.
#[derive(Debug, Clone)]
pub struct SemverCheckRequest {
    /// Workspace root where cargo should be executed.
    pub workspace_root: PathBuf,

    /// Which `cargo` binary to run. If unset, uses `"cargo"` from PATH.
    pub cargo_bin: Option<PathBuf>,

    /// Path to the package manifest to check.
    pub manifest_path: PathBuf,

    /// Baseline selection options.
    pub baseline: BaselineConfig,

    /// Feature-selection options.
    pub features: FeaturesConfig,

    /// Extra args appended to the engine invocation.
    pub extra_args: Vec<String>,

    /// Optional timeout.
    pub timeout: Option<Duration>,
}

/// Output from running a SemVer check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemverCheckOutput {
    /// Exit code from the process.
    pub exit_code: Option<i32>,
    /// True if the process exited successfully.
    pub success: bool,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Best-effort required bump inference from output.
    pub required_bump: Option<RequiredBump>,
}

/// Required version bump inferred from the tool output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequiredBump {
    /// Patch bump required.
    Patch,
    /// Minor bump required.
    Minor,
    /// Major bump required.
    Major,
    /// Could not infer required bump.
    Unknown,
}

impl RequiredBump {
    /// Infer a bump from cargo-semver-checks output (best-effort heuristic).
    pub fn infer(text: &str) -> Option<Self> {
        let hay = text.to_ascii_lowercase();
        // cargo-semver-checks output tends to include phrases like:
        // "major version bump required" / "minor version bump required"
        if hay.contains("major") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Major);
        }
        if hay.contains("minor") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Minor);
        }
        if hay.contains("patch") && hay.contains("bump") && hay.contains("required") {
            return Some(Self::Patch);
        }
        if hay.contains("bump required") {
            return Some(Self::Unknown);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // RequiredBump ordering tests
    // =========================================================================

    #[test]
    fn test_required_bump_variants() {
        // Ensure all variants are accessible
        let patch = RequiredBump::Patch;
        let minor = RequiredBump::Minor;
        let major = RequiredBump::Major;
        let unknown = RequiredBump::Unknown;

        assert!(matches!(patch, RequiredBump::Patch));
        assert!(matches!(minor, RequiredBump::Minor));
        assert!(matches!(major, RequiredBump::Major));
        assert!(matches!(unknown, RequiredBump::Unknown));
    }

    #[test]
    fn test_required_bump_clone() {
        let bump = RequiredBump::Major;
        let cloned = bump;
        assert!(matches!(cloned, RequiredBump::Major));
    }

    #[test]
    fn test_required_bump_copy() {
        let bump = RequiredBump::Minor;
        let copied: RequiredBump = bump;
        // bump should still be valid since RequiredBump is Copy
        assert!(matches!(bump, RequiredBump::Minor));
        assert!(matches!(copied, RequiredBump::Minor));
    }

    // =========================================================================
    // RequiredBump::infer heuristic tests
    // =========================================================================

    #[test]
    fn test_infer_major_bump() {
        let text = "A major version bump is required due to breaking changes";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Major)
        ));

        let text = "MAJOR BUMP REQUIRED";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Major)
        ));

        // All three words must be present
        let text = "major bump required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Major)
        ));
    }

    #[test]
    fn test_infer_minor_bump() {
        let text = "A minor version bump is required for new features";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Minor)
        ));

        let text = "MINOR BUMP REQUIRED";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Minor)
        ));

        // All three words must be present
        let text = "minor bump required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Minor)
        ));
    }

    #[test]
    fn test_infer_patch_bump() {
        let text = "A patch version bump is required for bug fixes";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Patch)
        ));

        let text = "PATCH BUMP REQUIRED";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Patch)
        ));

        // All three words must be present
        let text = "patch bump required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Patch)
        ));
    }

    #[test]
    fn test_infer_unknown_bump() {
        // Generic "bump required" without specific level
        let text = "A version bump required for this change";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Unknown)
        ));
    }

    #[test]
    fn test_infer_no_match() {
        let text = "All checks passed successfully";
        assert!(RequiredBump::infer(text).is_none());

        let text = "";
        assert!(RequiredBump::infer(text).is_none());

        let text = "bump major version"; // Missing "required"
        assert!(RequiredBump::infer(text).is_none());

        let text = "major changes detected"; // Missing "bump" and "required"
        assert!(RequiredBump::infer(text).is_none());
    }

    #[test]
    fn test_infer_case_insensitive() {
        let text = "MaJoR BuMp ReQuIrEd";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Major)
        ));

        let text = "minor BUMP required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Minor)
        ));
    }

    #[test]
    fn test_infer_words_in_different_positions() {
        // Words don't have to be adjacent
        let text = "Due to breaking API changes, a major version bump is required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Major)
        ));

        let text = "bump to minor version is required";
        assert!(matches!(
            RequiredBump::infer(text),
            Some(RequiredBump::Minor)
        ));
    }

    // =========================================================================
    // SemverCheckRequest building tests
    // =========================================================================

    #[test]
    fn test_semver_check_request_new() {
        let request = SemverCheckRequest {
            workspace_root: PathBuf::from("/workspace"),
            cargo_bin: None,
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            baseline: BaselineConfig::default(),
            features: FeaturesConfig::default(),
            extra_args: vec![],
            timeout: None,
        };

        assert_eq!(request.workspace_root, PathBuf::from("/workspace"));
        assert!(request.cargo_bin.is_none());
        assert_eq!(
            request.manifest_path,
            PathBuf::from("/workspace/Cargo.toml")
        );
        assert!(request.extra_args.is_empty());
        assert!(request.timeout.is_none());
    }

    #[test]
    fn test_semver_check_request_with_all_fields() {
        let request = SemverCheckRequest {
            workspace_root: PathBuf::from("/my/workspace"),
            cargo_bin: Some(PathBuf::from("/custom/cargo")),
            manifest_path: PathBuf::from("/my/workspace/crates/mylib/Cargo.toml"),
            baseline: BaselineConfig {
                kind: crate::BaselineKind::Git,
                version: None,
                rev: Some("v1.0.0".to_string()),
                root: None,
                rustdoc: None,
                on_error: crate::BaselineErrorBehavior::default(),
            },
            features: FeaturesConfig {
                all_features: true,
                default_features: false,
                only_explicit_features: false,
                features: vec!["serde".to_string()],
                baseline_features: vec![],
                current_features: vec![],
            },
            extra_args: vec!["--verbose".to_string()],
            timeout: Some(Duration::from_secs(300)),
        };

        assert_eq!(request.workspace_root, PathBuf::from("/my/workspace"));
        assert_eq!(request.cargo_bin, Some(PathBuf::from("/custom/cargo")));
        assert!(request.features.all_features);
        assert!(!request.features.default_features);
        assert_eq!(request.features.features, vec!["serde"]);
        assert_eq!(request.extra_args, vec!["--verbose"]);
        assert_eq!(request.timeout, Some(Duration::from_secs(300)));
    }

    #[test]
    fn test_semver_check_request_clone() {
        let request = SemverCheckRequest {
            workspace_root: PathBuf::from("/workspace"),
            cargo_bin: Some(PathBuf::from("/cargo")),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            baseline: BaselineConfig::default(),
            features: FeaturesConfig::default(),
            extra_args: vec!["--arg".to_string()],
            timeout: Some(Duration::from_secs(60)),
        };

        let cloned = request.clone();
        assert_eq!(cloned.workspace_root, request.workspace_root);
        assert_eq!(cloned.cargo_bin, request.cargo_bin);
        assert_eq!(cloned.extra_args, request.extra_args);
        assert_eq!(cloned.timeout, request.timeout);
    }

    // =========================================================================
    // SemverCheckOutput tests
    // =========================================================================

    #[test]
    fn test_semver_check_output_success() {
        let output = SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "All checks passed".to_string(),
            stderr: String::new(),
            required_bump: None,
        };

        assert_eq!(output.exit_code, Some(0));
        assert!(output.success);
        assert_eq!(output.stdout, "All checks passed");
        assert!(output.stderr.is_empty());
        assert!(output.required_bump.is_none());
    }

    #[test]
    fn test_semver_check_output_failure() {
        let output = SemverCheckOutput {
            exit_code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "Major bump required".to_string(),
            required_bump: Some(RequiredBump::Major),
        };

        assert_eq!(output.exit_code, Some(1));
        assert!(!output.success);
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, "Major bump required");
        assert!(matches!(output.required_bump, Some(RequiredBump::Major)));
    }

    #[test]
    fn test_semver_check_output_json_roundtrip() {
        let original = SemverCheckOutput {
            exit_code: Some(1),
            success: false,
            stdout: "stdout content".to_string(),
            stderr: "stderr content".to_string(),
            required_bump: Some(RequiredBump::Minor),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SemverCheckOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.exit_code, original.exit_code);
        assert_eq!(deserialized.success, original.success);
        assert_eq!(deserialized.stdout, original.stdout);
        assert_eq!(deserialized.stderr, original.stderr);
        assert!(matches!(
            deserialized.required_bump,
            Some(RequiredBump::Minor)
        ));
    }

    #[test]
    fn test_semver_check_output_json_roundtrip_none_values() {
        let original = SemverCheckOutput {
            exit_code: None, // Process killed by signal
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            required_bump: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SemverCheckOutput = serde_json::from_str(&json).unwrap();

        assert!(deserialized.exit_code.is_none());
        assert!(!deserialized.success);
        assert!(deserialized.required_bump.is_none());
    }

    // =========================================================================
    // RequiredBump serde tests
    // =========================================================================

    #[test]
    fn test_required_bump_json_roundtrip() {
        for bump in [
            RequiredBump::Patch,
            RequiredBump::Minor,
            RequiredBump::Major,
            RequiredBump::Unknown,
        ] {
            let json = serde_json::to_string(&bump).unwrap();
            let deserialized: RequiredBump = serde_json::from_str(&json).unwrap();
            assert!(matches!(
                (bump, deserialized),
                (RequiredBump::Patch, RequiredBump::Patch)
                    | (RequiredBump::Minor, RequiredBump::Minor)
                    | (RequiredBump::Major, RequiredBump::Major)
                    | (RequiredBump::Unknown, RequiredBump::Unknown)
            ));
        }
    }

    #[test]
    fn test_required_bump_json_kebab_case() {
        // Verify kebab-case serialization
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

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_semver_check_output_empty_strings() {
        let output = SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            required_bump: None,
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: SemverCheckOutput = serde_json::from_str(&json).unwrap();

        assert!(deserialized.stdout.is_empty());
        assert!(deserialized.stderr.is_empty());
    }

    #[test]
    fn test_semver_check_output_multiline_strings() {
        let output = SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "line1\nline2\nline3".to_string(),
            stderr: "error1\nerror2".to_string(),
            required_bump: None,
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: SemverCheckOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.stdout, "line1\nline2\nline3");
        assert_eq!(deserialized.stderr, "error1\nerror2");
    }

    #[test]
    fn test_semver_check_output_special_characters() {
        let output = SemverCheckOutput {
            exit_code: Some(0),
            success: true,
            stdout: "Special chars: \"quotes\", \\backslash, \ttab".to_string(),
            stderr: String::new(),
            required_bump: None,
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: SemverCheckOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.stdout,
            "Special chars: \"quotes\", \\backslash, \ttab"
        );
    }
}
