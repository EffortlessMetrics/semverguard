//! Integration tests for the CargoMetadataWorkspace adapter.

use semverguard_domain::WorkspaceProvider;
use semverguard_workspace::CargoMetadataWorkspace;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to get the path to the existing test fixtures.
fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// =============================================================================
// Tests using existing test_workspace fixture
// =============================================================================

mod existing_fixture_tests {
    use super::*;

    /// Get the path to the pre-existing test workspace fixture.
    fn test_workspace_path() -> PathBuf {
        fixtures_path().join("test_workspace")
    }

    #[test]
    fn load_valid_cargo_workspace_returns_correct_metadata() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // Verify workspace_root is populated
        assert!(
            metadata.workspace_root.ends_with("test_workspace"),
            "workspace_root should end with test_workspace, got: {:?}",
            metadata.workspace_root
        );

        // The fixture has 4 packages
        assert_eq!(
            metadata.packages.len(),
            4,
            "expected 4 packages, got: {:?}",
            metadata
                .packages
                .iter()
                .map(|p| &p.name)
                .collect::<Vec<_>>()
        );

        // Check package names are present
        let names: Vec<&str> = metadata.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"lib-pkg"), "should contain lib-pkg");
        assert!(names.contains(&"bin-pkg"), "should contain bin-pkg");
        assert!(
            names.contains(&"unpublishable-pkg"),
            "should contain unpublishable-pkg"
        );
        assert!(
            names.contains(&"registry-restricted-pkg"),
            "should contain registry-restricted-pkg"
        );
    }

    #[test]
    fn publishable_field_detection_publish_unset() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // lib-pkg has publish unset (defaults to publishable)
        let lib_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "lib-pkg")
            .expect("lib-pkg should exist");

        assert!(
            lib_pkg.publishable,
            "lib-pkg should be publishable (publish unset)"
        );
    }

    #[test]
    fn publishable_field_detection_publish_false() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // unpublishable-pkg has publish = false
        let unpub_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "unpublishable-pkg")
            .expect("unpublishable-pkg should exist");

        assert!(
            !unpub_pkg.publishable,
            "unpublishable-pkg should NOT be publishable (publish = false)"
        );
    }

    #[test]
    fn publishable_field_detection_publish_with_registry() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // registry-restricted-pkg has publish = ["my-private-registry"] (publishable to specific registries)
        let restricted_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "registry-restricted-pkg")
            .expect("registry-restricted-pkg should exist");

        assert!(
            restricted_pkg.publishable,
            "registry-restricted-pkg should be publishable (publish = [\"my-private-registry\"])"
        );
    }

    #[test]
    fn has_lib_field_detection_with_lib_target() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // lib-pkg has a lib target
        let lib_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "lib-pkg")
            .expect("lib-pkg should exist");

        assert!(lib_pkg.has_lib, "lib-pkg should have lib target");
    }

    #[test]
    fn has_lib_field_detection_without_lib_target() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // bin-pkg has only a binary target
        let bin_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "bin-pkg")
            .expect("bin-pkg should exist");

        assert!(!bin_pkg.has_lib, "bin-pkg should NOT have lib target");
    }

    #[test]
    fn package_paths_are_correct() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // Check lib-pkg paths
        let lib_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "lib-pkg")
            .expect("lib-pkg should exist");

        assert!(
            lib_pkg.manifest_path.ends_with("Cargo.toml"),
            "manifest_path should end with Cargo.toml, got: {:?}",
            lib_pkg.manifest_path
        );
        assert!(
            lib_pkg.manifest_path.to_string_lossy().contains("lib-pkg"),
            "manifest_path should contain lib-pkg, got: {:?}",
            lib_pkg.manifest_path
        );

        // package_root should be the parent of manifest_path
        assert_eq!(
            lib_pkg.package_root,
            lib_pkg
                .manifest_path
                .parent()
                .expect("manifest should have parent")
        );
    }

    #[test]
    fn package_versions_are_correct() {
        let provider = CargoMetadataWorkspace;
        let workspace_root = test_workspace_path();

        let metadata = provider
            .load(&workspace_root)
            .expect("should load workspace");

        // lib-pkg version = 1.0.0
        let lib_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "lib-pkg")
            .expect("lib-pkg should exist");
        assert_eq!(lib_pkg.version.to_string(), "1.0.0");

        // bin-pkg version = 2.3.4
        let bin_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "bin-pkg")
            .expect("bin-pkg should exist");
        assert_eq!(bin_pkg.version.to_string(), "2.3.4");

        // unpublishable-pkg version = 0.1.0
        let unpub_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "unpublishable-pkg")
            .expect("unpublishable-pkg should exist");
        assert_eq!(unpub_pkg.version.to_string(), "0.1.0");

        // registry-restricted-pkg version = 3.0.0-alpha.1 (prerelease)
        let restricted_pkg = metadata
            .packages
            .iter()
            .find(|p| p.name == "registry-restricted-pkg")
            .expect("registry-restricted-pkg should exist");
        assert_eq!(restricted_pkg.version.to_string(), "3.0.0-alpha.1");
        assert!(!restricted_pkg.version.pre.is_empty());
    }
}

