# Exit Codes

Reference for semverguard exit codes and their meanings.

## Summary

| Exit Code | Meaning | CI Behavior |
|-----------|---------|-------------|
| 0 | Success (pass or warn when warn-as-fail is disabled) | Pass |
| 1 | Tool/runtime error | Fail |
| 2 | SemVer policy failure | Fail |
| 3 | Warnings treated as failures (warn-as-fail) | Fail |

## Exit Code 0: Success

All checks passed, or only warnings were emitted and warn-as-fail is disabled.

### Conditions

- No SemVer violations
- No tool/runtime errors
- Baseline warnings are allowed (default)

## Exit Code 1: Tool/Runtime Error

The tool failed to execute correctly (configuration error, missing baseline history, engine failure, etc.).

### Common Causes

- Invalid configuration or TOML syntax
- Missing workspace root
- `cargo-semver-checks` not installed or failed to run
- Git baseline not available

### CI Behavior

Exit code 1 should fail the CI job and be treated as a tooling problem.

## Exit Code 2: SemVer Policy Failure

One or more packages failed SemVer checks due to API compatibility violations.

### Conditions

- At least one package returns a SemVer violation (breaking change)
- SemVer policy failure takes precedence over baseline warnings

## Exit Code 3: Warn-as-Fail

Warnings were emitted (e.g., baseline issues) and warn-as-fail is enabled.

### Conditions

- No SemVer violations
- No tool/runtime errors
- At least one warning, and `warn_as_fail = true`

## Shell Integration

### Bash

```bash
semverguard check
case $? in
  0)
    echo "Checks passed (or warnings allowed)"
    ;;
  1)
    echo "Tool/runtime error"
    exit 1
    ;;
  2)
    echo "SemVer violations detected"
    exit 2
    ;;
  3)
    echo "Warnings treated as failures"
    exit 3
    ;;
esac
```

### PowerShell

```powershell
semverguard check
switch ($LASTEXITCODE) {
    0 { Write-Host "Checks passed (or warnings allowed)" }
    1 { Write-Host "Tool/runtime error"; exit 1 }
    2 { Write-Host "SemVer violations detected"; exit 2 }
    3 { Write-Host "Warnings treated as failures"; exit 3 }
}
```

## Programmatic Access

For receipt output (`--format receipt`), use the verdict:

- `pass` → exit 0
- `warn` → exit 0 or 3 (if warn-as-fail)
- `fail` → exit 1 (tool error) or 2 (policy failure)
- `skip` → exit 0

For legacy `RunReport`, check `packages[].failure_kind` and summary counts.

## See Also

- [CLI Reference](./cli.md) - Command-line options
- [Configuration Reference](./config.md) - Config file options
- [Report Schema](./report-schema.md) - JSON output format
