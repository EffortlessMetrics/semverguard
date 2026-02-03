# Getting Started

This tutorial walks you through setting up semverguard in a Cargo workspace and running your first SemVer check.

## Prerequisites

Before you begin, ensure you have:

- **Rust toolchain**: Install via [rustup](https://rustup.rs/) if you haven't already
- **cargo-semver-checks**: semverguard delegates SemVer analysis to this tool

Install cargo-semver-checks:

```bash
cargo install cargo-semver-checks
```

Verify the installation:

```bash
cargo semver-checks --version
```

## Install semverguard

Install semverguard from crates.io:

```bash
cargo install semverguard-cli
```

Or build from source:

```bash
git clone https://github.com/your-org/semverguard.git
cd semverguard
cargo install --path crates/semverguard-cli
```

Verify the installation:

```bash
semverguard --version
```

## Create a Configuration File

While semverguard works without configuration (using sensible defaults), creating a `semverguard.toml` in your workspace root gives you explicit control.

Create a minimal `semverguard.toml`:

```toml
# semverguard.toml

[baseline]
# Compare against published crates.io versions (default)
kind = "crates-io"

[scope]
# Check all workspace packages (default)
mode = "workspace"
# Skip packages with publish = false
skip_publish_false = true
# Skip packages without a library target
skip_no_lib = true

[output]
# Human-readable output to stdout
format = "text"
```

## Run Your First Check

Navigate to your workspace root and run:

```bash
semverguard check
```

semverguard will:

1. Discover all packages in your workspace
2. Filter out non-publishable packages and binary-only crates
3. Run cargo-semver-checks on each eligible package
4. Report results

Example output for a workspace with no breaking changes:

```
semverguard: total=3 passed=3 failed=0 skipped=1
SKIP  my-internal-crate 0.1.0  (publish = false)
```

Example output with breaking changes detected:

```
semverguard: total=3 passed=2 failed=1 skipped=0
FAIL  my-lib 1.0.0  /path/to/my-lib/Cargo.toml
      inferred required bump: Major
      stderr (tail):
      --- Breaking changes detected ---
      function `public_api_fn` was removed
```

## Understanding the Output

The summary line shows:

- **total**: Number of packages in scope
- **passed**: Packages with no SemVer violations
- **failed**: Packages with breaking changes
- **skipped**: Packages filtered out by policy

For each package, you'll see:

- `SKIP` entries with the reason (e.g., `publish = false`, `no library target`)
- `FAIL` entries with the manifest path and inferred required version bump

## Verify Configuration

To see the effective configuration after defaults and CLI overrides:

```bash
semverguard print-config
```

This outputs the resolved TOML configuration, useful for debugging.

## Next Steps

Now that you have semverguard running locally:

- [Set up CI integration](./ci-integration.md) to automate SemVer checks
- [Configure changed-only mode](../how-to/check-changed-packages.md) for faster PR checks
- [Generate JSON reports](../how-to/generate-json-reports.md) for downstream tooling
