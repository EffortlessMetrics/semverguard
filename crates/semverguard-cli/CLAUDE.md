# CLAUDE.md - semverguard-cli

Binary entry point for semverguard. Handles CLI argument parsing, adapter wiring, and output formatting.

## Role in Architecture

This crate is the **composition root** that:
- Parses user commands via `clap`
- Loads configuration from `semverguard.toml`
- Wires concrete adapters into `SemverguardRunner`
- Formats and emits output (text, JSON, SARIF)

## Key Files

- `src/main.rs` - CLI entry, command routing, adapter wiring
- `src/progress.rs` - Progress bar UI using `indicatif`
- `src/sarif.rs` - SARIF format conversion for GitHub Code Scanning

## Commands

| Command | Description |
|---------|-------------|
| `check` | Run semver checks on packages |
| `list` | Preview which packages would be checked (dry-run) |
| `print-config` | Display resolved configuration |
| `validate-config` | Check config file for errors/warnings |

## CLI Override Behavior

CLI arguments override config file values:
- `--baseline-rev` overrides `[baseline].git_rev`
- `--changed` sets scope to changed mode
- `--json <path>` sets JSON output path

## Exit Codes

- `0` - All checks passed (or baseline warnings in PR mode)
- `1` - Tool/runtime error (engine missing, etc.)
- `2` - SemVer failures detected (policy violation)
- `3` - Baseline error (e.g., rev not found in Release mode)

## Dependencies

```
semverguard-domain    # SemverguardRunner orchestration
semverguard-types     # Config/Report types
semverguard-workspace # CargoMetadataWorkspace adapter
semverguard-git       # GitCli adapter
semverguard-engine    # CargoSemverChecksEngine adapter
```

## Testing

CLI tests are primarily integration tests. Unit tests for SARIF conversion are in `sarif.rs`.
