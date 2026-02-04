use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration for semverguard.
///
/// Intended to be loaded from `semverguard.toml`, with CLI flags overriding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SemverguardConfig {
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
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            kind: BaselineKind::CratesIo,
            version: None,
            rev: None,
            root: None,
            rustdoc: None,
        }
    }
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
        assert!(matches!(config.baseline.kind, BaselineKind::CratesIo));
        assert!(matches!(config.scope.mode, ScopeMode::Workspace));
        assert!(matches!(config.output.format, OutputFormat::Text));
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
        assert_eq!(
            config.artifacts_dir,
            PathBuf::from("artifacts/semverguard")
        );
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
        assert!(matches!(config.baseline.kind, BaselineKind::CratesIo));
        assert!(matches!(config.scope.mode, ScopeMode::Workspace));
    }

    #[test]
    fn test_semverguard_config_toml_full() {
        let toml_str = r#"
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
