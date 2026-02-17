# semverguard-workspace

`semverguard-workspace` provides the default `WorkspaceProvider` implementation using
`cargo metadata`.

## Responsibilities

- Discover workspace root and member crates
- Return only workspace members (not transitive dependencies)
- Expose package metadata needed by semverguard filtering:
  - `name`
  - `version`
  - `manifest_path`
  - `package_root`
  - `publishable`
  - `has_lib`

## Usage

```rust
use semverguard_domain::WorkspaceProvider;
use semverguard_workspace::CargoMetadataWorkspace;
use std::path::Path;

let provider = CargoMetadataWorkspace::default();
let metadata = provider.load(Path::new("/path/to/workspace"))?;
```

## Detection Rules

- `publishable = false` only when Cargo metadata reports `publish = false`.
- `has_lib = true` when package targets include `lib` or `proc-macro`.

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
