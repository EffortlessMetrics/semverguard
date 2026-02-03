# semverguard-workspace

Workspace adapter for semverguard - reads Cargo workspace structure via `cargo_metadata`.

## Overview

This crate implements the `WorkspaceProvider` trait from `semverguard-domain`, providing workspace and package information by invoking `cargo metadata`.

## Responsibilities

- Execute `cargo metadata` to discover workspace members
- Extract package metadata (name, version, publish settings, targets)
- Provide package manifest paths for semver checking

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_workspace::CargoMetadataWorkspace;
use semverguard_domain::WorkspaceProvider;

let workspace = CargoMetadataWorkspace::from_path(".")?;
let packages = workspace.packages();
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
