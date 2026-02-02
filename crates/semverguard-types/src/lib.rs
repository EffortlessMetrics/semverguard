#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Shared types for the `semverguard` workspace.
//!
//! This crate intentionally avoids any heavy runtime dependencies. It’s mostly:
//! - config structs (serde-friendly)
//! - report structs (serde-friendly)
//! - request/response structs for the engine adapter

pub mod config;
pub mod engine;
pub mod report;
pub mod workspace;

pub use config::*;
pub use engine::*;
pub use report::*;
pub use workspace::*;