// =============================================================================
// Tests using tempfile-based fixtures
// =============================================================================

mod tempfile_tests {
    use super::*;

    /// Creates a minimal valid single-package project (non-workspace).
    fn create_single_package_project(dir: &TempDir) -> PathBuf {
        let root = dir.path().to_path_buf();

        // Write Cargo.toml
        let cargo_toml = r#"[package]
name = "single-pkg"
version = "0.5.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#;
        fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

        // Create src directory and lib.rs
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(root.join("src/lib.rs"), "//! Single package lib\n").expect("write lib.rs");

        root
    }

    /// Creates a minimal workspace with two packages.
    fn create_minimal_workspace(dir: &TempDir) -> PathBuf {
        let root = dir.path().to_path_buf();

        // Write workspace Cargo.toml
        let workspace_toml = r#"[workspace]
resolver = "2"
members = ["crates/pkg-a", "crates/pkg-b"]
"#;
        fs::write(root.join("Cargo.toml"), workspace_toml).expect("write workspace Cargo.toml");

        // Create pkg-a (library)
        let pkg_a_dir = root.join("crates/pkg-a");
        fs::create_dir_all(&pkg_a_dir).expect("create pkg-a dir");
        fs::create_dir_all(pkg_a_dir.join("src")).expect("create pkg-a/src dir");
        let pkg_a_toml = r#"[package]
name = "pkg-a"
version = "1.2.3"
edition = "2021"

[lib]
path = "src/lib.rs"
"#;
        fs::write(pkg_a_dir.join("Cargo.toml"), pkg_a_toml).expect("write pkg-a Cargo.toml");
        fs::write(pkg_a_dir.join("src/lib.rs"), "//! pkg-a\n").expect("write pkg-a lib.rs");

        // Create pkg-b (binary)
        let pkg_b_dir = root.join("crates/pkg-b");
        fs::create_dir_all(&pkg_b_dir).expect("create pkg-b dir");
        fs::create_dir_all(pkg_b_dir.join("src")).expect("create pkg-b/src dir");
        let pkg_b_toml = r#"[package]
name = "pkg-b"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "pkg-b"
path = "src/main.rs"
"#;
        fs::write(pkg_b_dir.join("Cargo.toml"), pkg_b_toml).expect("write pkg-b Cargo.toml");
        fs::write(pkg_b_dir.join("src/main.rs"), "fn main() {}\n").expect("write pkg-b main.rs");

        root
    }

    #[test]
    fn load_single_package_project_as_workspace_of_one() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = create_single_package_project(&temp_dir);

        let provider = CargoMetadataWorkspace;
        let metadata = provider
            .load(&root)
            .expect("should load single-package project");

        // Should have exactly one package
        assert_eq!(
            metadata.packages.len(),
            1,
            "single-package project should have 1 package"
        );

        let pkg = &metadata.packages[0];
        assert_eq!(pkg.name, "single-pkg");
        assert_eq!(pkg.version.to_string(), "0.5.0");
        assert!(
            pkg.publishable,
            "single-pkg should be publishable by default"
        );
        assert!(pkg.has_lib, "single-pkg should have lib target");

