# semverguard-engine

Engine adapter for semverguard - executes `cargo-semver-checks check-release` commands.

## Overview

This crate implements the `SemverEngine` trait from `semverguard-domain`, providing the integration with the upstream [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) tool.

## Responsibilities

- Construct `cargo semver-checks check-release` command arguments
- Execute the check for individual packages
- Capture stdout/stderr and parse exit codes
- Infer required version bump from output

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_engine::CargoSemverChecksEngine;
use semverguard_domain::SemverEngine;
use semverguard_types::SemverCheckRequest;

let engine = CargoSemverChecksEngine::default();
let (command, output) = engine.check(request)?;
```

## Command Building

The `build_command()` function is exported for testing command construction:

```rust
use semverguard_engine::CargoSemverChecksEngine;

let args = CargoSemverChecksEngine::build_command(&request);
// ["semver-checks", "check-release", "--manifest-path", ...]
```

## Baseline Flags

| Config field | cargo-semver-checks flag |
|--------------|--------------------------|
| `git_rev` | `--baseline-rev` |
| `crates_io_version` | `--baseline-version` |
| `root_path` | `--baseline-root` |
| `rustdoc_path` | `--baseline-rustdoc` |

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
