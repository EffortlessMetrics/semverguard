# CI Integration

This tutorial shows how to integrate semverguard into your CI pipeline to automatically catch breaking changes before they're merged.

## GitHub Actions

Here's a complete GitHub Actions workflow for running semverguard on pull requests:

```yaml
# .github/workflows/semver.yml
name: SemVer Check

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

jobs:
  semver-check:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          # Important: fetch enough history for changed-mode git diff
          fetch-depth: 0

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install tools
        run: |
          cargo install cargo-semver-checks
          cargo install semverguard-cli

      - name: Run semverguard
        run: |
          semverguard check \
            --changed \
            --baseline-rev origin/main \
            --json semver-report.json

      - name: Upload report
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: semver-report
          path: semver-report.json
```

### Key Configuration Points

#### Fetch Depth

When using `--changed` mode, semverguard needs git history to compute which packages changed:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0  # Fetch all history
```

For large repositories, you can limit this:

```yaml
with:
  fetch-depth: 100  # Fetch last 100 commits
```

#### Baseline Reference

For pull requests, compare against the target branch:

```bash
semverguard check --changed --baseline-rev origin/main
```

The baseline revision should be a ref that cargo-semver-checks can resolve. Common choices:

- `origin/main` - the main branch
- `origin/develop` - a development branch
- A specific tag like `v1.0.0`

#### Artifact Upload

Always upload the JSON report, even on failure:

```yaml
- uses: actions/upload-artifact@v4
  if: always()  # Upload even if check fails
  with:
    name: semver-report
    path: semver-report.json
```

## Exit Codes for CI Gating

semverguard uses exit codes to integrate with CI systems:

| Exit Code | Meaning |
|-----------|---------|
| 0 | Pass (or warn when warn-as-fail is disabled) |
| 1 | Tool/runtime error |
| 2 | SemVer policy failure |
| 3 | Warn-as-fail |

Exit code 2 fails the CI job when breaking changes are detected, providing automatic gatekeeping.

## Caching

Speed up CI runs by caching cargo installations:

```yaml
- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/bin/
      ~/.cargo/registry/
      ~/.cargo/git/
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

- name: Install tools
  run: |
    which cargo-semver-checks || cargo install cargo-semver-checks
    which semverguard || cargo install semverguard-cli
```

## Full Workflow with Caching

Here's a production-ready workflow:

```yaml
name: SemVer Check

on:
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  semver-check:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/
            ~/.cargo/git/
          key: ${{ runner.os }}-cargo-semver-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-semver-
            ${{ runner.os }}-cargo-

      - name: Install tools
        run: |
          which cargo-semver-checks || cargo install cargo-semver-checks
          which semverguard || cargo install semverguard-cli

      - name: Run semverguard
        id: semver
        run: |
          semverguard check \
            --changed \
            --baseline-rev origin/${{ github.base_ref }} \
            --json semver-report.json \
            --format both

      - name: Upload report
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: semver-report
          path: semver-report.json
          retention-days: 30
```

## Other CI Systems

### GitLab CI

```yaml
semver-check:
  stage: test
  image: rust:latest
  before_script:
    - cargo install cargo-semver-checks semverguard-cli
  script:
    - semverguard check --changed --baseline-rev origin/main --json report.json
  artifacts:
    paths:
      - report.json
    when: always
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
```

### CircleCI

```yaml
version: 2.1

jobs:
  semver-check:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - run:
          name: Install tools
          command: cargo install cargo-semver-checks semverguard-cli
      - run:
          name: Run semverguard
          command: semverguard check --changed --baseline-rev origin/main --json report.json
      - store_artifacts:
          path: report.json

workflows:
  pr-checks:
    jobs:
      - semver-check
```

## Next Steps

- [Configure monorepo filtering](../how-to/configure-monorepos.md) to exclude internal crates
- [Parse JSON reports](../how-to/generate-json-reports.md) for custom CI tooling
- [Understand exit codes](../reference/exit-codes.md) for error handling
