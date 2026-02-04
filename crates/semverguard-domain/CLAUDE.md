# CLAUDE.md - semverguard-domain

Core orchestration logic implementing three-pass filtering and hexagonal architecture ports.

## Role in Architecture

This is the **domain core** with no external tool dependencies. It defines:
- Port traits for dependency injection
- `SemverguardRunner` orchestration logic
- Progress reporting abstraction

## Key Files

- `src/ports.rs` - Trait definitions (the "ports" in hexagonal architecture)
- `src/runner.rs` - `SemverguardRunner` implementing three-pass filtering
- `src/error.rs` - Domain error types via `thiserror`
- `src/progress.rs` - `ProgressCallback` trait and `ProgressEvent` enum
- `src/mocks.rs` - Test utilities (behind `test-utils` feature)

## Port Traits

```rust
pub trait WorkspaceProvider {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata>;
}

pub trait GitProvider {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>>;
}

pub trait SemverEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<SemverCheckOutput>;
}
```

## Three-Pass Filtering Strategy

Implemented in `runner.rs`:

1. **Static Filters** - Include/exclude globs, publishable flag, has-lib check, explicit `--package` selection
2. **Scope Selection** - Workspace mode (all packages) or Changed mode (git-diff based)
3. **Engine Execution** - Run semver check per package, collect results

## Key Methods

```rust
SemverguardRunner::new(workspace, git, engine, config)
SemverguardRunner::with_progress(workspace, git, engine, config, callback)
runner.run() -> RunReport        // Full check execution
runner.list_packages() -> ListResult  // Preview without execution
```

## Dependencies

Only depends on `semverguard-types` for data structures. No external tool dependencies.

## Testing

Enable `test-utils` feature for mock implementations:

```toml
[dev-dependencies]
semverguard-domain = { path = "../semverguard-domain", features = ["test-utils"] }
```
