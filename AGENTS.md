[**text**](CLAUDE.md)# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

semverguard is a Rust orchestration layer around `cargo-semver-checks` that runs semantic versioning checks across Cargo workspaces. It does not implement SemVer analysis itself—it delegates to upstream `cargo-semver-checks`.

Key features:
- Scopes checks to only changed crates (relative to git baseline) or entire workspace
- Centralizes configuration in `semverguard.toml`
- Outputs machine-readable JSON reports for CI artifact tracking
- Exit code 1 for SemVer failures (CI gating), exit code 2 for config errors

## Build Commands

```bash
# Build all crates
cargo build

# Run the CLI
cargo run -p semverguard-cli -- check [OPTIONS]
cargo run -p semverguard-cli -- print-config

# Example: check changed packages against origin/main
cargo run -p semverguard-cli -- check --baseline-rev origin/main --changed --json report.json

# Format and lint
cargo fmt
cargo fmt --check
```

## Architecture

**Hexagonal Architecture (Ports & Adapters)** with 6 crates:

```
semverguard-cli          # Binary entry point, arg parsing, wires adapters
       │
       ▼
semverguard-domain       # Pure orchestration logic, trait definitions (ports)
       │
       ├── WorkspaceProvider trait ─────► semverguard-workspace (cargo_metadata adapter)
       ├── GitProvider trait ────────────► semverguard-git (git CLI adapter)
       └── SemverEngine trait ───────────► semverguard-engine (cargo-semver-checks adapter)
       │
       ▼
semverguard-types        # Shared schema definitions (Config, Report, Workspace types)
```

**Key files:**
- `crates/semverguard-domain/src/ports.rs` - Trait definitions for all adapters
- `crates/semverguard-domain/src/runner.rs` - `SemverguardRunner` orchestration (filtering, scoping, execution)
- `crates/semverguard-types/src/config.rs` - Configuration schema
- `crates/semverguard-types/src/report.rs` - Output report schema

**Three-pass filtering strategy** (in runner.rs):
1. Static filters: include/exclude globs, publishable, has-lib checks
2. Scope selection: workspace mode (all) or changed mode (git-diff based)
3. Engine execution: run semver check per package, collect results

## Code Conventions

- `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` enforced across all crates
- All public items must be documented
- `thiserror` for error definitions
- Types implement `Debug`, `Clone`, `Serialize`, `Deserialize`
- 100-char line width (see `rustfmt.toml`)
- Import grouping: Std, External, Crate

## Configuration

Default config file: `semverguard.toml` in workspace root. CLI args override config values.

Key config sections:
- `[baseline]` - git rev or crates-io version to compare against
- `[scope]` - workspace vs changed mode, include/exclude globs
- `[output]` - text, json, or both; json_path for report file
