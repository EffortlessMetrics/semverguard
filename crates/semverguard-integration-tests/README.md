# semverguard-integration-tests

`semverguard-integration-tests` is a non-published crate that holds workspace-level integration
coverage for semverguard behavior.

## What It Contains

- Scenario-style tests in `tests/scenarios.rs`
- Cucumber BDD runner in `tests/cucumber/main.rs`
- Gherkin feature files in `features/*.feature`

The tests exercise orchestration behavior across config handling, filtering, scoping,
output shaping, and receipt/exit-code behavior.

## Run Tests

```bash
# Scenario tests + cucumber test target
cargo test -p semverguard-integration-tests

# Cucumber target directly
cargo test -p semverguard-integration-tests --test cucumber
```

## Notes

- `publish = false`
- Depends on workspace crates and `semverguard-domain` `test-utils` feature for mocks

## License

Licensed under either [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT).
