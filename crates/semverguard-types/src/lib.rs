#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Shared types for the `semverguard` workspace.
//!
//! This crate intentionally avoids any heavy runtime dependencies. It’s mostly:
//! - config structs (serde-friendly)
//! - report structs (serde-friendly)
//! - request/response structs for the engine adapter

/// Configuration schema for semverguard.
pub mod config;
/// Engine request/response types for cargo-semver-checks integration.
pub mod engine;
/// Report schema for semver check results.
pub mod report;
/// Receipt schema for cockpit integration.
pub mod receipt;
/// Workspace metadata types from cargo.
pub mod workspace;

pub use config::*;
pub use engine::*;
pub use report::*;
pub use receipt::*;
pub use workspace::*;
