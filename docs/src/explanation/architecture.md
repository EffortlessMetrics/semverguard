# Architecture

This document explains semverguard's architecture, crate layout, and design decisions.

## Design Philosophy

semverguard follows **hexagonal architecture** (ports and adapters), which:

- Separates business logic from external dependencies
- Makes the core logic testable without real file systems or processes
- Allows swapping implementations (e.g., different git backends)
- Keeps the domain model clean and focused

## Crate Layout

```
semverguard/
├── crates/
│   ├── semverguard-cli/       # Binary entry point
│   ├── semverguard-core/      # Embeddable orchestration + output pipeline
│   ├── semverguard-domain/    # Core orchestration logic
│   ├── semverguard-types/     # Shared type definitions
│   ├── semverguard-workspace/ # Cargo metadata adapter
│   ├── semverguard-git/       # Git CLI adapter
│   └── semverguard-engine/    # cargo-semver-checks adapter
```

### Dependency Graph

```
┌─────────────────────┐
│   semverguard-cli   │  Binary crate
│   (entry point)     │  - Argument parsing (clap)
└─────────┬───────────┘  - UX/progress/output wiring
          │
          ▼
┌─────────────────────┐
│   semverguard-core  │  Library crate
│  (pipeline facade)  │  - list/probe/pr-gate helpers
└─────────┬───────────┘  - receipt/comment/SARIF/exit-code
          │              - default-adapter pipeline wiring
          ▼
┌─────────────────────┐
│  semverguard-domain │  Domain crate
│    (orchestration)  │  - SemverguardRunner + ports
└─────────┬───────────┘
          │
    ┌─────┼──────────────────────────┐
    │     │                          │
    ▼     ▼                          ▼
┌───────────────┐  ┌──────────────┐  ┌─────────────────┐
│ semverguard-  │  │ semverguard- │  │ semverguard-    │
│   workspace   │  │     git      │  │    engine       │
│   (adapter)   │  │  (adapter)   │  │   (adapter)     │
└───────┬───────┘  └──────┬───────┘  └────────┬────────┘
        │                 │                   │
        ▼                 ▼                   ▼
   cargo_metadata      git CLI         cargo-semver-checks

                    ┌─────────────────────┐
                    │  semverguard-types  │  Shared types
                    │  (schema/config)    │  - SemverguardConfig
                    └─────────────────────┘  - RunReport
                                             - WorkspaceMetadata
```

## Crate Responsibilities

### `semverguard-core`

**Embeddable orchestration facade** used by the CLI and other integrators.

Key components:

- `pipeline`: end-to-end run + receipt writing helpers
- `operations`: list/probe/PR-gate helpers
- `config`: config loading + semantic validation
- `receipt` / `comment` / `sarif` / `exit_code`: output and policy layers

`semverguard-core` depends on `semverguard-domain` and `semverguard-types`, and
optionally wires default adapters via the `default-adapters` feature.

### `semverguard-types`

**Shared schema definitions** used across all crates.

Key types:
- `SemverguardConfig` - Configuration structure
- `RunReport`, `PackageReport`, `Summary` - Output report types
- `WorkspaceMetadata`, `WorkspacePackage` - Workspace model
- `SemverCheckRequest`, `SemverCheckOutput` - Engine I/O

This crate has no dependencies on other semverguard crates, making it a stable foundation.

### `semverguard-domain`

**Core business logic** - the "hexagon" in hexagonal architecture.

Key components:

- **`SemverguardRunner`**: Main orchestrator that coordinates the check process
- **Port traits**: Abstractions for external dependencies
  - `WorkspaceProvider` - Load workspace metadata
  - `GitProvider` - Query git for changed files
  - `SemverEngine` - Run SemVer checks

```rust
pub trait WorkspaceProvider {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata>;
}

pub trait GitProvider {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str)
        -> Result<Vec<PathBuf>>;
}

pub trait SemverEngine {
    fn check(&self, request: SemverCheckRequest)
        -> Result<(Vec<String>, SemverCheckOutput)>;
}
```

