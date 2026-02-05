use crate::error::SemverguardError;
use semverguard_types::{BaselineErrorCause, FailureKind, SemverCheckOutput};

/// Result of classifying an output, including detailed baseline error info.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// The high-level failure kind.
    pub kind: FailureKind,
    /// Detailed baseline error cause (if kind is BaselineError).
    pub baseline_cause: Option<BaselineErrorCause>,
}

impl ClassificationResult {
    /// Create a classification with just a kind (no baseline details).
    pub fn simple(kind: FailureKind) -> Self {
        Self {
            kind,
            baseline_cause: None,
        }
    }

    /// Create a baseline error classification with details.
    pub fn baseline(cause: BaselineErrorCause) -> Self {
        Self {
            kind: FailureKind::BaselineError,
            baseline_cause: Some(cause),
        }
    }
}

/// Classify a semver-check output into a failure kind.
pub fn classify_output(output: &SemverCheckOutput) -> FailureKind {
    classify_output_detailed(output, None).kind
}

/// Classify a semver-check output with detailed baseline error information.
///
/// The `crate_name` parameter is used to provide context in error messages.
pub fn classify_output_detailed(
    output: &SemverCheckOutput,
    crate_name: Option<&str>,
) -> ClassificationResult {
    if output.required_bump.is_some() {
        return ClassificationResult::simple(FailureKind::SemverViolation);
    }

    match output.exit_code {
        Some(1) => ClassificationResult::simple(FailureKind::SemverViolation),
        Some(2) | None => {
            if let Some(cause) = detect_baseline_error(&output.stdout, &output.stderr, crate_name) {
                ClassificationResult::baseline(cause)
            } else {
                ClassificationResult::simple(FailureKind::ToolError)
            }
        }
        _ => {
            if let Some(cause) = detect_baseline_error(&output.stdout, &output.stderr, crate_name) {
                ClassificationResult::baseline(cause)
            } else {
                ClassificationResult::simple(FailureKind::Unknown)
            }
        }
    }
}

/// Classify engine invocation errors.
pub fn classify_engine_error(_err: &SemverguardError) -> FailureKind {
    FailureKind::ToolError
}

/// Classify engine invocation errors with detailed baseline information.
pub fn classify_engine_error_detailed(
    err: &SemverguardError,
    crate_name: Option<&str>,
) -> ClassificationResult {
    let err_str = err.to_string().to_ascii_lowercase();

    // Check for baseline-related errors in the error message
    if err_str.contains("baseline") || err_str.contains("revision") {
        if let Some(cause) = detect_baseline_error_from_message(&err_str, crate_name) {
            return ClassificationResult::baseline(cause);
        }
    }

    ClassificationResult::simple(FailureKind::ToolError)
}

/// Detect specific baseline error causes from output text.
fn detect_baseline_error(
    stdout: &str,
    stderr: &str,
    crate_name: Option<&str>,
) -> Option<BaselineErrorCause> {
    let mut text = String::with_capacity(stdout.len() + stderr.len() + 1);
    text.push_str(stdout);
    text.push('\n');
    text.push_str(stderr);

    detect_baseline_error_from_message(&text.to_ascii_lowercase(), crate_name)
}

