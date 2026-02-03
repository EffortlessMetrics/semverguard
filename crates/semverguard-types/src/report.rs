use crate::engine::{RequiredBump, SemverCheckOutput};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level report for a semverguard run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    /// Semverguard version (from CLI crate).
    pub semverguard_version: String,
    /// ISO8601 start timestamp (UTC).
    pub started_at: String,
    /// ISO8601 finish timestamp (UTC).
    pub finished_at: String,
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// One entry per selected package.
    pub packages: Vec<PackageReport>,
    /// Summary counts and overall status.
    pub summary: Summary,
}

/// Summary of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Total number of package entries in the report (including skipped).
    pub total: usize,
    /// Number of packages that passed.
    pub passed: usize,
    /// Number of packages that failed.
    pub failed: usize,
    /// Number of packages that were skipped.
    pub skipped: usize,
}

impl Summary {
    /// True if no packages failed.
    pub fn overall_success(&self) -> bool {
        self.failed == 0
    }
}

/// Per-package result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReport {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: String,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Status classification.
    pub status: PackageStatus,
    /// Why this package was skipped (when status is `skipped`).
    pub skip_reason: Option<String>,
    /// How long the package check took (milliseconds).
    pub duration_ms: u128,
    /// Command executed by the engine (argv form).
    pub command: Vec<String>,
    /// Captured engine output (when executed).
    pub engine: Option<SemverCheckOutput>,
    /// Best-effort required bump inference.
    pub inferred_required_bump: Option<RequiredBump>,
}

