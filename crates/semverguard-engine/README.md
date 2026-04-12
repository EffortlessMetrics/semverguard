# semverguard-engine

`semverguard-engine` is the `SemverEngine` adapter that runs
`cargo semver-checks check-release`.

## Responsibilities

- Build command lines from `SemverCheckRequest`
- Map semverguard baseline/feature options to upstream flags
- Execute the subprocess in workspace context
- Capture stdout/stderr/exit code into `SemverCheckOutput`
- Infer required bump (`major`/`minor`/`patch`) from tool output heuristics

## Public API

```rust
use semverguard_engine::CargoSemverChecksEngine;

let engine = CargoSemverChecksEngine::default();
let (cargo_bin, args) = CargoSemverChecksEngine::build_command(&request);
let (command, output) = engine.check(request)?;
```

## Baseline Flag Mapping

| Config field | Upstream flag |
| --- | --- |
| `baseline.version` | `--baseline-version` |
| `baseline.rev` | `--baseline-rev` |
| `baseline.root` | `--baseline-root` |
| `baseline.rustdoc` | `--baseline-rustdoc` |

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
