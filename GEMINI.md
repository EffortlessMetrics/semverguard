# semverguard

`semverguard` is an orchestration layer for **cargo-semver-checks**, designed to gate Rust monorepos and workspaces in CI/CD pipelines. It provides smart scoping (only checking changed crates), deterministic reporting (receipt bundles), and multi-mode execution (PR vs. Release).

## Project Overview

- **Main Technologies:** Rust (Edition 2024), `cargo-semver-checks`, `cargo metadata`, `git`, `cucumber-rs`.
- **Architecture:** Ports and Adapters (Hexagonal) pattern.
    - `semverguard-cli`: CLI interface and adapter wiring.
    - `semverguard-core`: Domain-agnostic logic (reports, receipts, SARIF, exit codes).
    - `semverguard-domain`: Pure orchestration logic (the "Runner").
    - `semverguard-types`: Canonical data structures and schemas.
    - **Adapters:** `semverguard-workspace` (Cargo), `semverguard-git` (Git), `semverguard-engine` (cargo-semver-checks).
- **MSRV:** 1.92 (due to Rust 2024 edition).

## Building and Running

### Key Commands

- **Build:** `cargo build`
- **Run CLI:** `cargo run -p semverguard-cli -- [args]`
  - Example: `cargo run -p semverguard-cli -- check --changed`
- **List packages:** `cargo run -p semverguard-cli -- list`
- **Test:** `cargo test` (runs unit tests across all workspace members)
- **Integration Tests (Cucumber):** `cargo test -p semverguard-integration-tests --test cucumber`
- **Lint:** `cargo clippy`
- **Format:** `cargo fmt`

### CI Integration
The project includes a GitHub Action in `.github/actions/semverguard`.
Modes of operation:
- `pr`: Tolerant of baseline errors, skips unchanged crates.
- `release`: Strict enforcement, fails on baseline errors.
- `cockpit`: Receipt-driven, always exits 0 if a receipt is generated.

## Development Conventions

- **Rust Edition:** Uses the 2024 edition (`edition = "2024"` in root `Cargo.toml`).
- **Error Handling:** Uses `anyhow` for application-level errors and custom enums in `semverguard-domain` for domain errors.
- **Testing:**
    - **Unit Tests:** Located within each crate.
    - **Integration Tests:** BDD style using Cucumber (Gherkin) in `crates/semverguard-integration-tests`.
    - **Golden Files:** Uses golden output fixtures for deterministic ordering and schema validation (see `crates/semverguard-types/tests/golden`).
- **Coding Style:**
    - Strictly follows `rustfmt.toml`.
    - Avoids `unsafe` code (`#![forbid(unsafe_code)]` in many crates).
    - Documentation is prioritized (`#![deny(missing_docs)]` in core crates).
- **Exit Codes:**
    - `0`: Success / Pass.
    - `1`: Tool / Runtime Error.
    - `2`: SemVer Policy Failure (Violation).
    - `3`: Warning-as-Failure.

## Key Files

- `Cargo.toml`: Workspace configuration and shared dependencies.
- `semverguard.toml`: Example/Default configuration for the tool.
- `ROADMAP.md`: Current project status and future implementation phases.
- `docs/`: MD Book source for comprehensive documentation.
- `contracts/`: Versioned schema definitions for tool outputs.
