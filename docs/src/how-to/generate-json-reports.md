# Generate JSON Reports

This guide shows how to generate machine-readable JSON reports from semverguard for CI artifacts, dashboards, or downstream tooling.

## Quick Start

Generate a JSON report file:

```bash
semverguard check --json report.json
```

This creates `report.json` with full details about every package checked.

## CLI Options

### Write JSON to a File

```bash
semverguard check --json path/to/report.json
```

When `--json` is specified without `--format`, semverguard outputs both text (to stdout) and JSON (to the file).

### JSON Only

To output only JSON (no text):

```bash
semverguard check --json report.json --format json
```

### JSON to stdout

Omit the path to write JSON to stdout:

```toml
[output]
format = "json"
# json_path not set = stdout
```

```bash
semverguard check --format json
```

## Configuration File

Configure JSON output in `semverguard.toml`:

```toml
[output]
# Output both text and JSON
format = "both"

# Write JSON to this file
json_path = "semver-report.json"

# Pretty-print JSON (default: true)
pretty_json = true
```

### Compact JSON

For smaller file sizes in CI:

```toml
[output]
format = "json"
json_path = "report.json"
pretty_json = false
```

## Report Structure

The JSON report contains:

```json
{
  "semverguard_version": "0.1.0",
  "started_at": "2024-01-15T10:00:00Z",
  "finished_at": "2024-01-15T10:01:30Z",
  "workspace_root": "/path/to/workspace",
  "packages": [...],
  "summary": {
    "total": 5,
    "passed": 3,
    "failed": 1,
    "skipped": 1
  }
}
```

See [Report Schema](../reference/report-schema.md) for the complete schema.

## Parsing Reports

### Using jq

Extract failed packages:

```bash
jq '.packages[] | select(.status == "failed") | .name' report.json
```

Get the summary:

```bash
jq '.summary' report.json
```

Check if any packages failed:

```bash
jq '.summary.failed > 0' report.json
```

### Using Python

```python
import json

with open('report.json') as f:
    report = json.load(f)

for pkg in report['packages']:
    if pkg['status'] == 'failed':
        print(f"FAIL: {pkg['name']} requires {pkg['inferred_required_bump']} bump")

print(f"Summary: {report['summary']['passed']}/{report['summary']['total']} passed")
```

### Using Rust

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Report {
    packages: Vec<Package>,
    summary: Summary,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    status: String,
    inferred_required_bump: Option<String>,
}

#[derive(Deserialize)]
struct Summary {
    total: usize,
    passed: usize,
    failed: usize,
}

fn main() {
    let report: Report = serde_json::from_str(&std::fs::read_to_string("report.json").unwrap()).unwrap();

    for pkg in report.packages.iter().filter(|p| p.status == "failed") {
        println!("FAIL: {} requires {:?} bump", pkg.name, pkg.inferred_required_bump);
    }
}
```

## CI Integration

### GitHub Actions with Artifact Upload

```yaml
- name: Run semverguard
  run: semverguard check --changed --baseline-rev origin/main --json report.json

- name: Upload report
  uses: actions/upload-artifact@v4
  if: always()
  with:
    name: semver-report
    path: report.json
```

### Processing Reports in CI

```yaml
- name: Check for failures
  run: |
    if jq -e '.summary.failed > 0' report.json > /dev/null; then
      echo "::error::SemVer violations detected"
      jq -r '.packages[] | select(.status == "failed") | "- \(.name): requires \(.inferred_required_bump // "unknown") bump"' report.json
      exit 1
    fi
```

### Slack/Discord Notifications

```yaml
- name: Notify on failure
  if: failure()
  run: |
    FAILED=$(jq -r '[.packages[] | select(.status == "failed") | .name] | join(", ")' report.json)
    curl -X POST "$SLACK_WEBHOOK" \
      -H 'Content-type: application/json' \
      -d "{\"text\": \"SemVer check failed for: $FAILED\"}"
```

## Downstream Tooling Ideas

- **Release automation**: Parse required bumps to auto-increment versions
- **Changelog generation**: Extract breaking changes from stderr
- **Dashboards**: Track SemVer health over time
- **PR comments**: Post failed packages as PR comments
- **Metrics**: Count breaking changes per release

## Next Steps

- [Report Schema Reference](../reference/report-schema.md) for complete field documentation
- [CI Integration Tutorial](../tutorials/ci-integration.md) for full workflow examples
- [Exit Codes Reference](../reference/exit-codes.md) for CI error handling
