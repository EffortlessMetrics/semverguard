use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use semverguard_domain::SemverguardRunner;
use semverguard_engine::CargoSemverChecksEngine;
use semverguard_git::GitCli;
use semverguard_types::{OutputFormat, RunReport, SemverguardConfig};
use semverguard_workspace::CargoMetadataWorkspace;
use std::fs;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

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
    /// Print the effective configuration (after file + CLI overrides).
    PrintConfig(PrintConfigArgs),
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
struct CheckArgs {
    /// Path to semverguard.toml (defaults to ./semverguard.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Workspace root (defaults to .)
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,

    /// Use git baseline selection (`--baseline-rev` passed to cargo-semver-checks).
    #[arg(long)]
    baseline_rev: Option<String>,

    /// Use crates.io baseline selection (`--baseline-version` passed to cargo-semver-checks).
    #[arg(long)]
    baseline_version: Option<String>,

    /// Only check packages changed relative to the baseline revision.
    #[arg(long)]
    changed: bool,

    /// Output JSON report to this path.
    #[arg(long)]
    json: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum)]
    format: Option<FormatOpt>,

    /// Stop after first failing crate.
    #[arg(long)]
    fail_fast: bool,

    /// Which `cargo` binary to run (defaults to cargo on PATH).
    #[arg(long)]
    cargo_bin: Option<PathBuf>,

    /// Extra args appended to `cargo semver-checks check-release` (repeatable).
    #[arg(long = "engine-arg")]
    engine_args: Vec<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum FormatOpt {
    Text,
    Json,
    Both,
}

impl From<FormatOpt> for OutputFormat {
    fn from(v: FormatOpt) -> Self {
        match v {
            FormatOpt::Text => OutputFormat::Text,
            FormatOpt::Json => OutputFormat::Json,
            FormatOpt::Both => OutputFormat::Both,
        }
    }
}

fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e:?}");
        return std::process::ExitCode::from(2);
    }
    std::process::ExitCode::from(0)
}

fn run() -> Result<()> {
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
            Ok(())
        }
        Commands::Check(args) => {
            let cfg_path = args
                .config
                .clone()
                .unwrap_or_else(|| PathBuf::from("semverguard.toml"));
            ensure_workspace_root(&args.workspace_root)?;
            let mut config = load_config(&cfg_path)?;
            apply_cli_overrides(&mut config, &args);

            // Wire adapters.
            let workspace = CargoMetadataWorkspace::default();
            let git = GitCli::default();
            let engine = CargoSemverChecksEngine::default();

            let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);

            let started = OffsetDateTime::now_utc();
            let artifacts = runner.run(&args.workspace_root, &config)?;
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

            emit_outputs(&config, &report)?;

            if report.summary.overall_success() {
                Ok(())
            } else {
                // Non-zero exit for CI gating.
                std::process::exit(1)
            }
        }
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