        // workspace_root should equal the package root for single-package projects
        assert_eq!(
            metadata.workspace_root, pkg.package_root,
            "workspace_root should equal package_root for single-package project"
        );
    }

    #[test]
    fn load_minimal_workspace_returns_all_packages() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = create_minimal_workspace(&temp_dir);

        let provider = CargoMetadataWorkspace;
        let metadata = provider.load(&root).expect("should load workspace");

        assert_eq!(
            metadata.packages.len(),
            2,
            "workspace should have 2 packages"
        );

        let names: Vec<&str> = metadata.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"pkg-a"));
        assert!(names.contains(&"pkg-b"));

        // Verify pkg-a (library)
        let pkg_a = metadata
            .packages
            .iter()
            .find(|p| p.name == "pkg-a")
            .expect("pkg-a should exist");
        assert_eq!(pkg_a.version.to_string(), "1.2.3");
        assert!(pkg_a.has_lib);

        // Verify pkg-b (binary only)
        let pkg_b = metadata
            .packages
            .iter()
            .find(|p| p.name == "pkg-b")
            .expect("pkg-b should exist");
        assert_eq!(pkg_b.version.to_string(), "0.1.0");
        assert!(!pkg_b.has_lib);
    }

    #[test]
    fn load_workspace_with_mixed_publish_settings() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path().to_path_buf();

        // Write workspace Cargo.toml
        let workspace_toml = r#"[workspace]
resolver = "2"
members = ["crates/public", "crates/private", "crates/restricted"]
"#;
        fs::write(root.join("Cargo.toml"), workspace_toml).expect("write workspace Cargo.toml");

        // Create public package (publish unset = publishable)
        let public_dir = root.join("crates/public");
        fs::create_dir_all(public_dir.join("src")).expect("create public dir");
        let public_toml = r#"[package]
name = "public-crate"
version = "1.0.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#;
        fs::write(public_dir.join("Cargo.toml"), public_toml).expect("write public Cargo.toml");
        fs::write(public_dir.join("src/lib.rs"), "//! public\n").expect("write public lib.rs");

        // Create private package (publish = false)
        let private_dir = root.join("crates/private");
        fs::create_dir_all(private_dir.join("src")).expect("create private dir");
        let private_toml = r#"[package]
name = "private-crate"
version = "0.0.1"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"
"#;
        fs::write(private_dir.join("Cargo.toml"), private_toml).expect("write private Cargo.toml");
        fs::write(private_dir.join("src/lib.rs"), "//! private\n").expect("write private lib.rs");

        // Create restricted package (publish = ["custom-registry"])
        let restricted_dir = root.join("crates/restricted");
        fs::create_dir_all(restricted_dir.join("src")).expect("create restricted dir");
        let restricted_toml = r#"[package]
name = "restricted-crate"
version = "2.0.0"
edition = "2021"
publish = ["custom-registry"]

[lib]
path = "src/lib.rs"
"#;
        fs::write(restricted_dir.join("Cargo.toml"), restricted_toml)
            .expect("write restricted Cargo.toml");
        fs::write(restricted_dir.join("src/lib.rs"), "//! restricted\n")
            .expect("write restricted lib.rs");

        let provider = CargoMetadataWorkspace;
        let metadata = provider.load(&root).expect("should load workspace");

        assert_eq!(metadata.packages.len(), 3);

        let public = metadata
            .packages
            .iter()
            .find(|p| p.name == "public-crate")
            .expect("public-crate should exist");
        assert!(public.publishable, "public-crate should be publishable");

        let private = metadata
            .packages
            .iter()
            .find(|p| p.name == "private-crate")
            .expect("private-crate should exist");
        assert!(
            !private.publishable,
            "private-crate should NOT be publishable"
        );

        let restricted = metadata
            .packages
            .iter()
            .find(|p| p.name == "restricted-crate")
            .expect("restricted-crate should exist");
        assert!(
            restricted.publishable,
            "restricted-crate should be publishable (to specific registry)"
        );
    }
}

