#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-domain
//!
//! This crate contains the orchestration logic that ties together:
//! - workspace discovery (`cargo metadata`)
//! - optional git scoping (“only changed packages”)
//! - the semver-check engine (cargo-semver-checks)
//!
//! It is written against small “ports” traits so the CLI can wire in real adapters,
//! and tests can wire in fakes.

pub mod error;
pub mod ports;
pub mod runner;

pub use error::*;
pub use ports::*;
pub use runner::*;
