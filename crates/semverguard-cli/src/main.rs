use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use globset::Glob;
use semverguard_domain::SemverguardRunner;
use semverguard_engine::CargoSemverChecksEngine;
use semverguard_git::GitCli;
use semverguard_types::{
    BaselineKind, FailureKind, ListResult, OutputFormat, PackageStatus, RunMode, RunReport,
    ScopeMode, SemverguardConfig,
};
use semverguard_workspace::CargoMetadataWorkspace;
use std::fs;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

mod comment;
mod progress;
mod receipt;
mod sarif;
use progress::{create_progress_reporter, ProgressCallbackAdapter, ProgressChoice};
use receipt::{
    build_artifact_index, build_receipt, exit_code_from_receipt, has_tool_error,
    resolve_artifacts_dir, write_receipt_bundle, ToolErrorFinding,
};

#[derive(Parser, Debug)]
#[command(
    name = "semverguard",
    version,
    about = "Workspace orchestration for cargo-semver-checks"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run semver checks across the workspace.
    Check(CheckArgs),
    /// List packages that would be checked without running checks.
    List(ListArgs),
    /// Print the effective configuration (after file + CLI overrides).
    PrintConfig(PrintConfigArgs),
    /// Validate the configuration file for errors and warnings.
    ValidateConfig(ValidateConfigArgs),
}

#[derive(Parser, Debug)]
struct PrintConfigArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Workspace root (defaults to .)
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,
}

#[derive(Parser, Debug)]
struct ValidateConfigArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ListArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Workspace root (defaults to .)
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,

    /// Use git baseline selection (`--baseline-rev` passed to cargo-semver-checks).
    #[arg(long)]
    baseline_rev: Option<String>,

    /// Only check packages changed relative to the baseline revision.
    #[arg(long)]
    changed: bool,

    /// Output as JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct CheckArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Workspace root (defaults to .)
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,

    /// Run mode: pr, release, or auto (default).
    ///
    /// Modes provide sensible default behaviors for common CI scenarios:
    /// - pr: For pull request lanes. Baseline errors -> warn and skip.
    /// - release: For release/tag lanes. Strict enforcement.
    /// - auto: Detect from CI environment variables (GITHUB_REF, CI_COMMIT_TAG, etc.)
    #[arg(long, value_enum)]
    mode: Option<RunModeOpt>,

    /// Use git baseline selection (`--baseline-rev` passed to cargo-semver-checks).
    #[arg(long)]
    baseline_rev: Option<String>,

    /// Use crates.io baseline selection (`--baseline-version` passed to cargo-semver-checks).
    #[arg(long)]
    baseline_version: Option<String>,

    /// Only check packages changed relative to the baseline revision.
    #[arg(long)]
    changed: bool,

    /// Show what would be checked without actually running checks.
    #[arg(long)]
    dry_run: bool,

    /// Output JSON report to this path.
    #[arg(long)]
    json: Option<PathBuf>,

    /// Output SARIF report to this path.
    ///
    /// SARIF (Static Analysis Results Interchange Format) is supported by GitHub
    /// Code Scanning, VS Code SARIF Viewer, and other security analysis tools.
    /// Equivalent to --format sarif --json <path>.
    #[arg(long)]
    sarif: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum)]
    format: Option<FormatOpt>,

    /// Output directory for receipt artifacts (defaults to artifacts/semverguard).
    #[arg(long)]
    artifacts_dir: Option<PathBuf>,

    /// Stop after first failing crate.
    #[arg(long)]
    fail_fast: bool,

    /// Which `cargo` binary to run (defaults to cargo on PATH).
    #[arg(long)]
    cargo_bin: Option<PathBuf>,

    /// Extra args appended to `cargo semver-checks check-release` (repeatable).
    #[arg(long = "engine-arg")]
    engine_args: Vec<String>,

    /// When to show progress indicators.
    ///
    /// "auto" shows a spinner and progress bar if stderr is a TTY.
    /// "always" shows progress indicators even when output is redirected.
    /// "never" disables progress indicators entirely.
    #[arg(long, value_enum, default_value = "auto")]
    progress: ProgressChoiceOpt,
}

