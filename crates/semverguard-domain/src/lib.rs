#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-domain
//!
//! This crate contains the orchestration logic that ties together:
//! - workspace discovery (`cargo metadata`)
//! - optional git scoping ("only changed packages")
//! - the semver-check engine (cargo-semver-checks)
//!
//! It is written against small "ports" traits so the CLI can wire in real adapters,
//! and tests can wire in fakes.

/// Error types for semverguard operations.
pub mod error;
/// Mock/fake implementations of port traits for testing.
///
/// This module is only available when the `test-utils` feature is enabled or in test builds.
#[cfg(any(test, feature = "test-utils"))]
pub mod mocks;
/// Port trait definitions for dependency injection.
pub mod ports;
/// Progress tracking types and traits.
pub mod progress;
/// The main orchestration runner.
pub mod runner;

pub use error::*;
#[cfg(any(test, feature = "test-utils"))]
pub use mocks::*;
pub use ports::*;
pub use progress::*;
pub use runner::*;
