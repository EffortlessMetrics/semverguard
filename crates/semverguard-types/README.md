# semverguard-types

`semverguard-types` contains shared schema and transport types used across the workspace.

## Modules

- `config`: `SemverguardConfig` and related policy/settings types
- `workspace`: workspace/package metadata structures
- `engine`: engine request/output and required bump inference
- `report`: run/list report structures and failure classification enums
- `receipt`: `sensor.report.v1` receipt structures for cockpit/receipt mode

## Typical Usage

```rust
use semverguard_types::SemverguardConfig;

let config: SemverguardConfig = toml::from_str(r#"
mode = "pr"

[baseline]
kind = "git"
rev = "origin/main"

[scope]
mode = "changed"
"#)?;
```

```rust
use semverguard_types::{RunReport, SensorReportV1};

let report_json = serde_json::to_string_pretty(&run_report)?;
let receipt_json = serde_json::to_string_pretty(&receipt)?;
```

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