/// Progress display choice for CLI.
#[derive(Clone, Debug, ValueEnum)]
enum ProgressChoiceOpt {
    /// Automatically detect if terminal supports progress display (default).
    Auto,
    /// Always show progress, even when output is redirected.
    Always,
    /// Never show progress indicators.
    Never,
}

impl From<ProgressChoiceOpt> for ProgressChoice {
    fn from(v: ProgressChoiceOpt) -> Self {
        match v {
            ProgressChoiceOpt::Auto => ProgressChoice::Auto,
            ProgressChoiceOpt::Always => ProgressChoice::Always,
            ProgressChoiceOpt::Never => ProgressChoice::Never,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum FormatOpt {
    Text,
    Json,
    Both,
    /// SARIF (Static Analysis Results Interchange Format) for CI tools.
    Sarif,
    /// Cockpit receipt output (sensor.report.v1).
    Receipt,
}

impl From<FormatOpt> for OutputFormat {
    fn from(v: FormatOpt) -> Self {
        match v {
            FormatOpt::Text => OutputFormat::Text,
            FormatOpt::Json => OutputFormat::Json,
            FormatOpt::Both => OutputFormat::Both,
            FormatOpt::Sarif => OutputFormat::Sarif,
            FormatOpt::Receipt => OutputFormat::Receipt,
        }
    }
}

/// Run mode choice for CLI.
#[derive(Clone, Debug, ValueEnum)]
enum RunModeOpt {
    /// Automatically detect mode from CI environment (default).
    Auto,
    /// Pull request mode: tolerant of baseline errors.
    Pr,
    /// Release mode: strict enforcement.
    Release,
}

impl From<RunModeOpt> for RunMode {
    fn from(v: RunModeOpt) -> Self {
        match v {
            RunModeOpt::Auto => RunMode::Auto,
            RunModeOpt::Pr => RunMode::Pr,
            RunModeOpt::Release => RunMode::Release,
        }
    }
}

fn main() -> std::process::ExitCode {
    let exit_code = match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:?}");
            1
        }
    };
    std::process::ExitCode::from(exit_code as u8)
}

fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::PrintConfig(args) => {
            let cfg_path = args
                .config
                .unwrap_or_else(|| PathBuf::from("semverguard.toml"));
            let config = load_config(&cfg_path)?;
            // Nothing else to override (yet) besides ensuring workspace_root exists.
            ensure_workspace_root(&args.workspace_root)?;
            print_config(&config)?;
            Ok(0)
        }
        Commands::ValidateConfig(args) => {
            run_validate_config(&args)?;
            Ok(0)
        }
        Commands::List(args) => {
            run_list(&args)?;
            Ok(0)
        }
        Commands::Check(args) => run_check(&args),
    }
}

fn ensure_workspace_root(workspace_root: &Path) -> Result<()> {
    let md = fs::metadata(workspace_root).with_context(|| {
        format!(
            "workspace root does not exist: {}",
            workspace_root.display()
        )
    })?;
    if !md.is_dir() {
        anyhow::bail!(
            "workspace root is not a directory: {}",
            workspace_root.display()
        );
    }
    Ok(())
}

