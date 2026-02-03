# semverguard-git

Git adapter for semverguard - detects changed crates via the git CLI.

## Overview

This crate implements the `GitProvider` trait from `semverguard-domain`, enabling semverguard to scope checks to only packages that have changed relative to a baseline revision.

## Responsibilities

- Run `git diff --name-only` to find changed files
- Map changed file paths to affected package directories
- Provide the list of changed packages to the runner

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_git::GitCliProvider;
use semverguard_domain::GitProvider;

let git = GitCliProvider::new(workspace_root);
let changed = git.changed_packages("origin/main")?;
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
