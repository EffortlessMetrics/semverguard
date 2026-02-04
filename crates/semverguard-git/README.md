# semverguard-git

Git adapter for semverguard - detects changed files via the git CLI.

## Overview

This crate implements the `GitProvider` trait from `semverguard-domain`, enabling semverguard to scope checks to only packages that have changed relative to a baseline revision.

## Responsibilities

- Run `git diff --name-only <base>...<head>` (three-dot merge-base syntax)
- Parse output into file paths
- Return workspace-relative paths for change detection

## Usage

This crate is used internally by `semverguard-cli`:

```rust
use semverguard_git::GitCli;
use semverguard_domain::GitProvider;
use std::path::Path;

let git = GitCli::default();  // Uses "git" from PATH
let changed = git.changed_paths(
    Path::new("/workspace"),
    "origin/main",
    "HEAD"
)?;
```

## Custom Git Binary

```rust
use semverguard_git::GitCli;

let git = GitCli::new(Some("/usr/local/bin/git".into()));
```

## Three-Dot Diff

The three-dot syntax (`base...head`) compares the merge-base of base and head to head, which matches typical PR semantics - showing only files changed in the PR branch, not files changed in the target branch since the PR was created.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