fn load_config(path: &Path) -> Result<SemverguardConfig> {
    if !path.exists() {
        // Use defaults; missing config is not an error.
        return Ok(SemverguardConfig::default());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let cfg: SemverguardConfig =
        toml::from_str(&raw).with_context(|| format!("invalid TOML in {}", path.display()))?;
    Ok(cfg)
}

fn print_config(cfg: &SemverguardConfig) -> Result<()> {
    let out = toml::to_string_pretty(cfg).context("failed to serialize config to TOML")?;
    print!("{out}");
    Ok(())
}

fn apply_cli_overrides(cfg: &mut SemverguardConfig, args: &CheckArgs) {
    // Apply mode from CLI (takes precedence over config file)
    if let Some(mode) = &args.mode {
        cfg.mode = mode.clone().into();
    }

    if args.changed {
        cfg.scope.mode = semverguard_types::ScopeMode::Changed;
    }

    if let Some(rev) = &args.baseline_rev {
        cfg.baseline.kind = semverguard_types::BaselineKind::Git;
        cfg.baseline.rev = Some(rev.clone());
    }
    if let Some(ver) = &args.baseline_version {
        cfg.baseline.kind = semverguard_types::BaselineKind::CratesIo;
        cfg.baseline.version = Some(ver.clone());
    }

    if args.fail_fast {
        cfg.engine.fail_fast = true;
    }
    if let Some(cargo_bin) = &args.cargo_bin {
        cfg.engine.cargo_bin = Some(cargo_bin.clone());
    }
    if !args.engine_args.is_empty() {
        cfg.engine.extra_args.extend(args.engine_args.clone());
    }

    if let Some(fmt) = &args.format {
        cfg.output.format = fmt.clone().into();
    }

    if let Some(json_path) = &args.json {
        cfg.output.json_path = Some(json_path.clone());
        // If the user asked for JSON explicitly, default to both unless they also set --format.
        if args.format.is_none() && matches!(cfg.output.format, OutputFormat::Text) {
            cfg.output.format = OutputFormat::Both;
        }
    }

    // Handle --sarif shorthand flag (takes precedence over --json if both specified)
    if let Some(sarif_path) = &args.sarif {
        if matches!(cfg.output.format, OutputFormat::Receipt) {
            // Receipt mode can still emit SARIF, but keep receipt as primary format.
        } else {
            cfg.output.format = OutputFormat::Sarif;
            cfg.output.json_path = Some(sarif_path.clone());
        }
    }

    if let Some(dir) = &args.artifacts_dir {
        cfg.output.artifacts_dir = dir.clone();
    }
}

fn run_check(args: &CheckArgs) -> Result<i32> {
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("semverguard.toml"));

    let cfg_result = load_config(&cfg_path);
    let mut config = match &cfg_result {
        Ok(cfg) => cfg.clone(),
        Err(_) => SemverguardConfig::default(),
    };
    apply_cli_overrides(&mut config, args);

    let receipt_requested = matches!(config.output.format, OutputFormat::Receipt);
    let sarif_requested = receipt_requested && args.sarif.is_some();
    let artifacts_root = resolve_artifacts_dir(&args.workspace_root, &config.output.artifacts_dir);

    if let Err(e) = cfg_result {
        return handle_tool_error(
            e,
            &config,
            &args.workspace_root,
            &artifacts_root,
            receipt_requested,
            sarif_requested,
        );
    }

    if let Err(e) = ensure_workspace_root(&args.workspace_root) {
        return handle_tool_error(
            e,
            &config,
            &args.workspace_root,
            &artifacts_root,
            receipt_requested,
            sarif_requested,
        );
    }

    // Wire adapters.
    let workspace = CargoMetadataWorkspace::default();
    let git = GitCli::default();
    let engine = CargoSemverChecksEngine::default();

    // Create progress reporter based on CLI flag
    let progress_choice: ProgressChoice = args.progress.clone().into();
    let progress_reporter = create_progress_reporter(progress_choice);
    let progress_callback = ProgressCallbackAdapter::new(progress_reporter);

    let runner =
        SemverguardRunner::with_progress(&workspace, Some(&git), &engine, progress_callback);

    // Handle --dry-run: show what would be checked without actually running
    if args.dry_run {
        let list_result = runner.list_packages(&args.workspace_root, &config)?;
        print_list_text(&list_result);
        return Ok(0);
    }

    let started = OffsetDateTime::now_utc();
    let artifacts = match runner.run(&args.workspace_root, &config) {
        Ok(artifacts) => artifacts,
        Err(e) => {
            return handle_tool_error(
                anyhow::Error::new(e),
                &config,
                &args.workspace_root,
                &artifacts_root,
                receipt_requested,
                sarif_requested,
            );
        }
    };
    let finished = OffsetDateTime::now_utc();

    let report = RunReport {
        semverguard_version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: started
            .format(&Rfc3339)
            .unwrap_or_else(|_| started.unix_timestamp().to_string()),
        finished_at: finished
            .format(&Rfc3339)
            .unwrap_or_else(|_| finished.unix_timestamp().to_string()),
        workspace_root: artifacts.workspace_root,
        packages: artifacts.packages,
        summary: artifacts.summary,
    };

    if receipt_requested {
        let artifact_index = build_artifact_index(
            &args.workspace_root,
            &artifacts_root,
            Some(&report),
            sarif_requested,
        );
        let receipt = build_receipt(
            Some(&report),
            &[],
            &artifact_index,
            &config.baseline,
            &args.workspace_root,
        );
        write_receipt_bundle(
            &artifacts_root,
            &receipt,
            Some(&report),
            sarif_requested,
            config.output.pretty_json,
        )?;
        let code = exit_code_from_receipt(
            &receipt.verdict,
            config.output.warn_as_fail,
            has_tool_error(&receipt.findings),
        );
        return Ok(code);
    }

    // Resolve the run mode (auto-detect from environment if needed)
    let resolved_mode = config.mode.resolve();

    emit_outputs(&config, &report)?;
    Ok(exit_code_from_report(
        &report,
        resolved_mode,
        config.output.warn_as_fail,
    ))
}

