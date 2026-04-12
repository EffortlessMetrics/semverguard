use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Run mode for semverguard.
///
/// Modes provide sensible default behaviors for common CI scenarios:
/// - `Pr` mode: For pull request lanes, tolerant of baseline errors
/// - `Release` mode: For release/tag lanes, strict enforcement
/// - `Auto` mode: Detect from CI environment variables
/// - `Cockpit` mode: For cockpitctl integration, always write receipt, exit 0 on success
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RunMode {
    /// Automatically detect mode from CI environment variables.
    ///
    /// Detection order:
    /// 1. If `CI_COMMIT_TAG` or `GITHUB_REF` contains `refs/tags/` -> Release mode
    /// 2. If `GITHUB_EVENT_NAME` is `pull_request` -> Pr mode
    /// 3. If `CI_PIPELINE_SOURCE` is `merge_request_event` -> Pr mode
    /// 4. Otherwise -> Pr mode (safe default)
    #[default]
    Auto,
    /// Pull request mode: tolerant of baseline errors.
    ///
    /// Behavior:
    /// - Baseline errors -> warn and skip (don't fail CI)
    /// - Good default for `--changed` scope
    /// - Designed for PR validation lanes
    Pr,
    /// Release mode: strict enforcement.
    ///
    /// Behavior:
    /// - Always run
    /// - Baseline errors -> fail (strict)
    /// - Good default for `--workspace` scope
    /// - Designed for release/tag lanes
    Release,
    /// Cockpit mode: always write receipt, exit 0 if written successfully.
    ///
    /// Behavior:
    /// - Forces `--format receipt` output
    /// - Exit 0 if receipt is written successfully (even with semver violations)
    /// - Exit 1 only if receipt write fails
    /// - Allows `cockpitctl` to be the sole gatekeeper
    Cockpit,
}

impl RunMode {
    /// Detect the run mode from environment variables.
    ///
    /// Returns the detected mode, or `Pr` if detection is ambiguous.
    pub fn detect_from_env() -> RunMode {
        Self::detect_from_env_with(|key| std::env::var(key).ok())
    }

    fn detect_from_env_with<F>(mut get: F) -> RunMode
    where
        F: FnMut(&str) -> Option<String>,
    {
        // GitHub Actions: check for tag ref
        if let Some(github_ref) = get("GITHUB_REF")
            && github_ref.starts_with("refs/tags/")
        {
            return RunMode::Release;
        }

        // GitLab CI: check for commit tag
        if get("CI_COMMIT_TAG").is_some() {
            return RunMode::Release;
        }

        // GitHub Actions: check for pull_request event
        if let Some(event_name) = get("GITHUB_EVENT_NAME")
            && (event_name == "pull_request" || event_name == "pull_request_target")
        {
            return RunMode::Pr;
        }

        // GitLab CI: check for merge request pipeline
        if let Some(source) = get("CI_PIPELINE_SOURCE")
            && source == "merge_request_event"
        {
            return RunMode::Pr;
        }

        // Azure DevOps: check for pull request
        if get("SYSTEM_PULLREQUEST_PULLREQUESTID").is_some() {
            return RunMode::Pr;
        }

        // CircleCI: check for pull request
        if get("CIRCLE_PULL_REQUEST").is_some() {
            return RunMode::Pr;
        }

        // Default to Pr mode (safer for CI)
        RunMode::Pr
    }

    /// Resolve Auto mode to a concrete mode using environment detection.
    ///
    /// Note: Cockpit mode does not resolve to another mode; it stays as Cockpit.
    pub fn resolve(self) -> RunMode {
        match self {
            RunMode::Auto => RunMode::detect_from_env(),
            other => other,
        }
    }

    /// Returns true if this is cockpit mode.
    pub fn is_cockpit(&self) -> bool {
        *self == RunMode::Cockpit
    }

    /// Returns true if baseline errors should be treated as warnings.
    ///
    /// In Pr and Cockpit modes, baseline errors are warnings (don't fail CI).
    /// In Release mode, baseline errors are failures.
    pub fn baseline_errors_are_warnings(&self) -> bool {
        let resolved = self.resolve();
        resolved == RunMode::Pr || resolved == RunMode::Cockpit
    }

    /// Returns the suggested default scope mode for this run mode.
    pub fn suggested_scope_mode(&self) -> ScopeMode {
        match self.resolve() {
            RunMode::Pr | RunMode::Auto | RunMode::Cockpit => ScopeMode::Changed,
            RunMode::Release => ScopeMode::Workspace,
        }
    }
}

