//! Finding code explanation registry.
//!
//! Provides a static registry of all known finding codes with human-readable
//! explanations, common causes, and suggested fixes.

use serde_json::json;

/// Explanation for a finding check_id + code pair.
#[derive(Debug, Clone)]
pub struct FindingExplanation {
    /// Producer identifier (e.g., "semver").
    pub check_id: &'static str,
    /// Classification code (e.g., "violation").
    pub code: &'static str,
    /// Human-readable title.
    pub title: &'static str,
    /// Detailed description.
    pub description: &'static str,
    /// Common causes.
    pub causes: &'static [&'static str],
    /// Suggested fixes.
    pub fixes: &'static [&'static str],
    /// Default severity level.
    pub default_level: &'static str,
}

impl FindingExplanation {
    /// Convert to a JSON value for structured output.
    pub fn to_json_value(&self) -> serde_json::Value {
        json!({
            "check_id": self.check_id,
            "code": self.code,
            "title": self.title,
            "description": self.description,
            "causes": self.causes,
            "fixes": self.fixes,
            "default_level": self.default_level,
        })
    }
}

/// Static registry of all known finding types.
pub static FINDING_REGISTRY: &[FindingExplanation] = &[
    FindingExplanation {
        check_id: "semver",
        code: "violation",
        title: "SemVer Policy Violation",
        description: "A breaking change to the public API was detected that violates semantic \
            versioning rules. The package's public API has changed in a way that is incompatible \
            with the declared version.",
        causes: &[
            "Removed or renamed a public function, method, or type",
            "Changed the signature of a public function (parameters, return type)",
            "Removed a public trait implementation",
            "Changed a public enum (added/removed variants without #[non_exhaustive])",
            "Changed a public struct's fields",
        ],
        fixes: &[
            "Revert the breaking change if unintentional",
            "Bump the major version (e.g., 1.0.0 -> 2.0.0)",
            "Add a waiver in semverguard.toml if the break is intentional",
            "Use #[doc(hidden)] for items not intended as public API",
        ],
        default_level: "error",
    },
    FindingExplanation {
        check_id: "baseline",
        code: "missing",
        title: "Baseline Resolution Error",
        description: "The baseline version or revision could not be resolved. This prevents \
            semver comparison because there is no reference point to compare against.",
        causes: &[
            "The crate has never been published to crates.io",
            "The specified git revision does not exist",
            "Shallow clone without enough history (increase fetch-depth)",
            "Network error when fetching from crates.io registry",
        ],
        fixes: &[
            "For new crates: set baseline.on_error.new_crate = 'skip' in config",
            "For git baselines: verify the revision exists with 'git log --oneline <rev>'",
            "For CI: ensure fetch-depth: 0 or sufficient depth",
            "Use --baseline-version to specify an explicit version",
        ],
        default_level: "warning",
    },
    FindingExplanation {
        check_id: "tool.runtime",
        code: "runtime_error",
        title: "Tool/Runtime Error",
        description: "Semverguard encountered an error during execution that prevented it from \
            completing the check. This is typically a configuration or environment issue, \
            not a semver problem.",
        causes: &[
            "Invalid configuration in semverguard.toml",
            "cargo-semver-checks not installed or not on PATH",
            "Workspace root does not exist or is not a Cargo workspace",
            "Permission errors accessing files or running commands",
        ],
        fixes: &[
            "Run 'semverguard validate-config' to check configuration",
            "Ensure cargo-semver-checks is installed: cargo install cargo-semver-checks",
            "Verify workspace root path is correct",
            "Check file permissions and CI environment setup",
        ],
        default_level: "error",
    },
    FindingExplanation {
        check_id: "engine",
        code: "unknown",
        title: "Unclassifiable Engine Failure",
        description: "The semver checking engine (cargo-semver-checks) failed with an error \
            that could not be classified into a specific category. Check the raw logs for details.",
        causes: &[
            "Internal error in cargo-semver-checks",
            "Rustdoc generation failure",
            "Incompatible toolchain version",
            "Corrupted build artifacts or cache",
        ],
        fixes: &[
            "Check the raw stderr log for the specific error",
            "Try clearing the cargo cache: cargo clean",
            "Update cargo-semver-checks to the latest version",
            "Report the issue if it persists",
        ],
        default_level: "error",
    },
];

/// Look up a finding explanation by check_id and code.
pub fn lookup(check_id: &str, code: &str) -> Option<&'static FindingExplanation> {
    FINDING_REGISTRY
        .iter()
        .find(|e| e.check_id == check_id && e.code == code)
}

/// Look up all findings matching a check_id.
pub fn lookup_by_check_id(check_id: &str) -> Vec<&'static FindingExplanation> {
    FINDING_REGISTRY
        .iter()
        .filter(|e| e.check_id == check_id)
        .collect()
}

/// Return all finding explanations.
pub fn all() -> &'static [FindingExplanation] {
    FINDING_REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_semver_violation() {
        let exp = lookup("semver", "violation").expect("should find semver/violation");
        assert_eq!(exp.title, "SemVer Policy Violation");
        assert_eq!(exp.default_level, "error");
    }

    #[test]
    fn test_lookup_baseline_missing() {
        let exp = lookup("baseline", "missing").expect("should find baseline/missing");
        assert_eq!(exp.title, "Baseline Resolution Error");
        assert_eq!(exp.default_level, "warning");
    }

    #[test]
    fn test_lookup_tool_runtime() {
        let exp = lookup("tool.runtime", "runtime_error")
            .expect("should find tool.runtime/runtime_error");
        assert_eq!(exp.title, "Tool/Runtime Error");
    }

    #[test]
    fn test_lookup_engine_unknown() {
        let exp = lookup("engine", "unknown").expect("should find engine/unknown");
        assert_eq!(exp.title, "Unclassifiable Engine Failure");
    }

    #[test]
    fn test_lookup_nonexistent() {
        assert!(lookup("nonexistent", "code").is_none());
    }

    #[test]
    fn test_lookup_by_check_id() {
        let results = lookup_by_check_id("semver");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].code, "violation");
    }

    #[test]
    fn test_all_returns_registry() {
        let entries = all();
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn test_to_json_value() {
        let exp = lookup("semver", "violation").unwrap();
        let json = exp.to_json_value();
        assert_eq!(json["check_id"], "semver");
        assert_eq!(json["code"], "violation");
        assert!(json["causes"].is_array());
        assert!(json["fixes"].is_array());
    }
}