fn handle_tool_error(
    err: anyhow::Error,
    config: &SemverguardConfig,
    workspace_root: &Path,
    artifacts_root: &Path,
    receipt_requested: bool,
    sarif_requested: bool,
) -> Result<i32> {
    if !receipt_requested {
        return Err(err);
    }

    eprintln!("error: {err:?}");
    let errors = vec![ToolErrorFinding::new(err.to_string())];
    let artifact_index =
        build_artifact_index(workspace_root, artifacts_root, None, sarif_requested);
    let receipt = build_receipt(
        None,
        &errors,
        &artifact_index,
        &config.baseline,
        workspace_root,
    );
    write_receipt_bundle(
        artifacts_root,
        &receipt,
        None,
        sarif_requested,
        config.output.pretty_json,
    )?;
    let code = exit_code_from_receipt(
        &receipt.verdict,
        config.output.warn_as_fail,
        has_tool_error(&receipt.findings),
    );
    Ok(code)
}

fn exit_code_from_report(
    report: &RunReport,
    resolved_mode: RunMode,
    warn_as_fail: bool,
) -> i32 {
    let mut has_tool_error_flag = false;
    let mut has_semver_violation = false;
    let mut has_baseline_error = false;

    for pkg in &report.packages {
        if pkg.status != PackageStatus::Failed {
            continue;
        }
        match pkg.failure_kind.unwrap_or(FailureKind::Unknown) {
            FailureKind::ToolError => has_tool_error_flag = true,
            FailureKind::BaselineError => has_baseline_error = true,
            FailureKind::SemverViolation | FailureKind::Unknown => has_semver_violation = true,
        }
    }

    if has_tool_error_flag {
        1
    } else if has_semver_violation {
        2
    } else if has_baseline_error {
        // In Pr mode, baseline errors are warnings (exit 0 unless warn_as_fail is set)
        // In Release mode, baseline errors are failures (exit 3)
        let baseline_errors_are_warnings = resolved_mode.baseline_errors_are_warnings();
        if baseline_errors_are_warnings && !warn_as_fail {
            0
        } else {
            3
        }
    } else {
        0
    }
}

fn emit_outputs(cfg: &SemverguardConfig, report: &RunReport) -> Result<()> {
    match cfg.output.format {
        OutputFormat::Text => {
            print_text(report);
        }
        OutputFormat::Json => {
            write_json(cfg, report)?;
        }
        OutputFormat::Both => {
            print_text(report);
            write_json(cfg, report)?;
        }
        OutputFormat::Sarif => {
            write_sarif(cfg, report)?;
        }
        OutputFormat::Receipt => {
            // Receipt output is handled separately.
        }
    }
    Ok(())
}

