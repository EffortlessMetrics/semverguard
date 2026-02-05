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

impl RunReport {
    /// Sort packages by name for deterministic output ordering.
    ///
    /// This ensures JSON output is stable across runs when package
    /// discovery order may vary.
    pub fn sort_packages_by_name(&mut self) {
        self.packages
            .sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    }
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
    /// Failure classification (for failed packages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<FailureKind>,
    /// Detailed baseline error cause (when failure_kind is BaselineError).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_error: Option<BaselineErrorCause>,
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

/// Failure classification for a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Semantic versioning policy violation.
    SemverViolation,
    /// Tool or execution error.
    ToolError,
    /// Baseline selection error.
    BaselineError,
    /// Unknown or unclassified failure.
    Unknown,
}

/// Detailed baseline error causes.
///
/// These provide more specific information about why a baseline comparison failed,
/// allowing for better error messages and configurable behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BaselineErrorCause {
    /// The specified git revision does not exist.
    ///
    /// Common causes:
    /// - Typo in the revision name
    /// - Shallow clone missing the commit
    /// - Branch/tag was deleted
    RevisionNotFound {
        /// The revision that was requested.
        rev: String,
    },

    /// The crate did not exist in the baseline.
    ///
    /// This is expected for new crates being added to the workspace.
    /// Usually should be treated as a warning, not a failure.
    CrateAbsentFromBaseline {
        /// Name of the crate that was not found.
        crate_name: String,
    },

    /// The git repository is a shallow clone and cannot resolve the baseline.
    ///
    /// Common in CI environments. Fix by fetching with `--unshallow` or
    /// specifying a depth that includes the baseline commit.
    ShallowClone {
        /// Additional context about the shallow clone issue.
        detail: Option<String>,
    },

    /// Could not compute merge base between baseline and current.
    ///
    /// This can happen when:
    /// - The baseline branch has no common ancestor with current
    /// - Git history is incomplete
    MergeBaseNotFound {
        /// The base revision.
        base: String,
        /// The head revision.
        head: String,
    },

    /// The baseline rustdoc JSON could not be generated.
    ///
    /// This can happen due to:
    /// - Toolchain mismatch (wrong nightly version)
    /// - Compilation errors in baseline code
    /// - Missing dependencies in baseline
    RustdocGenerationFailed {
        /// Additional error details.
        detail: Option<String>,
    },

    /// The crate is not published to crates.io.
    ///
    /// This happens when using `baseline.kind = "crates-io"` but the crate
    /// has never been published or the specified version doesn't exist.
    NotPublished {
        /// Name of the crate.
        crate_name: String,
        /// Version that was requested (if any).
        version: Option<String>,
    },

    /// Generic baseline error with a message.
    ///
    /// Used when the error doesn't fit other categories.
    Other {
        /// Error message.
        message: String,
    },
}

impl std::fmt::Display for BaselineErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RevisionNotFound { rev } => {
                write!(f, "git revision '{}' not found", rev)
            }
            Self::CrateAbsentFromBaseline { crate_name } => {
                write!(f, "crate '{}' does not exist in baseline", crate_name)
            }
            Self::ShallowClone { detail } => {
                write!(f, "shallow clone cannot resolve baseline")?;
                if let Some(d) = detail {
                    write!(f, ": {}", d)?;
                }
                Ok(())
            }
            Self::MergeBaseNotFound { base, head } => {
                write!(f, "no merge base found between '{}' and '{}'", base, head)
            }
            Self::RustdocGenerationFailed { detail } => {
                write!(f, "failed to generate baseline rustdoc")?;
                if let Some(d) = detail {
                    write!(f, ": {}", d)?;
                }
                Ok(())
            }
            Self::NotPublished { crate_name, version } => {
                write!(f, "crate '{}' not published to crates.io", crate_name)?;
                if let Some(v) = version {
                    write!(f, " (version {})", v)?;
                }
                Ok(())
            }
            Self::Other { message } => write!(f, "{}", message),
        }
    }
}

impl BaselineErrorCause {
    /// Returns true if this error is typically expected and should be treated as a warning.
    ///
    /// For example, a new crate that doesn't exist in the baseline is expected
    /// behavior, not a real failure.
    pub fn is_expected(&self) -> bool {
        matches!(self, Self::CrateAbsentFromBaseline { .. })
    }

    /// Returns true if this error is likely recoverable with CI configuration changes.
    ///
    /// For example, shallow clone issues can be fixed by fetching more history.
    pub fn is_ci_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ShallowClone { .. } | Self::MergeBaseNotFound { .. }
        )
    }
}

