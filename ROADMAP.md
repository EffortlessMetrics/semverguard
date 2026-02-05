# semverguard Roadmap

**Position:** Workspace SemVer gate with receipts + CI ergonomics
**Non-goals:** Implementing semver logic, release orchestration, automatic version bump fixes

---

## v1.0 Core Capabilities

### 1. Receipt Bundle as Primary Integration Surface

The receipt bundle should be the canonical integration API, not legacy JSON.

- [ ] **Canonical artifact structure**
  - `artifacts/semverguard/report.json` (sensor envelope)
  - `artifacts/semverguard/comment.md` (deterministic)
  - `artifacts/semverguard/logs/` (raw logs per crate)
  - Optional SARIF output

- [ ] **Contract enforcement**
  - Schema validation tests for all output formats
  - Golden output fixtures with deterministic ordering
  - Deterministic ordering guarantees (sort by crate name, etc.)

### 2. Baseline Failure Taxonomy

Classify errors operationally to avoid "false red" killing adoption.

- [ ] **Error taxonomy**
  - `SemverViolation` → Exit 2 (CI policy fail)
  - `ToolError` (engine missing, spawn failure, parse error) → Exit 1 (tool fail)
  - `BaselineError` (rev missing, shallow clone, crate absent) → Configurable behavior

- [ ] **Baseline error handling**
  - Warn/skip in PR lanes for baseline errors
  - Fail in release lanes for baseline errors
  - Clear error messages explaining the baseline issue

### 3. Mode as First-Class Concept

Make the tool self-documenting with explicit modes.

- [ ] **PR mode** (`--mode pr`)
  - Skip unless version bump detected or explicit label
  - Baseline errors → warn and skip
  - Default for `changed` scope

- [ ] **Release mode** (`--mode release`)
  - Always run
  - Baseline errors → fail
  - Default for `workspace` scope

- [ ] **Mode detection**
  - Auto-detect from environment (CI_COMMIT_TAG, GITHUB_REF, etc.)
  - Explicit override via `--mode` flag

### 4. SARIF Honesty

SARIF output should be credible, not security theater.

- [ ] **Honest location data**
  - Attach to manifest paths when source spans unavailable
  - Include raw log references
  - Never invent region data

- [ ] **Taxonomy-aligned rules**
  - Rules keyed to: semver violation, baseline error, tool error
  - Clear rule descriptions
  - Severity mapping aligned with exit codes

### 5. Distribution & Installation

Make installation boring and deterministic.

- [ ] **Prebuilt binaries**
  - GitHub Releases for major platforms (linux-x64, macos-x64, macos-arm64, windows-x64)
  - Checksums and signatures

- [ ] **GitHub Action**
  - Reusable workflow snippet
  - Pin `cargo-semver-checks` version (MSRV drift protection)
  - Artifact upload and SARIF integration
  - One-paste installation

- [ ] **Installation docs**
  - Clear installation guide
  - Version compatibility matrix

---

## Implementation Phases

### Phase 1: Foundation (Current)
- [x] Workspace discovery + publishability + lib/proc-macro detection
- [x] Scope selection (workspace/changed)
- [x] Baseline modes (git rev, crates.io version)
- [x] Receipt bundle + comment generation
- [x] Exit codes (policy vs tool vs warn-as-fail)
- [x] SARIF as optional renderer
- [x] Integration tests

### Phase 2: Operability
- [ ] Harden baseline failure taxonomy
- [ ] Add mode concept (pr/release)
- [ ] Schema validation tests
- [ ] Golden output fixtures

### Phase 3: Distribution
- [ ] CI workflow for prebuilt binaries
- [ ] GitHub Action
- [ ] Installation documentation

### Phase 4: Polish
- [ ] SARIF honesty improvements
- [ ] Deterministic ordering enforcement
- [ ] Per-crate log capture

---

## Market Wedge

> "Make SemVer gating usable in multi-crate Rust workspaces without turning CI into a science project."

Key differentiators:
- Workspace orchestration (not just single-crate)
- Scoping intelligence (changed vs all)
- Evidence + stable outputs (receipts, not just stdout)
- CI-native (modes, triggers, artifacts)