/// Top-level configuration for semverguard.
///
/// Intended to be loaded from `semverguard.toml`, with CLI flags overriding.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SemverguardConfig {
    /// Run mode (pr, release, auto).
    ///
    /// Modes provide sensible default behaviors for common CI scenarios.
    pub mode: RunMode,

    /// Baseline selection for the SemVer comparison.
    pub baseline: BaselineConfig,

    /// Which crates to check.
    pub scope: ScopeConfig,

    /// Feature-selection flags passed through to cargo-semver-checks.
    pub features: FeaturesConfig,

    /// Engine (process) settings for invoking cargo-semver-checks.
    pub engine: EngineConfig,

    /// Output/reporting configuration.
    pub output: OutputConfig,

    /// Waivers for intentional breaking changes.
    #[serde(default)]
    pub waivers: Vec<WaiverEntry>,
}

/// Baseline selection kind.
///
/// - `crates-io`: compare against a published version on crates.io (or registries).
/// - `git`: compare against the workspace state at a git revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BaselineKind {
    /// Compare against a released version.
    #[default]
    CratesIo,
    /// Compare against a git revision.
    Git,
}

/// Baseline configuration.
///
/// These map closely to cargo-semver-checks flags:
/// - `--baseline-version`
/// - `--baseline-rev`
/// - `--baseline-root`
/// - `--baseline-rustdoc`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BaselineConfig {
    /// How to interpret the baseline fields.
    pub kind: BaselineKind,

    /// Baseline version number (typically the last released version).
    ///
    /// Used when `kind = "crates-io"`. If `None`, cargo-semver-checks will usually pick the most
    /// recent release automatically.
    pub version: Option<String>,

    /// Baseline git revision (tag/sha/ref).
    ///
    /// Used when `kind = "git"`.
    pub rev: Option<String>,

    /// Path to the baseline workspace root (rare; mostly for monorepos).
    pub root: Option<PathBuf>,

    /// Path to a pre-generated baseline rustdoc JSON (advanced/CI caching).
    pub rustdoc: Option<PathBuf>,

    /// How to handle baseline errors (e.g., missing revision, new crate).
    pub on_error: BaselineErrorBehavior,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: None,
            rustdoc: None,
            on_error: BaselineErrorBehavior::default(),
        }
    }
}

/// Behavior when a baseline error occurs.
///
/// Different baseline errors have different severity levels. This configuration
/// allows controlling whether errors should fail the check or just warn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BaselineErrorBehavior {
    /// How to handle new crates that don't exist in the baseline.
    ///
    /// Default: `warn` (new crates are expected, not a failure)
    pub new_crate: ErrorAction,

    /// How to handle missing git revisions.
    ///
    /// Default: `fail` (misconfiguration should be fixed)
    pub missing_revision: ErrorAction,

    /// How to handle shallow clone issues.
    ///
    /// Default: `fail` (CI should be configured to fetch enough history)
    pub shallow_clone: ErrorAction,

    /// How to handle rustdoc generation failures.
    ///
    /// Default: `fail` (usually indicates toolchain or code issues)
    pub rustdoc_failure: ErrorAction,

    /// How to handle crates not published to crates.io.
    ///
    /// Default: `warn` (expected for internal/new crates)
    pub not_published: ErrorAction,
}

impl Default for BaselineErrorBehavior {
    fn default() -> Self {
        Self {
            new_crate: ErrorAction::Warn,
            missing_revision: ErrorAction::Fail,
            shallow_clone: ErrorAction::Fail,
            rustdoc_failure: ErrorAction::Fail,
            not_published: ErrorAction::Warn,
        }
    }
}

/// Action to take when an error occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorAction {
    /// Treat as a failure (exit code 1).
    #[default]
    Fail,
    /// Emit a warning but continue (exit code 0 if no other failures).
    Warn,
    /// Skip silently without warning.
    Skip,
}

