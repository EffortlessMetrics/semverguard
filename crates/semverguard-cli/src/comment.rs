//! PR comment rendering module.
//!
//! This module provides functionality to render a markdown comment suitable
//! for PR feedback from a semverguard receipt.

use semverguard_types::{FailureKind, PackageStatus, SensorReportV1, VerdictStatus};
use std::collections::HashMap;

/// Render a PR-friendly markdown comment from a receipt.
pub fn render_comment(receipt: &SensorReportV1) -> String {
    let mut lines: Vec<String> = Vec::new();

    let verdict = match receipt.verdict.status {
        VerdictStatus::Pass => "pass",
        VerdictStatus::Warn => "warn",
        VerdictStatus::Fail => "fail",
        VerdictStatus::Skip => "skip",
    };

    lines.push(format!("## Semverguard: {verdict}"));

    if let Some(data) = &receipt.data {
        let summary = &data.report.summary;
        lines.push(format!(
            "Summary: total={} passed={} failed={} skipped={}",
            summary.total, summary.passed, summary.failed, summary.skipped
        ));
    } else {
        lines.push("Summary: (no report available)".to_string());
    }

    lines.push(String::new());

    // Build raw log lookup by package+version.
    let mut raw_log_lookup: HashMap<(String, String), (Option<String>, Option<String>)> =
        HashMap::new();
    for raw in &receipt.artifacts.raw_logs {
        raw_log_lookup.insert(
            (raw.package.clone(), raw.version.clone()),
            (raw.stdout.clone(), raw.stderr.clone()),
        );
    }

    // Failed packages (SemverViolation/Unknown)
    lines.push("### Failed packages".to_string());
    if let Some(data) = &receipt.data {
        let mut failed = data
            .report
            .packages
            .iter()
            .filter(|p| p.status == PackageStatus::Failed)
            .filter(|p| match p.failure_kind {
                Some(FailureKind::BaselineError) | Some(FailureKind::ToolError) => false,
                _ => true,
            })
            .collect::<Vec<_>>();
        failed.sort_by(|a, b| {
            (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str()))
        });

        if failed.is_empty() {
            lines.push("- (none)".to_string());
        } else {
            for pkg in failed {
                let bump = pkg
                    .inferred_required_bump
                    .map(|b| format!("{b:?}"))
                    .unwrap_or_else(|| "unknown".to_string());
                let logs = raw_log_lookup
                    .get(&(pkg.name.clone(), pkg.version.clone()))
                    .and_then(|(_out, err)| err.clone())
                    .or_else(|| {
                        raw_log_lookup
                            .get(&(pkg.name.clone(), pkg.version.clone()))
                            .and_then(|(out, _)| out.clone())
                    });
                let log_suffix = logs.map(|p| format!(" (log: {p})")).unwrap_or_default();
                lines.push(format!(
                    "- {} {} (required bump: {}){}",
                    pkg.name, pkg.version, bump, log_suffix
                ));
            }
        }
    } else {
        lines.push("- (none)".to_string());
    }

    lines.push(String::new());

    // Skipped packages
    lines.push("### Skipped packages".to_string());
    if let Some(data) = &receipt.data {
        let mut skipped = data
            .report
            .packages
            .iter()
            .filter(|p| p.status == PackageStatus::Skipped)
            .collect::<Vec<_>>();
        skipped.sort_by(|a, b| {
            (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str()))
        });

        if skipped.is_empty() {
            lines.push("- (none)".to_string());
        } else {
            for pkg in skipped {
                let reason = pkg
                    .skip_reason
                    .clone()
                    .unwrap_or_else(|| "skipped".to_string());
                lines.push(format!("- {} {} ({})", pkg.name, pkg.version, reason));
            }
        }
    } else {
        lines.push("- (none)".to_string());
    }

    lines.push(String::new());

    // Warnings / tool errors
    lines.push("### Warnings / tool errors".to_string());
    let mut warn_lines = receipt
        .findings
        .iter()
        .filter(|f| f.level != semverguard_types::FindingLevel::Info)
        .filter(|f| f.level == semverguard_types::FindingLevel::Warning || f.check_id == "tool")
        .map(|f| format!("- {}: {}", f.check_id, f.message))
        .collect::<Vec<_>>();

    warn_lines.sort();
    if warn_lines.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        lines.extend(warn_lines);
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{
        ArtifactIndex, BaselineConfig, Finding, FindingLevel, PackageReport, RawLogRef,
        RequiredBump, RunInfo, RunReport, SemverguardData, Summary, ToolInfo, Verdict,
    };
    use std::path::PathBuf;

    fn minimal_receipt() -> SensorReportV1 {
        SensorReportV1 {
            schema: "sensor.report.v1".to_string(),
            tool: ToolInfo {
                name: "semverguard".to_string(),
                version: "0.1.0".to_string(),
                repository_url: None,
            },
            run: RunInfo {
                started_at: "2024-01-15T10:00:00Z".to_string(),
                finished_at: "2024-01-15T10:01:00Z".to_string(),
                duration_ms: 60000,
                workspace_root: PathBuf::from("/workspace"),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                reason: None,
            },
            findings: vec![],
            data: None,
            artifacts: ArtifactIndex {
                report_json: "report.json".to_string(),
                comment_md: "comment.md".to_string(),
                sarif_json: None,
                raw_logs: vec![],
            },
        }
    }

    fn minimal_report(packages: Vec<PackageReport>) -> RunReport {
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

        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:01:00Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages,
            summary: Summary {
                total: passed + failed + skipped,
                passed,
                failed,
                skipped,
            },
        }
    }

    fn make_package(name: &str, version: &str, status: PackageStatus) -> PackageReport {
        PackageReport {
            name: name.to_string(),
            version: version.to_string(),
            manifest_path: PathBuf::from(format!("/workspace/{}/Cargo.toml", name)),
            status,
            skip_reason: if status == PackageStatus::Skipped {
                Some("filtered by glob".to_string())
            } else {
                None
            },
            duration_ms: 100,
            command: vec!["cargo".to_string()],
            engine: None,
            inferred_required_bump: if status == PackageStatus::Failed {
                Some(RequiredBump::Major)
            } else {
                None
            },
            failure_kind: if status == PackageStatus::Failed {
                Some(FailureKind::SemverViolation)
            } else {
                None
            },
            baseline_error: None,
        }
    }

    // =========================================================================
    // Empty/All-passed scenarios
    // =========================================================================

    #[test]
    fn test_empty_findings_all_passed() {
        let packages = vec![
            make_package("lib-a", "1.0.0", PackageStatus::Passed),
            make_package("lib-b", "2.0.0", PackageStatus::Passed),
        ];
        let mut receipt = minimal_receipt();
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        assert!(comment.contains("## Semverguard: pass"));
        assert!(comment.contains("passed=2"));
        assert!(comment.contains("failed=0"));
        assert!(comment.contains("### Failed packages"));
        assert!(comment.contains("- (none)")); // No failures
    }

    #[test]
    fn test_no_data_available() {
        let receipt = minimal_receipt();

        let comment = render_comment(&receipt);

        assert!(comment.contains("## Semverguard: pass"));
        assert!(comment.contains("(no report available)"));
    }

    // =========================================================================
    // Long package names
    // =========================================================================

    #[test]
    fn test_very_long_package_name() {
        let long_name = "a".repeat(150); // Very long package name
        let packages = vec![make_package(&long_name, "1.0.0", PackageStatus::Failed)];
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Fail;
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        // Should contain the full long name
        assert!(comment.contains(&long_name));
        assert!(comment.contains("## Semverguard: fail"));
    }

    // =========================================================================
    // Markdown special characters
    // =========================================================================

    #[test]
    fn test_markdown_special_characters_in_package_name() {
        // Package names with special markdown characters
        let packages = vec![
            make_package("my_lib-core", "1.0.0", PackageStatus::Passed),
            make_package("pkg-with-dashes", "1.0.0", PackageStatus::Skipped),
        ];
        let mut receipt = minimal_receipt();
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        assert!(comment.contains("my_lib-core"));
        assert!(comment.contains("pkg-with-dashes"));
    }

    // =========================================================================
    // Unicode in messages
    // =========================================================================

    #[test]
    fn test_unicode_in_skip_reason() {
        let mut package = make_package("lib", "1.0.0", PackageStatus::Skipped);
        package.skip_reason = Some("скип причина 跳过原因".to_string());

        let packages = vec![package];
        let mut receipt = minimal_receipt();
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        assert!(comment.contains("скип причина 跳过原因"));
    }

    #[test]
    fn test_unicode_in_findings() {
        let mut receipt = minimal_receipt();
        receipt.findings = vec![Finding {
            check_id: "tool".to_string(),
            code: "warning".to_string(),
            level: FindingLevel::Warning,
            message: "警告: 需要版本升级".to_string(),
            location: None,
            data: None,
            fingerprint: None,
        }];

        let comment = render_comment(&receipt);

        assert!(comment.contains("警告: 需要版本升级"));
    }

    // =========================================================================
    // All packages skipped
    // =========================================================================

    #[test]
    fn test_all_packages_skipped() {
        let packages = vec![
            make_package("lib-a", "1.0.0", PackageStatus::Skipped),
            make_package("lib-b", "2.0.0", PackageStatus::Skipped),
            make_package("lib-c", "0.1.0", PackageStatus::Skipped),
        ];
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Skip;
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        assert!(comment.contains("## Semverguard: skip"));
        assert!(comment.contains("skipped=3"));
        assert!(comment.contains("### Skipped packages"));
        assert!(comment.contains("lib-a"));
        assert!(comment.contains("lib-b"));
        assert!(comment.contains("lib-c"));
    }

    // =========================================================================
    // Mixed status packages
    // =========================================================================

    #[test]
    fn test_mixed_status_packages() {
        let packages = vec![
            make_package("lib-a", "1.0.0", PackageStatus::Passed),
            make_package("lib-b", "2.0.0", PackageStatus::Failed),
            make_package("lib-c", "0.1.0", PackageStatus::Skipped),
        ];
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Fail;
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        assert!(comment.contains("passed=1"));
        assert!(comment.contains("failed=1"));
        assert!(comment.contains("skipped=1"));
        assert!(comment.contains("### Failed packages"));
        assert!(comment.contains("lib-b 2.0.0"));
        assert!(comment.contains("### Skipped packages"));
        assert!(comment.contains("lib-c 0.1.0"));
    }

    // =========================================================================
    // Raw log lookup
    // =========================================================================

    #[test]
    fn test_raw_logs_included_in_failed_packages() {
        let packages = vec![make_package("lib-a", "1.0.0", PackageStatus::Failed)];
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Fail;
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });
        receipt.artifacts.raw_logs = vec![RawLogRef {
            package: "lib-a".to_string(),
            version: "1.0.0".to_string(),
            stdout: None,
            stderr: Some("logs/lib-a.stderr.log".to_string()),
        }];

        let comment = render_comment(&receipt);

        assert!(comment.contains("(log: logs/lib-a.stderr.log)"));
    }

    // =========================================================================
    // Warnings section
    // =========================================================================

    #[test]
    fn test_warnings_in_comment() {
        let mut receipt = minimal_receipt();
        receipt.findings = vec![
            Finding {
                check_id: "tool".to_string(),
                code: "missing-baseline".to_string(),
                level: FindingLevel::Warning,
                message: "Could not resolve baseline".to_string(),
                location: None,
                data: None,
                fingerprint: None,
            },
            Finding {
                check_id: "tool".to_string(),
                code: "git-not-found".to_string(),
                level: FindingLevel::Warning,
                message: "Git binary not found".to_string(),
                location: None,
                data: None,
                fingerprint: None,
            },
        ];

        let comment = render_comment(&receipt);

        assert!(comment.contains("### Warnings / tool errors"));
        assert!(comment.contains("Could not resolve baseline"));
        assert!(comment.contains("Git binary not found"));
    }

    #[test]
    fn test_info_findings_not_in_warnings() {
        let mut receipt = minimal_receipt();
        receipt.findings = vec![Finding {
            check_id: "info".to_string(),
            code: "informational".to_string(),
            level: FindingLevel::Info,
            message: "This is just informational".to_string(),
            location: None,
            data: None,
            fingerprint: None,
        }];

        let comment = render_comment(&receipt);

        // Info level should not appear in warnings section
        assert!(!comment.contains("This is just informational"));
    }

    // =========================================================================
    // Verdict status rendering
    // =========================================================================

    #[test]
    fn test_verdict_pass() {
        let receipt = minimal_receipt();
        let comment = render_comment(&receipt);
        assert!(comment.contains("## Semverguard: pass"));
    }

    #[test]
    fn test_verdict_warn() {
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Warn;
        let comment = render_comment(&receipt);
        assert!(comment.contains("## Semverguard: warn"));
    }

    #[test]
    fn test_verdict_fail() {
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Fail;
        let comment = render_comment(&receipt);
        assert!(comment.contains("## Semverguard: fail"));
    }

    #[test]
    fn test_verdict_skip() {
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Skip;
        let comment = render_comment(&receipt);
        assert!(comment.contains("## Semverguard: skip"));
    }

    // =========================================================================
    // Sorting behavior
    // =========================================================================

    #[test]
    fn test_packages_sorted_alphabetically() {
        let packages = vec![
            make_package("zebra", "1.0.0", PackageStatus::Failed),
            make_package("alpha", "1.0.0", PackageStatus::Failed),
            make_package("beta", "1.0.0", PackageStatus::Failed),
        ];
        let mut receipt = minimal_receipt();
        receipt.verdict.status = VerdictStatus::Fail;
        receipt.data = Some(SemverguardData {
            report: minimal_report(packages),
        });

        let comment = render_comment(&receipt);

        let alpha_pos = comment.find("alpha").unwrap();
        let beta_pos = comment.find("beta").unwrap();
        let zebra_pos = comment.find("zebra").unwrap();

        assert!(
            alpha_pos < beta_pos,
            "alpha should come before beta in sorted output"
        );
        assert!(
            beta_pos < zebra_pos,
            "beta should come before zebra in sorted output"
        );
    }
}
