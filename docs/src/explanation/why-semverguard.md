# Why semverguard?

This document explains when and why you might choose semverguard over using cargo-semver-checks directly.

## What cargo-semver-checks Does

[cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) is the tool that performs actual SemVer analysis. It:

- Compares two versions of a Rust crate's public API
- Detects breaking changes (removed functions, changed signatures, etc.)
- Reports which version bump is required
- Works on individual crates

cargo-semver-checks is excellent at its core job: analyzing a single crate's API compatibility.

## What semverguard Adds

semverguard is an **orchestration layer** that wraps cargo-semver-checks to handle workspace-level concerns:

### 1. Workspace-Wide Orchestration

**Problem**: cargo-semver-checks operates on one package at a time. In a monorepo with 20+ crates, you need to:
- Identify which crates need checking
- Run checks on each
- Aggregate results

**Solution**: semverguard runs checks across your entire workspace in one command:

```bash
# Instead of running 20 separate commands...
semverguard check
```

### 2. Smart Scoping

**Problem**: Not all packages in a workspace need SemVer checks:
- Internal packages (`publish = false`)
- Binary-only crates
- Test/example crates
- Unchanged packages in PRs

**Solution**: semverguard filters packages intelligently:

```toml
[scope]
include = ["public-*"]
exclude = ["*-internal"]
skip_publish_false = true
skip_no_lib = true
mode = "changed"  # Only check what changed
```

### 3. Centralized Configuration

**Problem**: Running cargo-semver-checks with the right flags for each package is tedious and error-prone.

**Solution**: One config file for the whole workspace:

```toml
# semverguard.toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
exclude = ["*-test"]

[output]
format = "both"
json_path = "report.json"
```

### 4. Machine-Readable Reports

**Problem**: Parsing cargo-semver-checks text output for CI tooling is fragile.

**Solution**: Structured JSON reports with consistent schema:

```json
{
  "packages": [...],
  "summary": { "total": 10, "passed": 8, "failed": 1, "skipped": 1 }
}
```

### 5. CI-Friendly Exit Codes

**Problem**: Distinguishing "check failed" from "config error" in CI scripts.

**Solution**: Semantic exit codes:
- `0` = All checks passed
- `1` = SemVer violations (fail the build)
- `2` = Configuration error (fix the setup)

## When to Use Which Tool

### Use cargo-semver-checks directly when:

- You have a single crate (not a workspace)
- You want to check one specific package
- You're learning how SemVer checking works
- You need features semverguard doesn't expose

```bash
# Single crate check
cargo semver-checks check-release
```

### Use semverguard when:

- You have a Cargo workspace with multiple crates
- You want to check only changed packages in PRs
- You need filtering (exclude internal crates, etc.)
- You want machine-readable output for CI
- You want one command for your entire workspace

```bash
# Workspace check with filtering
semverguard check --changed --baseline-rev origin/main
```

## Comparison Table

| Feature | cargo-semver-checks | semverguard |
|---------|---------------------|-------------|
| SemVer analysis | ✓ Core feature | Delegated |
| Single crate | ✓ | ✓ |
| Workspace orchestration | Manual | ✓ Automatic |
| Changed-only mode | ✗ | ✓ |
| Include/exclude patterns | ✗ | ✓ |
| Skip unpublishable | ✗ | ✓ |
| Centralized config | ✗ | ✓ |
| JSON reports | ✗ | ✓ |
| Semantic exit codes | Partial | ✓ |

## The Relationship

semverguard doesn't replace cargo-semver-checks—it enhances it:

```
┌─────────────────────────────────────────────────────────┐
│                     semverguard                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │  Orchestration, filtering, configuration,        │    │
│  │  reporting, CI integration                       │    │
│  └─────────────────────────────────────────────────┘    │
│                          │                               │
│                          ▼                               │
│  ┌─────────────────────────────────────────────────┐    │
│  │            cargo-semver-checks                   │    │
│  │    (actual SemVer analysis per package)          │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

Think of it like:
- **cargo-semver-checks** = the engine that detects breaking changes
- **semverguard** = the car that gets you where you need to go

## Example: Before and After

### Before (manual approach)

```bash
#!/bin/bash
# check-semver.sh - fragile, manual workspace checking

packages=$(cargo metadata --format-version 1 | jq -r '.packages[].name')

for pkg in $packages; do
  if [[ $pkg == *"-internal"* ]]; then
    continue  # Skip internal packages
  fi

  # Check if package changed (complex git logic)
  # ...

  cargo semver-checks check-release -p "$pkg" || exit 1
done
```

### After (semverguard)

```bash
semverguard check --changed --baseline-rev origin/main
```

With `semverguard.toml`:
```toml
[scope]
exclude = ["*-internal"]
skip_publish_false = true
```

## See Also

- [Getting Started](../tutorials/getting-started.md) - First-time setup
- [Architecture](./architecture.md) - How semverguard is built
- [CI Integration](../tutorials/ci-integration.md) - Setting up CI