/// Result of listing packages without running checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Packages that would be checked.
    pub would_check: Vec<ListedPackage>,
    /// Packages that would be skipped.
    pub would_skip: Vec<SkippedPackage>,
}

impl ListResult {
    /// Sort all package lists by name for deterministic output ordering.
    ///
    /// This ensures JSON output is stable across runs when package
    /// discovery order may vary.
    pub fn sort_packages_by_name(&mut self) {
        self.would_check
            .sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
        self.would_skip
            .sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    }
}

/// A package that would be checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedPackage {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: String,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
}

/// A package that would be skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedPackage {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: String,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Why this package would be skipped.
    pub reason: String,
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

    #[test]
    fn test_failure_kind_json_kebab_case() {
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
            failure_kind: None,
            baseline_error: None,
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
            failure_kind: Some(FailureKind::SemverViolation),
            baseline_error: None,
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
            failure_kind: None,
            baseline_error: None,
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
            failure_kind: None,
            baseline_error: None,
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
                    failure_kind: None,
                    baseline_error: None,
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
                    failure_kind: Some(FailureKind::SemverViolation),
                    baseline_error: None,
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
                failure_kind: None,
                baseline_error: None,
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
                failure_kind: Some(FailureKind::Unknown),
                baseline_error: None,
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
                failure_kind: None,
                baseline_error: None,
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
            failure_kind: None,
            baseline_error: None,
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
            failure_kind: None,
            baseline_error: None,
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: PackageReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.duration_ms, u128::MAX);
    }

    // =========================================================================
    // BaselineErrorCause tests
    // =========================================================================

    #[test]
    fn test_baseline_error_cause_revision_not_found() {
        let cause = BaselineErrorCause::RevisionNotFound {
            rev: "origin/missing".to_string(),
        };
        assert_eq!(
            cause.to_string(),
            "git revision 'origin/missing' not found"
        );
        assert!(!cause.is_expected());
        assert!(!cause.is_ci_recoverable());
    }

    #[test]
    fn test_baseline_error_cause_crate_absent() {
        let cause = BaselineErrorCause::CrateAbsentFromBaseline {
            crate_name: "new-crate".to_string(),
        };
        assert_eq!(
            cause.to_string(),
            "crate 'new-crate' does not exist in baseline"
        );
        assert!(cause.is_expected());
        assert!(!cause.is_ci_recoverable());
    }

    #[test]
    fn test_baseline_error_cause_shallow_clone() {
        let cause = BaselineErrorCause::ShallowClone {
            detail: Some("fetch depth 1".to_string()),
        };
        assert_eq!(
            cause.to_string(),
            "shallow clone cannot resolve baseline: fetch depth 1"
        );
        assert!(!cause.is_expected());
        assert!(cause.is_ci_recoverable());

        let cause_no_detail = BaselineErrorCause::ShallowClone { detail: None };
        assert_eq!(
            cause_no_detail.to_string(),
            "shallow clone cannot resolve baseline"
        );
    }

    #[test]
    fn test_baseline_error_cause_merge_base_not_found() {
        let cause = BaselineErrorCause::MergeBaseNotFound {
            base: "origin/main".to_string(),
            head: "HEAD".to_string(),
        };
        assert_eq!(
            cause.to_string(),
            "no merge base found between 'origin/main' and 'HEAD'"
        );
        assert!(!cause.is_expected());
        assert!(cause.is_ci_recoverable());
    }

    #[test]
    fn test_baseline_error_cause_rustdoc_generation_failed() {
        let cause = BaselineErrorCause::RustdocGenerationFailed {
            detail: Some("nightly toolchain required".to_string()),
        };
        assert_eq!(
            cause.to_string(),
            "failed to generate baseline rustdoc: nightly toolchain required"
        );
        assert!(!cause.is_expected());
        assert!(!cause.is_ci_recoverable());
    }

    #[test]
    fn test_baseline_error_cause_not_published() {
        let cause = BaselineErrorCause::NotPublished {
            crate_name: "my-crate".to_string(),
            version: Some("1.0.0".to_string()),
        };
        assert_eq!(
            cause.to_string(),
            "crate 'my-crate' not published to crates.io (version 1.0.0)"
        );
        assert!(!cause.is_expected());
        assert!(!cause.is_ci_recoverable());

        let cause_no_version = BaselineErrorCause::NotPublished {
            crate_name: "my-crate".to_string(),
            version: None,
        };
        assert_eq!(
            cause_no_version.to_string(),
            "crate 'my-crate' not published to crates.io"
        );
    }

    #[test]
    fn test_baseline_error_cause_other() {
        let cause = BaselineErrorCause::Other {
            message: "some unknown error".to_string(),
        };
        assert_eq!(cause.to_string(), "some unknown error");
        assert!(!cause.is_expected());
        assert!(!cause.is_ci_recoverable());
    }

    #[test]
    fn test_baseline_error_cause_json_roundtrip() {
        let causes = vec![
            BaselineErrorCause::RevisionNotFound {
                rev: "v1.0.0".to_string(),
            },
            BaselineErrorCause::CrateAbsentFromBaseline {
                crate_name: "new-crate".to_string(),
            },
            BaselineErrorCause::ShallowClone {
                detail: Some("depth 1".to_string()),
            },
            BaselineErrorCause::MergeBaseNotFound {
                base: "main".to_string(),
                head: "HEAD".to_string(),
            },
            BaselineErrorCause::RustdocGenerationFailed {
                detail: None,
            },
            BaselineErrorCause::NotPublished {
                crate_name: "my-crate".to_string(),
                version: Some("2.0.0".to_string()),
            },
            BaselineErrorCause::Other {
                message: "custom error".to_string(),
            },
        ];

        for cause in causes {
            let json = serde_json::to_string(&cause).unwrap();
            let deserialized: BaselineErrorCause = serde_json::from_str(&json).unwrap();
            assert_eq!(cause, deserialized);
        }
    }

    #[test]
    fn test_package_report_with_baseline_error() {
        let report = PackageReport {
            name: "failing-crate".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 100,
            command: vec!["cargo".to_string(), "semver-checks".to_string()],
            engine: None,
            inferred_required_bump: None,
            failure_kind: Some(FailureKind::BaselineError),
            baseline_error: Some(BaselineErrorCause::ShallowClone {
                detail: Some("CI clone depth 1".to_string()),
            }),
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: PackageReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.failure_kind, Some(FailureKind::BaselineError));
        assert!(deserialized.baseline_error.is_some());
        if let Some(BaselineErrorCause::ShallowClone { detail }) = deserialized.baseline_error {
            assert_eq!(detail, Some("CI clone depth 1".to_string()));
        } else {
            panic!("Expected ShallowClone variant");
        }
    }

    // =========================================================================
    // Deterministic ordering tests
    // =========================================================================

    #[test]
    fn test_run_report_sort_packages_by_name() {
        let mut report = RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
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
                    version: "2.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                    status: PackageStatus::Failed,
                    skip_reason: None,
                    duration_ms: 200,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
                PackageReport {
                    name: "beta".to_string(),
                    version: "1.0.0".to_string(),
                    manifest_path: PathBuf::from("/workspace/beta-old/Cargo.toml"),
                    status: PackageStatus::Skipped,
                    skip_reason: Some("filtered".to_string()),
                    duration_ms: 0,
                    command: vec![],
                    engine: None,
                    inferred_required_bump: None,
                    failure_kind: None,
                    baseline_error: None,
                },
            ],
            summary: Summary {
                total: 4,
                passed: 2,
                failed: 1,
                skipped: 1,
            },
        };

        report.sort_packages_by_name();

        assert_eq!(report.packages[0].name, "alpha");
        assert_eq!(report.packages[1].name, "beta");
        assert_eq!(report.packages[1].version, "1.0.0"); // Sorted by version too
        assert_eq!(report.packages[2].name, "beta");
        assert_eq!(report.packages[2].version, "2.0.0");
        assert_eq!(report.packages[3].name, "zebra");
    }

    #[test]
    fn test_list_result_sort_packages_by_name() {
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

        result.sort_packages_by_name();

        assert_eq!(result.would_check[0].name, "alpha");
        assert_eq!(result.would_check[1].name, "zebra");
        assert_eq!(result.would_skip[0].name, "gamma");
        assert_eq!(result.would_skip[1].name, "omega");
    }

    #[test]
    fn test_listed_package_equality() {
        let pkg1 = ListedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/test/Cargo.toml"),
        };
        let pkg2 = ListedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/test/Cargo.toml"),
        };
        let pkg3 = ListedPackage {
            name: "other".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/other/Cargo.toml"),
        };

        assert_eq!(pkg1, pkg2);
        assert_ne!(pkg1, pkg3);
    }

    #[test]
    fn test_skipped_package_equality() {
        let pkg1 = SkippedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/test/Cargo.toml"),
            reason: "publish = false".to_string(),
        };
        let pkg2 = SkippedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/test/Cargo.toml"),
            reason: "publish = false".to_string(),
        };
        let pkg3 = SkippedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/test/Cargo.toml"),
            reason: "different reason".to_string(),
        };

        assert_eq!(pkg1, pkg2);
        assert_ne!(pkg1, pkg3);
    }
}
