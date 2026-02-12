# CLAUDE.md - semverguard-workspace

Concrete adapter using `cargo metadata` to discover workspace packages.

## Role in Architecture

Implements the `WorkspaceProvider` port trait. This crate knows how to:
- Execute `cargo metadata` on a workspace
- Filter to workspace members (excluding transitive dependencies)
- Detect package publish status and library targets

## Key Components

```rust
pub struct CargoMetadataWorkspace { /* internal state */ }

impl WorkspaceProvider for CargoMetadataWorkspace {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata>;
}
```

## Package Detection Logic

### Publishability

Determined from `Cargo.toml` `publish` field:

| Config | Publishable |
|--------|-------------|
| `publish = false` | No |
| `publish = ["registry"]` | Yes |
| `publish` unset | Yes |

### Has Library Target

A package `has_lib = true` if it has:
- A `[lib]` target, OR
- A proc-macro target (proc-macros are libraries)

Binary-only crates have `has_lib = false` and are typically skipped by semverguard since they have no public API to check.

## Dependencies

```
semverguard-domain  # WorkspaceProvider trait, error types
semverguard-types   # WorkspaceMetadata, WorkspacePackage
cargo_metadata      # External crate for workspace introspection
```

## Testing

Integration tests use fixtures in `tests/fixtures/test_workspace/` to verify:
- Publishability detection
- Library target detection
- Workspace member filtering
