# semverguard-workspace

Workspace adapter for semverguard - reads Cargo workspace structure via `cargo_metadata`.

## Overview

This crate implements the `WorkspaceProvider` trait from `semverguard-domain`, providing workspace and package information by invoking `cargo metadata`.

## Responsibilities

- Execute `cargo metadata --format-version 1` to discover workspace members
- Extract package metadata (name, version, publish settings, targets)
- Filter to workspace members only (exclude transitive dependencies)
- Detect publishability and library targets

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_workspace::CargoMetadataWorkspace;
use semverguard_domain::WorkspaceProvider;
use std::path::Path;

let workspace = CargoMetadataWorkspace::default();
let metadata = workspace.load(Path::new("/path/to/workspace"))?;

for pkg in &metadata.packages {
    println!("{}: publishable={}, has_lib={}",
        pkg.name, pkg.publishable, pkg.has_lib);
}
```

## Package Detection

### Publishability

| `Cargo.toml` setting | `publishable` |
|---------------------|---------------|
| `publish = false` | `false` |
| `publish = ["registry"]` | `true` |
| (unset) | `true` |

### Library Target

A package has `has_lib = true` if:
- It has a `[lib]` target, OR
- It's a proc-macro crate

Binary-only crates have `has_lib = false` and are typically filtered out since they have no public API to check.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
