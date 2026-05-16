//! In-memory receipt source for embedders and batteries-included mode.
//!
//! When semverguard-core is linked into a host process (e.g. cockpit), the host
//! may hand arbitrary path strings alongside deserialized receipts. This module
//! provides [`InMemoryReceiptSource`] which:
//!
//! - **Filters** reserved artifact paths (`artifacts/buildfix/…`, `artifacts/cockpit/…`)
//! - **Normalizes** path separators for cross-platform determinism
//! - **Sorts** entries by normalized path for stable ordering

use semverguard_types::SensorReportV1;

// ─────────────────────────────────────────────────────────────────────────────
// Reserved path segments (director / actuator artifacts, not sensor evidence)
// ─────────────────────────────────────────────────────────────────────────────

/// Reserved artifact directory segments that are filtered out.
/// These represent director output and actuator artifacts, not sensor evidence.
const RESERVED_SEGMENTS: &[&str] = &["artifacts/buildfix", "artifacts/cockpit"];

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// A single receipt entry with its source path and deserialized report.
#[derive(Debug, Clone)]
pub struct ReceiptEntry {
    /// Original path as supplied by the embedder.
    pub path: String,
    /// Deserialized sensor report.
    pub receipt: SensorReportV1,
}

/// In-memory receipt source that accepts untrusted path strings from embedders
/// and produces a filtered, deterministically-ordered collection of receipts.
#[derive(Debug, Clone, Default)]
pub struct InMemoryReceiptSource {
    entries: Vec<ReceiptEntry>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Path normalization
// ─────────────────────────────────────────────────────────────────────────────

/// Normalize a path string for filtering and sorting:
/// - Strip Windows extended-length path prefix (`\\?\`)
/// - Replace backslashes with forward slashes
fn normalize_path(path: &str) -> String {
    let stripped = path
        .strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix(r"//?/"))
        .unwrap_or(path);
    stripped.replace('\\', "/")
}

