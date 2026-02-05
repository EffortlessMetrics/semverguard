# semverguard

**Workspace orchestration for cargo-semver-checks**

semverguard is a Rust orchestration layer around [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) that runs semantic versioning checks across Cargo workspaces. It does not implement SemVer analysis itself—it delegates to upstream cargo-semver-checks.

## Key Features

- **Scoped checks**: Run checks only on changed crates (relative to a git baseline) or the entire workspace
- **Centralized configuration**: Configure all options in `semverguard.toml`
- **Machine-readable output**: Generate JSON, SARIF, or receipt bundles for CI artifact tracking
- **GitHub Code Scanning**: SARIF output integrates with GitHub's security dashboard
- **CI-friendly exit codes**: Exit 2 for SemVer failures, exit 1 for tool/runtime errors
- **Preview mode**: List which packages would be checked before running

## Quick Start

```bash
# Install semverguard
cargo install semverguard-cli

# Run checks on all workspace packages
semverguard check

# Run checks only on changed packages
semverguard check --changed --baseline-rev origin/main

# Preview what would be checked
semverguard list

# Generate SARIF for GitHub Code Scanning
semverguard check --sarif results.sarif
```

## I want to...

| Goal | Guide |
|------|-------|
| Set up semverguard for the first time | [Getting Started](./tutorials/getting-started.md) |
| Add semverguard to my CI pipeline | [CI Integration](./tutorials/ci-integration.md) |
| Only check packages I changed | [Check Changed Packages](./how-to/check-changed-packages.md) |
| Configure a monorepo with many crates | [Configure Monorepos](./how-to/configure-monorepos.md) |
| Generate reports for downstream tools | [Generate JSON Reports](./how-to/generate-json-reports.md) |
| Compare against published crates.io versions | [Use crates.io Baseline](./how-to/use-crates-io-baseline.md) |
| See all CLI flags and options | [CLI Reference](./reference/cli.md) |
| Understand the config file format | [Configuration Reference](./reference/config.md) |
| Parse the JSON output programmatically | [Report Schema](./reference/report-schema.md) |
| Understand exit codes for CI | [Exit Codes](./reference/exit-codes.md) |
| Learn how semverguard is architected | [Architecture](./explanation/architecture.md) |
| Understand how package filtering works | [Filtering Strategy](./explanation/filtering-strategy.md) |
| Know when to use semverguard vs cargo-semver-checks | [Why semverguard?](./explanation/why-semverguard.md) |

## Documentation Structure

This documentation follows the [Diataxis](https://diataxis.fr/) framework:

- **[Tutorials](./tutorials/getting-started.md)**: Learning-oriented guides that walk you through using semverguard
- **[How-to Guides](./how-to/check-changed-packages.md)**: Task-oriented recipes for specific goals
- **[Reference](./reference/cli.md)**: Technical descriptions of CLI, configuration, and output schemas
- **[Explanation](./explanation/architecture.md)**: Understanding-oriented discussions of architecture and design decisions