The domain crate depends only on `semverguard-types` and standard library types. It doesn't know about cargo_metadata, git commands, or cargo-semver-checks.

### `semverguard-workspace`

**Adapter for workspace discovery** using `cargo_metadata`.

Implements `WorkspaceProvider` by:

1. Running `cargo metadata --format-version 1`
2. Parsing the JSON output
3. Extracting package information

### `semverguard-git`

**Adapter for git operations** using the git CLI.

Implements `GitProvider` by:

1. Running `git diff --name-only <base>...<head>` (three-dot syntax for merge-base comparison)
2. Parsing the output as file paths
3. Returning workspace-relative paths

### `semverguard-engine`

**Adapter for SemVer checking** using `cargo-semver-checks`.

Implements `SemverEngine` by:

1. Building the command line for `cargo semver-checks check-release`
2. Running the process
3. Capturing output and inferring required bump

### `semverguard-cli`

**Binary entry point** that provides UX on top of `semverguard-core`.

Subcommands:

- **`check`**: Run semver checks across the workspace
- **`list`**: Preview which packages would be checked (dry-run filtering)
- **`print-config`**: Display resolved configuration
- **`validate-config`**: Check config file for errors/warnings

Responsibilities:

1. Parse CLI arguments (clap)
2. Load and merge configuration
3. Call `semverguard-core` operations/pipeline APIs
4. Execute run and format output (text, JSON, SARIF, or receipt)
5. Display progress (spinner/progress bar) when running interactively
6. Return appropriate exit code

## Why Hexagonal Architecture?

### Testability

The domain crate can be unit tested with mock implementations:

```rust
struct MockWorkspaceProvider { /* ... */ }
struct MockGitProvider { /* ... */ }
struct MockSemverEngine { /* ... */ }

#[test]
fn test_changed_mode_only_runs_changed_packages() {
    let workspace = MockWorkspaceProvider::new(/* ... */);
    let git = MockGitProvider::new(vec![PathBuf::from("pkg-a/src/lib.rs")]);
    let engine = MockSemverEngine::new(/* ... */);

    let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
    let result = runner.run(/* ... */);

    // Assert only changed packages were checked
}
```

### Extensibility

New adapters can be added without changing core logic:

- Want to use libgit2 instead of git CLI? Implement `GitProvider`
- Want to cache rustdoc JSON? Create a caching `SemverEngine` wrapper
- Want to read workspaces from a manifest cache? Implement `WorkspaceProvider`

### Separation of Concerns

Each crate has a single responsibility:

- `types`: Data structures
- `domain`: Business rules
- `core`: Reusable orchestration + output pipeline facade
- `workspace`/`git`/`engine`: External integrations
- `cli`: User interface

## Data Flow

```
1. CLI parses args, loads config
         │
         ▼
2. CLI calls `semverguard-core` operations/pipeline APIs
         │
         ▼
3. Core pipeline invokes `SemverguardRunner`
         │
         ├─▶ WorkspaceProvider.load() → WorkspaceMetadata
         │
         ├─▶ Apply static filters (include/exclude, publish, lib)
         │
         ├─▶ If changed mode:
         │      GitProvider.changed_paths() → changed package set
         │      Filter to changed packages only
         │
         └─▶ For each eligible package:
                SemverEngine.check() → SemverCheckOutput
         │
         ▼
4. Domain returns RunArtifacts
         │
         ▼
5. Core builds report/receipt/comment/SARIF and computes exit code
         │
         ▼
6. CLI renders outputs and returns process exit code
```

## Error Handling

Errors are defined in `semverguard-domain`:

```rust
pub enum SemverguardError {
    Workspace(String),    // WorkspaceProvider errors
    Git(String),          // GitProvider errors
    Engine(String),       // SemverEngine errors
    InvalidConfig(String), // Configuration validation
}
```

Adapters convert their specific errors into these domain errors, keeping the core logic ignorant of implementation details.

## See Also

- [Filtering Strategy](./filtering-strategy.md) - How package filtering works
- [Why semverguard?](./why-semverguard.md) - Value proposition
