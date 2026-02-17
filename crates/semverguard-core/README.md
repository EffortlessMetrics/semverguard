# semverguard-core

`semverguard-core` is the reusable orchestration and reporting layer used by the CLI.

It sits above `semverguard-domain` and can be embedded by other tools that want semverguard
behavior without pulling in CLI argument parsing.

## Responsibilities

- Load and validate semverguard config
- Run pipeline orchestration (`pipeline::run` / `pipeline::run_with_adapters`)
- Build receipt bundles (`sensor.report.v1`) with findings and artifact index
- Render markdown comments from receipts
- Convert reports/receipts to SARIF
- Compute exit codes from report or receipt verdicts
- Provide finding explanation registry (`explain`)
- Promote git baselines in `semverguard.toml`

## Feature Flag

- `default-adapters` (enabled by default): enables convenience functions that wire
  `semverguard-workspace`, `semverguard-git`, and `semverguard-engine`

## Usage

```rust
use semverguard_core::pipeline::{run, PipelineOptions};

let options = PipelineOptions {
    workspace_root,
    config,
    sarif: false,
    tool_version: None,
    progress: None,
};

let result = run(&options)?;
```

For custom adapters, use `pipeline::run_with_adapters`.

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