/// Result classification for a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageStatus {
    /// Passed the SemVer check.
    Passed,
    /// Failed the SemVer check.
    Failed,
    /// Skipped by semverguard policy/scoping.
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // PackageStatus tests
    // =========================================================================

    #[test]
    fn test_package_status_variants() {
        let passed = PackageStatus::Passed;
        let failed = PackageStatus::Failed;
        let skipped = PackageStatus::Skipped;

        assert_eq!(passed, PackageStatus::Passed);
        assert_eq!(failed, PackageStatus::Failed);
        assert_eq!(skipped, PackageStatus::Skipped);
    }

    #[test]
    fn test_package_status_copy() {
        let status = PackageStatus::Passed;
        let copied: PackageStatus = status;
        // status is still valid since PackageStatus is Copy
        assert_eq!(status, PackageStatus::Passed);
        assert_eq!(copied, PackageStatus::Passed);
    }

    #[test]
    fn test_package_status_json_roundtrip() {
        for status in [
            PackageStatus::Passed,
            PackageStatus::Failed,
            PackageStatus::Skipped,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: PackageStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_package_status_json_kebab_case() {
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

    // =========================================================================
    // Summary tests
    // =========================================================================

    #[test]
    fn test_summary_overall_success_no_failures() {
        let summary = Summary {
            total: 5,
            passed: 3,
            failed: 0,
            skipped: 2,
        };
        assert!(summary.overall_success());
    }

    #[test]
    fn test_summary_overall_success_with_failures() {
        let summary = Summary {
            total: 5,
            passed: 2,
            failed: 1,
            skipped: 2,
        };
        assert!(!summary.overall_success());
    }

    #[test]
    fn test_summary_overall_success_all_passed() {
        let summary = Summary {
            total: 3,
            passed: 3,
            failed: 0,
            skipped: 0,
        };
        assert!(summary.overall_success());
    }

    #[test]
    fn test_summary_overall_success_all_skipped() {
        let summary = Summary {
            total: 3,
            passed: 0,
            failed: 0,
            skipped: 3,
        };
        assert!(summary.overall_success());
    }

    #[test]
    fn test_summary_overall_success_empty() {
        let summary = Summary {
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
        };
        assert!(summary.overall_success());
    }

    #[test]
    fn test_summary_json_roundtrip() {
        let original = Summary {
            total: 10,
            passed: 5,
            failed: 2,
            skipped: 3,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Summary = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total, 10);
        assert_eq!(deserialized.passed, 5);
        assert_eq!(deserialized.failed, 2);
        assert_eq!(deserialized.skipped, 3);
    }

    // =========================================================================
    // PackageReport tests
    // =========================================================================

    #[test]
    fn test_package_report_passed() {
        let report = PackageReport {
            name: "mylib".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/crates/mylib/Cargo.toml"),
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
        };

        assert_eq!(report.name, "mylib");
        assert_eq!(report.version, "1.0.0");
        assert_eq!(report.status, PackageStatus::Passed);
        assert!(report.skip_reason.is_none());
        assert!(report.inferred_required_bump.is_none());
    }

    #[test]
    fn test_package_report_failed() {
        let report = PackageReport {
            name: "mylib".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/crates/mylib/Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 1200,
            command: vec![
                "cargo".to_string(),
                "semver-checks".to_string(),
                "check-release".to_string(),
            ],
            engine: Some(SemverCheckOutput {
                exit_code: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "Breaking changes detected".to_string(),
                required_bump: Some(RequiredBump::Major),
            }),
            inferred_required_bump: Some(RequiredBump::Major),
        };

        assert_eq!(report.status, PackageStatus::Failed);
        assert!(matches!(
            report.inferred_required_bump,
            Some(RequiredBump::Major)
        ));
    }

    #[test]
    fn test_package_report_skipped() {
        let report = PackageReport {
            name: "mylib-internal".to_string(),
            version: "0.1.0".to_string(),
            manifest_path: PathBuf::from("/workspace/crates/mylib-internal/Cargo.toml"),
            status: PackageStatus::Skipped,
            skip_reason: Some("publish = false".to_string()),
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
        };

        assert_eq!(report.status, PackageStatus::Skipped);
        assert_eq!(report.skip_reason, Some("publish = false".to_string()));
        assert!(report.engine.is_none());
    }

    #[test]
    fn test_package_report_json_roundtrip() {
        let original = PackageReport {
            name: "mylib".to_string(),
            version: "2.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
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
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PackageReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, original.name);
        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.status, original.status);
        assert_eq!(deserialized.duration_ms, original.duration_ms);
    }

    // =========================================================================
    // RunReport tests
    // =========================================================================

    #[test]
    fn test_run_report_json_roundtrip() {
        let original = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                PackageReport {
                    name: "lib-a".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/crates/lib-a/Cargo.toml"),
                    status: PackageStatus::Passed,
                    skip_reason: None,
                    duration_ms: 500,
                    command: vec!["cargo".to_string()],
                    engine: None,
                    inferred_required_bump: None,
                },
                PackageReport {
                    name: "lib-b".to_string(),
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/crates/lib-b/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 1000,
                    command: vec!["cargo".to_string()],
                    engine: Some(SemverCheckOutput {
                        exit_code: Some(1),
                        success: false,
                        stdout: String::new(),
                        stderr: "Error".to_string(),
                        required_bump: Some(RequiredBump::Minor),
                    }),
                    inferred_required_bump: Some(RequiredBump::Minor),
                },
            ],
            summary: Summary {
                total: 2,
                passed: 1,
                failed: 1,
                skipped: 0,
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.semverguard_version, "0.1.0");
        assert_eq!(deserialized.packages.len(), 2);
        assert_eq!(deserialized.summary.total, 2);
        assert_eq!(deserialized.summary.passed, 1);
        assert_eq!(deserialized.summary.failed, 1);
    }

    #[test]
    fn test_run_report_empty_packages() {
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

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: RunReport = serde_json::from_str(&json).unwrap();

        assert!(deserialized.packages.is_empty());
        assert!(deserialized.summary.overall_success());
    }

    #[test]
    fn test_run_report_pretty_json() {
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

        let pretty_json = serde_json::to_string_pretty(&report).unwrap();
        // Pretty JSON should contain newlines
        assert!(pretty_json.contains('\n'));
    }

    // =========================================================================
    // Summary calculations tests
    // =========================================================================

    #[test]
    fn test_summary_calculations_match_packages() {
        // A helper to verify summary matches packages
        fn calculate_summary(packages: &[PackageReport]) -> Summary {
            let total = packages.len();
            let passed = packages
                .iter()
                .filter(|p| p.status == PackageStatus::Passed)
                .count();
            let failed = packages
                .iter()
                .filter(|p| p.status == PackageStatus::Failed)
                .count();
            let skipped = packages
                .iter()
                .filter(|p| p.status == PackageStatus::Skipped)
                .count();
            Summary {
                total,
                passed,
                failed,
                skipped,
            }
        }

        let packages = vec![
            PackageReport {
                name: "a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/a"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
            PackageReport {
                name: "b".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/b"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
            PackageReport {
                name: "c".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/c"),
                status: PackageStatus::Skipped,
                skip_reason: Some("excluded".to_string()),
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
        ];

        let summary = calculate_summary(&packages);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert!(!summary.overall_success());
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_package_report_empty_strings() {
        let report = PackageReport {
            name: String::new(),
            version: String::new(),
            manifest_path: PathBuf::new(),
            status: PackageStatus::Skipped,
            skip_reason: Some(String::new()),
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: PackageReport = serde_json::from_str(&json).unwrap();

        assert!(deserialized.name.is_empty());
        assert!(deserialized.version.is_empty());
        assert_eq!(deserialized.skip_reason, Some(String::new()));
    }

    #[test]
    fn test_run_report_special_characters_in_paths() {
        let report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace/path with spaces/project"),
            packages: vec![],
            summary: Summary {
                total: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
            },
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: RunReport = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.workspace_root,
            PathBuf::from("/workspace/path with spaces/project")
        );
    }

    #[test]
    fn test_large_duration_ms() {
        let report = PackageReport {
            name: "slow-lib".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            status: PackageStatus::Passed,
            skip_reason: None,
            duration_ms: u128::MAX,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: PackageReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.duration_ms, u128::MAX);
    }
}
