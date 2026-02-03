# CLI Reference

Complete reference for the semverguard command-line interface.

## Synopsis

```
semverguard <COMMAND>

Commands:
  check        Run semver checks across the workspace
  print-config Print the effective configuration
  help         Print help information
```

## Global Options

```
-h, --help     Print help information
-V, --version  Print version information
```

## Commands

### `semverguard check`

Run semver checks across the workspace.

```
semverguard check [OPTIONS]
```

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--config <PATH>` | Path | `./semverguard.toml` | Path to configuration file |
| `--workspace-root <PATH>` | Path | `.` | Workspace root directory |
| `--baseline-rev <REV>` | String | — | Git revision for baseline (sets `kind = git`) |
| `--baseline-version <VER>` | String | — | crates.io version for baseline (sets `kind = crates-io`) |
| `--changed` | Flag | false | Only check packages changed relative to baseline |
| `--json <PATH>` | Path | — | Write JSON report to this path |
| `--format <FORMAT>` | Enum | `text` | Output format: `text`, `json`, or `both` |
| `--fail-fast` | Flag | false | Stop after first failing package |
| `--cargo-bin <PATH>` | Path | `cargo` | Path to cargo binary |
| `--engine-arg <ARG>` | String | — | Extra arg for cargo-semver-checks (repeatable) |

#### Examples

```bash
# Basic check with defaults
semverguard check

# Check only changed packages against main
semverguard check --changed --baseline-rev origin/main

# Generate JSON report
semverguard check --json report.json

# Use a specific config file
semverguard check --config ci-config.toml

# Check against a specific crates.io version
semverguard check --baseline-version 1.0.0

# Stop on first failure with verbose output
semverguard check --fail-fast --engine-arg --verbose

# Use a custom cargo binary
semverguard check --cargo-bin ~/.cargo/bin/cargo-nightly
```

#### Behavior Notes

- When `--json` is specified without `--format`, the format defaults to `both` (text + JSON)
- `--baseline-rev` and `--baseline-version` are mutually exclusive (last one wins)
- `--changed` requires `--baseline-rev` (git baseline)
- `--engine-arg` can be repeated: `--engine-arg --verbose --engine-arg --release`

### `semverguard print-config`

Print the effective configuration after loading the config file and applying defaults.

```
semverguard print-config [OPTIONS]
```

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--config <PATH>` | Path | `./semverguard.toml` | Path to configuration file |
| `--workspace-root <PATH>` | Path | `.` | Workspace root directory |

#### Examples

```bash
# Print effective configuration
semverguard print-config

# Print config from a specific file
semverguard print-config --config production.toml
```

#### Output

Outputs the resolved configuration as TOML:

```toml
[baseline]
kind = "crates-io"

[scope]
mode = "workspace"
include = []
exclude = []
skip_publish_false = true
skip_no_lib = true

[features]
all_features = false
default_features = true
only_explicit_features = false
features = []
baseline_features = []
current_features = []

[engine]
fail_fast = false
extra_args = []

[output]
format = "text"
pretty_json = true
```

## Configuration Override Precedence

CLI arguments override configuration file values:

1. **Defaults**: Built-in default values
2. **Config file**: Values from `semverguard.toml`
3. **CLI flags**: Command-line arguments (highest priority)

Example:

```toml
# semverguard.toml
[scope]
mode = "workspace"
```

```bash
# CLI overrides config: mode becomes "changed"
semverguard check --changed --baseline-rev origin/main
```

## Environment Variables

semverguard does not currently read environment variables directly. Use CLI flags or configuration files instead.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success: all checks passed |
| 1 | Failure: SemVer violations detected |
| 2 | Error: configuration or invocation error |

See [Exit Codes](./exit-codes.md) for detailed descriptions.

## Feature Flags

Feature selection flags are passed through to cargo-semver-checks via configuration:

```toml
[features]
all_features = false
default_features = true
only_explicit_features = false
features = ["serde"]
baseline_features = []
current_features = []
```

These map to cargo-semver-checks flags:

| Config | cargo-semver-checks flag |
|--------|-------------------------|
| `all_features = true` | `--all-features` |
| `default_features = false` | `--no-default-features` |
| `only_explicit_features = true` | `--only-explicit-features` |
| `features = ["a", "b"]` | `--features a,b` |
| `baseline_features = ["x"]` | `--baseline-features x` |
| `current_features = ["y"]` | `--current-features y` |

## See Also

- [Configuration Reference](./config.md) - Full config file schema
- [Exit Codes](./exit-codes.md) - Exit code meanings
- [Report Schema](./report-schema.md) - JSON output format