/// Returns true if the normalized path matches a reserved artifact directory.
fn is_reserved(normalized: &str) -> bool {
    for segment in RESERVED_SEGMENTS {
        // Matches segment as a directory component:
        // - "artifacts/buildfix/report.json" (starts_with)
        // - "/repo/artifacts/buildfix/report.json" (contains with leading slash)
        // - "./artifacts/buildfix/report.json" (contains with leading slash)
        let with_slash = format!("{segment}/");
        let after_slash = format!("/{segment}/");
        if normalized.starts_with(&with_slash) || normalized.contains(&after_slash) {
            return true;
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl InMemoryReceiptSource {
    /// Create a new empty receipt source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a receipt entry. The path is kept as-is for logging; normalization
    /// is applied during filtering and sorting.
    pub fn push(&mut self, path: impl Into<String>, receipt: SensorReportV1) {
        self.entries.push(ReceiptEntry {
            path: path.into(),
            receipt,
        });
    }

    /// Consume the source and return filtered, sorted receipt entries.
    ///
    /// Entries whose paths match reserved directories (buildfix, cockpit) are
    /// removed. Remaining entries are sorted by normalized path for deterministic
    /// ordering regardless of OS path conventions.
    pub fn into_receipts(self) -> Vec<ReceiptEntry> {
        let mut entries: Vec<ReceiptEntry> = self
            .entries
            .into_iter()
            .filter(|e| !is_reserved(&normalize_path(&e.path)))
            .collect();

        entries.sort_by_key(|e| normalize_path(&e.path));
        entries
    }

    /// Return the number of unfiltered entries currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries have been added.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_types::{
        ArtifactIndex, BaselineConfig, SensorReportV1, ToolInfo, Verdict, VerdictStatus,
    };
    use std::path::PathBuf;

    fn stub_receipt(schema: &str) -> SensorReportV1 {
        SensorReportV1 {
            schema: schema.to_string(),
            tool: ToolInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                repository_url: None,
            },
            run: semverguard_types::RunInfo {
                started_at: "2024-01-01T00:00:00Z".to_string(),
                finished_at: "2024-01-01T00:00:01Z".to_string(),
                duration_ms: 1000,
                workspace_root: PathBuf::from("/workspace"),
                baseline: BaselineConfig::default(),
                capabilities: None,
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                reasons: vec![],
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

    // ─────────────────────────────────────────────────────────────────────
    // Normalization unit tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_normalize_backslashes() {
        assert_eq!(
            normalize_path(r"artifacts\semverguard\report.json"),
            "artifacts/semverguard/report.json"
        );
    }

    #[test]
    fn test_normalize_forward_slashes_unchanged() {
        assert_eq!(
            normalize_path("artifacts/semverguard/report.json"),
            "artifacts/semverguard/report.json"
        );
    }

    #[test]
    fn test_normalize_windows_extended_length_prefix() {
        assert_eq!(
            normalize_path(r"\\?\C:\repo\artifacts\semverguard\report.json"),
            "C:/repo/artifacts/semverguard/report.json"
        );
    }

    #[test]
    fn test_normalize_windows_extended_forward_slash_prefix() {
        assert_eq!(
            normalize_path(r"//?/C:\repo\artifacts\semverguard\report.json"),
            "C:/repo/artifacts/semverguard/report.json"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Reserved path filtering
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_reserved_buildfix_relative() {
        assert!(is_reserved("artifacts/buildfix/report.json"));
    }

    #[test]
    fn test_reserved_buildfix_absolute() {
        assert!(is_reserved("/repo/artifacts/buildfix/report.json"));
    }

    #[test]
    fn test_reserved_buildfix_dot_relative() {
        assert!(is_reserved("./artifacts/buildfix/report.json"));
    }

    #[test]
    fn test_reserved_cockpit_relative() {
        assert!(is_reserved("artifacts/cockpit/report.json"));
    }

    #[test]
    fn test_reserved_cockpit_absolute() {
        assert!(is_reserved("/repo/artifacts/cockpit/report.json"));
    }

    #[test]
    fn test_not_reserved_semverguard() {
        assert!(!is_reserved("artifacts/semverguard/report.json"));
    }

    #[test]
    fn test_not_reserved_buildfixer() {
        // "buildfixer" should NOT be caught — only exact "buildfix/" segment
        assert!(!is_reserved("artifacts/buildfixer/report.json"));
    }

    #[test]
    fn test_reserved_windows_extended_path_after_normalize() {
        let norm = normalize_path(r"\\?\C:\repo\artifacts\buildfix\report.json");
        assert!(is_reserved(&norm));
    }

    #[test]
    fn test_reserved_backslash_path_after_normalize() {
        let norm = normalize_path(r"artifacts\cockpit\report.json");
        assert!(is_reserved(&norm));
    }

    // ─────────────────────────────────────────────────────────────────────
    // InMemoryReceiptSource integration tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_source_returns_empty() {
        let source = InMemoryReceiptSource::new();
        assert!(source.is_empty());
        assert_eq!(source.into_receipts().len(), 0);
    }

    #[test]
    fn test_filters_buildfix_and_cockpit() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            "artifacts/semverguard/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            "artifacts/buildfix/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            "artifacts/cockpit/report.json",
            stub_receipt("sensor.report.v1"),
        );

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].path, "artifacts/semverguard/report.json");
    }

    #[test]
    fn test_filters_windows_backslash_paths() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            r"artifacts\semverguard\report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            r"artifacts\buildfix\report.json",
            stub_receipt("sensor.report.v1"),
        );

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
    }

    #[test]
    fn test_filters_windows_extended_length_paths() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            r"\\?\C:\repo\artifacts\buildfix\report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            r"\\?\C:\repo\artifacts\semverguard\report.json",
            stub_receipt("sensor.report.v1"),
        );

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].path,
            r"\\?\C:\repo\artifacts\semverguard\report.json"
        );
    }

    #[test]
    fn test_sorts_by_normalized_path() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            "artifacts/zeta/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            "artifacts/alpha/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push("artifacts/mu/report.json", stub_receipt("sensor.report.v1"));

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 3);
        assert_eq!(receipts[0].path, "artifacts/alpha/report.json");
        assert_eq!(receipts[1].path, "artifacts/mu/report.json");
        assert_eq!(receipts[2].path, "artifacts/zeta/report.json");
    }

    #[test]
    fn test_sorts_by_normalized_path_mixed_slashes() {
        let mut source = InMemoryReceiptSource::new();
        // Backslash version of "alpha" should still sort before "zeta"
        source.push(
            r"artifacts\zeta\report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            r"artifacts/alpha/report.json",
            stub_receipt("sensor.report.v1"),
        );

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 2);
        // Sorted by normalized path (forward slashes), so alpha < zeta
        assert_eq!(receipts[0].path, "artifacts/alpha/report.json");
        assert_eq!(receipts[1].path, r"artifacts\zeta\report.json");
    }

    #[test]
    fn test_filters_absolute_linux_paths() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            "/home/ci/repo/artifacts/buildfix/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            "/home/ci/repo/artifacts/semverguard/report.json",
            stub_receipt("sensor.report.v1"),
        );

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
    }

    #[test]
    fn test_preserves_original_path_in_entry() {
        let mut source = InMemoryReceiptSource::new();
        let original = r"\\?\C:\repo\artifacts\semverguard\report.json";
        source.push(original, stub_receipt("sensor.report.v1"));

        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
        // Original path preserved (not normalized)
        assert_eq!(receipts[0].path, original);
    }

    #[test]
    fn test_len_counts_before_filtering() {
        let mut source = InMemoryReceiptSource::new();
        source.push(
            "artifacts/buildfix/report.json",
            stub_receipt("sensor.report.v1"),
        );
        source.push(
            "artifacts/semverguard/report.json",
            stub_receipt("sensor.report.v1"),
        );

        // len() reflects raw count before filtering
        assert_eq!(source.len(), 2);
        // into_receipts() applies filtering
        let receipts = source.into_receipts();
        assert_eq!(receipts.len(), 1);
    }
}