/// Detect baseline error from a message string.
fn detect_baseline_error_from_message(
    hay: &str,
    crate_name: Option<&str>,
) -> Option<BaselineErrorCause> {
    // Must mention baseline to be a baseline error
    if !hay.contains("baseline") && !hay.contains("revision") {
        return None;
    }

    // Check for shallow clone issues
    if hay.contains("shallow") {
        let detail = extract_detail(hay, &["shallow clone", "shallow repository"]);
        return Some(BaselineErrorCause::ShallowClone { detail });
    }

    // Check for revision not found
    if hay.contains("unknown revision")
        || (hay.contains("revision") && hay.contains("not found"))
        || hay.contains("bad revision")
        || hay.contains("unknown ref")
    {
        let rev = extract_revision(hay).unwrap_or_else(|| "unknown".to_string());
        return Some(BaselineErrorCause::RevisionNotFound { rev });
    }

    // Check for crate not found in baseline
    if (hay.contains("crate") || hay.contains("package"))
        && (hay.contains("not found") || hay.contains("does not exist") || hay.contains("missing"))
        && hay.contains("baseline")
    {
        let name = crate_name.map(String::from).unwrap_or_else(|| "unknown".to_string());
        return Some(BaselineErrorCause::CrateAbsentFromBaseline { crate_name: name });
    }

    // Check for merge base not found
    if hay.contains("merge base") || hay.contains("merge-base") {
        return Some(BaselineErrorCause::MergeBaseNotFound {
            base: "unknown".to_string(),
            head: "HEAD".to_string(),
        });
    }

    // Check for rustdoc generation failures
    if hay.contains("rustdoc") && (hay.contains("failed") || hay.contains("error")) {
        let detail = extract_detail(hay, &["rustdoc", "failed to generate"]);
        return Some(BaselineErrorCause::RustdocGenerationFailed { detail });
    }

    // Check for not published errors
    if hay.contains("crates.io")
        || hay.contains("registry")
        || (hay.contains("not published") || hay.contains("never published"))
    {
        let name = crate_name.map(String::from).unwrap_or_else(|| "unknown".to_string());
        return Some(BaselineErrorCause::NotPublished {
            crate_name: name,
            version: None,
        });
    }

    // Generic baseline error - could not determine specific cause
    if hay.contains("baseline")
        && (hay.contains("not found")
            || hay.contains("missing")
            || hay.contains("error")
            || hay.contains("failed"))
    {
        return Some(BaselineErrorCause::Other {
            message: "baseline comparison failed".to_string(),
        });
    }

    None
}

