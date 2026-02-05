use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Run mode for semverguard.
///
/// Modes provide sensible default behaviors for common CI scenarios:
/// - `Pr` mode: For pull request lanes, tolerant of baseline errors
/// - `Release` mode: For release/tag lanes, strict enforcement
/// - `Auto` mode: Detect from CI environment variables
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
}

impl RunMode {
    /// Detect the run mode from environment variables.
    ///
    /// Returns the detected mode, or `Pr` if detection is ambiguous.
    pub fn detect_from_env() -> RunMode {
        // GitHub Actions: check for tag ref
        if let Ok(github_ref) = std::env::var("GITHUB_REF") {
            if github_ref.starts_with("refs/tags/") {
                return RunMode::Release;
            }
        }

        // GitLab CI: check for commit tag
        if std::env::var("CI_COMMIT_TAG").is_ok() {
            return RunMode::Release;
        }

        // GitHub Actions: check for pull_request event
        if let Ok(event_name) = std::env::var("GITHUB_EVENT_NAME") {
            if event_name == "pull_request" || event_name == "pull_request_target" {
                return RunMode::Pr;
            }
        }

        // GitLab CI: check for merge request pipeline
        if let Ok(source) = std::env::var("CI_PIPELINE_SOURCE") {
            if source == "merge_request_event" {
                return RunMode::Pr;
            }
        }

        // Azure DevOps: check for pull request
        if std::env::var("SYSTEM_PULLREQUEST_PULLREQUESTID").is_ok() {
            return RunMode::Pr;
        }

        // CircleCI: check for pull request
        if std::env::var("CIRCLE_PULL_REQUEST").is_ok() {
            return RunMode::Pr;
        }

        // Default to Pr mode (safer for CI)
        RunMode::Pr
    }

    /// Resolve Auto mode to a concrete mode using environment detection.
    pub fn resolve(self) -> RunMode {
        match self {
            RunMode::Auto => RunMode::detect_from_env(),
            other => other,
        }
    }

    /// Returns true if baseline errors should be treated as warnings.
    ///
    /// In Pr mode, baseline errors are warnings (don't fail CI).
    /// In Release mode, baseline errors are failures.
    pub fn baseline_errors_are_warnings(&self) -> bool {
        matches!(self.resolve(), RunMode::Pr)
    }

    /// Returns the suggested default scope mode for this run mode.
    pub fn suggested_scope_mode(&self) -> ScopeMode {
        match self.resolve() {
            RunMode::Pr | RunMode::Auto => ScopeMode::Changed,
            RunMode::Release => ScopeMode::Workspace,
        }
    }
}

/// Top-level configuration for semverguard.
///
/// Intended to be loaded from `semverguard.toml`, with CLI flags overriding.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for SemverguardConfig {
    fn default() -> Self {
        Self {
            mode: RunMode::default(),
            baseline: BaselineConfig::default(),
            scope: ScopeConfig::default(),
            features: FeaturesConfig::default(),
            engine: EngineConfig::default(),
            output: OutputConfig::default(),
        }
    }
}

/// Baseline selection kind.
///
/// - `crates-io`: compare against a published version on crates.io (or registries).
/// - `git`: compare against the workspace state at a git revision.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BaselineKind {
    /// Compare against a released version.
    CratesIo,
    /// Compare against a git revision.
    Git,
}

impl Default for BaselineKind {
    fn default() -> Self {
        BaselineKind::CratesIo
    }
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeMode {
    /// Check all eligible workspace packages.
    Workspace,
    /// Check only workspace packages that changed relative to the baseline revision.
    ///
    /// Requires `baseline.kind = "git"` and a configured baseline revision.
    Changed,
}

impl Default for ScopeMode {
    fn default() -> Self {
        ScopeMode::Workspace
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Human-readable summary to stdout.
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

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Text
    }
}

/// Color output choice for terminal rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ColorChoice {
    /// Automatically detect if terminal supports colors.
    Auto,
    /// Always use colors, even when output is redirected.
    Always,
    /// Never use colors.
    Never,
}

