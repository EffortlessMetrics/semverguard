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
- **Coverage receipt** — `target/coverage/coverage-receipt.json` with execution metadata
- **Codecov dashboard** — Historical trend and badge

## Configuration

Coverage behavior is defined in:

- `.github/workflows/coverage.yml` — Workflow definition
- `codecov.yml` — Codecov reporting thresholds and comment policy

## Setup

### Codecov Token

The workflow generates coverage artifacts regardless of the `CODECOV_TOKEN` secret.
To upload to Codecov:

1. Visit [codecov.io](https://codecov.io) and sign in with GitHub
2. Enable coverage for `EffortlessMetrics/semverguard`
3. Retrieve the repository upload token
4. Add to GitHub repo settings:
   - **Settings** → **Secrets and variables** → **Actions**
   - **New repository secret**: `CODECOV_TOKEN=<token>`

With the token absent, the workflow skips Codecov uploads but still produces local artifacts.

## Troubleshooting

### Artifacts Missing
Check GitHub Actions run details:
- **Validate coverage output** step confirms file sizes
- **Upload coverage artifacts** step should complete even if Codecov fails

### Codecov Upload Fails
- On `main`: Workflow fails (hard fail)
- On PRs: Workflow succeeds with warning (soft fail)

Review the **Upload coverage to Codecov** step logs for details.

### Token Issues
If `CODECOV_TOKEN` is not configured:
- Artifacts are still generated and uploaded to GitHub Actions
- Codecov dashboard does not receive data
- No Codecov badge is updated

Re-add the secret and re-run the workflow to upload to Codecov.
