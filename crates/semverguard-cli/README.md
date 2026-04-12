# semverguard-cli

`semverguard-cli` provides the `semverguard` binary.

It wires the semverguard core/domain crates with default adapters and exposes commands for
checking, listing, configuration, explanation, and baseline promotion.

## Install

```bash
cargo install cargo-semver-checks
cargo install semverguard-cli
```

## Commands

| Command | Purpose |
| --- | --- |
| `check` | Run semver checks for selected workspace crates |
| `list` | Preview package selection without running checks |
| `print-config` | Print effective config |
| `validate-config` | Validate config and show warnings/errors |
| `explain` | Explain finding `check_id`/`code` pairs |
| `promote-baseline` | Resolve a git ref and update `baseline.rev` |

## Examples

```bash
# Check all eligible workspace crates
semverguard check

# Check only crates changed from origin/main
semverguard check --changed --baseline-rev origin/main

# Preview selection only
semverguard check --dry-run --changed --baseline-rev origin/main

# Emit JSON and SARIF
semverguard check --json report.json
semverguard check --sarif report.sarif

# Receipt artifacts (for cockpit flow)
semverguard check --format receipt --artifacts-dir artifacts/semverguard

# Explain findings
semverguard explain
semverguard explain semver violation --json

# Promote baseline revision in config
semverguard promote-baseline --rev origin/main --write
```

## Exit Codes

- `0`: success
- `1`: tool/runtime error
- `2`: semver violation
- `3`: baseline error treated as failure

In cockpit mode (`--mode cockpit`), semverguard exits `0` if receipt output is written
successfully.

## Configuration

By default the CLI reads `./semverguard.toml`. Use `print-config` to inspect effective values
and `validate-config` to catch invalid or risky settings.

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
