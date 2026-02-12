# Check Changed Packages

This guide shows how to configure semverguard to only check packages that have changed relative to a git baseline, rather than checking the entire workspace.

## Why Check Only Changed Packages?

In large workspaces, running semver checks on every package can be slow and noisy. Changed-only mode:

- **Speeds up PR checks** by skipping unmodified packages
- **Reduces noise** by only reporting on relevant changes
- **Integrates well with CI** for incremental validation

## Prerequisites

Changed mode requires:

1. A git repository (semverguard uses git to determine which files changed)
2. A baseline revision to compare against
3. `baseline.kind = "git"` in configuration

## Using the CLI

The quickest way to enable changed mode is via CLI flags:

```bash
semverguard check --changed --baseline-rev origin/main
```

This:

1. Compares the current HEAD against `origin/main`
2. Identifies which packages have modified files
3. Runs semver checks only on those packages

### Common Baseline References

```bash
# Compare against main branch
semverguard check --changed --baseline-rev origin/main

# Compare against a specific tag
semverguard check --changed --baseline-rev v1.2.0

# Compare against a specific commit
semverguard check --changed --baseline-rev abc1234
```

## Using Configuration

For persistent configuration, set these options in `semverguard.toml`:

```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
```

Then run without CLI flags:

```bash
semverguard check
```

## How Changes Are Detected

semverguard determines which packages changed by:

1. Running `git diff --name-only <baseline>..HEAD`
2. Mapping changed file paths to package directories
3. Including any package whose directory contains a changed file

### Workspace-Level Changes

If a file changes outside any package directory (e.g., the root `Cargo.toml`, `.github/` files), semverguard conservatively marks **all packages** as changed. This ensures that workspace-wide configuration changes trigger full verification.

## Combining with Other Filters

Changed mode combines with include/exclude filters:

```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
# Only check packages matching this pattern
include = ["my-lib-*"]
# Exclude test packages even if changed
exclude = ["*-test"]
# Skip unpublishable packages
skip_publish_false = true
```

The filtering pipeline is:

1. Apply include/exclude patterns
2. Apply skip_publish_false and skip_no_lib
3. Filter to only changed packages
4. Run checks on remaining packages

## Example Output

```
semverguard: total=5 passed=1 failed=0 skipped=4
SKIP  my-lib-core 1.0.0  (unchanged relative to origin/main)
SKIP  my-lib-utils 1.0.0  (unchanged relative to origin/main)
SKIP  my-lib-internal 0.1.0  (publish = false)
SKIP  other-crate 1.0.0  (filtered by include/exclude patterns)
```

Packages are skipped with specific reasons:

- `unchanged relative to <baseline>` - no files changed
- `publish = false` - filtered by skip_publish_false
- `filtered by include/exclude patterns` - didn't match include or matched exclude

## Troubleshooting

### "scope.mode=changed requires baseline.rev"

You must specify a baseline revision:

```bash
semverguard check --changed --baseline-rev origin/main
```

Or in config:

```toml
[baseline]
rev = "origin/main"
```

### "scope.mode=changed requires baseline.kind = git"

Changed mode only works with git baselines, not crates.io baselines:

```toml
[baseline]
kind = "git"  # Required for changed mode
rev = "origin/main"
```

### All Packages Marked as Changed

This happens when files outside package directories are modified. Check for:

- Root `Cargo.toml` changes
- Workspace configuration changes
- CI/CD file changes (`.github/`, `.gitlab-ci.yml`)

This is intentional—workspace-level changes could affect all packages.

## Next Steps

- [Configure monorepo patterns](./configure-monorepos.md) to filter packages by name
- [Generate JSON reports](./generate-json-reports.md) for CI artifact tracking
- [Understand filtering strategy](../explanation/filtering-strategy.md) for the full filtering pipeline
