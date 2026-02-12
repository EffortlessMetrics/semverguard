# Installation & Pinning

This guide covers how to install semverguard and pin versions for reproducible CI builds.

## Install from crates.io

The simplest way to install semverguard:

```bash
cargo install semverguard-cli
```

You also need `cargo-semver-checks` (the upstream analysis tool):

```bash
cargo install cargo-semver-checks
```

## Install from GitHub Releases

Prebuilt binaries are available on the [GitHub Releases](https://github.com/EffortlessMetrics/semverguard/releases) page. Download the archive for your platform:

| Platform | Archive |
|----------|---------|
| Linux x86_64 (glibc) | `semverguard-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x86_64 (musl, static) | `semverguard-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `semverguard-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `semverguard-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `semverguard-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `semverguard-x86_64-pc-windows-msvc.zip` |

### Verify checksums

Each archive has a corresponding `.sha256` file. Verify after download:

```bash
# Linux/macOS
shasum -a 256 -c semverguard-x86_64-unknown-linux-gnu.tar.gz.sha256

# Or manually compare
shasum -a 256 semverguard-x86_64-unknown-linux-gnu.tar.gz
cat semverguard-x86_64-unknown-linux-gnu.tar.gz.sha256
```

### Extract and install

```bash
tar -xzf semverguard-x86_64-unknown-linux-gnu.tar.gz
sudo mv semverguard /usr/local/bin/
```

## Build from Source

Clone and build:

```bash
git clone https://github.com/EffortlessMetrics/semverguard.git
cd semverguard
cargo build --release -p semverguard-cli
# Binary is at target/release/semverguard
```

## Version Pinning

Pinning versions ensures reproducible builds and prevents unexpected breakage.

### GitHub Action

Use the `semverguard-version` and `cargo-semver-checks-version` inputs:

```yaml
- uses: EffortlessMetrics/semverguard/.github/actions/semverguard@main
  with:
    cargo-semver-checks-version: "0.35.0"
    semverguard-version: "0.1.0"  # or "source" to build from repo
```

### Cargo.toml dependency (embeddable library)

For programmatic use via `semverguard-core`:

```toml
[dependencies]
semverguard-core = "0.1"
```

### Scripts and CI

Pin with `--version` and `--locked`:

```bash
cargo install semverguard-cli --version "0.1.0" --locked
cargo install cargo-semver-checks --version "0.35.0" --locked
```

### Why pin cargo-semver-checks?

`cargo-semver-checks` occasionally bumps its MSRV (Minimum Supported Rust Version) in patch releases. If your CI runner has an older Rust toolchain, an unpinned install could fail to compile. Pinning to a tested version avoids this drift:

```yaml
# Pinned: predictable, reproducible
cargo install cargo-semver-checks --version "0.35.0" --locked

# Unpinned: may fail if upstream bumps MSRV
cargo install cargo-semver-checks
```

The GitHub Action pins `cargo-semver-checks` by default. Update the version after verifying compatibility with your project's Rust version.

## See Also

- [Getting Started](../tutorials/getting-started.md) - First-time setup walkthrough
- [CI Integration](../tutorials/ci-integration.md) - Adding semverguard to your pipeline