/// Scoping configuration: which packages to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScopeConfig {
    /// Mode for selecting packages.
    pub mode: ScopeMode,

    /// Explicit package names to check. When non-empty, ONLY these packages are
    /// checked (takes precedence over include/exclude globs).
    ///
    /// Typically set via CLI `--package` / `-p` flags rather than config file.
    pub explicit_packages: Vec<String>,

    /// Include package-name globs. Empty means "include everything".
    /// Ignored when `explicit_packages` is non-empty.
    pub include: Vec<String>,

    /// Exclude package-name globs.
    /// Ignored when `explicit_packages` is non-empty.
    pub exclude: Vec<String>,

    /// Skip packages with `publish = false` in Cargo.toml.
    pub skip_publish_false: bool,

    /// Skip packages without a library target.
    pub skip_no_lib: bool,
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            mode: ScopeMode::Workspace,
            explicit_packages: vec![],
            include: vec![],
            exclude: vec![],
            skip_publish_false: true,
            skip_no_lib: true,
        }
    }
}

/// Package selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeMode {
    /// Check all eligible workspace packages.
    #[default]
    Workspace,
    /// Check only workspace packages that changed relative to the baseline revision.
    ///
    /// Requires `baseline.kind = "git"` and a configured baseline revision.
    Changed,
}

/// Feature-selection flags passed through to cargo-semver-checks.
///
/// These mirror the CLI flags of cargo-semver-checks. They are intentionally “flat” so that they
/// remain stable even if cargo adds new combinators later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeaturesConfig {
    /// Pass `--all-features`.
    pub all_features: bool,

    /// Pass `--default-features`.
    pub default_features: bool,

    /// Pass `--only-explicit-features`.
    pub only_explicit_features: bool,

    /// Pass `--features <...>`.
    pub features: Vec<String>,

    /// Pass `--baseline-features <...>`.
    pub baseline_features: Vec<String>,

    /// Pass `--current-features <...>`.
    pub current_features: Vec<String>,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            all_features: false,
            default_features: true,
            only_explicit_features: false,
            features: vec![],
            baseline_features: vec![],
            current_features: vec![],
        }
    }
}

/// Settings for invoking cargo-semver-checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineConfig {
    /// Which `cargo` binary to run.
    ///
    /// If unset, uses `"cargo"` from PATH.
    pub cargo_bin: Option<PathBuf>,

    /// Extra arguments appended to `cargo semver-checks check-release`.
    pub extra_args: Vec<String>,

    /// Stop after the first failure.
    pub fail_fast: bool,

    /// Run package checks in parallel using rayon.
    ///
    /// When enabled, multiple packages are checked concurrently.
    /// When `fail_fast` is also enabled, remaining parallel checks
    /// will be cancelled on the first failure.
    pub parallel: bool,

    /// Dry run mode: build commands but do not execute them.
    ///
    /// When true, the engine will build the command that would be executed
    /// and return it without actually running the subprocess. Useful for
    /// CI debugging and verifying configuration.
    pub dry_run: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            cargo_bin: None,
            extra_args: vec![],
            fail_fast: false,
            parallel: true,
            dry_run: false,
        }
    }
}

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Human-readable summary to stdout.
    #[default]
    Text,
    /// JSON report (to a file or stdout).
    Json,
    /// Both text and JSON.
    Both,
    /// SARIF (Static Analysis Results Interchange Format) output.
    ///
    /// Used for integration with security and code quality tools like GitHub Code Scanning,
    /// VS Code SARIF Viewer, and other security analysis platforms.
    Sarif,
    /// Cockpit receipt output (sensor.report.v1).
    Receipt,
}

/// Color output choice for terminal rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorChoice {
    /// Automatically detect if terminal supports colors.
    #[default]
    Auto,
    /// Always use colors, even when output is redirected.
    Always,
    /// Never use colors.
    Never,
}

/// Output verbosity level.
///
/// Controls how much detail is shown in output. Higher verbosity shows more information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Verbosity {
    /// Suppress non-essential output. Only output on failure.
    /// Summary line only, no per-package details.
    Quiet,
    /// Default behavior. Summary + failed/skipped packages.
    #[default]
    Normal,
    /// Show all packages including passed.
    /// Show timing for each package.
    /// Show full commands executed.
    Verbose,
    /// All verbose output plus full stdout/stderr from cargo-semver-checks.
    /// Also shows config after CLI overrides applied.
    Debug,
}

impl Verbosity {
    /// Returns true if this verbosity level shows passed packages.
    pub fn shows_passed(&self) -> bool {
        *self >= Verbosity::Verbose
    }

    /// Returns true if this verbosity level shows skipped packages.
    pub fn shows_skipped(&self) -> bool {
        *self != Verbosity::Quiet
    }

