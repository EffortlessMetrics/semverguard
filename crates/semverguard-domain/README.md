# semverguard-domain

Domain logic and port traits for semverguard - the pure orchestration layer that coordinates workspace scanning, git diffing, and semver checking.

## Overview

This crate contains:

- **Port traits** (`WorkspaceProvider`, `GitProvider`, `SemverEngine`) - abstractions for external dependencies
- **`SemverguardRunner`** - the main orchestrator that implements the three-pass filtering strategy
- **Progress reporting** (`ProgressCallback`, `ProgressEvent`) - abstraction for UI progress updates
- **Mock implementations** (behind `test-utils` feature) for testing

## Architecture

```
SemverguardRunner
       │
       ├── WorkspaceProvider  ← semverguard-workspace adapter
       ├── GitProvider        ← semverguard-git adapter
       └── SemverEngine       ← semverguard-engine adapter
```

## Port Traits

```rust
pub trait WorkspaceProvider {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata>;
}

pub trait GitProvider {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>>;
}

pub trait SemverEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)>;
}
```

## Three-Pass Filtering Strategy

1. **Static filters**: include/exclude globs, publishable check, has-lib check
2. **Scope selection**: workspace mode (all) or changed mode (git-diff based)
3. **Engine execution**: run semver check per package, collect results

## Usage

This crate is typically used internally by `semverguard-cli`. If building a custom integration:

```rust
use semverguard_domain::{SemverguardRunner, WorkspaceProvider, GitProvider, SemverEngine};
use semverguard_types::SemverguardConfig;

// Create adapters
let workspace = MyWorkspaceProvider::new();
let git = MyGitProvider::new();
let engine = MyEngine::new();

// Create runner
let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);

// Run checks
let artifacts = runner.run(workspace_root, &config)?;

// Or preview without running
let list_result = runner.list_packages(workspace_root, &config)?;
```

## Features

- `test-utils` - Enables mock implementations for testing

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
