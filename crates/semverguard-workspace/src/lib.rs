#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! semverguard-workspace
//!
//! Adapter for discovering workspace packages using `cargo metadata`.

use cargo_metadata::MetadataCommand;
use semverguard_domain::{Result, SemverguardError, WorkspaceProvider};
use semverguard_types::{WorkspaceMetadata, WorkspacePackage};
use std::path::{Path, PathBuf};

/// Workspace provider backed by `cargo metadata`.
#[derive(Debug, Default)]
pub struct CargoMetadataWorkspace;

impl WorkspaceProvider for CargoMetadataWorkspace {
    fn load(&self, workspace_root: &Path) -> Result<WorkspaceMetadata> {
        let mut cmd = MetadataCommand::new();
        cmd.current_dir(workspace_root);

        // If the workspace root has a Cargo.toml, prefer it as the explicit manifest.
        let manifest = workspace_root.join("Cargo.toml");
        if manifest.exists() {
            cmd.manifest_path(&manifest);
        }

        let metadata = cmd
            .exec()
            .map_err(|e| SemverguardError::Workspace(format!("cargo metadata failed: {e}")))?;

        let workspace_root = PathBuf::from(metadata.workspace_root.as_std_path());

        let workspace_members = metadata.workspace_members.clone();
        let mut packages = Vec::new();

        for pkg in metadata.packages {
            if !workspace_members.contains(&pkg.id) {
                continue;
            }

            let manifest_path = PathBuf::from(pkg.manifest_path.as_std_path());
            let package_root = manifest_path
                .parent()
                .ok_or_else(|| SemverguardError::Workspace("manifest has no parent dir".into()))?
                .to_path_buf();

            // In cargo metadata:
            // - publish = false -> publish = Some([]) (empty list)
            // - publish unset -> publish = None
            // - publish = ["my-registry"] -> publish = Some(["my-registry"])
            let publishable = match pkg.publish {
                None => true,
                Some(registries) => !registries.is_empty(),
            };

            let has_lib = pkg.targets.iter().any(|t| t.kind.iter().any(|k| k == "lib"));

            packages.push(WorkspacePackage {
                name: pkg.name,
                version: pkg.version,
                manifest_path,
                package_root,
                publishable,
                has_lib,
            });
        }

        Ok(WorkspaceMetadata {
            workspace_root,
            packages,
        })
    }
}
