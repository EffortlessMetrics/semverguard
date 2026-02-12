use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A simplified view of `cargo metadata` for semverguard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Workspace packages eligible for selection.
    pub packages: Vec<WorkspacePackage>,
}

/// A package inside the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePackage {
    /// Cargo package name.
    pub name: String,
    /// Cargo package version.
    pub version: Version,
    /// Path to the package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Directory containing the package's `Cargo.toml`.
    pub package_root: PathBuf,
    /// Whether the package is publishable (`publish != false`).
    pub publishable: bool,
    /// Whether the package has a library target.
    pub has_lib: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // WorkspacePackage tests
    // =========================================================================

    #[test]
    fn test_workspace_package_new() {
        let package = WorkspacePackage {
            name: "mylib".to_string(),
            version: Version::new(1, 2, 3),
            manifest_path: PathBuf::from("/workspace/crates/mylib/Cargo.toml"),
            package_root: PathBuf::from("/workspace/crates/mylib"),
            publishable: true,
            has_lib: true,
        };

        assert_eq!(package.name, "mylib");
        assert_eq!(package.version, Version::new(1, 2, 3));
        assert!(package.publishable);
        assert!(package.has_lib);
    }

    #[test]
    fn test_workspace_package_version_with_prerelease() {
        let package = WorkspacePackage {
            name: "mylib".to_string(),
            version: Version::parse("1.0.0-alpha.1").unwrap(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            package_root: PathBuf::from("/workspace"),
            publishable: true,
            has_lib: true,
        };

        assert_eq!(package.version.to_string(), "1.0.0-alpha.1");
        assert!(!package.version.pre.is_empty());
    }

    #[test]
    fn test_workspace_package_version_with_build_metadata() {
        let package = WorkspacePackage {
            name: "mylib".to_string(),
            version: Version::parse("1.0.0+build.123").unwrap(),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            package_root: PathBuf::from("/workspace"),
            publishable: true,
            has_lib: true,
        };

        assert_eq!(package.version.to_string(), "1.0.0+build.123");
        assert!(!package.version.build.is_empty());
    }

    #[test]
    fn test_workspace_package_not_publishable() {
        let package = WorkspacePackage {
            name: "internal-crate".to_string(),
            version: Version::new(0, 1, 0),
            manifest_path: PathBuf::from("/workspace/crates/internal/Cargo.toml"),
            package_root: PathBuf::from("/workspace/crates/internal"),
            publishable: false,
            has_lib: true,
        };

        assert!(!package.publishable);
    }

    #[test]
    fn test_workspace_package_no_lib() {
        let package = WorkspacePackage {
            name: "cli-tool".to_string(),
            version: Version::new(1, 0, 0),
            manifest_path: PathBuf::from("/workspace/crates/cli/Cargo.toml"),
            package_root: PathBuf::from("/workspace/crates/cli"),
            publishable: true,
            has_lib: false,
        };

        assert!(!package.has_lib);
    }

    #[test]
    fn test_workspace_package_clone() {
        let original = WorkspacePackage {
            name: "mylib".to_string(),
            version: Version::new(1, 0, 0),
            manifest_path: PathBuf::from("/workspace/Cargo.toml"),
            package_root: PathBuf::from("/workspace"),
            publishable: true,
            has_lib: true,
        };

        let cloned = original.clone();
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.version, original.version);
        assert_eq!(cloned.manifest_path, original.manifest_path);
    }

    #[test]
    fn test_workspace_package_json_roundtrip() {
        let original = WorkspacePackage {
            name: "mylib".to_string(),
            version: Version::new(2, 1, 0),
            manifest_path: PathBuf::from("/workspace/crates/mylib/Cargo.toml"),
            package_root: PathBuf::from("/workspace/crates/mylib"),
            publishable: true,
            has_lib: true,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: WorkspacePackage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, original.name);
        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.manifest_path, original.manifest_path);
        assert_eq!(deserialized.package_root, original.package_root);
        assert_eq!(deserialized.publishable, original.publishable);
        assert_eq!(deserialized.has_lib, original.has_lib);
    }

    #[test]
    fn test_workspace_package_json_version_serialization() {
        let package = WorkspacePackage {
            name: "test".to_string(),
            version: Version::new(1, 2, 3),
            manifest_path: PathBuf::from("/Cargo.toml"),
            package_root: PathBuf::from("/"),
            publishable: true,
            has_lib: true,
        };

        let json = serde_json::to_string(&package).unwrap();
        // Version should serialize as a string
        assert!(json.contains("\"1.2.3\""));
    }

    // =========================================================================
    // WorkspaceMetadata tests
    // =========================================================================

    #[test]
    fn test_workspace_metadata_new() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/my/workspace"),
            packages: vec![],
        };

        assert_eq!(metadata.workspace_root, PathBuf::from("/my/workspace"));
        assert!(metadata.packages.is_empty());
    }

    #[test]
    fn test_workspace_metadata_with_packages() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                WorkspacePackage {
                    name: "lib-a".to_string(),
                    version: Version::new(1, 0, 0),
                    manifest_path: PathBuf::from("/workspace/crates/a/Cargo.toml"),
                    package_root: PathBuf::from("/workspace/crates/a"),
                    publishable: true,
                    has_lib: true,
                },
                WorkspacePackage {
                    name: "lib-b".to_string(),
                    version: Version::new(2, 0, 0),
                    manifest_path: PathBuf::from("/workspace/crates/b/Cargo.toml"),
                    package_root: PathBuf::from("/workspace/crates/b"),
                    publishable: true,
                    has_lib: true,
                },
            ],
        };

        assert_eq!(metadata.packages.len(), 2);
        assert_eq!(metadata.packages[0].name, "lib-a");
        assert_eq!(metadata.packages[1].name, "lib-b");
    }

    #[test]
    fn test_workspace_metadata_clone() {
        let original = WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![WorkspacePackage {
                name: "mylib".to_string(),
                version: Version::new(1, 0, 0),
                manifest_path: PathBuf::from("/workspace/Cargo.toml"),
                package_root: PathBuf::from("/workspace"),
                publishable: true,
                has_lib: true,
            }],
        };

        let cloned = original.clone();
        assert_eq!(cloned.workspace_root, original.workspace_root);
        assert_eq!(cloned.packages.len(), 1);
        assert_eq!(cloned.packages[0].name, "mylib");
    }

    #[test]
    fn test_workspace_metadata_json_roundtrip() {
        let original = WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![
                WorkspacePackage {
                    name: "lib-a".to_string(),
                    version: Version::new(1, 0, 0),
                    manifest_path: PathBuf::from("/workspace/crates/a/Cargo.toml"),
                    package_root: PathBuf::from("/workspace/crates/a"),
                    publishable: true,
                    has_lib: true,
                },
                WorkspacePackage {
                    name: "lib-b".to_string(),
                    version: Version::parse("0.1.0-beta").unwrap(),
                    manifest_path: PathBuf::from("/workspace/crates/b/Cargo.toml"),
                    package_root: PathBuf::from("/workspace/crates/b"),
                    publishable: false,
                    has_lib: false,
                },
            ],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: WorkspaceMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.workspace_root, original.workspace_root);
        assert_eq!(deserialized.packages.len(), 2);
        assert_eq!(deserialized.packages[0].name, "lib-a");
        assert_eq!(deserialized.packages[0].version, Version::new(1, 0, 0));
        assert_eq!(deserialized.packages[1].name, "lib-b");
        assert_eq!(
            deserialized.packages[1].version,
            Version::parse("0.1.0-beta").unwrap()
        );
        assert!(!deserialized.packages[1].publishable);
        assert!(!deserialized.packages[1].has_lib);
    }

    #[test]
    fn test_workspace_metadata_empty_packages_json() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: WorkspaceMetadata = serde_json::from_str(&json).unwrap();

        assert!(deserialized.packages.is_empty());
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_workspace_package_empty_name() {
        let package = WorkspacePackage {
            name: String::new(),
            version: Version::new(0, 0, 0),
            manifest_path: PathBuf::new(),
            package_root: PathBuf::new(),
            publishable: false,
            has_lib: false,
        };

        let json = serde_json::to_string(&package).unwrap();
        let deserialized: WorkspacePackage = serde_json::from_str(&json).unwrap();

        assert!(deserialized.name.is_empty());
        assert_eq!(deserialized.version, Version::new(0, 0, 0));
    }

    #[test]
    fn test_workspace_metadata_path_with_spaces() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/my workspace/with spaces/project"),
            packages: vec![WorkspacePackage {
                name: "test-package".to_string(),
                version: Version::new(1, 0, 0),
                manifest_path: PathBuf::from("/my workspace/with spaces/project/Cargo.toml"),
                package_root: PathBuf::from("/my workspace/with spaces/project"),
                publishable: true,
                has_lib: true,
            }],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: WorkspaceMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.workspace_root,
            PathBuf::from("/my workspace/with spaces/project")
        );
    }

    #[test]
    fn test_workspace_package_complex_version() {
        let package = WorkspacePackage {
            name: "complex-version".to_string(),
            version: Version::parse("1.0.0-alpha.1+build.12345").unwrap(),
            manifest_path: PathBuf::from("/Cargo.toml"),
            package_root: PathBuf::from("/"),
            publishable: true,
            has_lib: true,
        };

        let json = serde_json::to_string(&package).unwrap();
        let deserialized: WorkspacePackage = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.version,
            Version::parse("1.0.0-alpha.1+build.12345").unwrap()
        );
    }

    #[test]
    fn test_workspace_metadata_pretty_json() {
        let metadata = WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages: vec![],
        };

        let pretty_json = serde_json::to_string_pretty(&metadata).unwrap();
        // Pretty JSON should contain newlines
        assert!(pretty_json.contains('\n'));
    }
}
