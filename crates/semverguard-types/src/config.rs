use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration for semverguard.
///
/// Intended to be loaded from `semverguard.toml`, with CLI flags overriding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
pub struct ScopeConfig {
    /// Mode for selecting packages.
    pub mode: ScopeMode,

    /// Include package-name globs. Empty means “include everything”.
    pub include: Vec<String>,

    /// Exclude package-name globs.
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
#[serde(default)]
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
#[serde(default)]
pub struct EngineConfig {
    /// Which `cargo` binary to run.
    ///
    /// If unset, uses `"cargo"` from PATH.
    pub cargo_bin: Option<PathBuf>,

    /// Extra arguments appended to `cargo semver-checks check-release`.
    pub extra_args: Vec<String>,

    /// Stop after the first failure.
    pub fail_fast: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            cargo_bin: None,
            extra_args: vec![],
            fail_fast: false,
        }
    }
}

/// Output format.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Human-readable summary to stdout.
    Text,
    /// JSON report (to a file or stdout).
    Json,
    /// Both text and JSON.
    Both,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Text
    }
}

/// Output/reporting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Which formats to emit.
    pub format: OutputFormat,

    /// Where to write JSON (when JSON output is enabled). If None, JSON prints to stdout.
    pub json_path: Option<PathBuf>,

    /// Pretty-print the JSON report.
    pub pretty_json: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Text,
            json_path: None,
            pretty_json: true,
        }
    }
}
