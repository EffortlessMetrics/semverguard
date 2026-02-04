use crate::error::SemverguardError;
use semverguard_types::{FailureKind, SemverCheckOutput};

/// Classify a semver-check output into a failure kind.
pub fn classify_output(output: &SemverCheckOutput) -> FailureKind {
    if output.required_bump.is_some() {
        return FailureKind::SemverViolation;
    }

    match output.exit_code {
        Some(1) => FailureKind::SemverViolation,
        Some(2) | None => {
            if looks_like_baseline_error(&output.stdout, &output.stderr) {
                FailureKind::BaselineError
            } else {
                FailureKind::ToolError
            }
        }
        _ => {
            if looks_like_baseline_error(&output.stdout, &output.stderr) {
                FailureKind::BaselineError
            } else {
                FailureKind::Unknown
            }
        }
    }
}

/// Classify engine invocation errors.
pub fn classify_engine_error(_err: &SemverguardError) -> FailureKind {
    FailureKind::ToolError
}

fn looks_like_baseline_error(stdout: &str, stderr: &str) -> bool {
    let mut text = String::with_capacity(stdout.len() + stderr.len() + 1);
    text.push_str(stdout);
    text.push('\n');
    text.push_str(stderr);

    let hay = text.to_ascii_lowercase();
    if !hay.contains("baseline") {
        return false;
    }

    let keywords = [
        "not found",
        "unknown revision",
        "ambiguous",
        "shallow",
        "missing",
    ];

    keywords.iter().any(|k| hay.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::RequiredBump;

    fn output(exit_code: Option<i32>, stdout: &str, stderr: &str, bump: Option<RequiredBump>) -> SemverCheckOutput {
        SemverCheckOutput {
            exit_code,
            success: exit_code == Some(0),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            required_bump: bump,
        }
    }

    #[test]
    fn test_classify_output_semver_violation_exit_code() {
        let out = output(Some(1), "", "", None);
        assert!(matches!(classify_output(&out), FailureKind::SemverViolation));
    }

    #[test]
    fn test_classify_output_semver_violation_required_bump() {
        let out = output(Some(2), "", "", Some(RequiredBump::Major));
        assert!(matches!(classify_output(&out), FailureKind::SemverViolation));
    }

    #[test]
    fn test_classify_output_tool_error_exit_code() {
        let out = output(Some(2), "some error", "", None);
        assert!(matches!(classify_output(&out), FailureKind::ToolError));
    }

    #[test]
    fn test_classify_output_baseline_error() {
        let out = output(
            Some(2),
            "baseline not found",
            "unknown revision",
            None,
        );
        assert!(matches!(classify_output(&out), FailureKind::BaselineError));
    }
}
