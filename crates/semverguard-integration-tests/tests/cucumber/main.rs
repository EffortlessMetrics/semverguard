//! Cucumber BDD test runner for semverguard.
//!
//! This module provides the entry point for running Gherkin-based BDD tests
//! using cucumber-rs. Tests are written in feature files under `features/`
//! and step definitions are implemented in the `steps` module.

mod steps;
mod world;

use cucumber::World;
use world::TestWorld;

#[tokio::main]
async fn main() {
    TestWorld::run("tests/../features").await;
}
