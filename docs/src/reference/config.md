# Configuration Reference

Complete reference for `semverguard.toml` configuration options.

## Overview

semverguard reads configuration from `semverguard.toml` in the workspace root. All sections and values are optional—missing values use defaults.

## Complete Example

```toml
[baseline]
kind = "git"
rev = "origin/main"
# version = "1.0.0"       # For crates-io baseline
# root = "/path/to/baseline"
# rustdoc = "/path/to/baseline.json"

[scope]
mode = "changed"
include = ["my-lib-*"]
exclude = ["*-test", "*-internal"]
skip_publish_false = true
skip_no_lib = true

[features]
all_features = false
default_features = true
only_explicit_features = false
features = []
baseline_features = []
current_features = []

[engine]
# cargo_bin = "/path/to/cargo"
extra_args = []
fail_fast = false

[output]
format = "both"
json_path = "semver-report.json"
pretty_json = true
artifacts_dir = "artifacts/semverguard"
warn_as_fail = false
```

## Sections

### `[baseline]`

Controls the baseline for SemVer comparison—what version to compare against.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kind` | `"crates-io"` \| `"git"` | `"crates-io"` | Baseline source |
| `version` | String | — | Version number for crates-io baseline |
| `rev` | String | — | Git revision for git baseline |
| `root` | Path | — | Path to baseline workspace root (advanced) |
| `rustdoc` | Path | — | Path to pre-generated rustdoc JSON (advanced) |

#### `kind`

- `"crates-io"`: Compare against a published version on crates.io
- `"git"`: Compare against a git revision in the repository

#### `version`

For `kind = "crates-io"`. If omitted, cargo-semver-checks auto-detects the latest published version.

```toml
[baseline]
kind = "crates-io"
version = "1.2.3"
```

#### `rev`

For `kind = "git"`. Any git ref: branch, tag, or commit SHA.

```toml
[baseline]
kind = "git"
rev = "origin/main"
```

```toml
[baseline]
kind = "git"
rev = "v1.0.0"
```

#### `root` and `rustdoc`

Advanced options for monorepo setups or CI caching. Passed through to cargo-semver-checks.

---

### `[scope]`

Controls which packages to check.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | `"workspace"` \| `"changed"` | `"workspace"` | Package selection mode |
| `include` | String[] | `[]` | Glob patterns to include |
| `exclude` | String[] | `[]` | Glob patterns to exclude |
| `skip_publish_false` | bool | `true` | Skip packages with `publish = false` |
| `skip_no_lib` | bool | `true` | Skip packages without library targets |

#### `mode`

- `"workspace"`: Check all eligible packages in the workspace
- `"changed"`: Check only packages with files changed since baseline

`"changed"` requires `kind = "git"` and a `rev` value.

#### `include`

Glob patterns for package names to include. Empty means "include all".

```toml
[scope]
include = ["my-lib-*", "shared-*"]
```

#### `exclude`

Glob patterns for package names to exclude. Exclusion takes precedence over inclusion.

```toml
[scope]
exclude = ["*-test", "*-internal", "*-bench"]
```

#### Glob Pattern Syntax

| Pattern | Meaning |
|---------|---------|
| `*` | Match any characters |
| `?` | Match single character |
| `[abc]` | Match a, b, or c |
| `[!abc]` | Match anything except a, b, c |

---

### `[features]`

Feature selection passed through to cargo-semver-checks.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `all_features` | bool | `false` | Enable all features |
| `default_features` | bool | `true` | Enable default features |
| `only_explicit_features` | bool | `false` | Only use explicitly specified features |
| `features` | String[] | `[]` | Features to enable |
| `baseline_features` | String[] | `[]` | Features for baseline only |
| `current_features` | String[] | `[]` | Features for current only |

#### Example

```toml
[features]
all_features = false
default_features = true
features = ["serde", "async"]
```

---

### `[engine]`

Settings for invoking cargo-semver-checks.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `cargo_bin` | Path | — | Path to cargo binary (defaults to PATH) |
| `extra_args` | String[] | `[]` | Extra arguments for cargo-semver-checks |
| `fail_fast` | bool | `false` | Stop after first failure |

#### `cargo_bin`

Override the cargo binary:

```toml
[engine]
cargo_bin = "/home/user/.cargo/bin/cargo"
```

#### `extra_args`

Arguments appended to `cargo semver-checks check-release`:

```toml
[engine]
extra_args = ["--verbose", "--release"]
```

#### `fail_fast`

Stop checking packages after the first failure:

```toml
[engine]
fail_fast = true
```

---

### `[output]`

Controls output format and location.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | `"text"` \| `"json"` \| `"both"` \| `"sarif"` \| `"receipt"` | `"text"` | Output format |
| `json_path` | Path | — | JSON output file path |
| `pretty_json` | bool | `true` | Pretty-print JSON |
| `artifacts_dir` | Path | `artifacts/semverguard` | Output directory for receipt artifacts |
| `warn_as_fail` | bool | `false` | Treat warnings as failures for exit codes |

#### `format`

- `"text"`: Human-readable summary to stdout
- `"json"`: JSON report (to file or stdout)
- `"both"`: Text to stdout and JSON to file
- `"sarif"`: SARIF format for GitHub Code Scanning and security tools
- `"receipt"`: Cockpit receipt bundle (sensor.report.v1)

#### `json_path`

Where to write the JSON report. If omitted with `format = "json"`, writes to stdout.

```toml
[output]
format = "both"
json_path = "semver-report.json"
```

#### `pretty_json`

Enable indented JSON output (larger but readable):

```toml
[output]
pretty_json = true   # Indented
pretty_json = false  # Compact
```

#### `artifacts_dir`

Directory for receipt artifacts (when `format = "receipt"`):

```toml
[output]
format = "receipt"
artifacts_dir = "artifacts/semverguard"
```

#### `warn_as_fail`

Treat warnings as failures for exit codes:

```toml
[output]
warn_as_fail = true
```

---

## Default Configuration

If no `semverguard.toml` exists, these defaults apply:

```toml
[baseline]
kind = "crates-io"

[scope]
mode = "workspace"
include = []
exclude = []
skip_publish_false = true
skip_no_lib = true

[features]
all_features = false
default_features = true
only_explicit_features = false
features = []
baseline_features = []
current_features = []

[engine]
fail_fast = false
extra_args = []

[output]
format = "text"
pretty_json = true
artifacts_dir = "artifacts/semverguard"
warn_as_fail = false
```

## CLI Override

CLI flags override config file values. See [CLI Reference](./cli.md) for flag mappings.

## See Also

- [CLI Reference](./cli.md) - Command-line options
- [Filtering Strategy](../explanation/filtering-strategy.md) - How filters are applied