/// Try to extract a git revision from an error message.
fn extract_revision(text: &str) -> Option<String> {
    // Look for patterns like 'origin/main' or 'v1.0.0' near revision-related words
    // This is a best-effort heuristic
    let patterns = ["revision", "rev", "ref"];

    for pattern in patterns {
        if let Some(pos) = text.find(pattern) {
            // Look for quoted strings after the pattern
            let after = &text[pos..];
            if let Some(quote_start) = after.find('\'') {
                let after_quote = &after[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('\'') {
                    let rev = &after_quote[..quote_end];
                    if !rev.is_empty() && rev.len() < 100 {
                        return Some(rev.to_string());
                    }
                }
            }
            // Also try double quotes
            if let Some(quote_start) = after.find('"') {
                let after_quote = &after[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let rev = &after_quote[..quote_end];
                    if !rev.is_empty() && rev.len() < 100 {
                        return Some(rev.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Extract detail text after a keyword.
fn extract_detail(text: &str, keywords: &[&str]) -> Option<String> {
    for keyword in keywords {
        if let Some(pos) = text.find(keyword) {
            // Take a reasonable snippet after the keyword
            let after = &text[pos..];
            let snippet: String = after.chars().take(100).collect();
            // Clean up the snippet
            let snippet = snippet.trim();
            if !snippet.is_empty() && snippet != *keyword {
                return Some(snippet.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::RequiredBump;

    fn output(
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
        bump: Option<RequiredBump>,
    ) -> SemverCheckOutput {
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
        assert!(matches!(
            classify_output(&out),
            FailureKind::SemverViolation
        ));
    }

    #[test]
    fn test_classify_output_semver_violation_required_bump() {
        let out = output(Some(2), "", "", Some(RequiredBump::Major));
        assert!(matches!(
            classify_output(&out),
            FailureKind::SemverViolation
        ));
    }

    #[test]
    fn test_classify_output_tool_error_exit_code() {
        let out = output(Some(2), "some error", "", None);
        assert!(matches!(classify_output(&out), FailureKind::ToolError));
    }

    #[test]
    fn test_classify_output_baseline_error() {
        let out = output(Some(2), "baseline not found", "unknown revision", None);
        assert!(matches!(classify_output(&out), FailureKind::BaselineError));
    }

    // =========================================================================
    // Detailed classification tests
    // =========================================================================

    #[test]
    fn test_classify_detailed_shallow_clone() {
        let out = output(
            Some(2),
            "",
            "error: shallow clone cannot resolve baseline revision",
            None,
        );
        let result = classify_output_detailed(&out, Some("my-crate"));
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert!(matches!(
            result.baseline_cause,
            Some(BaselineErrorCause::ShallowClone { .. })
        ));
    }

    #[test]
    fn test_classify_detailed_revision_not_found() {
        let out = output(
            Some(2),
            "",
            "error: unknown revision 'origin/missing'",
            None,
        );
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        if let Some(BaselineErrorCause::RevisionNotFound { rev }) = result.baseline_cause {
            assert_eq!(rev, "origin/missing");
        } else {
            panic!("Expected RevisionNotFound");
        }
    }

    #[test]
    fn test_classify_detailed_crate_not_in_baseline() {
        let out = output(
            Some(2),
            "crate 'new-feature' not found in baseline",
            "",
            None,
        );
        let result = classify_output_detailed(&out, Some("new-feature"));
        assert_eq!(result.kind, FailureKind::BaselineError);
        if let Some(BaselineErrorCause::CrateAbsentFromBaseline { crate_name }) =
            result.baseline_cause
        {
            assert_eq!(crate_name, "new-feature");
        } else {
            panic!("Expected CrateAbsentFromBaseline");
        }
    }

    #[test]
    fn test_classify_detailed_rustdoc_failure() {
        let out = output(
            Some(2),
            "",
            "error: failed to generate baseline rustdoc: nightly toolchain required",
            None,
        );
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert!(matches!(
            result.baseline_cause,
            Some(BaselineErrorCause::RustdocGenerationFailed { .. })
        ));
    }

    #[test]
    fn test_classify_detailed_not_published() {
        let out = output(
            Some(2),
            "",
            "error: crate has never been published to crates.io",
            None,
        );
        let result = classify_output_detailed(&out, Some("my-internal-crate"));
        assert_eq!(result.kind, FailureKind::BaselineError);
        if let Some(BaselineErrorCause::NotPublished { crate_name, .. }) = result.baseline_cause {
            assert_eq!(crate_name, "my-internal-crate");
        } else {
            panic!("Expected NotPublished");
        }
    }

    #[test]
    fn test_classify_detailed_merge_base_not_found() {
        let out = output(Some(2), "", "error: merge-base not found", None);
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert!(matches!(
            result.baseline_cause,
            Some(BaselineErrorCause::MergeBaseNotFound { .. })
        ));
    }

    #[test]
    fn test_classify_detailed_generic_baseline_error() {
        let out = output(Some(2), "", "error: baseline failed for unknown reason", None);
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert!(matches!(
            result.baseline_cause,
            Some(BaselineErrorCause::Other { .. })
        ));
    }

    #[test]
    fn test_classify_detailed_semver_violation() {
        let out = output(Some(1), "breaking changes detected", "", None);
        let result = classify_output_detailed(&out, Some("my-crate"));
        assert_eq!(result.kind, FailureKind::SemverViolation);
        assert!(result.baseline_cause.is_none());
    }

    #[test]
    fn test_classify_detailed_tool_error() {
        let out = output(Some(2), "", "cargo-semver-checks not found", None);
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::ToolError);
        assert!(result.baseline_cause.is_none());
    }

    #[test]
    fn test_classification_result_simple() {
        let result = ClassificationResult::simple(FailureKind::SemverViolation);
        assert_eq!(result.kind, FailureKind::SemverViolation);
        assert!(result.baseline_cause.is_none());
    }

    #[test]
    fn test_classification_result_baseline() {
        let result = ClassificationResult::baseline(BaselineErrorCause::ShallowClone {
            detail: Some("test".to_string()),
        });
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert!(result.baseline_cause.is_some());
    }
}
