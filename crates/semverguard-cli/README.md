# semverguard-cli

CLI for semverguard - orchestrates [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) across Cargo workspaces with filtering, scoping, and machine-readable reports.

## Installation

```bash
cargo install semverguard-cli
```

**Prerequisite:** Install the upstream checker:

```bash
cargo install cargo-semver-checks
```

## Usage

```bash
# Check all packages in workspace
semverguard check

# Check only changed packages (relative to git baseline)
semverguard check --baseline-rev origin/main --changed

# Output JSON report
semverguard check --json report.json

# Output SARIF for GitHub Code Scanning
semverguard check --sarif results.sarif

# List packages that would be checked (preview filtering)
semverguard list

# Print effective configuration
semverguard print-config

# Validate configuration file
semverguard validate-config
```

## Commands

| Command | Description |
|---------|-------------|
| `check` | Run semver checks across the workspace |
| `list` | List packages that would be checked (dry-run) |
| `print-config` | Print the effective configuration |
| `validate-config` | Validate config file for errors/warnings |

## Configuration

Create `semverguard.toml` in your workspace root:

```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
include = ["*"]
exclude = []

[output]
format = "both"
json_path = "semverguard-report.json"
```

## Exit Codes

- `0` - All checked crates passed (or baseline errors as warnings in PR mode)
- `1` - Tool or runtime error (engine missing, config error, etc.)
- `2` - SemVer policy violation detected
- `3` - Baseline error (in Release mode) or when `--warn-as-fail` is enabled

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