// =============================================================================
// Error case tests
// =============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn load_missing_cargo_toml_returns_error() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let empty_dir = temp_dir.path();

        let provider = CargoMetadataWorkspace;
        let result = provider.load(empty_dir);

        assert!(
            result.is_err(),
            "loading directory without Cargo.toml should fail"
        );

        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(
            err_str.contains("cargo metadata failed"),
            "error should mention cargo metadata failed, got: {err_str}"
        );
    }

    #[test]
    fn load_invalid_cargo_toml_returns_error() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path();

        // Write invalid Cargo.toml (not valid TOML)
        fs::write(root.join("Cargo.toml"), "this is not valid toml {{{")
            .expect("write invalid Cargo.toml");

        let provider = CargoMetadataWorkspace;
        let result = provider.load(root);

        assert!(result.is_err(), "loading invalid Cargo.toml should fail");

        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(
            err_str.contains("cargo metadata failed"),
            "error should mention cargo metadata failed, got: {err_str}"
        );
    }

    #[test]
    fn load_incomplete_cargo_toml_returns_error() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path();

        // Write incomplete Cargo.toml (valid TOML but not a valid Cargo manifest)
        let incomplete_toml = r#"
[package]
name = "incomplete"
# Missing version field
edition = "2021"
"#;
        fs::write(root.join("Cargo.toml"), incomplete_toml).expect("write incomplete Cargo.toml");

        let provider = CargoMetadataWorkspace;
        let result = provider.load(root);

        assert!(result.is_err(), "loading incomplete Cargo.toml should fail");

        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(
            err_str.contains("cargo metadata failed"),
            "error should mention cargo metadata failed, got: {err_str}"
        );
    }

    #[test]
    fn load_nonexistent_directory_returns_error() {
        let provider = CargoMetadataWorkspace;
        let nonexistent = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let result = provider.load(&nonexistent);

        assert!(result.is_err(), "loading nonexistent directory should fail");

        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(
            err_str.contains("cargo metadata failed"),
            "error should mention cargo metadata failed, got: {err_str}"
        );
    }

    #[test]
    fn load_workspace_with_missing_member_returns_error() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path();

        // Write workspace Cargo.toml referencing nonexistent member
        let workspace_toml = r#"[workspace]
resolver = "2"
members = ["crates/nonexistent-member"]
"#;
        fs::write(root.join("Cargo.toml"), workspace_toml).expect("write workspace Cargo.toml");

        let provider = CargoMetadataWorkspace;
        let result = provider.load(root);

        assert!(
            result.is_err(),
            "loading workspace with missing member should fail"
        );

        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(
            err_str.contains("cargo metadata failed"),
            "error should mention cargo metadata failed, got: {err_str}"
        );
    }
}

// =============================================================================
// Additional package field tests
// =============================================================================

mod package_field_tests {
    use super::*;

    #[test]
    fn package_with_both_lib_and_bin_targets() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path().to_path_buf();

        // Create a package with both lib and bin targets
        let cargo_toml = r#"[package]
name = "mixed-targets"
version = "1.0.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[[bin]]
name = "mixed-targets"
path = "src/main.rs"
"#;
        fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(root.join("src/lib.rs"), "//! lib\n").expect("write lib.rs");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");

        let provider = CargoMetadataWorkspace;
        let metadata = provider.load(&root).expect("should load project");

        let pkg = &metadata.packages[0];
        assert_eq!(pkg.name, "mixed-targets");
        assert!(
            pkg.has_lib,
            "package with lib target should have has_lib = true"
        );
    }

    #[test]
    fn package_with_proc_macro_has_lib() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path().to_path_buf();

        // Create a proc-macro package
        let cargo_toml = r#"[package]
name = "my-macro"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;
        fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(
            root.join("src/lib.rs"),
            "//! proc-macro crate\nextern crate proc_macro;\n",
        )
        .expect("write lib.rs");

        let provider = CargoMetadataWorkspace;
        let metadata = provider.load(&root).expect("should load project");

        let pkg = &metadata.packages[0];
        assert_eq!(pkg.name, "my-macro");
        // proc-macro crates should have has_lib = true (they produce a lib target)
        assert!(pkg.has_lib, "proc-macro package should have has_lib = true");
    }

    #[test]
    fn package_version_with_build_metadata() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path().to_path_buf();

        let cargo_toml = r#"[package]
name = "build-meta"
version = "1.0.0+git.abc123"
edition = "2021"

[lib]
path = "src/lib.rs"
"#;
        fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(root.join("src/lib.rs"), "//! lib\n").expect("write lib.rs");

        let provider = CargoMetadataWorkspace;
        let metadata = provider.load(&root).expect("should load project");

        let pkg = &metadata.packages[0];
        assert_eq!(pkg.version.to_string(), "1.0.0+git.abc123");
        assert!(!pkg.version.build.is_empty());
    }
}
