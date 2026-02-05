# semverguard

`semverguard` is a small orchestration layer around **cargo-semver-checks**:

- run checks across a workspace (optionally only for changed crates)
- keep the flags/config in one place (`semverguard.toml`)
- emit machine-readable JSON, SARIF, or receipt bundles for CI artifacts
- exit non‑zero when a crate fails SemVer policy (so CI can gate PRs)

This repository is a Rust workspace with multiple crates:
- `semverguard-cli` (binary) — the thing you run in CI
- `semverguard-domain` — orchestration logic (pure-ish, testable)
- `semverguard-types` — config + report schema
- `semverguard-workspace` — adapter over `cargo metadata`
- `semverguard-git` — adapter over `git` CLI (for “changed crates” scoping)
- `semverguard-engine` — adapter that runs `cargo semver-checks check-release`

## Quick start

Install the upstream checker:

```bash
cargo install cargo-semver-checks
```

Build and run semverguard:

```bash
cargo run -p semverguard-cli -- check --json semverguard-report.json
```

To only check crates changed relative to a git revision:

```bash
cargo run -p semverguard-cli -- check --baseline-rev origin/main --changed
```

Preview which packages would be checked:

```bash
cargo run -p semverguard-cli -- list
```

Generate SARIF output for GitHub Code Scanning:

```bash
cargo run -p semverguard-cli -- check --sarif results.sarif
```

## Configuration

By default, `semverguard` looks for `semverguard.toml` in the working directory.
If it doesn’t exist, reasonable defaults are used.

Example `semverguard.toml`:

```toml
[baseline]
# kind = "crates-io" | "git"
kind = "git"
rev = "origin/main"

[scope]
# mode = "workspace" | "changed"
mode = "changed"

# Glob patterns over package names (optional)
include = ["*"]
exclude = []

# Skip packages that are not publishable or have no library target.
skip_publish_false = true
skip_no_lib = true

[features]
# Mirrors cargo-semver-checks flags. Keep false/empty unless you need them.
all_features = false
default_features = true
only_explicit_features = false
features = []
baseline_features = []
current_features = []

[engine]
# Extra args appended to `cargo semver-checks check-release`
extra_args = []

fail_fast = false

[output]
# format = "text" | "json" | "both" | "sarif" | "receipt"
format = "both"
json_path = "semverguard-report.json"
pretty_json = true
artifacts_dir = "artifacts/semverguard"
warn_as_fail = false
```

## Exit codes

- `0` — pass (or warn when warn-as-fail is disabled)
- `1` — tool/runtime error
- `2` — semver policy failure
- `3` — warn-as-fail

## CI integration

### GitHub Action (recommended)

The easiest way to integrate semverguard into your CI is with the official GitHub Action.
Copy this workflow to `.github/workflows/semver.yml`:

```yaml
name: SemVer Check

on:
  pull_request:
    branches: [main]

jobs:
  semver:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write  # Optional: for SARIF upload
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for git baseline

      - uses: EffortlessMetrics/semverguard/.github/actions/semverguard@main
        with:
          mode: pr
          baseline-rev: origin/main
          json-path: semverguard-report.json
          sarif-path: semverguard.sarif
          upload-sarif: "true"
          cargo-semver-checks-version: "0.35.0"  # Pinned for MSRV stability
```

### Action inputs

| Input | Default | Description |
|-------|---------|-------------|
| `mode` | `pr` | `pr` checks changed packages only; `release` checks entire workspace |
| `baseline-rev` | `origin/main` | Git revision to compare against |
| `changed` | `true` | Only check packages changed relative to baseline |
| `json-path` | `semverguard-report.json` | Path for JSON report output |
| `sarif-path` | (none) | Path for SARIF report (enables GitHub Code Scanning) |
| `upload-sarif` | `false` | Upload SARIF to GitHub Code Scanning |
| `cargo-semver-checks-version` | `0.35.0` | Pinned version for MSRV drift protection |
| `semverguard-version` | `source` | `source` builds from repo; or specify crates.io version |
| `extra-args` | (none) | Additional arguments for semverguard check |

### Action outputs

| Output | Description |
|--------|-------------|
| `exit-code` | `0`=pass, `1`=tool error, `2`=semver failure, `3`=warn-as-fail |
| `json-report` | Path to generated JSON report |
| `sarif-report` | Path to generated SARIF report (if enabled) |
| `summary` | Brief summary: `total=N passed=N failed=N skipped=N` |

### Manual setup

If you prefer manual control, here's a full workflow:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0  # required for git baseline revs

- uses: dtolnay/rust-toolchain@stable

- name: Install cargo-semver-checks
  run: cargo install cargo-semver-checks --version 0.35.0 --locked

- name: Run semverguard
  run: cargo run -p semverguard-cli -- check --baseline-rev origin/main --changed --json semverguard-report.json

- name: Upload report
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: semverguard-report
    path: semverguard-report.json

# Optional: Upload SARIF for GitHub Code Scanning
- name: Run semverguard (SARIF)
  run: cargo run -p semverguard-cli -- check --baseline-rev origin/main --changed --sarif results.sarif

- name: Upload SARIF
  if: always()
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Version pinning

The action pins `cargo-semver-checks` to a specific version (`0.35.0` by default) to prevent
MSRV drift issues. When upstream releases a new version that bumps MSRV, you can update
the version after verifying compatibility with your project's Rust version.

## Notes

- `semverguard` does **not** re-implement SemVer analysis; it delegates that to cargo-semver-checks.
- Baseline selection is passed straight through to cargo-semver-checks via flags like `--baseline-rev` or `--baseline-version`.
- If you need advanced cargo-semver-checks configuration (e.g. lint allow/deny), use the upstream tool’s configuration mechanisms;
  semverguard will happily run with those settings.
