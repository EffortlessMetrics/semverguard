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
        failed.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str())));

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
                let log_suffix = logs
                    .map(|p| format!(" (log: {p})"))
                    .unwrap_or_default();
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
        skipped.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(b.name.as_str(), b.version.as_str())));

        if skipped.is_empty() {
            lines.push("- (none)".to_string());
        } else {
            for pkg in skipped {
                let reason = pkg.skip_reason.clone().unwrap_or_else(|| "skipped".to_string());
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
