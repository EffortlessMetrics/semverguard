# Receipt Schema (sensor.report.v1)

Reference for the cockpit receipt format emitted by `semverguard` when `--format receipt` is used.

## Overview

Receipt output produces a canonical artifact bundle (paths are workspace-root-relative with
forward slashes when possible):

```
artifacts/semverguard/report.json
artifacts/semverguard/comment.md
artifacts/semverguard/sarif.json    # optional
artifacts/semverguard/raw/*.log
```

The receipt is a JSON envelope with stable fields for cockpit ingestion.

## Top-Level Structure

```json
{
  "schema": "sensor.report.v1",
  "tool": { ... },
  "run": { ... },
  "verdict": { ... },
  "findings": [ ... ],
  "data": { ... },
  "artifacts": { ... }
}
```

## Required Fields

- `schema`
- `tool`
- `run`
- `verdict`
- `findings`
- `artifacts`

## `tool`

```json
{
  "name": "semverguard",
  "version": "0.1.0",
  "repository_url": "https://github.com/EffortlessMetrics/semverguard"
}
```

## `run`

```json
{
  "started_at": "2024-01-15T10:00:00Z",
  "finished_at": "2024-01-15T10:01:30Z",
  "duration_ms": 90000,
  "workspace_root": "/path/to/workspace",
  "baseline": { ... }
}
```

## `verdict`

```json
{
  "status": "pass",
  "reason": "all checks passed"
}
```

`status` is one of: `pass`, `warn`, `fail`, `skip`.

## `findings`

Each finding uses a stable identity:

```json
{
  "check_id": "semver",
  "code": "violation",
  "level": "error",
  "message": "Package `my-lib` (v1.0.0) requires a major version bump",
  "location": {
    "path": "crates/my-lib/Cargo.toml",
    "raw_log": "artifacts/semverguard/raw/my-lib-1.0.0.stderr.log"
  },
  "data": {
    "package": "my-lib",
    "version": "1.0.0",
    "required_bump": "major",
    "failure_kind": "semver-violation"
  }
}
```

## `data`

Contains the legacy `RunReport` payload when available:

```json
{
  "report": { ... }
}
```

## `artifacts`

```json
{
  "report_json": "artifacts/semverguard/report.json",
  "comment_md": "artifacts/semverguard/comment.md",
  "sarif_json": "artifacts/semverguard/sarif.json",
  "raw_logs": [
    {
      "package": "my-lib",
      "version": "1.0.0",
      "stdout": "artifacts/semverguard/raw/my-lib-1.0.0.stdout.log",
      "stderr": "artifacts/semverguard/raw/my-lib-1.0.0.stderr.log"
    }
  ]
}
```

## `truncation`

Optional. Present when findings exceed the 500-finding limit:

```json
{
  "original_count": 742,
  "limit": 500
}
```

## Finding Identity Registry

These `check_id`/`code` pairs are **API surface**. New pairs may be added in minor versions.
Existing pairs will not be renamed without a major version bump.

| check_id | code | level | description |
|---|---|---|---|
| `semver` | `violation` | error | SemVer policy violation |
| `baseline` | `missing` | warning | Baseline not found |
| `tool` | `error` | error | Tool/runtime error |
| `engine` | `unknown` | error | Unclassifiable engine failure |

## See Also

- [CLI Reference](./cli.md)
- [Report Schema](./report-schema.md)
