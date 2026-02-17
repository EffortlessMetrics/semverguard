# semverguard-git

`semverguard-git` is the git-backed adapter for semverguard.

It implements `GitProvider` and additional helper methods used by core logic for capability
probing and PR-mode gating.

## Responsibilities

- List changed paths via `git diff --name-only <base>...<head>`
- Parse diff output into workspace-relative paths
- Resolve refs and HEAD to commit SHAs
- Detect shallow clone state
- Detect manifest `version = "..."` changes for PR-mode gate behavior

## Usage

```rust
use semverguard_git::GitCli;
use std::path::Path;

let git = GitCli::default();
let changed = git.changed_paths(Path::new("/workspace"), "origin/main", "HEAD")?;
let head_sha = git.resolve_head(Path::new("/workspace"))?;
let has_version_change =
    git.has_manifest_version_change(Path::new("/workspace"), "origin/main", "HEAD")?;
```

## Notes

- Changed-path checks use three-dot range syntax (`base...head`) to match typical PR semantics.
- `GitCli::new(Some(path))` can be used to point at a non-default git binary.

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
