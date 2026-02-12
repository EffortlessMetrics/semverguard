# Configure Monorepos

This guide shows how to configure semverguard for monorepos with many crates, using include/exclude patterns and package filters.

## The Challenge

Monorepos often contain:

- Public library crates (need SemVer checks)
- Internal/private crates (shouldn't be checked)
- Binary-only crates (no public API to check)
- Test/example crates (not published)

semverguard provides multiple filtering mechanisms to handle this.

## Include/Exclude Patterns

Use glob patterns to select packages by name.

### Include Specific Packages

Only check packages matching a pattern:

```toml
[scope]
include = ["my-lib-*"]
```

This checks only packages whose names start with `my-lib-`.

### Exclude Specific Packages

Exclude packages from checks:

```toml
[scope]
exclude = ["*-test", "*-bench", "*-internal"]
```

This skips any package ending with `-test`, `-bench`, or `-internal`.

### Combine Include and Exclude

Patterns can be combined. Exclude takes precedence:

```toml
[scope]
# Check packages starting with "my-lib-"
include = ["my-lib-*"]
# But exclude test packages
exclude = ["*-test"]
```

For a package named `my-lib-core-test`:

1. Matches include pattern `my-lib-*` ✓
2. Matches exclude pattern `*-test` ✗

Result: **skipped** (exclude wins).

## Publishability Filters

### Skip Unpublishable Packages

Packages with `publish = false` in their `Cargo.toml` aren't published to crates.io, so SemVer checks are often unnecessary:

```toml
[scope]
skip_publish_false = true  # default
```

### Skip Binary-Only Crates

Packages without a library target have no public Rust API:

```toml
[scope]
skip_no_lib = true  # default
```

## Complete Monorepo Configuration

Here's a comprehensive configuration for a typical monorepo:

```toml
# semverguard.toml

[baseline]
kind = "git"
rev = "origin/main"

[scope]
# Only check changed packages in PRs
mode = "changed"

# Only check our public library crates
include = [
    "mycompany-*",
    "shared-*",
]

# Exclude internal and test crates
exclude = [
    "*-internal",
    "*-test",
    "*-bench",
    "*-examples",
]

# Skip unpublishable crates
skip_publish_false = true

# Skip binary-only crates
skip_no_lib = true

[output]
format = "both"
json_path = "semver-report.json"
```

## Pattern Syntax

semverguard uses [glob](https://docs.rs/globset/) patterns:

| Pattern | Matches |
|---------|---------|
| `*` | Any sequence of characters (except `/`) |
| `?` | Any single character |
| `[abc]` | Any character in the set |
| `[!abc]` | Any character not in the set |

### Examples

```toml
[scope]
# Match any package starting with "lib-"
include = ["lib-*"]

# Match exactly "core" or "utils"
include = ["core", "utils"]

# Match packages with version suffix (lib-v1, lib-v2)
include = ["lib-v?"]

# Match packages starting with a, b, or c
include = ["[abc]*"]
```

## Example Directory Structure

```
my-monorepo/
├── Cargo.toml              # Workspace manifest
├── semverguard.toml
├── crates/
│   ├── my-lib-core/        # ✓ Checked (matches include)
│   ├── my-lib-utils/       # ✓ Checked (matches include)
│   ├── my-lib-core-test/   # ✗ Skipped (matches exclude)
│   ├── internal-tools/     # ✗ Skipped (doesn't match include)
│   └── my-cli/             # ✗ Skipped (no lib target)
└── examples/
    └── demo/               # ✗ Skipped (publish = false)
```

With configuration:

```toml
[scope]
include = ["my-lib-*"]
exclude = ["*-test"]
skip_publish_false = true
skip_no_lib = true
```

## Verifying Your Configuration

Use `print-config` to see the resolved configuration:

```bash
semverguard print-config
```

Run with `--format text` to see which packages are skipped and why:

```bash
semverguard check --format text
```

Output shows skip reasons:

```
SKIP  my-lib-core-test 0.1.0  (filtered by include/exclude patterns)
SKIP  internal-tools 0.1.0  (filtered by include/exclude patterns)
SKIP  my-cli 0.1.0  (no library target)
SKIP  demo 0.1.0  (publish = false)
```

## Next Steps

- [Check only changed packages](./check-changed-packages.md) to speed up CI
- [Generate JSON reports](./generate-json-reports.md) for downstream processing
- [Understand filtering strategy](../explanation/filtering-strategy.md) for filter ordering
