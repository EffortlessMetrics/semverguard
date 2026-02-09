use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use globset::Glob;
use semverguard_core::capability::build_capability_context;
use semverguard_core::exit_code::exit_code_from_report;
use semverguard_core::receipt::{
    CapabilityContext, ToolErrorFinding, build_artifact_index,
    build_receipt_with_capabilities_versioned, exit_code_from_receipt, has_tool_error,
    resolve_artifacts_dir, write_receipt_bundle,
};
use semverguard_core::{config, sarif};
use semverguard_domain::SemverguardRunner;
use semverguard_engine::CargoSemverChecksEngine;
use semverguard_git::GitCli;
use semverguard_types::{
    BaselineKind, ListResult, OutputFormat, RunMode, RunReport, ScopeMode, SemverguardConfig,
};
use semverguard_workspace::CargoMetadataWorkspace;
use std::fs;
use std::path::{Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

mod progress;
use progress::{ProgressCallbackAdapter, ProgressChoice, create_progress_reporter};

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
    /// Explain finding codes and their meanings.
    Explain(ExplainArgs),
    /// Promote the baseline revision to a new git ref.
    PromoteBaseline(PromoteBaselineArgs),
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
struct ExplainArgs {
    /// Finding check_id to explain (e.g., "semver", "baseline").
    check_id: Option<String>,
    /// Finding code to explain (e.g., "violation", "missing").
    code: Option<String>,
    /// Output as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct PromoteBaselineArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Workspace root (defaults to .)
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,

    /// Git ref to promote to (defaults to HEAD).
    #[arg(long, default_value = "HEAD")]
    rev: String,

    /// Actually write changes (dry-run by default).
    #[arg(long)]
    write: bool,
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
    /// Cockpit mode: always write receipt, exit 0 if written successfully.
    Cockpit,
}

impl From<RunModeOpt> for RunMode {
    fn from(v: RunModeOpt) -> Self {
        match v {
            RunModeOpt::Auto => RunMode::Auto,
            RunModeOpt::Pr => RunMode::Pr,
            RunModeOpt::Release => RunMode::Release,
            RunModeOpt::Cockpit => RunMode::Cockpit,
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

    run_with_cli(cli)
}

fn run_with_cli(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Commands::PrintConfig(args) => {
            let cfg_path = args
                .config
                .unwrap_or_else(|| PathBuf::from("semverguard.toml"));
            let cfg = config::load_config(&cfg_path)?;
            // Nothing else to override (yet) besides ensuring workspace_root exists.
            config::ensure_workspace_root(&args.workspace_root)?;
            print_config(&cfg)?;
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
        Commands::Explain(args) => {
            run_explain(&args)?;
            Ok(0)
        }
        Commands::PromoteBaseline(args) => {
            run_promote_baseline(&args)?;
            Ok(0)
        }
    }
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

    // Cockpit mode forces receipt format
    if cfg.mode.is_cockpit() {
        cfg.output.format = OutputFormat::Receipt;
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

    // Only apply format override if not in cockpit mode (cockpit forces receipt)
    if !cfg.mode.is_cockpit() {
        if let Some(fmt) = &args.format {
            cfg.output.format = fmt.clone().into();
        }
    }

    if let Some(json_path) = &args.json {
        cfg.output.json_path = Some(json_path.clone());
        // If the user asked for JSON explicitly, default to both unless they also set --format.
        if args.format.is_none()
            && matches!(cfg.output.format, OutputFormat::Text)
            && !cfg.mode.is_cockpit()
        {
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

    let cfg_result = config::load_config(&cfg_path);
    let mut cfg = match &cfg_result {
        Ok(cfg) => cfg.clone(),
        Err(_) => SemverguardConfig::default(),
    };
    apply_cli_overrides(&mut cfg, args);

    let receipt_requested = matches!(cfg.output.format, OutputFormat::Receipt);
    let sarif_requested = receipt_requested && args.sarif.is_some();
    let artifacts_root = resolve_artifacts_dir(&args.workspace_root, &cfg.output.artifacts_dir);

    if let Err(e) = cfg_result {
        return handle_tool_error(
            e,
            &cfg,
            &args.workspace_root,
            &artifacts_root,
            receipt_requested,
            sarif_requested,
        );
    }

    if let Err(e) = config::ensure_workspace_root(&args.workspace_root) {
        return handle_tool_error(
            e,
            &cfg,
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

    // Probe git capabilities
    let git_available = git.is_available(&args.workspace_root);
    let shallow_clone = git.is_shallow(&args.workspace_root).unwrap_or(false);
    let git_version = git.version(&args.workspace_root);

    // Create progress reporter based on CLI flag
    let progress_choice: ProgressChoice = args.progress.clone().into();
    let progress_reporter = create_progress_reporter(progress_choice);
    let progress_callback = ProgressCallbackAdapter::new(progress_reporter);

    let runner =
        SemverguardRunner::with_progress(&workspace, Some(&git), &engine, progress_callback);

    // Handle --dry-run: show what would be checked without actually running
    if args.dry_run {
        let list_result = runner.list_packages(&args.workspace_root, &cfg)?;
        print_list_text(&list_result);
        return Ok(0);
    }

    let started = OffsetDateTime::now_utc();
    let artifacts = match runner.run(&args.workspace_root, &cfg) {
        Ok(artifacts) => artifacts,
        Err(e) => {
            return handle_tool_error(
                anyhow::Error::new(e),
                &cfg,
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

        // Build capability context using probed git results
        let capability_ctx =
            build_capability_context(&cfg, &report, git_available, shallow_clone, git_version);

        let receipt = build_receipt_with_capabilities_versioned(
            Some(&report),
            &[],
            &artifact_index,
            &cfg.baseline,
            &args.workspace_root,
            Some(&capability_ctx),
            None,
            &cfg.waivers,
        );

        // In cockpit mode, exit 0 if receipt write succeeds (even with failures)
        if cfg.mode.is_cockpit() {
            match write_receipt_bundle(
                &artifacts_root,
                &receipt,
                Some(&report),
                sarif_requested,
                cfg.output.pretty_json,
            ) {
                Ok(()) => return Ok(0),
                Err(e) => {
                    eprintln!("FATAL: failed to write receipt: {e}");
                    return Ok(1);
                }
            }
        }

        let write_result = write_receipt_bundle(
            &artifacts_root,
            &receipt,
            Some(&report),
            sarif_requested,
            cfg.output.pretty_json,
        );
        write_result?;
        let code = exit_code_from_receipt(
            &receipt.verdict,
            cfg.output.warn_as_fail,
            has_tool_error(&receipt.findings),
        );
        return Ok(code);
    }

    // Resolve the run mode (auto-detect from environment if needed)
    let resolved_mode = cfg.mode.resolve();

    emit_outputs(&cfg, &report)?;
    Ok(exit_code_from_report(
        &report,
        resolved_mode,
        cfg.output.warn_as_fail,
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

    // Build capability context indicating failure
    let git_skipped = !matches!(config.baseline.kind, BaselineKind::Git);
    let capability_ctx = CapabilityContext::new()
        .with_git_available(matches!(config.baseline.kind, BaselineKind::Git))
        .with_baseline_available(false)
        .with_baseline_detail(format!("error: {err}"))
        .with_git_skipped(git_skipped);

    let receipt = build_receipt_with_capabilities_versioned(
        None,
        &errors,
        &artifact_index,
        &config.baseline,
        workspace_root,
        Some(&capability_ctx),
        None,
        &config.waivers,
    );

    // In cockpit mode, exit 0 if receipt write succeeds (even with tool errors)
    if config.mode.is_cockpit() {
        match write_receipt_bundle(
            artifacts_root,
            &receipt,
            None,
            sarif_requested,
            config.output.pretty_json,
        ) {
            Ok(()) => return Ok(0),
            Err(e) => {
                eprintln!("FATAL: failed to write receipt: {e}");
                return Ok(1);
            }
        }
    }

    let write_result = write_receipt_bundle(
        artifacts_root,
        &receipt,
        None,
        sarif_requested,
        config.output.pretty_json,
    );
    write_result?;
    let code = exit_code_from_receipt(
        &receipt.verdict,
        config.output.warn_as_fail,
        has_tool_error(&receipt.findings),
    );
    Ok(code)
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

    let cfg = config::load_config(&cfg_path)?;
    let result = validate_config(&cfg);

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

    // Validate waivers
    for (i, waiver) in config.waivers.iter().enumerate() {
        // Check fingerprint format (64-char hex)
        if waiver.fingerprint.len() != 64
            || !waiver.fingerprint.chars().all(|c| c.is_ascii_hexdigit())
        {
            result.add_error(format!(
                "waivers[{}].fingerprint must be a 64-character hex string, got: {}",
                i, waiver.fingerprint
            ));
        }
        // Check reason is not empty
        if waiver.reason.trim().is_empty() {
            result.add_error(format!("waivers[{}].reason must not be empty", i));
        }
        // Warn on expired waivers
        if let Some(expires) = &waiver.expires {
            let parts: Vec<&str> = expires.split('-').collect();
            if parts.len() == 3 {
                if let (Ok(y), Ok(m), Ok(d)) = (
                    parts[0].parse::<i32>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    if let Ok(month) = time::Month::try_from(m) {
                        if let Ok(date) = time::Date::from_calendar_date(y, month, d) {
                            let today = time::OffsetDateTime::now_utc().date();
                            if today > date {
                                result.add_warning(format!(
                                    "waivers[{}].expires ({}) has already passed",
                                    i, expires
                                ));
                            }
                        }
                    }
                }
            }
        }
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
    config::ensure_workspace_root(&args.workspace_root)?;
    let mut cfg = config::load_config(&cfg_path)?;

    // Apply CLI overrides for list command
    if args.changed {
        cfg.scope.mode = semverguard_types::ScopeMode::Changed;
    }
    if let Some(rev) = &args.baseline_rev {
        cfg.baseline.kind = semverguard_types::BaselineKind::Git;
        cfg.baseline.rev = Some(rev.clone());
    }

    // Wire adapters (we don't need engine for list)
    let workspace = CargoMetadataWorkspace::default();
    let git = GitCli::default();
    let engine = CargoSemverChecksEngine::default();

    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let list_result = runner.list_packages(&args.workspace_root, &cfg)?;

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

// =============================================================================
// Explain Command
// =============================================================================

fn run_explain(args: &ExplainArgs) -> Result<()> {
    use semverguard_core::explain;

    match (&args.check_id, &args.code) {
        (None, _) => {
            // List all finding types
            if args.json {
                let entries: Vec<_> = explain::all().iter().map(|e| e.to_json_value()).collect();
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                println!("Finding types:\n");
                println!(
                    "{:<16} {:<16} {:<8} {}",
                    "CHECK_ID", "CODE", "LEVEL", "TITLE"
                );
                println!("{}", "-".repeat(72));
                for entry in explain::all() {
                    println!(
                        "{:<16} {:<16} {:<8} {}",
                        entry.check_id, entry.code, entry.default_level, entry.title
                    );
                }
            }
        }
        (Some(check_id), None) => {
            // List all findings for a check_id
            let matches = explain::lookup_by_check_id(check_id);
            if matches.is_empty() {
                eprintln!("Unknown check_id: {check_id}");
                eprintln!("\nValid check_ids:");
                for entry in explain::all() {
                    eprintln!("  {}", entry.check_id);
                }
                anyhow::bail!("unknown check_id: {check_id}");
            }

            if args.json {
                let entries: Vec<_> = matches.iter().map(|e| e.to_json_value()).collect();
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                for entry in matches {
                    print_explanation(entry);
                }
            }
        }
        (Some(check_id), Some(code)) => {
            // Look up specific finding
            match explain::lookup(check_id, code) {
                Some(entry) => {
                    if args.json {
                        println!("{}", serde_json::to_string_pretty(&entry.to_json_value())?);
                    } else {
                        print_explanation(entry);
                    }
                }
                None => {
                    eprintln!("Unknown finding: {check_id}/{code}");
                    eprintln!("\nValid findings:");
                    for entry in explain::all() {
                        eprintln!("  {}/{}", entry.check_id, entry.code);
                    }
                    anyhow::bail!("unknown finding: {check_id}/{code}");
                }
            }
        }
    }

    Ok(())
}

fn print_explanation(entry: &semverguard_core::explain::FindingExplanation) {
    println!("{}/{}", entry.check_id, entry.code);
    println!("  Title: {}", entry.title);
    println!("  Level: {}", entry.default_level);
    println!("  Description: {}", entry.description);
    if !entry.causes.is_empty() {
        println!("  Common causes:");
        for cause in entry.causes {
            println!("    - {cause}");
        }
    }
    if !entry.fixes.is_empty() {
        println!("  Suggested fixes:");
        for fix in entry.fixes {
            println!("    - {fix}");
        }
    }
    println!();
}

// =============================================================================
// Promote Baseline Command
// =============================================================================

fn run_promote_baseline(args: &PromoteBaselineArgs) -> Result<()> {
    let cfg_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("semverguard.toml"));

    // Check if the current config uses crates-io baseline
    if cfg_path.exists() {
        let cfg = config::load_config(&cfg_path)?;
        if matches!(cfg.baseline.kind, semverguard_types::BaselineKind::CratesIo) {
            println!("Baseline kind is crates-io.");
            println!("Crates-io baselines automatically advance when you publish a new version.");
            println!("No configuration change needed.");
            return Ok(());
        }
    }

    // Resolve the git ref to a full SHA
    let git = semverguard_git::GitCli::default();
    let resolved_rev = if args.rev == "HEAD" {
        git.resolve_head(&args.workspace_root)
            .context("failed to resolve HEAD - is this a git repository?")?
    } else {
        git.resolve_ref(&args.workspace_root, &args.rev)
            .with_context(|| format!("failed to resolve ref: {}", args.rev))?
    };

    let result =
        semverguard_core::promote::promote_git_baseline(&cfg_path, &resolved_rev, args.write)?;

    if result.written {
        println!("Baseline promoted in {}", cfg_path.display());
    } else {
        println!("Dry run (use --write to apply):");
    }

    println!("  Key:      {}", result.config_key);
    if let Some(prev) = &result.previous {
        println!("  Previous: {prev}");
    } else {
        println!("  Previous: (not set)");
    }
    println!("  New:      {}", result.new_value);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use semverguard_core::explain::FindingExplanation;
    use semverguard_types::{
        BaselineErrorCause, BaselineKind, FailureKind, ListResult, ListedPackage, OutputFormat,
        PackageReport, PackageStatus, RequiredBump, RunMode, RunReport, ScopeMode,
        SemverCheckOutput, SkippedPackage, Summary, WaiverEntry,
    };
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;
    use tempfile::tempdir;

    fn base_check_args() -> CheckArgs {
        CheckArgs {
            config: None,
            workspace_root: PathBuf::from("."),
            mode: None,
            baseline_rev: None,
            baseline_version: None,
            changed: false,
            dry_run: false,
            json: None,
            sarif: None,
            format: None,
            artifacts_dir: None,
            fail_fast: false,
            cargo_bin: None,
            engine_args: Vec::new(),
            progress: ProgressChoiceOpt::Auto,
        }
    }

    fn sample_report() -> RunReport {
        let packages = vec![
            PackageReport {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 12,
                command: vec!["cargo".to_string()],
                engine: None,
                inferred_required_bump: None,
                failure_kind: None,
                baseline_error: None,
            },
            PackageReport {
                name: "beta".to_string(),
                version: "0.2.0".to_string(),
                manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                status: PackageStatus::Skipped,
                skip_reason: Some("filtered".to_string()),
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: None,
                baseline_error: None,
            },
            PackageReport {
                name: "gamma".to_string(),
                version: "2.1.0".to_string(),
                manifest_path: PathBuf::from("/workspace/gamma/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 42,
                command: vec!["cargo".to_string(), "semver-checks".to_string()],
                engine: Some(SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: String::new(),
                    stderr: "line1\nline2\nline3".to_string(),
                    required_bump: Some(RequiredBump::Major),
                }),
                inferred_required_bump: Some(RequiredBump::Major),
                failure_kind: Some(FailureKind::SemverViolation),
                baseline_error: None,
            },
            PackageReport {
                name: "delta".to_string(),
                version: "3.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/delta/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: Some("engine skipped".to_string()),
                duration_ms: 1,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
                failure_kind: Some(FailureKind::BaselineError),
                baseline_error: Some(BaselineErrorCause::Other {
                    message: "missing baseline".to_string(),
                }),
            },
        ];

        RunReport {
            semverguard_version: "0.1.0".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            finished_at: "2024-01-15T10:00:01Z".to_string(),
            workspace_root: PathBuf::from("/workspace"),
            packages,
            summary: Summary {
                total: 4,
                passed: 1,
                failed: 2,
                skipped: 1,
            },
        }
    }

    fn write_minimal_workspace() -> TempDir {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"[workspace]
resolver = "2"
members = ["member"]
"#,
        )
        .expect("write workspace Cargo.toml");
        let member_dir = dir.path().join("member");
        fs::create_dir_all(member_dir.join("src")).expect("create member src");
        fs::write(
            member_dir.join("Cargo.toml"),
            r#"[package]
name = "member"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .expect("write member Cargo.toml");
        fs::write(member_dir.join("src").join("lib.rs"), "pub fn lib() {}").expect("write lib.rs");
        dir
    }

    struct FakeCargo {
        _dir: TempDir,
        path: PathBuf,
    }

    fn write_fake_cargo(exit_code: i32) -> FakeCargo {
        let dir = tempdir().expect("tempdir");
        #[cfg(windows)]
        let path = dir.path().join("fake_cargo.cmd");
        #[cfg(not(windows))]
        let path = dir.path().join("fake_cargo.sh");

        #[cfg(windows)]
        let script = format!("@echo off\r\necho ok\r\nexit /b {exit_code}\r\n");
        #[cfg(not(windows))]
        let script = format!("#!/bin/sh\necho \"ok\"\nexit {exit_code}\n");

        fs::write(&path, script).expect("write fake cargo script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("set permissions");
        }

        FakeCargo { _dir: dir, path }
    }

    fn run_git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "git {:?} failed: {}", args, stderr);
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn init_git_repo(repo: &Path) -> String {
        run_git(repo, &["init"]);
        run_git(repo, &["config", "user.email", "test@example.com"]);
        run_git(repo, &["config", "user.name", "Test User"]);
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "-m", "initial"]);
        run_git(repo, &["rev-parse", "HEAD"])
    }

    #[test]
    fn test_apply_cli_overrides_cockpit_forces_receipt() {
        let mut cfg = SemverguardConfig::default();
        let mut args = base_check_args();
        args.mode = Some(RunModeOpt::Cockpit);
        args.format = Some(FormatOpt::Json);
        args.json = Some(PathBuf::from("report.json"));

        apply_cli_overrides(&mut cfg, &args);

        assert!(matches!(cfg.mode, RunMode::Cockpit));
        assert_eq!(cfg.output.format, OutputFormat::Receipt);
        assert_eq!(cfg.output.json_path, Some(PathBuf::from("report.json")));
    }

    #[test]
    fn test_apply_cli_overrides_json_sets_both_when_text() {
        let mut cfg = SemverguardConfig::default();
        let mut args = base_check_args();
        args.json = Some(PathBuf::from("report.json"));

        apply_cli_overrides(&mut cfg, &args);

        assert_eq!(cfg.output.format, OutputFormat::Both);
        assert_eq!(cfg.output.json_path, Some(PathBuf::from("report.json")));
    }

    #[test]
    fn test_apply_cli_overrides_sarif_overrides_json() {
        let mut cfg = SemverguardConfig::default();
        let mut args = base_check_args();
        args.json = Some(PathBuf::from("report.json"));
        args.sarif = Some(PathBuf::from("report.sarif.json"));

        apply_cli_overrides(&mut cfg, &args);

        assert_eq!(cfg.output.format, OutputFormat::Sarif);
        assert_eq!(
            cfg.output.json_path,
            Some(PathBuf::from("report.sarif.json"))
        );
    }

    #[test]
    fn test_apply_cli_overrides_baseline_scope_and_engine() {
        let mut cfg = SemverguardConfig::default();
        cfg.engine.extra_args.push("--flag".to_string());

        let mut args = base_check_args();
        args.changed = true;
        args.baseline_rev = Some("origin/main".to_string());
        args.baseline_version = Some("1.2.3".to_string());
        args.fail_fast = true;
        args.cargo_bin = Some(PathBuf::from("cargo-custom"));
        args.engine_args = vec!["-Z".to_string(), "unstable-options".to_string()];

        apply_cli_overrides(&mut cfg, &args);

        assert!(matches!(cfg.scope.mode, ScopeMode::Changed));
        assert!(matches!(cfg.baseline.kind, BaselineKind::CratesIo));
        assert_eq!(cfg.baseline.rev, Some("origin/main".to_string()));
        assert_eq!(cfg.baseline.version, Some("1.2.3".to_string()));
        assert!(cfg.engine.fail_fast);
        assert_eq!(cfg.engine.cargo_bin, Some(PathBuf::from("cargo-custom")));
        assert_eq!(
            cfg.engine.extra_args,
            vec![
                "--flag".to_string(),
                "-Z".to_string(),
                "unstable-options".to_string()
            ]
        );
    }

    #[test]
    fn test_apply_cli_overrides_format_override() {
        let mut cfg = SemverguardConfig::default();
        let mut args = base_check_args();
        args.format = Some(FormatOpt::Json);

        apply_cli_overrides(&mut cfg, &args);

        assert_eq!(cfg.output.format, OutputFormat::Json);
    }

    #[test]
    fn test_apply_cli_overrides_sarif_with_receipt_keeps_format() {
        let mut cfg = SemverguardConfig::default();
        let mut args = base_check_args();
        args.mode = Some(RunModeOpt::Cockpit);
        args.sarif = Some(PathBuf::from("report.sarif.json"));

        apply_cli_overrides(&mut cfg, &args);

        assert_eq!(cfg.output.format, OutputFormat::Receipt);
        assert!(cfg.output.json_path.is_none());
    }

    #[test]
    fn test_progress_choice_opt_conversion() {
        assert_eq!(
            ProgressChoice::from(ProgressChoiceOpt::Auto),
            ProgressChoice::Auto
        );
        assert_eq!(
            ProgressChoice::from(ProgressChoiceOpt::Always),
            ProgressChoice::Always
        );
        assert_eq!(
            ProgressChoice::from(ProgressChoiceOpt::Never),
            ProgressChoice::Never
        );
    }

    #[test]
    fn test_format_opt_conversion() {
        assert_eq!(OutputFormat::from(FormatOpt::Text), OutputFormat::Text);
        assert_eq!(OutputFormat::from(FormatOpt::Json), OutputFormat::Json);
        assert_eq!(OutputFormat::from(FormatOpt::Both), OutputFormat::Both);
        assert_eq!(OutputFormat::from(FormatOpt::Sarif), OutputFormat::Sarif);
        assert_eq!(
            OutputFormat::from(FormatOpt::Receipt),
            OutputFormat::Receipt
        );
    }

    #[test]
    fn test_run_mode_opt_conversion() {
        assert_eq!(RunMode::from(RunModeOpt::Auto), RunMode::Auto);
        assert_eq!(RunMode::from(RunModeOpt::Pr), RunMode::Pr);
        assert_eq!(RunMode::from(RunModeOpt::Release), RunMode::Release);
        assert_eq!(RunMode::from(RunModeOpt::Cockpit), RunMode::Cockpit);
    }

    #[test]
    fn test_run_with_cli_validate_config() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("semverguard.toml");
        fs::write(&config_path, "").expect("write config");
        let cli = Cli {
            cmd: Commands::ValidateConfig(ValidateConfigArgs {
                config: Some(config_path),
            }),
        };
        let code = run_with_cli(cli).expect("run_with_cli validate_config");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_with_cli_list() {
        let workspace = write_minimal_workspace();
        let config_path = workspace.path().join("semverguard.toml");
        fs::write(&config_path, "").expect("write config");
        let cli = Cli {
            cmd: Commands::List(ListArgs {
                config: Some(config_path),
                workspace_root: workspace.path().to_path_buf(),
                baseline_rev: None,
                changed: false,
                json: false,
            }),
        };
        let code = run_with_cli(cli).expect("run_with_cli list");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_with_cli_explain() {
        let cli = Cli {
            cmd: Commands::Explain(ExplainArgs {
                check_id: Some("semver".to_string()),
                code: Some("violation".to_string()),
                json: false,
            }),
        };
        let code = run_with_cli(cli).expect("run_with_cli explain");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_with_cli_promote_baseline() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"[baseline]
kind = "crates-io"
"#,
        )
        .expect("write config");
        let cli = Cli {
            cmd: Commands::PromoteBaseline(PromoteBaselineArgs {
                config: Some(config_path),
                workspace_root: dir.path().to_path_buf(),
                rev: "HEAD".to_string(),
                write: false,
            }),
        };
        let code = run_with_cli(cli).expect("run_with_cli promote_baseline");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_with_cli_check_dry_run() {
        let workspace = write_minimal_workspace();
        let config_path = workspace.path().join("semverguard.toml");
        fs::write(&config_path, "").expect("write config");
        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(config_path);
        args.dry_run = true;
        let cli = Cli {
            cmd: Commands::Check(args),
        };
        let code = run_with_cli(cli).expect("run_with_cli check");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_validate_config_reports_errors_and_warnings() {
        let dir = tempdir().expect("tempdir");
        let root_file = dir.path().join("root.txt");
        fs::write(&root_file, "x").expect("write root file");
        let rustdoc_dir = dir.path().join("rustdoc");
        fs::create_dir(&rustdoc_dir).expect("create rustdoc dir");

        let mut cfg = SemverguardConfig::default();
        cfg.scope.include = vec!["[".to_string()];
        cfg.scope.exclude = vec!["[".to_string()];
        cfg.baseline.kind = BaselineKind::Git;
        cfg.baseline.rev = None;
        cfg.baseline.version = Some("1.0.0".to_string());
        cfg.scope.mode = ScopeMode::Changed;
        cfg.baseline.root = Some(root_file);
        cfg.baseline.rustdoc = Some(rustdoc_dir);
        cfg.engine.cargo_bin = Some(PathBuf::from("does-not-exist"));
        cfg.features.all_features = true;
        cfg.features.only_explicit_features = true;
        cfg.features.features.clear();
        cfg.waivers = vec![WaiverEntry {
            fingerprint: "abc".to_string(),
            reason: " ".to_string(),
            ticket: None,
            expires: Some("2000-01-01".to_string()),
        }];

        let result = validate_config(&cfg);

        assert!(!result.errors.is_empty());
        assert!(!result.warnings.is_empty());
        let has_include_error = result.errors.iter().any(|e| e.contains("scope.include"));
        assert!(has_include_error, "expected include glob error");
        let has_baseline_error = result
            .errors
            .iter()
            .any(|e| e.contains("baseline.kind is \"git\""));
        assert!(has_baseline_error, "expected baseline.rev error");
        let has_version_warning = result
            .warnings
            .iter()
            .any(|w| w.contains("baseline.version is set"));
        assert!(has_version_warning, "expected baseline.version warning");
        let has_expire_warning = result.warnings.iter().any(|w| w.contains("expires"));
        assert!(has_expire_warning, "expected waiver expiration warning");
    }

    #[test]
    fn test_validate_config_missing_paths() {
        let dir = tempdir().expect("tempdir");
        let missing_root = dir.path().join("missing-root");
        let missing_rustdoc = dir.path().join("missing-rustdoc.json");

        let mut cfg = SemverguardConfig::default();
        cfg.baseline.root = Some(missing_root);
        cfg.baseline.rustdoc = Some(missing_rustdoc);

        let result = validate_config(&cfg);

        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("baseline.root does not exist"))
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("baseline.rustdoc does not exist"))
        );
    }

    #[test]
    fn test_validate_config_clean_config() {
        let cfg = SemverguardConfig::default();
        let result = validate_config(&cfg);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_config_warnings_only() {
        let mut cfg = SemverguardConfig::default();
        cfg.baseline.kind = BaselineKind::Git;
        cfg.baseline.rev = Some("origin/main".to_string());
        cfg.baseline.version = Some("1.2.3".to_string());

        let result = validate_config(&cfg);

        assert!(result.errors.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("baseline.version is set"))
        );
    }

    #[test]
    fn test_validate_config_changed_mode_requires_git_kind() {
        let mut cfg = SemverguardConfig::default();
        cfg.scope.mode = ScopeMode::Changed;
        cfg.baseline.kind = BaselineKind::CratesIo;
        cfg.baseline.rev = Some("origin/main".to_string());

        let result = validate_config(&cfg);

        let has_kind_error = result
            .errors
            .iter()
            .any(|e| e.contains("baseline.kind = \"git\""));
        assert!(has_kind_error, "expected baseline.kind error");
    }

    #[test]
    fn test_validate_config_invalid_fingerprint_hex() {
        let mut cfg = SemverguardConfig::default();
        cfg.waivers = vec![WaiverEntry {
            fingerprint: "g".repeat(64),
            reason: "hex check".to_string(),
            ticket: None,
            expires: None,
        }];

        let result = validate_config(&cfg);

        let has_fingerprint_error = result.errors.iter().any(|e| e.contains("fingerprint"));
        assert!(has_fingerprint_error, "expected fingerprint error");
    }

    #[test]
    fn test_validate_config_waiver_expired_warning() {
        let mut cfg = SemverguardConfig::default();
        cfg.waivers = vec![WaiverEntry {
            fingerprint: "a".repeat(64),
            reason: "expired".to_string(),
            ticket: None,
            expires: Some("2000-01-01".to_string()),
        }];

        let result = validate_config(&cfg);

        let has_expire_warning = result.warnings.iter().any(|w| w.contains("expires"));
        assert!(has_expire_warning, "expected waiver expiration warning");
    }

    #[test]
    fn test_validate_config_waiver_invalid_expires_formats_no_warning() {
        let mut cfg = SemverguardConfig::default();
        cfg.waivers = vec![
            WaiverEntry {
                fingerprint: "a".repeat(64),
                reason: "invalid day".to_string(),
                ticket: None,
                expires: Some("2024-02-30".to_string()),
            },
            WaiverEntry {
                fingerprint: "b".repeat(64),
                reason: "invalid month".to_string(),
                ticket: None,
                expires: Some("2024-13-01".to_string()),
            },
            WaiverEntry {
                fingerprint: "c".repeat(64),
                reason: "parse fail".to_string(),
                ticket: None,
                expires: Some("2024-xx-01".to_string()),
            },
            WaiverEntry {
                fingerprint: "d".repeat(64),
                reason: "bad format".to_string(),
                ticket: None,
                expires: Some("not-a-date".to_string()),
            },
            WaiverEntry {
                fingerprint: "e".repeat(64),
                reason: "short format".to_string(),
                ticket: None,
                expires: Some("bad".to_string()),
            },
        ];

        let result = validate_config(&cfg);
        assert!(!result.warnings.iter().any(|w| w.contains("expires")));
    }

    #[test]
    fn test_run_validate_config_missing_file_is_ok() {
        let args = ValidateConfigArgs {
            config: Some(PathBuf::from("does-not-exist.toml")),
        };
        let result = run_validate_config(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_config_valid_file_ok() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"[baseline]
kind = "crates-io"
"#,
        )
        .expect("write config");

        let args = ValidateConfigArgs { config: Some(path) };
        let result = run_validate_config(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_config_warns_only() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"[baseline]
kind = "crates-io"
rev = "abc123"
"#,
        )
        .expect("write config");

        let args = ValidateConfigArgs { config: Some(path) };
        let result = run_validate_config(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_config_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"[baseline]
kind = "git"
"#,
        )
        .expect("write config");

        let args = ValidateConfigArgs { config: Some(path) };
        let result = run_validate_config(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_validate_config_errors_and_warnings_prints_sections() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("semverguard.toml");
        fs::write(
            &path,
            r#"mode = "pr"
[baseline]
kind = "git"
version = "1.0.0"

[scope]
include = ["["]
"#,
        )
        .expect("write config");

        let args = ValidateConfigArgs { config: Some(path) };
        let result = run_validate_config(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_explain_unknown_check_id_errors() {
        let args = ExplainArgs {
            check_id: Some("unknown-check".to_string()),
            code: None,
            json: false,
        };
        let result = run_explain(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_explain_json_listing_ok() {
        let args = ExplainArgs {
            check_id: None,
            code: None,
            json: true,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_listing_text_ok() {
        let args = ExplainArgs {
            check_id: None,
            code: None,
            json: false,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_known_check_id_text_ok() {
        let args = ExplainArgs {
            check_id: Some("semver".to_string()),
            code: None,
            json: false,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_known_check_id_json_ok() {
        let args = ExplainArgs {
            check_id: Some("semver".to_string()),
            code: None,
            json: true,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_specific_entry_json_ok() {
        let args = ExplainArgs {
            check_id: Some("semver".to_string()),
            code: Some("violation".to_string()),
            json: true,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_specific_entry_text_ok() {
        let args = ExplainArgs {
            check_id: Some("semver".to_string()),
            code: Some("violation".to_string()),
            json: false,
        };
        let result = run_explain(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_explain_unknown_code_errors() {
        let args = ExplainArgs {
            check_id: Some("semver".to_string()),
            code: Some("nope".to_string()),
            json: false,
        };
        let result = run_explain(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_print_explanation_with_causes_and_fixes() {
        static CAUSES: &[&str] = &["cause one"];
        static FIXES: &[&str] = &["fix one"];
        let entry = FindingExplanation {
            check_id: "semver",
            code: "violation",
            title: "Semver violation",
            description: "Breaking change detected.",
            causes: CAUSES,
            fixes: FIXES,
            default_level: "error",
        };
        print_explanation(&entry);
    }

    #[test]
    fn test_print_explanation_without_causes_or_fixes() {
        static EMPTY: &[&str] = &[];
        let entry = FindingExplanation {
            check_id: "semver",
            code: "violation",
            title: "Semver violation",
            description: "Breaking change detected.",
            causes: EMPTY,
            fixes: EMPTY,
            default_level: "error",
        };
        print_explanation(&entry);
    }

    #[test]
    fn test_handle_tool_error_without_receipt_returns_err() {
        let cfg = SemverguardConfig::default();
        let dir = tempdir().expect("tempdir");
        let result = handle_tool_error(
            anyhow::anyhow!("boom"),
            &cfg,
            dir.path(),
            dir.path().join("artifacts").as_path(),
            false,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_tool_error_writes_receipt_in_cockpit_mode() {
        let mut cfg = SemverguardConfig::default();
        cfg.mode = RunMode::Cockpit;
        let dir = tempdir().expect("tempdir");
        let artifacts = dir.path().join("artifacts");

        let result = handle_tool_error(
            anyhow::anyhow!("boom"),
            &cfg,
            dir.path(),
            &artifacts,
            true,
            false,
        )
        .expect("should write receipt");

        assert_eq!(result, 0);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
    }

    #[test]
    fn test_handle_tool_error_receipt_non_cockpit_writes_bundle() {
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Receipt;
        cfg.mode = RunMode::Pr;
        let dir = tempdir().expect("tempdir");
        let artifacts = dir.path().join("artifacts");

        let result = handle_tool_error(
            anyhow::anyhow!("boom"),
            &cfg,
            dir.path(),
            &artifacts,
            true,
            false,
        )
        .expect("should write receipt");

        assert_eq!(result, 1);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
    }

    #[test]
    fn test_handle_tool_error_cockpit_write_failure_returns_code() {
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Receipt;
        cfg.mode = RunMode::Cockpit;
        let dir = tempdir().expect("tempdir");
        let artifacts = dir.path().join("artifacts");
        fs::write(&artifacts, "not a dir").expect("write artifacts file");

        let result = handle_tool_error(
            anyhow::anyhow!("boom"),
            &cfg,
            dir.path(),
            &artifacts,
            true,
            false,
        )
        .expect("should return code");

        assert_eq!(result, 1);
    }

    #[test]
    fn test_run_check_dry_run_lists_packages() {
        let workspace = write_minimal_workspace();
        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(workspace.path().join("semverguard.toml"));
        args.dry_run = true;

        let code = run_check(&args).expect("run_check dry-run");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_check_handles_runner_error() {
        let workspace = write_minimal_workspace();
        let config_path = workspace.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"[scope]
include = ["["]
"#,
        )
        .expect("write config");

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(config_path);

        let result = run_check(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_check_cockpit_write_failure_returns_code() {
        let workspace = write_minimal_workspace();
        let config_path = workspace.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"mode = "cockpit"

[scope]
include = ["nonexistent-*"]
"#,
        )
        .expect("write config");
        let artifacts = workspace.path().join("artifacts");
        fs::write(&artifacts, "not a dir").expect("write artifacts file");

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(config_path);
        args.artifacts_dir = Some(artifacts);

        let code = run_check(&args).expect("run_check cockpit failure");
        assert_eq!(code, 1);
    }

    #[test]
    fn test_run_check_text_output_success() {
        let workspace = write_minimal_workspace();
        let fake_cargo = write_fake_cargo(0);

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(workspace.path().join("semverguard.toml"));
        args.mode = Some(RunModeOpt::Release);
        args.cargo_bin = Some(fake_cargo.path.clone());

        let code = run_check(&args).expect("run_check text output");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_check_receipt_cockpit_writes_artifacts() {
        let workspace = write_minimal_workspace();
        let fake_cargo = write_fake_cargo(0);
        let artifacts = workspace.path().join("artifacts");

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(workspace.path().join("semverguard.toml"));
        args.mode = Some(RunModeOpt::Cockpit);
        args.cargo_bin = Some(fake_cargo.path.clone());
        args.artifacts_dir = Some(artifacts.clone());
        args.sarif = Some(artifacts.join("report.sarif.json"));

        let code = run_check(&args).expect("run_check cockpit");
        assert_eq!(code, 0);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
        assert!(artifacts.join("sarif.json").exists());
    }

    #[test]
    fn test_run_check_receipt_non_cockpit_returns_code() {
        let workspace = write_minimal_workspace();
        let fake_cargo = write_fake_cargo(0);
        let artifacts = workspace.path().join("artifacts");

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(workspace.path().join("semverguard.toml"));
        args.format = Some(FormatOpt::Receipt);
        args.cargo_bin = Some(fake_cargo.path.clone());
        args.artifacts_dir = Some(artifacts.clone());

        let code = run_check(&args).expect("run_check receipt");
        assert_eq!(code, 0);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
    }

    #[test]
    fn test_run_check_invalid_config_receipt_returns_tool_error_code() {
        let workspace = write_minimal_workspace();
        let config_path = workspace.path().join("semverguard.toml");
        fs::write(&config_path, "[baseline\nkind = \"git\"").expect("write invalid toml");
        let artifacts = workspace.path().join("artifacts");

        let mut args = base_check_args();
        args.workspace_root = workspace.path().to_path_buf();
        args.config = Some(config_path);
        args.format = Some(FormatOpt::Receipt);
        args.artifacts_dir = Some(artifacts.clone());

        let code = run_check(&args).expect("run_check invalid config");
        assert_eq!(code, 1);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
    }

    #[test]
    fn test_run_check_missing_workspace_root_receipt() {
        let dir = tempdir().expect("tempdir");
        let missing_root = dir.path().join("missing-workspace");
        let artifacts = dir.path().join("artifacts");

        let mut args = base_check_args();
        args.workspace_root = missing_root;
        args.config = Some(dir.path().join("semverguard.toml"));
        args.format = Some(FormatOpt::Receipt);
        args.artifacts_dir = Some(artifacts.clone());

        let code = run_check(&args).expect("run_check missing workspace");
        assert_eq!(code, 1);
        assert!(artifacts.join("report.json").exists());
        assert!(artifacts.join("comment.md").exists());
    }

    #[test]
    fn test_run_list_json_output() {
        let workspace = write_minimal_workspace();
        let args = ListArgs {
            config: Some(workspace.path().join("semverguard.toml")),
            workspace_root: workspace.path().to_path_buf(),
            baseline_rev: None,
            changed: false,
            json: true,
        };

        let result = run_list(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_list_changed_text_output() {
        let workspace = write_minimal_workspace();
        let baseline = init_git_repo(workspace.path());
        fs::write(
            workspace.path().join("member").join("src").join("lib.rs"),
            "pub fn lib() {}\npub fn changed() {}\n",
        )
        .expect("update lib.rs");
        run_git(workspace.path(), &["add", "."]);
        run_git(workspace.path(), &["commit", "-m", "change"]);
        let args = ListArgs {
            config: Some(workspace.path().join("semverguard.toml")),
            workspace_root: workspace.path().to_path_buf(),
            baseline_rev: Some(baseline),
            changed: true,
            json: false,
        };

        run_list(&args).expect("run_list failed");
    }

    #[test]
    fn test_print_list_text_empty() {
        let result = ListResult {
            workspace_root: PathBuf::from("/workspace"),
            would_check: vec![],
            would_skip: vec![],
        };
        print_list_text(&result);
    }

    #[test]
    fn test_print_list_text_with_skips() {
        let result = ListResult {
            workspace_root: PathBuf::from("/workspace"),
            would_check: vec![ListedPackage {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/workspace/alpha/Cargo.toml"),
            }],
            would_skip: vec![SkippedPackage {
                name: "beta".to_string(),
                version: "1.2.3".to_string(),
                manifest_path: PathBuf::from("/workspace/beta/Cargo.toml"),
                reason: "filtered".to_string(),
            }],
        };
        print_list_text(&result);
    }

    #[test]
    fn test_run_promote_baseline_crates_io_noop() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"[baseline]
kind = "crates-io"
"#,
        )
        .expect("write config");

        let args = PromoteBaselineArgs {
            config: Some(config_path),
            workspace_root: dir.path().to_path_buf(),
            rev: "HEAD".to_string(),
            write: false,
        };

        let result = run_promote_baseline(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_promote_baseline_missing_config_file_errors() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("missing.toml");
        fs::write(dir.path().join("README.md"), "init").expect("write file");
        let _head = init_git_repo(dir.path());

        let args = PromoteBaselineArgs {
            config: Some(config_path),
            workspace_root: dir.path().to_path_buf(),
            rev: "HEAD".to_string(),
            write: false,
        };

        let result = run_promote_baseline(&args);
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Config file not found"));
    }

    #[test]
    fn test_run_promote_baseline_head_dry_run() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"[baseline]
kind = "git"
rev = "oldrev"
"#,
        )
        .expect("write config");
        let _head = init_git_repo(dir.path());

        let args = PromoteBaselineArgs {
            config: Some(config_path.clone()),
            workspace_root: dir.path().to_path_buf(),
            rev: "HEAD".to_string(),
            write: false,
        };

        run_promote_baseline(&args).expect("run_promote_baseline failed");
        let content = fs::read_to_string(&config_path).expect("read config");
        assert!(content.contains("oldrev"));
    }

    #[test]
    fn test_run_promote_baseline_ref_write() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("semverguard.toml");
        fs::write(
            &config_path,
            r#"[baseline]
kind = "git"
"#,
        )
        .expect("write config");
        let head = init_git_repo(dir.path());
        run_git(dir.path(), &["tag", "v1.0.0"]);
        let args = PromoteBaselineArgs {
            config: Some(config_path.clone()),
            workspace_root: dir.path().to_path_buf(),
            rev: "v1.0.0".to_string(),
            write: true,
        };

        run_promote_baseline(&args).expect("run_promote_baseline failed");
        let content = fs::read_to_string(&config_path).expect("read config");
        assert!(content.contains(&head));
    }

    #[test]
    fn test_emit_outputs_writes_json() {
        let dir = tempdir().expect("tempdir");
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Json;
        cfg.output.pretty_json = false;
        cfg.output.json_path = Some(dir.path().join("report.json"));

        emit_outputs(&cfg, &sample_report()).expect("emit outputs");

        assert!(cfg.output.json_path.unwrap().exists());
    }

    #[test]
    fn test_emit_outputs_writes_sarif() {
        let dir = tempdir().expect("tempdir");
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Sarif;
        cfg.output.pretty_json = false;
        cfg.output.json_path = Some(dir.path().join("report.sarif.json"));

        emit_outputs(&cfg, &sample_report()).expect("emit outputs");

        assert!(cfg.output.json_path.unwrap().exists());
    }

    #[test]
    fn test_emit_outputs_both_and_receipt_noop() {
        let dir = tempdir().expect("tempdir");
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Both;
        cfg.output.pretty_json = false;
        cfg.output.json_path = Some(dir.path().join("report.json"));

        emit_outputs(&cfg, &sample_report()).expect("emit outputs");
        let json_path = cfg.output.json_path.clone().expect("json path");
        assert!(json_path.exists());

        cfg.output.format = OutputFormat::Receipt;
        cfg.output.json_path = None;
        emit_outputs(&cfg, &sample_report()).expect("emit outputs receipt");
    }

    #[test]
    fn test_emit_outputs_text() {
        let mut cfg = SemverguardConfig::default();
        cfg.output.format = OutputFormat::Text;
        emit_outputs(&cfg, &sample_report()).expect("emit outputs text");
    }

    #[test]
    fn test_print_config_ok() {
        let cfg = SemverguardConfig::default();
        print_config(&cfg).expect("print config");
    }

    #[test]
    fn test_write_json_prints_when_no_path() {
        let mut cfg = SemverguardConfig::default();
        cfg.output.pretty_json = true;
        cfg.output.json_path = None;
        write_json(&cfg, &sample_report()).expect("write json");
    }

    #[test]
    fn test_write_sarif_prints_when_no_path() {
        let mut cfg = SemverguardConfig::default();
        cfg.output.pretty_json = true;
        cfg.output.json_path = None;
        write_sarif(&cfg, &sample_report()).expect("write sarif");
    }

    #[test]
    fn test_print_text_covers_status_branches() {
        print_text(&sample_report());
    }

    #[test]
    fn test_tail_lines_basic() {
        let s = "a\nb\nc\nd";
        assert_eq!(tail_lines(s, 2), "c\nd");
        assert_eq!(tail_lines(s, 10), "a\nb\nc\nd");
        assert_eq!(tail_lines(s, 0), "");
    }

    proptest! {
        #[test]
        fn prop_tail_lines_matches_suffix(lines in proptest::collection::vec("[^\\n]*", 0..20), n in 0usize..40) {
            let s = lines.join("\n");
            let line_vec: Vec<&str> = s.lines().collect();
            let start = line_vec.len().saturating_sub(n);
            let expected = line_vec[start..].join("\n");
            prop_assert_eq!(tail_lines(&s, n), expected);
        }

        #[test]
        fn prop_indent_prefixes_each_line(lines in proptest::collection::vec("[^\\n]*", 0..20), prefix in "[^\\n]{0,8}") {
            let s = lines.join("\n");
            let expected = s
                .lines()
                .map(|line| format!("{prefix}{line}"))
                .collect::<Vec<_>>()
                .join("\n");
            prop_assert_eq!(indent(&s, &prefix), expected);
        }
    }
}