fn print_text(report: &RunReport) {
    println!(
        "semverguard: total={} passed={} failed={} skipped={}",
        report.summary.total, report.summary.passed, report.summary.failed, report.summary.skipped
    );

    for p in &report.packages {
        match p.status {
            semverguard_types::PackageStatus::Passed => {}
            semverguard_types::PackageStatus::Skipped => {
                println!(
                    "SKIP  {} {}  ({})",
                    p.name,
                    p.version,
                    p.skip_reason.clone().unwrap_or_default()
                );
            }
            semverguard_types::PackageStatus::Failed => {
                println!(
                    "FAIL  {} {}  {}",
                    p.name,
                    p.version,
                    p.manifest_path.display()
                );
                if let Some(b) = p.inferred_required_bump {
                    println!("      inferred required bump: {:?}", b);
                }
                if let Some(engine) = &p.engine {
                    // Print a small tail of stderr first; it's usually the actionable part.
                    let tail = tail_lines(&engine.stderr, 20);
                    if !tail.trim().is_empty() {
                        println!("      stderr (tail):\n{}", indent(&tail, "      "));
                    }
                } else if let Some(r) = &p.skip_reason {
                    println!("      {}", r);
                }
            }
        }
    }
}

fn write_json(cfg: &SemverguardConfig, report: &RunReport) -> Result<()> {
    let json = if cfg.output.pretty_json {
        serde_json::to_string_pretty(report)?
    } else {
        serde_json::to_string(report)?
    };

    match &cfg.output.json_path {
        Some(path) => {
            fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
        }
        None => {
            println!("{json}");
        }
    }
    Ok(())
}

fn write_sarif(cfg: &SemverguardConfig, report: &RunReport) -> Result<()> {
    let sarif_log = sarif::report_to_sarif(report);
    let json = sarif::sarif_to_json(&sarif_log, cfg.output.pretty_json)
        .context("failed to serialize SARIF report")?;

    match &cfg.output.json_path {
        Some(path) => {
            fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
        }
        None => {
            println!("{json}");
        }
    }
    Ok(())
}

fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<String>>()
        .join("\n")
}

// =============================================================================
// Config Validation
// =============================================================================

struct ValidationResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl ValidationResult {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add_error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}

fn run_validate_config(args: &ValidateConfigArgs) -> Result<()> {
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("semverguard.toml"));

    if !cfg_path.exists() {
        println!("Config file not found: {}", cfg_path.display());
        println!("Using default configuration (which is valid).");
        return Ok(());
    }

    let config = load_config(&cfg_path)?;
    let result = validate_config(&config);

    if result.errors.is_empty() && result.warnings.is_empty() {
        println!("Configuration is valid: {}", cfg_path.display());
        return Ok(());
    }

    if !result.warnings.is_empty() {
        println!("Warnings:");
        for warning in &result.warnings {
            println!("  - {}", warning);
        }
    }

    if !result.errors.is_empty() {
        if !result.warnings.is_empty() {
            println!();
        }
        println!("Errors:");
        for error in &result.errors {
            println!("  - {}", error);
        }
        anyhow::bail!(
            "Configuration validation failed with {} error(s)",
            result.errors.len()
        );
    }

    Ok(())
}

