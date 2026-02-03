# semverguard-domain

Domain logic and port traits for semverguard - the pure orchestration layer that coordinates workspace scanning, git diffing, and semver checking.

## Overview

This crate contains:

- **Port traits** (`WorkspaceProvider`, `GitProvider`, `SemverEngine`) - abstractions for external dependencies
- **`SemverguardRunner`** - the main orchestrator that implements the three-pass filtering strategy
- **Mock implementations** (behind `test-utils` feature) for testing

## Architecture

```
SemverguardRunner
       │
       ├── WorkspaceProvider  ← semverguard-workspace adapter
       ├── GitProvider        ← semverguard-git adapter
       └── SemverEngine       ← semverguard-engine adapter
```

## Three-Pass Filtering Strategy

1. **Static filters**: include/exclude globs, publishable check, has-lib check
2. **Scope selection**: workspace mode (all) or changed mode (git-diff based)
3. **Engine execution**: run semver check per package, collect results

## Usage

This crate is typically used internally by `semverguard-cli`. If building a custom integration:

```rust
use semverguard_domain::{SemverguardRunner, WorkspaceProvider, GitProvider, SemverEngine};

let runner = SemverguardRunner::new(workspace, git, engine);
let report = runner.run(&config)?;
```

## Features

- `test-utils` - Enables mock implementations for testing

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
