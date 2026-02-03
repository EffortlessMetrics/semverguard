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

# Print effective configuration
semverguard print-config
```

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

- `0` - All checked crates passed
- `1` - At least one crate failed SemVer policy
- `2` - Configuration or invocation error

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
