# Exit Codes

Reference for semverguard exit codes and their meanings.

## Summary

| Exit Code | Meaning | CI Behavior |
|-----------|---------|-------------|
| 0 | Success | Pass |
| 1 | SemVer failures detected | Fail |
| 2 | Configuration or invocation error | Fail |

## Exit Code 0: Success

All checks passed. No breaking changes detected in any checked packages.

### Conditions

- All eligible packages passed their SemVer check
- Skipped packages do not affect success/failure

### Example Scenario

```
semverguard: total=5 passed=3 failed=0 skipped=2
```

Even with skipped packages, exit code is 0 because no packages failed.

## Exit Code 1: SemVer Failures

One or more packages have breaking changes that require a version bump.

### Conditions

- At least one package's SemVer check failed
- Breaking API changes were detected

### Example Scenario

```
semverguard: total=5 passed=2 failed=1 skipped=2
FAIL  my-lib 1.0.0  /path/to/my-lib/Cargo.toml
      inferred required bump: Major
```

### CI Behavior

Exit code 1 should fail the CI job, preventing merge of breaking changes without version bump.

### Resolution

1. Review the breaking changes in the output
2. Either:
   - Bump the package version appropriately (major for breaking changes)
   - Revert the breaking changes
   - Mark the package as non-publishable if appropriate

## Exit Code 2: Configuration Error

Invalid configuration or invocation error prevented execution.

### Common Causes

#### Invalid Configuration

```
error: scope.mode=changed requires baseline.rev
```

Fix: Add `--baseline-rev` or configure in TOML:

```toml
[baseline]
rev = "origin/main"
```

#### Invalid Glob Pattern

```
error: bad glob '[invalid': unterminated character class
```

Fix: Correct the glob pattern syntax in scope.include or scope.exclude.

#### Missing Workspace Root

```
error: workspace root does not exist: /nonexistent/path
```

Fix: Ensure `--workspace-root` points to an existing directory.

#### Invalid TOML Syntax

```
error: invalid TOML in semverguard.toml: expected newline at line 5
```

Fix: Correct the TOML syntax error.

#### Incompatible Options

```
error: scope.mode=changed requires baseline.kind = "git"
```

Fix: Use git baseline with changed mode:

```toml
[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
```

### CI Behavior

Exit code 2 should fail the CI job, but indicates a setup issue rather than actual SemVer violations.

### Resolution

1. Check the error message for the specific issue
2. Fix the configuration or invocation
3. Use `semverguard print-config` to debug effective configuration

## Shell Integration

### Bash

```bash
semverguard check
case $? in
  0)
    echo "All checks passed"
    ;;
  1)
    echo "Breaking changes detected"
    exit 1
    ;;
  2)
    echo "Configuration error"
    exit 2
    ;;
esac
```

### GitHub Actions

```yaml
- name: Run semverguard
  id: semver
  run: semverguard check
  continue-on-error: true

- name: Handle result
  run: |
    if [ "${{ steps.semver.outcome }}" == "failure" ]; then
      echo "SemVer check failed"
      exit 1
    fi
```

### PowerShell

```powershell
semverguard check
switch ($LASTEXITCODE) {
    0 { Write-Host "All checks passed" }
    1 { Write-Host "Breaking changes detected"; exit 1 }
    2 { Write-Host "Configuration error"; exit 2 }
}
```

## Programmatic Access

The JSON report includes enough information to determine the exit code:

```bash
# Check for failures
if jq -e '.summary.failed > 0' report.json > /dev/null; then
  echo "Exit code would be 1"
fi
```

```python
import json
import sys

with open('report.json') as f:
    report = json.load(f)

if report['summary']['failed'] > 0:
    sys.exit(1)
sys.exit(0)
```

## Troubleshooting

### Unexpected Exit Code 2

Run with `print-config` to verify configuration:

```bash
semverguard print-config
```

Check for:
- Typos in config file keys
- Invalid TOML syntax
- Incompatible option combinations

### Exit Code 1 but No Output

Ensure output format includes text:

```bash
semverguard check --format both
```

Or check the JSON report:

```bash
semverguard check --json report.json
cat report.json | jq '.packages[] | select(.status == "failed")'
```

## See Also

- [CLI Reference](./cli.md) - Command-line options
- [Configuration Reference](./config.md) - Config file options
- [CI Integration](../tutorials/ci-integration.md) - CI setup guide