    /// Returns true if this verbosity level shows timing information.
    pub fn shows_timing(&self) -> bool {
        *self >= Verbosity::Verbose
    }

    /// Returns true if this verbosity level shows commands executed.
    pub fn shows_commands(&self) -> bool {
        *self >= Verbosity::Verbose
    }

    /// Returns true if this verbosity level shows full engine output.
    pub fn shows_full_output(&self) -> bool {
        *self == Verbosity::Debug
    }

    /// Returns true if this verbosity level shows effective config.
    pub fn shows_config(&self) -> bool {
        *self == Verbosity::Debug
    }

    /// Returns true if this is quiet mode (suppress most output).
    pub fn is_quiet(&self) -> bool {
        *self == Verbosity::Quiet
    }
}

/// A waiver entry for intentional breaking changes.
///
/// Waivers allow known breaking changes to pass CI while maintaining an audit trail.
/// Each waiver targets a specific finding by its stable fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaiverEntry {
    /// Stable fingerprint of the finding to waive (64-char hex SHA-256).
    pub fingerprint: String,
    /// Human-readable reason for the waiver.
    pub reason: String,
    /// Optional ticket reference (e.g., "GH#42").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// Optional expiration date (ISO8601 date, e.g., "2025-06-01").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Output/reporting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputConfig {
    /// Which formats to emit.
    pub format: OutputFormat,

    /// Where to write JSON (when JSON output is enabled). If None, JSON prints to stdout.
    pub json_path: Option<PathBuf>,

    /// Pretty-print the JSON report.
    pub pretty_json: bool,

    /// Output directory for receipt artifacts.
    pub artifacts_dir: PathBuf,

    /// Treat warnings as failures for exit codes.
    pub warn_as_fail: bool,

    /// Color output mode for terminal rendering.
    pub color: ColorChoice,

    /// Output verbosity level.
    pub verbosity: Verbosity,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Text,
            json_path: None,
            pretty_json: true,
            artifacts_dir: PathBuf::from("artifacts/semverguard"),
            warn_as_fail: false,
            color: ColorChoice::Auto,
            verbosity: Verbosity::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn detect_with_env(pairs: &[(&str, &str)]) -> RunMode {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        RunMode::detect_from_env_with(|key| map.get(key).cloned())
    }

    // =========================================================================
    // Default implementation tests
    // =========================================================================

    #[test]
    fn test_semverguard_config_default() {
        let config = SemverguardConfig::default();
        assert_eq!(config.mode, RunMode::Auto);
        assert_eq!(config.baseline.kind, BaselineKind::CratesIo);
        assert_eq!(config.scope.mode, ScopeMode::Workspace);
        assert_eq!(config.output.format, OutputFormat::Text);
    }

    // =========================================================================
    // RunMode tests
    // =========================================================================

    #[test]
    fn test_run_mode_default() {
        let mode = RunMode::default();
        assert_eq!(mode, RunMode::Auto);
    }

    #[test]
    fn test_run_mode_json_roundtrip() {
        for mode in [
            RunMode::Auto,
            RunMode::Pr,
            RunMode::Release,
            RunMode::Cockpit,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: RunMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    #[test]
    fn test_run_mode_json_kebab_case() {
        assert_eq!(serde_json::to_string(&RunMode::Auto).unwrap(), "\"auto\"");
        assert_eq!(serde_json::to_string(&RunMode::Pr).unwrap(), "\"pr\"");
        assert_eq!(
            serde_json::to_string(&RunMode::Release).unwrap(),
            "\"release\""
        );
        assert_eq!(
            serde_json::to_string(&RunMode::Cockpit).unwrap(),
            "\"cockpit\""
        );
    }

    #[test]
    fn test_run_mode_baseline_errors_are_warnings() {
        assert!(RunMode::Pr.baseline_errors_are_warnings());
        assert!(!RunMode::Release.baseline_errors_are_warnings());
        assert!(RunMode::Cockpit.baseline_errors_are_warnings());
    }

    #[test]
    fn test_run_mode_suggested_scope_mode() {
        assert_eq!(RunMode::Pr.suggested_scope_mode(), ScopeMode::Changed);
        assert_eq!(
            RunMode::Release.suggested_scope_mode(),
            ScopeMode::Workspace
        );
        assert_eq!(RunMode::Cockpit.suggested_scope_mode(), ScopeMode::Changed);
    }

    #[test]
    fn test_run_mode_resolve_pr() {
        assert_eq!(RunMode::Pr.resolve(), RunMode::Pr);
    }

    #[test]
    fn test_run_mode_resolve_release() {
        assert_eq!(RunMode::Release.resolve(), RunMode::Release);
    }

    #[test]
    fn test_run_mode_resolve_cockpit() {
        assert_eq!(RunMode::Cockpit.resolve(), RunMode::Cockpit);
    }

    #[test]
    fn test_run_mode_is_cockpit() {
        assert!(!RunMode::Auto.is_cockpit());
        assert!(!RunMode::Pr.is_cockpit());
        assert!(!RunMode::Release.is_cockpit());
        assert!(RunMode::Cockpit.is_cockpit());
    }

    #[test]
    fn test_baseline_kind_default() {
        let kind = BaselineKind::default();
        assert_eq!(kind, BaselineKind::CratesIo);
    }

    #[test]
    fn test_baseline_config_default() {
        let config = BaselineConfig::default();
        assert_eq!(config.kind, BaselineKind::CratesIo);
        assert!(config.version.is_none());
        assert!(config.rev.is_none());
        assert!(config.root.is_none());
        assert!(config.rustdoc.is_none());
    }

    #[test]
    fn test_scope_config_default() {
        let config = ScopeConfig::default();
        assert_eq!(config.mode, ScopeMode::Workspace);
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
        assert!(config.skip_publish_false);
        assert!(config.skip_no_lib);
    }

    #[test]
    fn test_scope_mode_default() {
        let mode = ScopeMode::default();
        assert_eq!(mode, ScopeMode::Workspace);
    }

    #[test]
    fn test_features_config_default() {
        let config = FeaturesConfig::default();
        assert!(!config.all_features);
        assert!(config.default_features);
        assert!(!config.only_explicit_features);
        assert!(config.features.is_empty());
        assert!(config.baseline_features.is_empty());
        assert!(config.current_features.is_empty());
    }

    #[test]
    fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert!(config.cargo_bin.is_none());
        assert!(config.extra_args.is_empty());
        assert!(!config.fail_fast);
        assert!(config.parallel);
    }

    #[test]
    fn test_output_format_default() {
        let format = OutputFormat::default();
        assert_eq!(format, OutputFormat::Text);
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert_eq!(config.format, OutputFormat::Text);
        assert!(config.json_path.is_none());
        assert!(config.pretty_json);
        assert_eq!(config.artifacts_dir, PathBuf::from("artifacts/semverguard"));
        assert!(!config.warn_as_fail);
    }

    // =========================================================================
    // Serde JSON round-trip tests
    // =========================================================================

    #[test]
    fn test_semverguard_config_json_roundtrip() {
        let original = SemverguardConfig::default();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SemverguardConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.baseline.kind, BaselineKind::CratesIo);
        assert_eq!(deserialized.scope.mode, ScopeMode::Workspace);
    }

    #[test]
    fn test_baseline_kind_json_roundtrip() {
        for kind in [BaselineKind::CratesIo, BaselineKind::Git] {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: BaselineKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn test_baseline_config_json_roundtrip_with_values() {
        let original = BaselineConfig {
            kind: BaselineKind::Git,
            version: Some("1.0.0".to_string()),
            rev: Some("origin/main".to_string()),
            root: Some(PathBuf::from("/workspace")),
            rustdoc: Some(PathBuf::from("/docs/rustdoc.json")),
            on_error: BaselineErrorBehavior::default(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: BaselineConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.kind, BaselineKind::Git);
        assert_eq!(deserialized.version, Some("1.0.0".to_string()));
        assert_eq!(deserialized.rev, Some("origin/main".to_string()));
        assert_eq!(deserialized.root, Some(PathBuf::from("/workspace")));
        assert_eq!(
            deserialized.rustdoc,
            Some(PathBuf::from("/docs/rustdoc.json"))
        );
    }

    #[test]
    fn test_scope_mode_json_roundtrip() {
        for mode in [ScopeMode::Workspace, ScopeMode::Changed] {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: ScopeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    #[test]
    fn test_scope_config_json_roundtrip_with_values() {
        let original = ScopeConfig {
            mode: ScopeMode::Changed,
            include: vec!["pkg-*".to_string(), "core".to_string()],
            exclude: vec!["test-*".to_string()],
            explicit_packages: vec![],
            skip_publish_false: false,
            skip_no_lib: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ScopeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mode, ScopeMode::Changed);
        assert_eq!(deserialized.include, vec!["pkg-*", "core"]);
        assert_eq!(deserialized.exclude, vec!["test-*"]);
        assert!(!deserialized.skip_publish_false);
        assert!(!deserialized.skip_no_lib);
    }

    #[test]
    fn test_features_config_json_roundtrip_with_values() {
        let original = FeaturesConfig {
            all_features: true,
            default_features: false,
            only_explicit_features: true,
            features: vec!["feature1".to_string(), "feature2".to_string()],
            baseline_features: vec!["base_feat".to_string()],
            current_features: vec!["curr_feat".to_string()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: FeaturesConfig = serde_json::from_str(&json).unwrap();

        assert!(deserialized.all_features);
        assert!(!deserialized.default_features);
        assert!(deserialized.only_explicit_features);
        assert_eq!(deserialized.features, vec!["feature1", "feature2"]);
        assert_eq!(deserialized.baseline_features, vec!["base_feat"]);
        assert_eq!(deserialized.current_features, vec!["curr_feat"]);
    }

    #[test]
    fn test_engine_config_json_roundtrip_with_values() {
        let original = EngineConfig {
            cargo_bin: Some(PathBuf::from("/usr/bin/cargo")),
            extra_args: vec!["--verbose".to_string(), "--release".to_string()],
            fail_fast: true,
            parallel: false,
            dry_run: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EngineConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.cargo_bin,
            Some(PathBuf::from("/usr/bin/cargo"))
        );
        assert_eq!(deserialized.extra_args, vec!["--verbose", "--release"]);
        assert!(deserialized.fail_fast);
        assert!(!deserialized.parallel);
    }

    #[test]
    fn test_output_format_json_roundtrip() {
        for format in [
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Both,
            OutputFormat::Receipt,
        ] {
            let json = serde_json::to_string(&format).unwrap();
            let deserialized: OutputFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(format, deserialized);
        }
    }

    #[test]
    fn test_output_config_json_roundtrip_with_values() {
        let original = OutputConfig {
            format: OutputFormat::Both,
            json_path: Some(PathBuf::from("/output/report.json")),
            pretty_json: false,
            artifacts_dir: PathBuf::from("/output/artifacts"),
            warn_as_fail: true,
            color: ColorChoice::Auto,
            verbosity: Verbosity::Normal,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: OutputConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.format, OutputFormat::Both);
        assert_eq!(
            deserialized.json_path,
            Some(PathBuf::from("/output/report.json"))
        );
        assert!(!deserialized.pretty_json);
        assert_eq!(
            deserialized.artifacts_dir,
            PathBuf::from("/output/artifacts")
        );
        assert!(deserialized.warn_as_fail);
    }

    // =========================================================================
    // TOML deserialization tests
    // =========================================================================

    #[test]
    fn test_semverguard_config_toml_empty() {
        let toml_str = "";
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        // Should use all defaults
        assert_eq!(config.mode, RunMode::Auto);
        assert_eq!(config.baseline.kind, BaselineKind::CratesIo);
        assert_eq!(config.scope.mode, ScopeMode::Workspace);
    }

    #[test]
    fn test_semverguard_config_toml_full() {
        let toml_str = r#"
mode = "release"

[baseline]
kind = "git"
version = "2.0.0"
rev = "v2.0.0"
root = "/some/path"
rustdoc = "/docs/rustdoc.json"

[scope]
mode = "changed"
include = ["mylib-*"]
exclude = ["mylib-internal"]
skip_publish_false = false
skip_no_lib = false

[features]
all_features = true
default_features = false
only_explicit_features = true
features = ["serde", "async"]
baseline_features = ["serde"]
current_features = ["async"]

[engine]
cargo_bin = "/custom/cargo"
extra_args = ["--verbose"]
fail_fast = true

[output]
format = "both"
json_path = "report.json"
pretty_json = false
"#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();

        // Mode
        assert_eq!(config.mode, RunMode::Release);

        // Baseline
        assert_eq!(config.baseline.kind, BaselineKind::Git);
        assert_eq!(config.baseline.version, Some("2.0.0".to_string()));
        assert_eq!(config.baseline.rev, Some("v2.0.0".to_string()));
        assert_eq!(config.baseline.root, Some(PathBuf::from("/some/path")));
        assert_eq!(
            config.baseline.rustdoc,
            Some(PathBuf::from("/docs/rustdoc.json"))
        );

        // Scope
        assert_eq!(config.scope.mode, ScopeMode::Changed);
        assert_eq!(config.scope.include, vec!["mylib-*"]);
        assert_eq!(config.scope.exclude, vec!["mylib-internal"]);
        assert!(!config.scope.skip_publish_false);
        assert!(!config.scope.skip_no_lib);

        // Features
        assert!(config.features.all_features);
        assert!(!config.features.default_features);
        assert!(config.features.only_explicit_features);
        assert_eq!(config.features.features, vec!["serde", "async"]);
        assert_eq!(config.features.baseline_features, vec!["serde"]);
        assert_eq!(config.features.current_features, vec!["async"]);

        // Engine
        assert_eq!(
            config.engine.cargo_bin,
            Some(PathBuf::from("/custom/cargo"))
        );
        assert_eq!(config.engine.extra_args, vec!["--verbose"]);
        assert!(config.engine.fail_fast);

        // Output
        assert_eq!(config.output.format, OutputFormat::Both);
        assert_eq!(config.output.json_path, Some(PathBuf::from("report.json")));
        assert!(!config.output.pretty_json);
        assert_eq!(
            config.output.artifacts_dir,
            PathBuf::from("artifacts/semverguard")
        );
        assert!(!config.output.warn_as_fail);
    }

    #[test]
    fn test_baseline_kind_toml_kebab_case() {
        let toml_str = r#"kind = "crates-io""#;
        let config: BaselineConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.kind, BaselineKind::CratesIo);

        let toml_str = r#"kind = "git""#;
        let config: BaselineConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.kind, BaselineKind::Git);
    }

    #[test]
    fn test_scope_mode_toml_kebab_case() {
        let toml_str = r#"mode = "workspace""#;
        let config: ScopeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, ScopeMode::Workspace);

        let toml_str = r#"mode = "changed""#;
        let config: ScopeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, ScopeMode::Changed);
    }

    #[test]
    fn test_run_mode_toml_kebab_case() {
        let toml_str = r#"mode = "auto""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, RunMode::Auto);

        let toml_str = r#"mode = "pr""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, RunMode::Pr);

        let toml_str = r#"mode = "release""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, RunMode::Release);
    }

    #[test]
    fn test_output_format_toml_kebab_case() {
        let toml_str = r#"format = "text""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.format, OutputFormat::Text);

        let toml_str = r#"format = "json""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.format, OutputFormat::Json);

        let toml_str = r#"format = "both""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.format, OutputFormat::Both);

        let toml_str = r#"format = "receipt""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.format, OutputFormat::Receipt);
    }

    #[test]
    fn test_partial_toml_config_uses_defaults() {
        let toml_str = r#"
[baseline]
rev = "origin/main"

[scope]
exclude = ["test-*"]
"#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();

        // Baseline: defaults except rev
        assert_eq!(config.baseline.kind, BaselineKind::CratesIo);
        assert!(config.baseline.version.is_none());
        assert_eq!(config.baseline.rev, Some("origin/main".to_string()));

        // Scope: defaults except exclude
        assert_eq!(config.scope.mode, ScopeMode::Workspace);
        assert!(config.scope.include.is_empty());
        assert_eq!(config.scope.exclude, vec!["test-*"]);
        assert!(config.scope.skip_publish_false);

        // Features: all defaults
        assert!(!config.features.all_features);
        assert!(config.features.default_features);

        // Engine: all defaults
        assert!(config.engine.cargo_bin.is_none());

        // Output: all defaults
        assert_eq!(config.output.format, OutputFormat::Text);
        assert_eq!(
            config.output.artifacts_dir,
            PathBuf::from("artifacts/semverguard")
        );
        assert!(!config.output.warn_as_fail);
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_empty_strings_in_config() {
        let toml_str = r#"
[baseline]
version = ""
rev = ""

[scope]
include = [""]
exclude = [""]

[features]
features = [""]

[engine]
extra_args = [""]
"#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.baseline.version, Some("".to_string()));
        assert_eq!(config.baseline.rev, Some("".to_string()));
        assert_eq!(config.scope.include, vec![""]);
        assert_eq!(config.scope.exclude, vec![""]);
        assert_eq!(config.features.features, vec![""]);
        assert_eq!(config.engine.extra_args, vec![""]);
    }

    #[test]
    fn test_empty_arrays_in_config() {
        let toml_str = r#"
[scope]
include = []
exclude = []

[features]
features = []
baseline_features = []
current_features = []

[engine]
extra_args = []
"#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();

        assert!(config.scope.include.is_empty());
        assert!(config.scope.exclude.is_empty());
        assert!(config.features.features.is_empty());
        assert!(config.features.baseline_features.is_empty());
        assert!(config.features.current_features.is_empty());
        assert!(config.engine.extra_args.is_empty());
    }

    #[test]
    fn test_waiver_config_toml() {
        let toml_str = r#"
[[waivers]]
fingerprint = "d168f6b47520325f9be045d09b02b05e47f7ad212853431c50711c420d3e9635"
reason = "Intentional API redesign for v2.0"
ticket = "GH#42"
expires = "2025-06-01"

[[waivers]]
fingerprint = "55c6c63ec662ae3c54cafae7d58ad3ed08e5a41be73abc97e54bf6c028945674"
reason = "Known baseline issue"
"#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.waivers.len(), 2);
        assert_eq!(
            config.waivers[0].fingerprint,
            "d168f6b47520325f9be045d09b02b05e47f7ad212853431c50711c420d3e9635"
        );
        assert_eq!(
            config.waivers[0].reason,
            "Intentional API redesign for v2.0"
        );
        assert_eq!(config.waivers[0].ticket, Some("GH#42".to_string()));
        assert_eq!(config.waivers[0].expires, Some("2025-06-01".to_string()));
        assert_eq!(config.waivers[1].ticket, None);
        assert_eq!(config.waivers[1].expires, None);
    }

    #[test]
    fn test_no_waivers_default() {
        let config = SemverguardConfig::default();
        assert!(config.waivers.is_empty());
    }

    #[test]
    fn test_run_mode_detect_from_env_release_github_tag() {
        assert_eq!(
            detect_with_env(&[("GITHUB_REF", "refs/tags/v1.2.3")]),
            RunMode::Release
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_release_gitlab_tag() {
        assert_eq!(
            detect_with_env(&[("CI_COMMIT_TAG", "v1.0.0")]),
            RunMode::Release
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_pr_github_event() {
        assert_eq!(
            detect_with_env(&[("GITHUB_EVENT_NAME", "pull_request")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_pr_gitlab() {
        assert_eq!(
            detect_with_env(&[("CI_PIPELINE_SOURCE", "merge_request_event")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_pr_azure_circle() {
        assert_eq!(
            detect_with_env(&[("SYSTEM_PULLREQUEST_PULLREQUESTID", "123")]),
            RunMode::Pr
        );
        assert_eq!(
            detect_with_env(&[("CIRCLE_PULL_REQUEST", "url")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_default_pr() {
        assert_eq!(detect_with_env(&[]), RunMode::Pr);
    }

    #[test]
    fn test_run_mode_detect_from_env_non_tag_ref_defaults_to_pr() {
        assert_eq!(
            detect_with_env(&[("GITHUB_REF", "refs/heads/main")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_non_pr_event_defaults_to_pr() {
        assert_eq!(
            detect_with_env(&[("GITHUB_EVENT_NAME", "push")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_non_merge_request_defaults_to_pr() {
        assert_eq!(
            detect_with_env(&[("CI_PIPELINE_SOURCE", "schedule")]),
            RunMode::Pr
        );
    }

    #[test]
    fn test_run_mode_detect_from_env_smoke() {
        let detected = RunMode::detect_from_env();
        assert!([RunMode::Pr, RunMode::Release].contains(&detected));

        let resolved = RunMode::Auto.resolve();
        assert!([RunMode::Pr, RunMode::Release].contains(&resolved));
    }

    #[test]
    fn test_color_choice_default() {
        assert_eq!(ColorChoice::default(), ColorChoice::Auto);
    }

    #[test]
    fn test_verbosity_flags() {
        assert!(Verbosity::Verbose.shows_passed());
        assert!(Verbosity::Debug.shows_passed());
        assert!(!Verbosity::Quiet.shows_skipped());
        assert!(Verbosity::Normal.shows_skipped());
        assert!(Verbosity::Verbose.shows_timing());
        assert!(Verbosity::Debug.shows_commands());
        assert!(Verbosity::Debug.shows_full_output());
        assert!(Verbosity::Debug.shows_config());
        assert!(Verbosity::Quiet.is_quiet());
    }
}
