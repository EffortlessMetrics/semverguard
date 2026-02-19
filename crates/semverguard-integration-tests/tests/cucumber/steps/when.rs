//! When step definitions for executing actions.

use crate::world::TestWorld;
use cucumber::when;

#[when("running check")]
fn running_check(world: &mut TestWorld) {
    world.execute_run();
}

#[when("running list")]
fn running_list(world: &mut TestWorld) {
    world.execute_list();
}

#[when("validating config")]
fn validating_config(world: &mut TestWorld) {
    // Config validation happens during run, so we attempt a run
    world.execute_run();
}

#[when("running pipeline check")]
fn running_pipeline_check(world: &mut TestWorld) {
    world.execute_pipeline_run();
}

#[when("writing pipeline receipt bundle")]
fn writing_pipeline_receipt_bundle(world: &mut TestWorld) {
    world.execute_write_pipeline_receipt();
}
