# CLAUDE.md - semverguard-engine

Concrete adapter that shells out to `cargo semver-checks check-release`.

## Role in Architecture

Implements the `SemverEngine` port trait. This crate knows how to:
- Build the correct `cargo semver-checks` command line
- Execute the command and capture output
- Parse results and infer required version bump

## Key Components

```rust
pub struct CargoSemverChecksEngine { /* cargo binary path */ }

impl SemverEngine for CargoSemverChecksEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<SemverCheckOutput>;
}

// Exported for testing command construction
pub fn build_command(request: &SemverCheckRequest) -> Vec<String>;
```

## Command Construction

`build_command()` generates arguments in this order:

```
cargo semver-checks check-release
  --manifest-path <path>
  [baseline args: --baseline-rev | --baseline-version | --baseline-root | --baseline-rustdoc]
  [feature args: --all-features | --default-features | --only-explicit-features | --features | ...]
  [extra args: pass-through from config]
```

### Baseline Arguments

| Config | Flag |
|--------|------|
| `git_rev` | `--baseline-rev` |
| `crates_io_version` | `--baseline-version` |
| `root_path` | `--baseline-root` |
| `rustdoc_path` | `--baseline-rustdoc` |

### Feature Arguments

| Config | Flag |
|--------|------|
| `all_features: true` | `--all-features` |
| `default_features: false` | `--default-features=false` |
| `only_explicit: true` | `--only-explicit-features` |
| `features: ["foo"]` | `--features foo` |
| `baseline_features: ["bar"]` | `--baseline-features bar` |
| `current_features: ["baz"]` | `--current-features baz` |

## Dependencies

```
semverguard-domain  # SemverEngine trait, error types
semverguard-types   # SemverCheckRequest, SemverCheckOutput
```

## Testing

Extensive unit tests (100+) cover all flag combinations. Tests use `build_command()` directly to verify argument construction without executing cargo.