impl Default for ColorChoice {
    fn default() -> Self {
        ColorChoice::Auto
    }
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
        matches!(self, Verbosity::Verbose | Verbosity::Debug)
    }

    /// Returns true if this verbosity level shows skipped packages.
    pub fn shows_skipped(&self) -> bool {
        !matches!(self, Verbosity::Quiet)
    }

    /// Returns true if this verbosity level shows timing information.
    pub fn shows_timing(&self) -> bool {
        matches!(self, Verbosity::Verbose | Verbosity::Debug)
    }

    /// Returns true if this verbosity level shows commands executed.
    pub fn shows_commands(&self) -> bool {
        matches!(self, Verbosity::Verbose | Verbosity::Debug)
    }

    /// Returns true if this verbosity level shows full engine output.
    pub fn shows_full_output(&self) -> bool {
        matches!(self, Verbosity::Debug)
    }

    /// Returns true if this verbosity level shows effective config.
    pub fn shows_config(&self) -> bool {
        matches!(self, Verbosity::Debug)
    }

    /// Returns true if this is quiet mode (suppress most output).
    pub fn is_quiet(&self) -> bool {
        matches!(self, Verbosity::Quiet)
    }
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

    // =========================================================================
    // Default implementation tests
    // =========================================================================

    #[test]
    fn test_semverguard_config_default() {
        let config = SemverguardConfig::default();
        assert!(matches!(config.mode, RunMode::Auto));
        assert!(matches!(config.baseline.kind, BaselineKind::CratesIo));
        assert!(matches!(config.scope.mode, ScopeMode::Workspace));
        assert!(matches!(config.output.format, OutputFormat::Text));
    }

    // =========================================================================
    // RunMode tests
    // =========================================================================

    #[test]
    fn test_run_mode_default() {
        let mode = RunMode::default();
        assert!(matches!(mode, RunMode::Auto));
    }

    #[test]
    fn test_run_mode_json_roundtrip() {
        for mode in [RunMode::Auto, RunMode::Pr, RunMode::Release] {
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
    }

    #[test]
    fn test_run_mode_baseline_errors_are_warnings() {
        assert!(RunMode::Pr.baseline_errors_are_warnings());
        assert!(!RunMode::Release.baseline_errors_are_warnings());
    }

    #[test]
    fn test_run_mode_suggested_scope_mode() {
        assert!(matches!(
            RunMode::Pr.suggested_scope_mode(),
            ScopeMode::Changed
        ));
        assert!(matches!(
            RunMode::Release.suggested_scope_mode(),
            ScopeMode::Workspace
        ));
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
    fn test_baseline_kind_default() {
        let kind = BaselineKind::default();
        assert!(matches!(kind, BaselineKind::CratesIo));
    }

    #[test]
    fn test_baseline_config_default() {
        let config = BaselineConfig::default();
        assert!(matches!(config.kind, BaselineKind::CratesIo));
        assert!(config.version.is_none());
        assert!(config.rev.is_none());
        assert!(config.root.is_none());
        assert!(config.rustdoc.is_none());
    }

    #[test]
    fn test_scope_config_default() {
        let config = ScopeConfig::default();
        assert!(matches!(config.mode, ScopeMode::Workspace));
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
        assert!(config.skip_publish_false);
        assert!(config.skip_no_lib);
    }

    #[test]
    fn test_scope_mode_default() {
        let mode = ScopeMode::default();
        assert!(matches!(mode, ScopeMode::Workspace));
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
        assert!(matches!(format, OutputFormat::Text));
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert!(matches!(config.format, OutputFormat::Text));
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

        assert!(matches!(deserialized.baseline.kind, BaselineKind::CratesIo));
        assert!(matches!(deserialized.scope.mode, ScopeMode::Workspace));
    }

    #[test]
    fn test_baseline_kind_json_roundtrip() {
        for kind in [BaselineKind::CratesIo, BaselineKind::Git] {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: BaselineKind = serde_json::from_str(&json).unwrap();
            assert!(matches!(
                (kind, deserialized),
                (BaselineKind::CratesIo, BaselineKind::CratesIo)
                    | (BaselineKind::Git, BaselineKind::Git)
            ));
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

        assert!(matches!(deserialized.kind, BaselineKind::Git));
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
            assert!(matches!(
                (mode, deserialized),
                (ScopeMode::Workspace, ScopeMode::Workspace)
                    | (ScopeMode::Changed, ScopeMode::Changed)
            ));
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

        assert!(matches!(deserialized.mode, ScopeMode::Changed));
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
            assert!(matches!(
                (format, deserialized),
                (OutputFormat::Text, OutputFormat::Text)
                    | (OutputFormat::Json, OutputFormat::Json)
                    | (OutputFormat::Both, OutputFormat::Both)
                    | (OutputFormat::Receipt, OutputFormat::Receipt)
            ));
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

        assert!(matches!(deserialized.format, OutputFormat::Both));
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
        assert!(matches!(config.mode, RunMode::Auto));
        assert!(matches!(config.baseline.kind, BaselineKind::CratesIo));
        assert!(matches!(config.scope.mode, ScopeMode::Workspace));
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
        assert!(matches!(config.mode, RunMode::Release));

        // Baseline
        assert!(matches!(config.baseline.kind, BaselineKind::Git));
        assert_eq!(config.baseline.version, Some("2.0.0".to_string()));
        assert_eq!(config.baseline.rev, Some("v2.0.0".to_string()));
        assert_eq!(config.baseline.root, Some(PathBuf::from("/some/path")));
        assert_eq!(
            config.baseline.rustdoc,
            Some(PathBuf::from("/docs/rustdoc.json"))
        );

        // Scope
        assert!(matches!(config.scope.mode, ScopeMode::Changed));
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
        assert!(matches!(config.output.format, OutputFormat::Both));
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
        assert!(matches!(config.kind, BaselineKind::CratesIo));

        let toml_str = r#"kind = "git""#;
        let config: BaselineConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.kind, BaselineKind::Git));
    }

    #[test]
    fn test_scope_mode_toml_kebab_case() {
        let toml_str = r#"mode = "workspace""#;
        let config: ScopeConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.mode, ScopeMode::Workspace));

        let toml_str = r#"mode = "changed""#;
        let config: ScopeConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.mode, ScopeMode::Changed));
    }

    #[test]
    fn test_run_mode_toml_kebab_case() {
        let toml_str = r#"mode = "auto""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.mode, RunMode::Auto));

        let toml_str = r#"mode = "pr""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.mode, RunMode::Pr));

        let toml_str = r#"mode = "release""#;
        let config: SemverguardConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.mode, RunMode::Release));
    }

    #[test]
    fn test_output_format_toml_kebab_case() {
        let toml_str = r#"format = "text""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.format, OutputFormat::Text));

        let toml_str = r#"format = "json""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.format, OutputFormat::Json));

        let toml_str = r#"format = "both""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.format, OutputFormat::Both));

        let toml_str = r#"format = "receipt""#;
        let config: OutputConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.format, OutputFormat::Receipt));
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
        assert!(matches!(config.baseline.kind, BaselineKind::CratesIo));
        assert!(config.baseline.version.is_none());
        assert_eq!(config.baseline.rev, Some("origin/main".to_string()));

        // Scope: defaults except exclude
        assert!(matches!(config.scope.mode, ScopeMode::Workspace));
        assert!(config.scope.include.is_empty());
        assert_eq!(config.scope.exclude, vec!["test-*"]);
        assert!(config.scope.skip_publish_false);

        // Features: all defaults
        assert!(!config.features.all_features);
        assert!(config.features.default_features);

        // Engine: all defaults
        assert!(config.engine.cargo_bin.is_none());

        // Output: all defaults
        assert!(matches!(config.output.format, OutputFormat::Text));
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
}
