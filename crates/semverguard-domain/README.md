# semverguard-domain

`semverguard-domain` is the pure orchestration layer for semverguard.

It defines ports for external dependencies and implements `SemverguardRunner`, which performs
filtering, scoping, and engine execution without depending on concrete git/cargo adapters.

## Main Pieces

- Port traits: `WorkspaceProvider`, `GitProvider`, `SemverEngine`
- Runner: `SemverguardRunner`
- Progress API: `ProgressCallback`, `ProgressEvent`
- Failure classification helpers
- Test doubles behind `test-utils`

## Runner Flow

`SemverguardRunner` applies three passes:

1. Static filtering (`include`/`exclude`, `skip_publish_false`, `skip_no_lib`, explicit packages)
2. Scope filtering (`workspace` or `changed` via `GitProvider`)
3. Engine execution and result classification

## Usage

```rust
use semverguard_domain::SemverguardRunner;

let runner = SemverguardRunner::new(&workspace_provider, Some(&git_provider), &engine);
let run_artifacts = runner.run(workspace_root, &config)?;
let list_result = runner.list_packages(workspace_root, &config)?;
```

## Feature Flags

- `test-utils`: exports mock providers/engine for tests and integration scenarios

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