fn validate_config(config: &SemverguardConfig) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Validate glob patterns
    for (i, pattern) in config.scope.include.iter().enumerate() {
        if let Err(e) = Glob::new(pattern) {
            result.add_error(format!(
                "Invalid glob in scope.include[{}] \"{}\": {}",
                i, pattern, e
            ));
        }
    }

    for (i, pattern) in config.scope.exclude.iter().enumerate() {
        if let Err(e) = Glob::new(pattern) {
            result.add_error(format!(
                "Invalid glob in scope.exclude[{}] \"{}\": {}",
                i, pattern, e
            ));
        }
    }

    // Validate baseline configuration
    match config.baseline.kind {
        BaselineKind::Git => {
            if config.baseline.rev.is_none() {
                result
                    .add_error("baseline.kind is \"git\" but baseline.rev is not set".to_string());
            }
            if config.baseline.version.is_some() {
                result.add_warning(
                    "baseline.version is set but baseline.kind is \"git\"; version will be ignored"
                        .to_string(),
                );
            }
        }
        BaselineKind::CratesIo => {
            if config.baseline.rev.is_some() {
                result.add_warning(
                    "baseline.rev is set but baseline.kind is \"crates-io\"; rev will be ignored"
                        .to_string(),
                );
            }
        }
    }

    // Validate scope mode consistency
    if matches!(config.scope.mode, ScopeMode::Changed) {
        if !matches!(config.baseline.kind, BaselineKind::Git) {
            result.add_error(
                "scope.mode is \"changed\" requires baseline.kind = \"git\"".to_string(),
            );
        }
        if config.baseline.rev.is_none() {
            result
                .add_error("scope.mode is \"changed\" requires baseline.rev to be set".to_string());
        }
    }

    // Validate file paths
    if let Some(ref root) = config.baseline.root {
        if !root.exists() {
            result.add_error(format!("baseline.root does not exist: {}", root.display()));
        } else if !root.is_dir() {
            result.add_error(format!(
                "baseline.root is not a directory: {}",
                root.display()
            ));
        }
    }

    if let Some(ref rustdoc) = config.baseline.rustdoc {
        if !rustdoc.exists() {
            result.add_error(format!(
                "baseline.rustdoc does not exist: {}",
                rustdoc.display()
            ));
        } else if !rustdoc.is_file() {
            result.add_error(format!(
                "baseline.rustdoc is not a file: {}",
                rustdoc.display()
            ));
        }
    }

    if let Some(ref cargo_bin) = config.engine.cargo_bin {
        if !cargo_bin.exists() {
            result.add_warning(format!(
                "engine.cargo_bin does not exist: {}",
                cargo_bin.display()
            ));
        }
    }

    // Validate features configuration
    if config.features.all_features && config.features.only_explicit_features {
        result.add_warning(
            "all_features and only_explicit_features both true; all_features takes precedence"
                .to_string(),
        );
    }

    if config.features.only_explicit_features && config.features.features.is_empty() {
        result.add_warning("only_explicit_features is true but features list is empty".to_string());
    }

    result
}

// =============================================================================
// List Command
// =============================================================================

fn run_list(args: &ListArgs) -> Result<()> {
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("semverguard.toml"));
    ensure_workspace_root(&args.workspace_root)?;
    let mut config = load_config(&cfg_path)?;

    // Apply CLI overrides for list command
    if args.changed {
        config.scope.mode = semverguard_types::ScopeMode::Changed;
    }
    if let Some(rev) = &args.baseline_rev {
        config.baseline.kind = semverguard_types::BaselineKind::Git;
        config.baseline.rev = Some(rev.clone());
    }

    // Wire adapters (we don't need engine for list)
    let workspace = CargoMetadataWorkspace::default();
    let git = GitCli::default();
    let engine = CargoSemverChecksEngine::default();

    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let list_result = runner.list_packages(&args.workspace_root, &config)?;

    if args.json {
        let json = serde_json::to_string_pretty(&list_result)?;
        println!("{json}");
    } else {
        print_list_text(&list_result);
    }

    Ok(())
}

fn print_list_text(result: &ListResult) {
    println!("Workspace: {}", result.workspace_root.display());
    println!();

    if !result.would_check.is_empty() {
        println!("Would check ({} packages):", result.would_check.len());
        for pkg in &result.would_check {
            println!("  {} {}", pkg.name, pkg.version);
        }
    } else {
        println!("Would check: (none)");
    }

    println!();

    if !result.would_skip.is_empty() {
        println!("Would skip ({} packages):", result.would_skip.len());
        for pkg in &result.would_skip {
            println!("  {} {} ({})", pkg.name, pkg.version, pkg.reason);
        }
    } else {
        println!("Would skip: (none)");
    }
}
