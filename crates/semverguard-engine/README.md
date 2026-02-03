# semverguard-engine

Engine adapter for semverguard - executes `cargo-semver-checks check-release` commands.

## Overview

This crate implements the `SemverEngine` trait from `semverguard-domain`, providing the integration with the upstream [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) tool.

## Responsibilities

- Construct `cargo semver-checks check-release` command arguments
- Execute the check for individual packages
- Parse and return results in semverguard's report format

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_engine::CargoSemverChecksEngine;
use semverguard_domain::SemverEngine;

let engine = CargoSemverChecksEngine::new();
let result = engine.check(&package, &config)?;
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
