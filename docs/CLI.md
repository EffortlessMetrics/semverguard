# CLI Reference

Stable CLI surface for semverguard.

## Commands

### `semverguard check`

Run semantic versioning checks across a Cargo workspace.

```
semverguard check [OPTIONS]
```

**Options:**

| Flag | Description |
|------|-------------|
| `--mode <MODE>` | Run mode: `auto`, `pr`, `release`, `cockpit` |
| `--baseline-rev <REV>` | Git revision for baseline comparison |
| `--baseline-version <VER>` | Crates.io version for baseline comparison |
| `--changed` | Only check packages changed relative to baseline |
| `--format <FMT>` | Output format: `text`, `json`, `both`, `sarif`, `receipt` |
| `--json <PATH>` | Write JSON report to file |
| `--sarif <PATH>` | Write SARIF report to file |
| `--artifacts-dir <DIR>` | Output directory for receipt artifacts (default: `artifacts/semverguard`) |
| `--config <PATH>` | Path to `semverguard.toml` (default: `./semverguard.toml`) |
| `--workspace-root <DIR>` | Workspace root directory (default: `.`) |
| `--fail-fast` | Stop after first failing crate |
| `--dry-run` | Show what would be checked without running |
| `--cargo-bin <PATH>` | Path to cargo binary |
| `--engine-arg <ARG>` | Extra args for `cargo semver-checks` (repeatable) |
| `--progress <WHEN>` | Progress display: `auto`, `always`, `never` |

### `semverguard list`

List packages that would be checked without running checks.

```
semverguard list [OPTIONS]
```

### `semverguard print-config`

Print the effective configuration after merging file and CLI overrides.

### `semverguard validate-config`

Validate the configuration file for errors and warnings.

## Modes

### Standard mode (`pr` / `release` / `auto`)

Exit codes reflect check results:

| Code | Meaning |
|------|---------|
| `0` | All checks passed (or warnings only) |
| `1` | Tool/runtime error |
| `2` | SemVer violations detected |
| `3` | Warnings treated as failures (`--warn-as-fail`) |

### Cockpit mode

Zero-config integration for cockpit ingestion:

```
semverguard check --mode cockpit
```

Behavior:
- Forces `--format receipt` output automatically
- Writes receipt bundle to `artifacts/semverguard/` (configurable via `--artifacts-dir`)
- Exit `0` if receipt is written successfully (even with SemVer violations)
- Exit `1` only if receipt write fails
- No additional flags required beyond `--mode cockpit`

Output artifacts:
- `report.json` &mdash; `sensor.report.v1` receipt
- `comment.md` &mdash; PR comment markdown
- `raw/<pkg>-<ver>.{stdout,stderr}.log` &mdash; per-package engine logs

## Embedding

For programmatic use without the CLI binary:

```rust
use semverguard_core::pipeline::{PipelineOptions, run_with_adapters};
```

See `semverguard_core::pipeline` module documentation for the full API.
