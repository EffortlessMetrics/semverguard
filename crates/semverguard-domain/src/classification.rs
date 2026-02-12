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
    // Must mention baseline-related keywords to be a baseline error
    if !hay.contains("baseline")
        && !hay.contains("revision")
        && !hay.contains("merge-base")
        && !hay.contains("merge base")
        && !hay.contains("not published")
        && !hay.contains("never published")
        && !hay.contains("crates.io")
    {
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
        let name = crate_name
            .map(String::from)
            .unwrap_or_else(|| "unknown".to_string());
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
        let name = crate_name
            .map(String::from)
            .unwrap_or_else(|| "unknown".to_string());
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

    fn assert_baseline_cause_variant(
        actual: &Option<BaselineErrorCause>,
        expected: BaselineErrorCause,
    ) {
        assert_eq!(
            actual.as_ref().map(std::mem::discriminant),
            Some(std::mem::discriminant(&expected))
        );
    }

    #[test]
    fn test_classify_output_semver_violation_exit_code() {
        let out = output(Some(1), "", "", None);
        assert_eq!(classify_output(&out), FailureKind::SemverViolation);
    }

    #[test]
    fn test_classify_output_semver_violation_required_bump() {
        let out = output(Some(2), "", "", Some(RequiredBump::Major));
        assert_eq!(classify_output(&out), FailureKind::SemverViolation);
    }

    #[test]
    fn test_classify_output_unknown_exit_code() {
        let out = output(Some(3), "", "", None);
        assert_eq!(classify_output(&out), FailureKind::Unknown);
    }

    #[test]
    fn test_classify_output_unknown_exit_code_with_baseline_error() {
        let out = output(Some(3), "baseline not found", "", None);
        assert_eq!(classify_output(&out), FailureKind::BaselineError);
    }

    #[test]
    fn test_classify_engine_error_simple() {
        let err = SemverguardError::Engine("engine failed".to_string());
        assert_eq!(classify_engine_error(&err), FailureKind::ToolError);
    }

    #[test]
    fn test_classify_engine_error_detailed_baseline() {
        let err = SemverguardError::Engine("error: unknown revision 'origin/main'".to_string());
        let result = classify_engine_error_detailed(&err, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert_eq!(
            result.baseline_cause,
            Some(BaselineErrorCause::RevisionNotFound {
                rev: "origin/main".to_string(),
            })
        );
    }

    #[test]
    fn test_classify_engine_error_detailed_baseline_without_cause() {
        let err = SemverguardError::InvalidConfig("baseline status ok".to_string());
        let result = classify_engine_error_detailed(&err, None);
        assert_eq!(result.kind, FailureKind::ToolError);
        assert!(result.baseline_cause.is_none());
    }

    #[test]
    fn test_detect_baseline_error_generic_other() {
        let cause = detect_baseline_error_from_message("baseline failed for unknown reason", None);
        assert_baseline_cause_variant(
            &cause,
            BaselineErrorCause::Other {
                message: String::new(),
            },
        );
    }

    #[test]
    fn test_detect_baseline_error_revision_not_found_phrase() {
        let cause = detect_baseline_error_from_message("revision not found in baseline", None);
        assert_baseline_cause_variant(
            &cause,
            BaselineErrorCause::RevisionNotFound { rev: String::new() },
        );
    }

    #[test]
    fn test_detect_baseline_error_crate_absent_without_name() {
        let cause = detect_baseline_error_from_message("crate missing in baseline", None);
        assert_eq!(
            cause,
            Some(BaselineErrorCause::CrateAbsentFromBaseline {
                crate_name: "unknown".to_string(),
            })
        );
    }

    #[test]
    fn test_detect_baseline_error_rustdoc_error_only() {
        let cause = detect_baseline_error_from_message("baseline rustdoc error output", None);
        assert_baseline_cause_variant(
            &cause,
            BaselineErrorCause::RustdocGenerationFailed { detail: None },
        );
    }

    #[test]
    fn test_detect_baseline_error_not_published_without_name() {
        let cause = detect_baseline_error_from_message("crate not published to crates.io", None);
        assert_eq!(
            cause,
            Some(BaselineErrorCause::NotPublished {
                crate_name: "unknown".to_string(),
                version: None,
            })
        );
    }

    #[test]
    fn test_detect_baseline_error_none() {
        let cause = detect_baseline_error_from_message("all good here", None);
        assert!(cause.is_none());
    }

    #[test]
    fn test_detect_baseline_error_baseline_without_failure_returns_none() {
        let cause = detect_baseline_error_from_message("baseline status ok", None);
        assert!(cause.is_none());
    }

    #[test]
    fn test_extract_revision_single_and_double_quotes() {
        let single = extract_revision("error: unknown revision 'origin/main'");
        assert_eq!(single, Some("origin/main".to_string()));
        let double = extract_revision("error: unknown revision \"v1.0.0\"");
        assert_eq!(double, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_extract_revision_empty_or_too_long_returns_none() {
        let empty = extract_revision("error: unknown revision ''");
        assert!(empty.is_none());
        let long = "a".repeat(120);
        let long_msg = format!("error: unknown revision \"{}\"", long);
        assert!(extract_revision(&long_msg).is_none());
    }

    #[test]
    fn test_extract_revision_missing_closing_quotes_returns_none() {
        let missing_single = extract_revision("error: unknown revision 'origin/main");
        assert!(missing_single.is_none());
        let missing_double = extract_revision("error: unknown revision \"origin/main");
        assert!(missing_double.is_none());
    }

    #[test]
    fn test_extract_detail_snippet() {
        let detail = extract_detail("rustdoc: failed to generate output", &["rustdoc"]);
        assert!(detail.is_some());
    }

    #[test]
    fn test_extract_detail_none() {
        let detail = extract_detail("unrelated text", &["rustdoc"]);
        assert!(detail.is_none());
    }

    #[test]
    fn test_extract_detail_keyword_only_returns_none() {
        let detail = extract_detail("rustdoc", &["rustdoc"]);
        assert!(detail.is_none());
    }

    #[test]
    fn test_classify_output_tool_error_exit_code() {
        let out = output(Some(2), "some error", "", None);
        assert_eq!(classify_output(&out), FailureKind::ToolError);
    }

    #[test]
    fn test_classify_output_baseline_error() {
        let out = output(Some(2), "baseline not found", "unknown revision", None);
        assert_eq!(classify_output(&out), FailureKind::BaselineError);
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
        assert_baseline_cause_variant(
            &result.baseline_cause,
            BaselineErrorCause::ShallowClone { detail: None },
        );
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
        assert_eq!(
            result.baseline_cause,
            Some(BaselineErrorCause::RevisionNotFound {
                rev: "origin/missing".to_string(),
            })
        );
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
        assert_eq!(
            result.baseline_cause,
            Some(BaselineErrorCause::CrateAbsentFromBaseline {
                crate_name: "new-feature".to_string(),
            })
        );
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
        assert_baseline_cause_variant(
            &result.baseline_cause,
            BaselineErrorCause::RustdocGenerationFailed { detail: None },
        );
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
        assert_eq!(
            result.baseline_cause,
            Some(BaselineErrorCause::NotPublished {
                crate_name: "my-internal-crate".to_string(),
                version: None,
            })
        );
    }

    #[test]
    fn test_classify_detailed_merge_base_not_found() {
        let out = output(Some(2), "", "error: merge-base not found", None);
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert_baseline_cause_variant(
            &result.baseline_cause,
            BaselineErrorCause::MergeBaseNotFound {
                base: String::new(),
                head: String::new(),
            },
        );
    }

    #[test]
    fn test_classify_detailed_generic_baseline_error() {
        let out = output(
            Some(2),
            "",
            "error: baseline failed for unknown reason",
            None,
        );
        let result = classify_output_detailed(&out, None);
        assert_eq!(result.kind, FailureKind::BaselineError);
        assert_baseline_cause_variant(
            &result.baseline_cause,
            BaselineErrorCause::Other {
                message: String::new(),
            },
        );
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
