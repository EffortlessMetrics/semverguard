#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-engine
//!
//! Runs `cargo semver-checks check-release` as a subprocess.

use semverguard_domain::{Result, SemverEngine, SemverguardError};
use semverguard_types::{RequiredBump, SemverCheckOutput, SemverCheckRequest};
use std::path::PathBuf;
use std::process::Command;

/// Engine implementation that shells out to `cargo semver-checks`.
#[derive(Debug, Default)]
pub struct CargoSemverChecksEngine;

impl CargoSemverChecksEngine {
    fn build_command(req: &SemverCheckRequest) -> (PathBuf, Vec<String>) {
        // NOTE: The request currently does not carry a configurable cargo binary; we invoke `cargo`
        // from PATH. If you need a toolchain-specific cargo, add a field and plumb it through.
        let cargo = req
            .cargo_bin
            .clone()
            .unwrap_or_else(|| PathBuf::from("cargo"));

        let mut args: Vec<String> = Vec::new();
        args.push("semver-checks".into());
        args.push("check-release".into());

        // Strongly prefer explicit manifest path: makes workspace selection deterministic.
        args.push("--manifest-path".into());
        args.push(req.manifest_path.to_string_lossy().to_string());

        // Baseline selection
        match req.baseline.kind {
            semverguard_types::BaselineKind::CratesIo => {
                if let Some(v) = &req.baseline.version {
                    args.push("--baseline-version".into());
                    args.push(v.clone());
                }
            }
            semverguard_types::BaselineKind::Git => {
                if let Some(r) = &req.baseline.rev {
                    args.push("--baseline-rev".into());
                    args.push(r.clone());
                }
            }
        }

        if let Some(root) = &req.baseline.root {
            args.push("--baseline-root".into());
            args.push(root.to_string_lossy().to_string());
        }
        if let Some(rustdoc) = &req.baseline.rustdoc {
            args.push("--baseline-rustdoc".into());
            args.push(rustdoc.to_string_lossy().to_string());
        }

        // Features
        if req.features.all_features {
            args.push("--all-features".into());
        }
        if req.features.default_features {
            args.push("--default-features".into());
        }
        if req.features.only_explicit_features {
            args.push("--only-explicit-features".into());
        }
        if !req.features.features.is_empty() {
            args.push("--features".into());
            args.push(req.features.features.join(","));
        }
        if !req.features.baseline_features.is_empty() {
            args.push("--baseline-features".into());
            args.push(req.features.baseline_features.join(","));
        }
        if !req.features.current_features.is_empty() {
            args.push("--current-features".into());
            args.push(req.features.current_features.join(","));
        }

        // Extra args
        args.extend(req.extra_args.iter().cloned());

        (cargo, args)
    }
}

impl SemverEngine for CargoSemverChecksEngine {
    fn check(&self, request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)> {
        let (bin, args) = Self::build_command(&request);

        let mut cmd = Command::new(&bin);
        cmd.args(&args).current_dir(&request.workspace_root);

        let output = cmd
            .output()
            .map_err(|e| SemverguardError::Engine(format!("failed to run cargo semver-checks: {e}")))?;

        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;

        let exit_code = output.status.code();
        let success = output.status.success();

        // Heuristic: infer required bump from either stream.
        let required = RequiredBump::infer(&stdout).or_else(|| RequiredBump::infer(&stderr));

        let mut command = Vec::with_capacity(1 + args.len());
        command.push(bin.to_string_lossy().to_string());
        command.extend(args);

        Ok((
            command,
            SemverCheckOutput {
                exit_code,
                success,
                stdout,
                stderr,
                required_bump: required,
            },
        ))
    }
}
