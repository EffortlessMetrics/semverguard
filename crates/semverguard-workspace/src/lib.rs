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

            let has_lib = pkg
                .targets
                .iter()
                .any(|t| t.kind.iter().any(|k| k == "lib" || k == "proc-macro"));

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

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_domain::WorkspaceProvider;
    use std::fs;

    /// Returns the path to the test workspace fixture.
    fn test_workspace_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("test_workspace")
    }

    /// Helper to create a temporary workspace with specified package configs.
    fn create_temp_workspace(
        packages: &[(&str, &str, bool, bool)], // (name, version, has_lib, publishable)
    ) -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        let workspace_root = temp.path();

        // Create workspace Cargo.toml
        let member_paths: Vec<String> = packages
            .iter()
            .map(|(name, _, _, _)| format!("\"{}\"", name))
            .collect();
        let workspace_toml = format!(
            r#"[workspace]
resolver = "2"
members = [{}]
"#,
            member_paths.join(", ")
        );
        fs::write(workspace_root.join("Cargo.toml"), workspace_toml).unwrap();

        // Create each package
        for (name, version, has_lib, publishable) in packages {
            let pkg_dir = workspace_root.join(name);
            fs::create_dir_all(pkg_dir.join("src")).unwrap();

            let publish_line = if *publishable {
                ""
            } else {
                "publish = false\n"
            };
            let target_section = if *has_lib {
                format!(
                    r#"[lib]
path = "src/lib.rs"
"#
                )
            } else {
                format!(
                    r#"[[bin]]
name = "{}"
path = "src/main.rs"
"#,
                    name
                )
            };

            let cargo_toml = format!(
                r#"[package]
name = "{}"
version = "{}"
edition = "2021"
{}
{}
"#,
                name, version, publish_line, target_section
            );
            fs::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();

            if *has_lib {
                fs::write(pkg_dir.join("src").join("lib.rs"), "// lib").unwrap();
            } else {
                fs::write(pkg_dir.join("src").join("main.rs"), "fn main() {}").unwrap();
            }
        }

        temp
    }

    #[test]
    fn test_load_identifies_workspace_members() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        // Should have exactly 4 workspace members
        assert_eq!(metadata.packages.len(), 4);

        let names: Vec<&str> = metadata.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"lib-pkg"));
        assert!(names.contains(&"bin-pkg"));
        assert!(names.contains(&"unpublishable-pkg"));
        assert!(names.contains(&"registry-restricted-pkg"));
    }

    #[test]
    fn test_publishable_detection() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        let find_pkg = |name: &str| metadata.packages.iter().find(|p| p.name == name).unwrap();

        // lib-pkg: publish unset -> publishable = true
        let lib_pkg = find_pkg("lib-pkg");
        assert!(lib_pkg.publishable);

        // bin-pkg: publish unset -> publishable = true
        let bin_pkg = find_pkg("bin-pkg");
        assert!(bin_pkg.publishable);

        // unpublishable-pkg: publish = false -> publishable = false
        let unpub_pkg = find_pkg("unpublishable-pkg");
        assert!(!unpub_pkg.publishable);

        // registry-restricted-pkg: publish = ["my-private-registry"] -> publishable = true
        let registry_pkg = find_pkg("registry-restricted-pkg");
        assert!(registry_pkg.publishable);
    }

    #[test]
    fn test_library_target_detection() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        let find_pkg = |name: &str| metadata.packages.iter().find(|p| p.name == name).unwrap();

        // lib-pkg: has [lib] target -> has_lib = true
        let lib_pkg = find_pkg("lib-pkg");
        assert!(lib_pkg.has_lib, "lib-pkg should have a library target");

        // bin-pkg: only [[bin]] target -> has_lib = false
        let bin_pkg = find_pkg("bin-pkg");
        assert!(!bin_pkg.has_lib, "bin-pkg should not have a library target");

        // unpublishable-pkg: has [lib] target -> has_lib = true
        let unpub_pkg = find_pkg("unpublishable-pkg");
        assert!(unpub_pkg.has_lib);

        // registry-restricted-pkg: has [lib] target -> has_lib = true
        let registry_pkg = find_pkg("registry-restricted-pkg");
        assert!(registry_pkg.has_lib);
    }

    #[test]
    fn test_manifest_path_is_correctly_captured() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        for pkg in &metadata.packages {
            // manifest_path should end with Cargo.toml
            assert!(pkg.manifest_path.ends_with("Cargo.toml"));

            // manifest_path should exist
            assert!(pkg.manifest_path.exists());

            // manifest_path should be absolute
            assert!(pkg.manifest_path.is_absolute());

            // package_root should be the parent of manifest_path
            assert_eq!(pkg.package_root, pkg.manifest_path.parent().unwrap());

            // package_root should contain the package name in the path
            let root_str = pkg.package_root.to_string_lossy();
            let normalized_name = pkg.name.replace('-', "_");
            let root_matches = root_str.contains(&pkg.name) || root_str.contains(&normalized_name);
            assert!(root_matches);
        }
    }

    #[test]
    fn test_workspace_root_is_correctly_set() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        // workspace_root should be an absolute path that exists
        assert!(metadata.workspace_root.is_absolute());
        assert!(metadata.workspace_root.exists());

        // The workspace root should contain a Cargo.toml
        assert!(metadata.workspace_root.join("Cargo.toml").exists());

        // The workspace root path should end with the expected directory name
        // (handling both Unix and Windows path separators and \\?\ prefix)
        let root_str = metadata.workspace_root.to_string_lossy();
        assert!(root_str.ends_with("test_workspace"));
    }

    #[test]
    fn test_package_versions_are_parsed() {
        let workspace_path = test_workspace_path();
        let provider = CargoMetadataWorkspace;

        let metadata = provider
            .load(&workspace_path)
            .expect("failed to load workspace");

        let find_pkg = |name: &str| metadata.packages.iter().find(|p| p.name == name).unwrap();

        // lib-pkg: version = "1.0.0"
        let lib_pkg = find_pkg("lib-pkg");
        assert_eq!(lib_pkg.version.major, 1);
        assert_eq!(lib_pkg.version.minor, 0);
        assert_eq!(lib_pkg.version.patch, 0);

        // bin-pkg: version = "2.3.4"
        let bin_pkg = find_pkg("bin-pkg");
        assert_eq!(bin_pkg.version.major, 2);
        assert_eq!(bin_pkg.version.minor, 3);
        assert_eq!(bin_pkg.version.patch, 4);

        // unpublishable-pkg: version = "0.1.0"
        let unpub_pkg = find_pkg("unpublishable-pkg");
        assert_eq!(unpub_pkg.version.major, 0);
        assert_eq!(unpub_pkg.version.minor, 1);
        assert_eq!(unpub_pkg.version.patch, 0);

        // registry-restricted-pkg: version = "3.0.0-alpha.1"
        let registry_pkg = find_pkg("registry-restricted-pkg");
        assert_eq!(registry_pkg.version.major, 3);
        assert_eq!(registry_pkg.version.minor, 0);
        assert_eq!(registry_pkg.version.patch, 0);
        assert!(!registry_pkg.version.pre.is_empty());
    }

    #[test]
    fn test_temp_workspace_with_custom_config() {
        let temp = create_temp_workspace(&[
            ("my-lib", "0.5.0", true, true),    // lib, publishable
            ("my-bin", "1.0.0", false, true),   // bin, publishable
            ("internal", "0.0.1", true, false), // lib, not publishable
        ]);

        let provider = CargoMetadataWorkspace;
        let metadata = provider
            .load(temp.path())
            .expect("failed to load workspace");

        assert_eq!(metadata.packages.len(), 3);

        let find_pkg = |name: &str| metadata.packages.iter().find(|p| p.name == name).unwrap();

        let my_lib = find_pkg("my-lib");
        assert!(my_lib.has_lib);
        assert!(my_lib.publishable);

        let my_bin = find_pkg("my-bin");
        assert!(!my_bin.has_lib);
        assert!(my_bin.publishable);

        let internal = find_pkg("internal");
        assert!(internal.has_lib);
        assert!(!internal.publishable);
    }

    #[test]
    fn test_error_on_invalid_workspace() {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        let provider = CargoMetadataWorkspace;

        // Empty directory without Cargo.toml should fail
        let result = provider.load(temp.path());
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(err_string.contains("cargo metadata failed"));
    }

    #[test]
    fn test_excludes_non_workspace_dependencies() {
        // When cargo metadata is run, it may include path dependencies that are not
        // workspace members. This test verifies we only return workspace members.
        let workspace = tempfile::tempdir().expect("temp workspace");
        let external = tempfile::tempdir().expect("external dep");

        let external_path = external.path().to_string_lossy().replace('\\', "/");
        fs::create_dir_all(external.path().join("src")).unwrap();
        fs::write(
            external.path().join("Cargo.toml"),
            r#"[package]
name = "external-dep"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .unwrap();
        fs::write(
            external.path().join("src").join("lib.rs"),
            "pub fn ext() {}",
        )
        .unwrap();

        fs::write(
            workspace.path().join("Cargo.toml"),
            r#"[workspace]
resolver = "2"
members = ["sole-member"]
"#,
        )
        .unwrap();

        let member_dir = workspace.path().join("sole-member");
        fs::create_dir_all(member_dir.join("src")).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            format!(
                r#"[package]
name = "sole-member"
version = "1.0.0"
edition = "2021"

[dependencies]
external-dep = {{ path = "{}" }}

[lib]
path = "src/lib.rs"
"#,
                external_path
            ),
        )
        .unwrap();
        fs::write(member_dir.join("src").join("lib.rs"), "pub fn lib() {}").unwrap();

        let provider = CargoMetadataWorkspace;
        let metadata = provider
            .load(workspace.path())
            .expect("failed to load workspace");

        // Should only have our workspace member, not the external dependency
        assert_eq!(metadata.packages.len(), 1);
        assert_eq!(metadata.packages[0].name, "sole-member");
    }
}
