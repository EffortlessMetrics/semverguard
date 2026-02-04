# CLAUDE.md - semverguard-types

Lightweight shared types library. Contains all configuration and reporting schemas.

## Role in Architecture

This crate is the **shared vocabulary** - pure data structures with no business logic. Intentionally minimal dependencies.

## Key Files

- `src/config.rs` - Configuration schema (`SemverguardConfig`, `BaselineConfig`, `ScopeConfig`, etc.)
- `src/report.rs` - Output report schema (`RunReport`, `PackageReport`, `Summary`, `ListResult`)
- `src/engine.rs` - Engine adapter types (`SemverCheckRequest`, `SemverCheckOutput`, `RequiredBump`)
- `src/workspace.rs` - Workspace metadata types (`WorkspaceMetadata`, `WorkspacePackage`)

## Configuration Types

```rust
SemverguardConfig        // Top-level config
├── BaselineConfig       // Git rev or crates-io version
├── ScopeConfig          // Workspace vs changed mode, include/exclude globs
├── FeaturesConfig       // Feature flags for cargo-semver-checks
├── EngineConfig         // Cargo binary path, extra args, fail-fast
└── OutputConfig         // Format (text/json/both), json_path
```

## Report Types

```rust
RunReport                // Top-level output
├── packages: Vec<PackageReport>  // Per-package results
└── summary: Summary     // Counts (passed, failed, skipped, total)

PackageReport
├── status: PackageStatus  // Passed | Failed | Skipped
├── command: Option<String>  // Executed command (for debugging)
├── required_bump: Option<RequiredBump>  // Major | Minor | Patch | Unknown
└── duration_ms: Option<u64>
```

## Design Conventions

All public types derive:
- `Debug`, `Clone`
- `Serialize`, `Deserialize` (serde)

Config types use:
- `#[serde(default)]` - Allows partial configs
- `#[serde(deny_unknown_fields)]` - Catches typos

## RequiredBump Inference

`RequiredBump::infer(output: &str)` searches engine output for keywords (case-insensitive) to determine bump level.

## Dependencies

Minimal by design:
- `serde` + `serde_json` - Serialization
- `semver` - Version parsing
