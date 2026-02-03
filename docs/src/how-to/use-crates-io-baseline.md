# Use crates.io Baseline

This guide shows how to configure semverguard to compare against versions published to crates.io instead of a git revision.

## When to Use crates.io Baseline

Use crates.io baseline when:

- You want to compare against the **last published version**
- Your published crates and development branches differ significantly
- You don't have access to git history (e.g., downloaded source tarball)
- You want to verify a release candidate against production

Use git baseline when:

- You want to compare against a specific commit or branch
- You're checking incremental changes in a PR
- You need `--changed` mode (git-only feature)

## Default Behavior

By default, semverguard uses crates.io baseline:

```toml
[baseline]
kind = "crates-io"
# version = not set (auto-detect latest)
```

cargo-semver-checks will automatically find the latest published version on crates.io.

## Explicit Version

Specify a version to compare against:

```bash
semverguard check --baseline-version 1.2.0
```

Or in configuration:

```toml
[baseline]
kind = "crates-io"
version = "1.2.0"
```

### Use Cases for Explicit Versions

1. **Pre-release comparison**: Compare against last stable before publishing beta
   ```toml
   version = "1.0.0"  # Compare against 1.0.0 even if 1.1.0-beta exists
   ```

2. **Major version boundary**: Check compatibility with an older major version
   ```toml
   version = "1.0.0"  # Am I still compatible with v1?
   ```

3. **Reproducible CI**: Pin to a specific version for consistent checks
   ```toml
   version = "2.3.4"
   ```

## CLI Override

Override config file baseline with CLI flags:

```bash
# Use crates.io with auto-detected version
semverguard check --baseline-version ""

# Use specific version
semverguard check --baseline-version 1.2.3
```

Note: `--baseline-version` automatically sets `kind = "crates-io"`.

## Combining with Git Baseline

You can't use both baselines simultaneously, but you can:

### Per-Environment Configuration

Development (git baseline for PRs):

```bash
semverguard check --changed --baseline-rev origin/main
```

Release validation (crates.io baseline):

```bash
semverguard check --baseline-version 1.0.0
```

### Configuration Profiles

Create separate config files:

```bash
# For PR checks
semverguard check --config semverguard-pr.toml

# For release validation
semverguard check --config semverguard-release.toml
```

`semverguard-pr.toml`:
```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
```

`semverguard-release.toml`:
```toml
[baseline]
kind = "crates-io"
# Auto-detect latest published version
```

## Limitations

### Changed Mode Not Supported

`scope.mode = "changed"` requires git baseline:

```toml
# This will error:
[baseline]
kind = "crates-io"

[scope]
mode = "changed"  # Error: requires git baseline
```

Changed mode needs git diff to determine which packages were modified.

### Private Registries

semverguard passes baseline options through to cargo-semver-checks. For private registries, consult cargo-semver-checks documentation.

## Example Workflows

### Release Checklist

Before publishing to crates.io:

```bash
# Compare current state against last published version
semverguard check

# Review the report
cat semver-report.json | jq '.packages[] | select(.status == "failed")'

# If breaking changes detected, bump major version
cargo set-version --workspace 2.0.0
```

### Breaking Change Audit

Check if changes since v1.0.0 require a major bump:

```bash
semverguard check --baseline-version 1.0.0 --json audit.json

# Extract required bumps
jq '.packages[] | select(.inferred_required_bump == "major") | .name' audit.json
```

## Troubleshooting

### "Package not found on crates.io"

The package hasn't been published yet. Either:

1. Publish an initial version first
2. Use git baseline instead:
   ```bash
   semverguard check --baseline-rev origin/main
   ```

### Version Mismatch

If cargo-semver-checks can't find the specified version:

```
Error: could not find version 1.2.3 of my-crate
```

Verify the version exists:

```bash
cargo search my-crate
```

## Next Steps

- [Check changed packages](./check-changed-packages.md) with git baseline
- [CLI Reference](../reference/cli.md) for all baseline options
- [Configuration Reference](../reference/config.md) for baseline settings
