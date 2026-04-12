# semverguard

`semverguard` is a Rust orchestration layer around
[`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) for Cargo workspaces.

It does not re-implement SemVer analysis. It selects packages, runs upstream checks, and
normalizes outputs for CI.

## What It Does

- Check every eligible workspace crate or only crates changed from a git baseline.
- Keep behavior in one config file: `semverguard.toml`.
- Emit text, JSON, SARIF, or receipt artifacts.
- Return CI-friendly exit codes.

## Workspace Crates

| Crate | Responsibility |
| --- | --- |
| `semverguard-cli` | End-user CLI (`semverguard`) |
| `semverguard-core` | Reusable pipeline and reporting logic (config, receipt, SARIF, comments, exit codes) |
| `semverguard-domain` | Pure orchestration domain (`SemverguardRunner`) and port traits |
| `semverguard-types` | Shared schema types (config, workspace, engine, report, receipt) |
| `semverguard-workspace` | `WorkspaceProvider` adapter over `cargo metadata` |
| `semverguard-git` | `GitProvider` adapter over `git` CLI |
| `semverguard-engine` | `SemverEngine` adapter over `cargo semver-checks check-release` |
| `semverguard-integration-tests` | Workspace-level scenario and cucumber tests |

## Quick Start

Install the upstream checker:

```bash
cargo install cargo-semver-checks
```

Run a check from this workspace:

```bash
cargo run -p semverguard-cli -- check --json semverguard-report.json
```

Changed-crate mode against a git baseline:

```bash
cargo run -p semverguard-cli -- check --changed --baseline-rev origin/main
```

Preview package selection without running checks:

```bash
cargo run -p semverguard-cli -- list --changed --baseline-rev origin/main
```

## CLI Commands

- `check` - run semver checks
- `list` - show packages that would be checked/skipped
- `print-config` - print effective config
- `validate-config` - validate `semverguard.toml`
- `explain` - explain finding codes
- `promote-baseline` - resolve a ref and update `baseline.rev`

## Configuration

By default, semverguard reads `./semverguard.toml`.

```toml
mode = "auto" # auto | pr | release | cockpit

[baseline]
kind = "git"  # git | crates-io
rev = "origin/main"

[scope]
mode = "changed" # workspace | changed
include = []
exclude = []
skip_publish_false = true
skip_no_lib = true

[engine]
fail_fast = false
parallel = true
dry_run = false
extra_args = []

[output]
format = "both" # text | json | both | sarif | receipt
json_path = "semverguard-report.json"
pretty_json = true
artifacts_dir = "artifacts/semverguard"
warn_as_fail = false
color = "auto"
verbosity = "normal"
```

## Exit Codes

- `0`: success (or baseline warnings in lenient modes)
- `1`: tool/runtime error
- `2`: semver violation
- `3`: baseline error treated as failure

In `mode = "cockpit"` with receipt output, semverguard exits `0` when receipt writing succeeds.

## CI

- GitHub Action: `.github/actions/semverguard/action.yml`
- Install and pinning guide: `docs/src/how-to/install.md`
- Cockpit flow: `docs/src/how-to/cockpit-integration.md`
- Full docs index: `docs/src/index.md`

## License

Licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).
