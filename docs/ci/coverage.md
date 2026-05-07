# Coverage

Codecov coverage is Rust execution-surface evidence.

## What It Answers

> Did tests execute this Rust surface?

## What It Does NOT Answer

- Whether SemVer analysis is correct
- Whether `cargo-semver-checks` is correct
- Whether package selection is correct
- Whether changed-crate detection is correct
- Whether baseline handling is correct
- Whether SARIF, JSON, receipt, or text output is complete
- Whether the GitHub Action behavior is correct
- Whether contract provenance is complete
- Whether release readiness is proven

Those are separate proof lanes.

## Trigger

The Coverage workflow runs on:

- **Push to `main`**: Automatic, required step. Uploads to Codecov with hard failure if token present.
- **`workflow_dispatch`**: Manual trigger. Generates artifacts; uploads to Codecov if token present.
- **PRs labeled `coverage`, `full-ci`, or `ci:full`**: Opt-in. Generates artifacts; advisory Codecov upload (soft fail).

## Receipts

Durable receipts are:

- `coverage.json` — JSON report from `cargo-llvm-cov`
- `coverage.txt` — Human-readable summary
- `lcov.info` — LCOV format for Codecov
- **GitHub Actions artifact** — `coverage-report` containing all three files (14-day retention)
- **Codecov dashboard** — Historical trend and badge

## Configuration

Coverage behavior is defined in:

- `.github/workflows/coverage.yml` — Workflow definition
- `codecov.yml` — Codecov reporting thresholds and comment policy
