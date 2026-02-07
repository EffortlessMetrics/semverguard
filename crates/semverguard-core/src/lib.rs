#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-core
//!
//! Embeddable core library for semverguard. Provides pipeline orchestration,
//! receipt building, PR comment rendering, SARIF conversion, and exit code logic
//! without any CLI dependencies (no clap, no indicatif).
//!
//! # Features
//!
//! - `default-adapters` (default): Enables the convenience `pipeline::run()` function
//!   that wires up default adapters (workspace, git, engine).

/// Capability context for receipt generation.
pub mod capability;
/// PR comment rendering.
pub mod comment;
/// Configuration loading helpers.
pub mod config;
/// Exit code computation.
pub mod exit_code;
/// Pipeline orchestration (run checks -> build receipt -> compute exit code).
pub mod pipeline;
/// Receipt building, fingerprinting, and writing.
pub mod receipt;
/// SARIF 2.1.0 report generation.
pub mod sarif;

pub use capability::*;
pub use receipt::{
    CAP_REASON_GIT_UNAVAILABLE, CAP_REASON_NOT_REQUIRED, CAP_REASON_RESOLUTION_FAILED,
    CAP_REASON_SHALLOW_CLONE, CHECK_BASELINE, CHECK_ENGINE, CHECK_SEMVER, CHECK_TOOL, CODE_ERROR,
    CODE_MISSING, CODE_UNKNOWN, CODE_VIOLATION, CapabilityContext, REASON_ALL_PACKAGES_SKIPPED,
    REASON_BASELINE_UNAVAILABLE, REASON_ENGINE_ERROR, REASON_SEMVER_VIOLATION, REASON_TOOL_ERROR,
    REASON_TRUNCATED, ToolErrorFinding, build_artifact_index, build_receipt,
    build_receipt_with_capabilities, exit_code_from_receipt, has_tool_error, resolve_artifacts_dir,
    write_receipt_bundle,
};
