# Cockpit Integration

This guide explains how to use semverguard in cockpit mode for receipt-driven CI orchestration.

## What Cockpit Mode Does

In cockpit mode (`--mode cockpit`), semverguard always exits 0 and emits a receipt bundle. The receipt contains all findings, verdicts, and capability information. A separate director process reads the receipt and makes the pass/fail decision.

This separation is useful when:

- Multiple sensors (linters, tests, semver checks) feed into a single decision engine
- Pass/fail policy is centralized rather than per-tool
- You want to collect evidence without gating individual steps

## Lane Summary

| Behavior | PR Lane | Release Lane | Cockpit Lane |
|----------|---------|--------------|--------------|
| Scope | changed | workspace | changed |
| Baseline errors | warn | fail | warn (in receipt) |
| SemVer failures | exit 2 | exit 2 | exit 0, director decides |
| Missing receipt | N/A | N/A | exit 1 |

## Configuration Presets

### PR lane

```toml
# semverguard.toml — PR lane
mode = "pr"

[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"

[output]
format = "both"
json_path = "semverguard-report.json"
```

### Release lane

```toml
# semverguard.toml — Release lane
mode = "release"

[baseline]
kind = "crates-io"

[scope]
mode = "workspace"

[output]
format = "both"
json_path = "semverguard-report.json"
warn_as_fail = true
```

### Cockpit lane

```toml
# semverguard.toml — Cockpit lane
mode = "cockpit"

[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"

[output]
format = "receipt"
artifacts_dir = "artifacts/semverguard"
```

## Director-Side Configuration

The cockpit director is not a semverguard concept — it is the external process that reads receipt artifacts and applies policy. A typical `cockpit.toml` snippet might look like:

```toml
# cockpit.toml (director configuration — not part of semverguard)
[[sensors]]
name = "semverguard"
artifact = "artifacts/semverguard/report.json"
schema = "sensor.report.v1"

[sensors.policy]
# Fail the pipeline if any semver violations found
fail_on = ["semver_violation"]
# Warn but don't fail on baseline issues
warn_on = ["baseline_unavailable"]
```

## GitHub Actions Examples

### PR + Release lanes

```yaml
name: SemVer Check

on:
  pull_request:
    branches: [main]
  push:
    tags: ["v*"]

jobs:
  semver-pr:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: EffortlessMetrics/semverguard/.github/actions/semverguard@main
        with:
          mode: pr
          baseline-rev: origin/main
          changed: "true"

  semver-release:
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: EffortlessMetrics/semverguard/.github/actions/semverguard@main
        with:
          mode: release
          changed: "false"
```

### Cockpit mode

```yaml
name: Cockpit Sensor

on:
  pull_request:
    branches: [main]

jobs:
  semver-sensor:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: EffortlessMetrics/semverguard/.github/actions/semverguard@main
        with:
          mode: cockpit
          baseline-rev: origin/main
          changed: "true"

      - name: Upload receipt artifact
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: semverguard-receipt
          path: artifacts/semverguard/
```

## Receipt Artifact Structure

In cockpit mode with `format = "receipt"`, semverguard writes to the `artifacts_dir`:

```
artifacts/semverguard/
  report.json          # sensor.report.v1 envelope
```

The `report.json` follows the `sensor.report.v1` schema (see [Receipt Schema](../reference/receipt-schema.md)) and contains:

- **identity**: sensor name, version, fingerprint
- **capabilities**: git availability, resolution status
- **findings**: individual semver violations per package
- **verdict**: pass/fail/warn with reason codes

## See Also

- [Receipt Schema](../reference/receipt-schema.md) - Full schema reference
- [Configuration Reference](../reference/config.md) - All config options including `mode`
- [Exit Codes](../reference/exit-codes.md) - Exit code semantics per mode
