# Filtering Strategy

This document explains how semverguard filters packages through a three-pass pipeline, ensuring only relevant packages are checked.

## Overview

semverguard uses a **three-pass filtering strategy**:

1. **Pass 1: Static Filters** - include/exclude patterns, publishability, library target
2. **Pass 2: Scope Selection** - workspace mode (all) vs changed mode (git-based)
3. **Pass 3: Engine Execution** - run SemVer check, collect results

Each pass progressively narrows the set of packages.

## Why Three Passes?

The ordering is intentional:

1. **Static filters first**: Cheap operations that don't require external calls
2. **Git operations second**: Only compute changed files for packages that passed static filters
3. **SemVer checks last**: Expensive operation run only on truly eligible packages

This minimizes unnecessary work.

## Pass 1: Static Filters

These filters are applied first, using only information from workspace metadata.

### Explicit Package Selection

When specific packages are requested via CLI (e.g., `--package my-lib`), those packages take precedence over include/exclude patterns. Only the explicitly named packages are considered.

### Include/Exclude Patterns

```toml
[scope]
include = ["my-lib-*"]
exclude = ["*-test", "*-internal"]
```

Logic:
```
included = (include is empty) OR (name matches any include pattern)
excluded = name matches any exclude pattern
passes = included AND NOT excluded
```

Empty `include` means "include all". Exclude takes precedence over include.

### Publishability Filter

```toml
[scope]
skip_publish_false = true  # default
```

Packages with `publish = false` in their Cargo.toml are skipped. These packages aren't published to crates.io, so SemVer compatibility is typically irrelevant.

### Library Target Filter

```toml
[scope]
skip_no_lib = true  # default
```

Packages without a library target (`[lib]` in Cargo.toml) are skipped. Binary-only crates have no public Rust API to check.

### Filter Order in Pass 1

1. Explicit package selection (`--package` takes precedence)
2. Include/exclude patterns (package name)
3. `skip_publish_false` (publishability)
4. `skip_no_lib` (library target)

The first matching filter determines the skip reason.

## Pass 2: Scope Selection

After static filtering, scope selection determines which remaining packages to check.

### Workspace Mode (Default)

```toml
[scope]
mode = "workspace"
```

All packages that passed static filters are checked.

### Changed Mode

```toml
[scope]
mode = "changed"
```

Only packages with changed files (relative to baseline) are checked.

#### Change Detection Algorithm

1. Get changed file paths via `git diff --name-only <baseline>...HEAD` (three-dot syntax)
2. For each changed path, find which package directory contains it
3. Mark those packages as changed

The three-dot syntax compares the merge-base to HEAD, matching typical PR semantics.

```
Changed file: crates/my-lib/src/api.rs
Package roots: crates/my-lib, crates/other-lib
Result: my-lib is marked as changed
```

#### Workspace-Level Changes

If a file changes outside all package directories (e.g., root `Cargo.toml`, `.github/`), **all packages are marked as changed**.

This is conservative—workspace-level config changes could affect any package.

```
Changed file: Cargo.toml (workspace root)
Result: All packages marked as changed
```

## Pass 3: Engine Execution

For each package that passed filtering:

1. Build the `cargo semver-checks check-release` command
2. Execute and capture output
3. Parse success/failure and required bump
4. Add to results

### Fail-Fast Mode

```toml
[engine]
fail_fast = true
```

When enabled, execution stops after the first failing package. Useful for quick CI feedback.

## Complete Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    All Workspace Packages                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Pass 1: Static Filters                    │
├─────────────────────────────────────────────────────────────┤
│  0. Explicit package selection (--package)                   │
│     → Only named packages proceed (overrides include/exclude)│
│                                                              │
│  1. Include/exclude patterns                                 │
│     → SKIP: "filtered by include/exclude patterns"           │
│                                                              │
│  2. skip_publish_false = true                                │
│     → SKIP: "publish = false"                                │
│                                                              │
│  3. skip_no_lib = true                                       │
│     → SKIP: "no library target"                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Pass 2: Scope Selection                    │
├─────────────────────────────────────────────────────────────┤
│  mode = "workspace"                                          │
│     → All remaining packages proceed                         │
│                                                              │
│  mode = "changed"                                            │
│     → Only packages with changed files proceed               │
│     → SKIP: "unchanged relative to <baseline>"               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Pass 3: Engine Execution                   │
├─────────────────────────────────────────────────────────────┤
│  For each remaining package:                                 │
│     → Run cargo semver-checks check-release                  │
│     → PASS or FAIL based on exit code                        │
│                                                              │
│  If fail_fast = true and package fails:                      │
│     → Stop execution                                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                         Results                              │
├─────────────────────────────────────────────────────────────┤
│  packages: [...]  (all packages with status + reason)        │
│  summary: { total, passed, failed, skipped }                 │
└─────────────────────────────────────────────────────────────┘
```

## Skip Reasons

The report includes why each package was skipped:

| Skip Reason | Filter Pass | Cause |
|-------------|-------------|-------|
| `"filtered by include/exclude patterns"` | Pass 1 | Didn't match include or matched exclude |
| `"publish = false"` | Pass 1 | Package has `publish = false` |
| `"no library target"` | Pass 1 | Package has no lib target |
| `"unchanged relative to <rev>"` | Pass 2 | No changed files in changed mode |
| `"engine invocation failed: <error>"` | Pass 3 | Engine error (counted as failure) |

## Example Scenario

Configuration:
```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
include = ["my-lib-*"]
exclude = ["*-test"]
skip_publish_false = true
skip_no_lib = true
```

Workspace packages:
| Package | Publishable | Has Lib | Changed |
|---------|-------------|---------|---------|
| my-lib-core | ✓ | ✓ | ✓ |
| my-lib-utils | ✓ | ✓ | ✗ |
| my-lib-test | ✓ | ✓ | ✓ |
| internal-tools | ✗ | ✓ | ✓ |
| my-cli | ✓ | ✗ | ✓ |
| other-crate | ✓ | ✓ | ✓ |

Results:
| Package | Status | Reason |
|---------|--------|--------|
| my-lib-core | **CHECKED** | Passes all filters, is changed |
| my-lib-utils | SKIP | unchanged relative to origin/main |
| my-lib-test | SKIP | filtered by include/exclude patterns |
| internal-tools | SKIP | filtered by include/exclude patterns |
| my-cli | SKIP | no library target |
| other-crate | SKIP | filtered by include/exclude patterns |

Only `my-lib-core` is actually checked.

## See Also

- [Configure Monorepos](../how-to/configure-monorepos.md) - Practical filtering configuration
- [Check Changed Packages](../how-to/check-changed-packages.md) - Changed mode details
- [Architecture](./architecture.md) - Overall system design
