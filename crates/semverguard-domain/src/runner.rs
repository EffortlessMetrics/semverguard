use crate::error::{Result, SemverguardError};
use crate::ports::{GitProvider, SemverEngine, WorkspaceProvider};
use crate::progress::{NoopProgressCallback, ProgressCallback, ProgressEvent};
use globset::{Glob, GlobSet, GlobSetBuilder};
use semverguard_types::{
    ListResult, ListedPackage, PackageReport, PackageStatus, SemverCheckRequest, SkippedPackage,
    Summary, WorkspaceMetadata, WorkspacePackage,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// The output of a semverguard run (without timestamps/version decoration).
#[derive(Debug)]
pub struct RunArtifacts {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Per-package results (including skipped).
    pub packages: Vec<PackageReport>,
    /// Summary counts.
    pub summary: Summary,
}

/// A runner wires ports together and produces a report.
pub struct SemverguardRunner<'a> {
    workspace: &'a dyn WorkspaceProvider,
    git: Option<&'a dyn GitProvider>,
    engine: &'a dyn SemverEngine,
    progress: Arc<dyn ProgressCallback>,
}

impl<'a> SemverguardRunner<'a> {
    /// Create a new runner.
    pub fn new(
        workspace: &'a dyn WorkspaceProvider,
        git: Option<&'a dyn GitProvider>,
        engine: &'a dyn SemverEngine,
    ) -> Self {
        Self {
            workspace,
            git,
            engine,
            progress: Arc::new(NoopProgressCallback),
        }
    }

    /// Create a new runner with a progress callback.
    pub fn with_progress<P: ProgressCallback + 'static>(
        workspace: &'a dyn WorkspaceProvider,
        git: Option<&'a dyn GitProvider>,
        engine: &'a dyn SemverEngine,
        progress: P,
    ) -> Self {
        Self {
            workspace,
            git,
            engine,
            progress: Arc::new(progress),
        }
    }

    /// Set the progress callback.
    pub fn set_progress<P: ProgressCallback + 'static>(&mut self, progress: P) {
        self.progress = Arc::new(progress);
    }

    /// Run semver checks.
    pub fn run(
        &self,
        workspace_root: &Path,
        config: &semverguard_types::SemverguardConfig,
    ) -> Result<RunArtifacts> {
        let metadata = self.workspace.load(workspace_root)?;

        let mut reports: Vec<PackageReport> = Vec::new();

        // First pass: apply static filters (publishability, has-lib, name patterns).
        // If explicit_packages is non-empty, it takes precedence over include/exclude globs.
        let mut eligible: Vec<WorkspacePackage> = Vec::new();

        if !config.scope.explicit_packages.is_empty() {
            // Explicit package selection mode
            let requested: HashSet<&str> = config
                .scope
                .explicit_packages
                .iter()
                .map(|s| s.as_str())
                .collect();
            let available: HashSet<&str> =
                metadata.packages.iter().map(|p| p.name.as_str()).collect();

            // Warn about packages that don't exist in the workspace
            let mut missing: Vec<&str> = Vec::new();
            for name in &requested {
                if !available.contains(name) {
                    eprintln!(
                        "warning: package '{}' not found in workspace (available: {:?})",
                        name,
                        available.iter().take(5).collect::<Vec<_>>()
                    );
                    missing.push(name);
                }
            }

            // Error if ALL specified packages are missing
            if missing.len() == requested.len() {
                return Err(SemverguardError::InvalidConfig(format!(
                    "none of the specified packages exist in workspace: {:?}",
                    config.scope.explicit_packages
                )));
            }

            for pkg in &metadata.packages {
                if !requested.contains(pkg.name.as_str()) {
                    // Not explicitly requested; skip silently (don't add to report)
                    continue;
                }

                // Still apply publishability and has-lib filters unless forced
                if config.scope.skip_publish_false && !pkg.publishable {
                    reports.push(skipped(pkg, "publish = false"));
                    continue;
                }
                if config.scope.skip_no_lib && !pkg.has_lib {
                    reports.push(skipped(pkg, "no library target"));
                    continue;
                }
                eligible.push(pkg.clone());
            }
        } else {
            // Standard glob-based filtering
            let include_set = build_globset(&config.scope.include)?;
            let exclude_set = build_globset(&config.scope.exclude)?;

            for pkg in &metadata.packages {
                if !matches_name_filters(pkg, &include_set, &exclude_set) {
                    reports.push(skipped(pkg, "filtered by include/exclude patterns"));
                    continue;
                }
                if config.scope.skip_publish_false && !pkg.publishable {
                    reports.push(skipped(pkg, "publish = false"));
                    continue;
                }
                if config.scope.skip_no_lib && !pkg.has_lib {
                    reports.push(skipped(pkg, "no library target"));
                    continue;
                }
                eligible.push(pkg.clone());
            }
        }

        // Second pass: scope selection (changed vs workspace).
        let eligible = match config.scope.mode {
            semverguard_types::ScopeMode::Workspace => eligible,
            semverguard_types::ScopeMode::Changed => {
                let baseline_rev = config.baseline.rev.as_deref().ok_or_else(|| {
                    SemverguardError::InvalidConfig(
                        "scope.mode=changed requires baseline.rev".into(),
                    )
                })?;

                if !matches!(config.baseline.kind, semverguard_types::BaselineKind::Git) {
                    return Err(SemverguardError::InvalidConfig(
                        "scope.mode=changed requires baseline.kind = \"git\"".into(),
                    ));
                }

                let git = self.git.ok_or_else(|| {
                    SemverguardError::InvalidConfig(
                        "scope.mode=changed requires a GitProvider".into(),
                    )
                })?;

                let changed =
                    changed_packages(&metadata, git, workspace_root, baseline_rev, "HEAD")?;

                let mut scoped = Vec::new();
                for pkg in eligible {
                    if changed.contains(&pkg.name) {
                        scoped.push(pkg);
                    } else {
                        // Not changed, record as skipped.
                        reports.push(skipped(
                            &pkg,
                            &format!("unchanged relative to {baseline_rev}"),
                        ));
                    }
                }
                scoped
            }
        };

        // Third pass: execute engine for each remaining eligible package.
        let total = eligible.len();
        self.progress
            .on_progress(ProgressEvent::TotalPackages { total });

        for (index, pkg) in eligible.into_iter().enumerate() {
            self.progress.on_progress(ProgressEvent::PackageStarted {
                name: pkg.name.clone(),
                index,
                total,
            });

            let t0 = Instant::now();

            let request = SemverCheckRequest {
                workspace_root: metadata.workspace_root.clone(),
                cargo_bin: config.engine.cargo_bin.clone(),
                manifest_path: pkg.manifest_path.clone(),
                baseline: config.baseline.clone(),
                features: config.features.clone(),
                extra_args: config.engine.extra_args.clone(),
                timeout: None, // keep simple; add later if you need it
            };

            let result = self.engine.check(request);
            let duration_ms = t0.elapsed().as_millis();

            match result {
                Ok((command, output)) => {
                    let success = output.success;
                    let status = if success {
                        PackageStatus::Passed
                    } else {
                        PackageStatus::Failed
                    };

                    self.progress.on_progress(ProgressEvent::PackageCompleted {
                        name: pkg.name.clone(),
                        status: status.clone(),
                        duration_ms,
                    });

                    reports.push(PackageReport {
                        name: pkg.name.clone(),
                        version: pkg.version.to_string(),
                        manifest_path: pkg.manifest_path.clone(),
                        status,
                        skip_reason: None,
                        duration_ms,
                        command,
                        inferred_required_bump: output.required_bump,
                        engine: Some(output),
                    });

                    if !success && config.engine.fail_fast {
                        break;
                    }
                }
                Err(e) => {
                    self.progress.on_progress(ProgressEvent::PackageCompleted {
                        name: pkg.name.clone(),
                        status: PackageStatus::Failed,
                        duration_ms,
                    });

                    reports.push(PackageReport {
                        name: pkg.name.clone(),
                        version: pkg.version.to_string(),
                        manifest_path: pkg.manifest_path.clone(),
                        status: PackageStatus::Failed,
                        skip_reason: Some(format!("engine invocation failed: {e}")),
                        duration_ms,
                        command: vec![],
                        inferred_required_bump: None,
                        engine: None,
                    });

                    if config.engine.fail_fast {
                        break;
                    }
                }
            }
        }

        let summary = summarize(&reports);

        self.progress.on_progress(ProgressEvent::Finished {
            passed: summary.passed,
            failed: summary.failed,
            skipped: summary.skipped,
        });

        Ok(RunArtifacts {
            workspace_root: metadata.workspace_root,
            packages: reports,
            summary,
        })
    }

    /// List packages that would be checked without actually running checks.
    ///
    /// This performs the same filtering logic as `run()` (passes 1 and 2) but
    /// does NOT invoke the engine. Useful for previewing what would be checked.
    pub fn list_packages(
        &self,
        workspace_root: &Path,
        config: &semverguard_types::SemverguardConfig,
    ) -> Result<ListResult> {
        let metadata = self.workspace.load(workspace_root)?;
        let include_set = build_globset(&config.scope.include)?;
        let exclude_set = build_globset(&config.scope.exclude)?;

        let mut would_skip: Vec<SkippedPackage> = Vec::new();

        // First pass: apply static filters (publishability, has-lib, name patterns).
        let mut eligible: Vec<WorkspacePackage> = Vec::new();
        for pkg in &metadata.packages {
            if !matches_name_filters(pkg, &include_set, &exclude_set) {
                would_skip.push(SkippedPackage {
                    name: pkg.name.clone(),
                    version: pkg.version.to_string(),
                    manifest_path: pkg.manifest_path.clone(),
                    reason: "filtered by include/exclude".to_string(),
                });
                continue;
            }
            if config.scope.skip_publish_false && !pkg.publishable {
                would_skip.push(SkippedPackage {
                    name: pkg.name.clone(),
                    version: pkg.version.to_string(),
                    manifest_path: pkg.manifest_path.clone(),
                    reason: "publish = false".to_string(),
                });
                continue;
            }
            if config.scope.skip_no_lib && !pkg.has_lib {
                would_skip.push(SkippedPackage {
                    name: pkg.name.clone(),
                    version: pkg.version.to_string(),
                    manifest_path: pkg.manifest_path.clone(),
                    reason: "no library target".to_string(),
                });
                continue;
            }
            eligible.push(pkg.clone());
        }

        // Second pass: scope selection (changed vs workspace).
        let eligible = match config.scope.mode {
            semverguard_types::ScopeMode::Workspace => eligible,
            semverguard_types::ScopeMode::Changed => {
                let baseline_rev = config.baseline.rev.as_deref().ok_or_else(|| {
                    SemverguardError::InvalidConfig(
                        "scope.mode=changed requires baseline.rev".into(),
                    )
                })?;

                if !matches!(config.baseline.kind, semverguard_types::BaselineKind::Git) {
                    return Err(SemverguardError::InvalidConfig(
                        "scope.mode=changed requires baseline.kind = \"git\"".into(),
                    ));
                }

                let git = self.git.ok_or_else(|| {
                    SemverguardError::InvalidConfig(
                        "scope.mode=changed requires a GitProvider".into(),
                    )
                })?;

                let changed =
                    changed_packages(&metadata, git, workspace_root, baseline_rev, "HEAD")?;

                let mut scoped = Vec::new();
                for pkg in eligible {
                    if changed.contains(&pkg.name) {
                        scoped.push(pkg);
                    } else {
                        // Not changed, record as skipped.
                        would_skip.push(SkippedPackage {
                            name: pkg.name.clone(),
                            version: pkg.version.to_string(),
                            manifest_path: pkg.manifest_path.clone(),
                            reason: format!("unchanged relative to {baseline_rev}"),
                        });
                    }
                }
                scoped
            }
        };

        // Convert eligible packages to ListedPackage.
        let would_check: Vec<ListedPackage> = eligible
            .into_iter()
            .map(|pkg| ListedPackage {
                name: pkg.name,
                version: pkg.version.to_string(),
                manifest_path: pkg.manifest_path,
            })
            .collect();

        Ok(ListResult {
            workspace_root: metadata.workspace_root,
            would_check,
            would_skip,
        })
    }
}

fn build_globset(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p)
            .map_err(|e| SemverguardError::InvalidConfig(format!("bad glob '{p}': {e}")))?;
        builder.add(glob);
    }
    let set = builder
        .build()
        .map_err(|e| SemverguardError::InvalidConfig(format!("globset build error: {e}")))?;
    Ok(Some(set))
}

fn matches_name_filters(
    pkg: &WorkspacePackage,
    include: &Option<GlobSet>,
    exclude: &Option<GlobSet>,
) -> bool {
    let included = match include {
        None => true,
        Some(s) => s.is_match(&pkg.name),
    };
    let excluded = match exclude {
        None => false,
        Some(s) => s.is_match(&pkg.name),
    };
    included && !excluded
}

fn skipped(pkg: &WorkspacePackage, reason: &str) -> PackageReport {
    PackageReport {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
        manifest_path: pkg.manifest_path.clone(),
        status: PackageStatus::Skipped,
        skip_reason: Some(reason.to_string()),
        duration_ms: 0,
        command: vec![],
        inferred_required_bump: None,
        engine: None,
    }
}

fn summarize(reports: &[PackageReport]) -> Summary {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for r in reports {
        match r.status {
            PackageStatus::Passed => passed += 1,
            PackageStatus::Failed => failed += 1,
            PackageStatus::Skipped => skipped += 1,
        }
    }
    Summary {
        total: reports.len(),
        passed,
        failed,
        skipped,
    }
}

fn changed_packages(
    metadata: &WorkspaceMetadata,
    git: &dyn GitProvider,
    workspace_root: &Path,
    base: &str,
    head: &str,
) -> Result<HashSet<String>> {
    let changed_paths = git.changed_paths(workspace_root, base, head)?;

    // Convert package roots to workspace-relative paths.
    let mut package_roots: Vec<(String, PathBuf)> = Vec::new();
    for p in &metadata.packages {
        let rel = path_relative_to(&p.package_root, &metadata.workspace_root)
            .unwrap_or_else(|| p.package_root.clone());
        package_roots.push((p.name.clone(), rel));
    }

    // If we see any changes outside all package roots (e.g. workspace config),
    // conservatively treat it as "everything changed".
    let mut all_changed = false;

    let mut changed_pkgs = HashSet::new();
    'paths: for ch in &changed_paths {
        let ch_norm = normalize_rel(ch);

        let mut matched_any = false;
        for (name, root) in &package_roots {
            if is_under(&ch_norm, root) {
                changed_pkgs.insert(name.clone());
                matched_any = true;
            }
        }
        if !matched_any {
            // Something changed at the workspace-level; be conservative.
            all_changed = true;
            break 'paths;
        }
    }

    if all_changed {
        Ok(metadata.packages.iter().map(|p| p.name.clone()).collect())
    } else {
        Ok(changed_pkgs)
    }
}

fn is_under(path: &Path, root: &Path) -> bool {
    // Fast-ish prefix check using components, avoids string hacks.
    let mut p_it = path.components();
    let mut r_it = root.components();
    loop {
        match (r_it.next(), p_it.next()) {
            (None, _) => return true,
            (Some(r), Some(p)) if r == p => continue,
            _ => return false,
        }
    }
}

fn normalize_rel(p: &Path) -> PathBuf {
    // Normalize "./foo" -> "foo", and strip any leading separators.
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            CurDir => {}
            ParentDir => out.push(".."),
            Normal(x) => out.push(x),
            _ => {}
        }
    }
    out
}

fn path_relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    path.strip_prefix(base).ok().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use semverguard_types::{
        BaselineConfig, BaselineKind, EngineConfig, FeaturesConfig, OutputConfig, RequiredBump,
        ScopeConfig, ScopeMode, SemverCheckOutput, SemverguardConfig, WorkspaceMetadata,
        WorkspacePackage,
    };
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // =========================================================================
    // Mock implementations
    // =========================================================================

    /// Mock workspace provider that returns controlled metadata.
    struct MockWorkspaceProvider {
        metadata: WorkspaceMetadata,
    }

    impl MockWorkspaceProvider {
        fn new(metadata: WorkspaceMetadata) -> Self {
            Self { metadata }
        }
    }

    impl WorkspaceProvider for MockWorkspaceProvider {
        fn load(&self, _workspace_root: &Path) -> Result<WorkspaceMetadata> {
            Ok(self.metadata.clone())
        }
    }

    /// Mock workspace provider that returns an error.
    struct FailingWorkspaceProvider {
        error_msg: String,
    }

    impl WorkspaceProvider for FailingWorkspaceProvider {
        fn load(&self, _workspace_root: &Path) -> Result<WorkspaceMetadata> {
            Err(SemverguardError::Workspace(self.error_msg.clone()))
        }
    }

    /// Mock git provider that returns controlled changed paths.
    struct MockGitProvider {
        changed_paths: Vec<PathBuf>,
    }

    impl MockGitProvider {
        fn new(changed_paths: Vec<PathBuf>) -> Self {
            Self { changed_paths }
        }
    }

    impl GitProvider for MockGitProvider {
        fn changed_paths(
            &self,
            _workspace_root: &Path,
            _base: &str,
            _head: &str,
        ) -> Result<Vec<PathBuf>> {
            Ok(self.changed_paths.clone())
        }
    }

    /// Mock git provider that returns an error.
    struct FailingGitProvider {
        error_msg: String,
    }

    impl GitProvider for FailingGitProvider {
        fn changed_paths(
            &self,
            _workspace_root: &Path,
            _base: &str,
            _head: &str,
        ) -> Result<Vec<PathBuf>> {
            Err(SemverguardError::Git(self.error_msg.clone()))
        }
    }

    /// Mock semver engine that returns controlled output.
    struct MockSemverEngine {
        /// Results to return for each call, in order.
        results: RefCell<Vec<Result<(Vec<String>, SemverCheckOutput)>>>,
        /// Number of times check was called.
        call_count: AtomicUsize,
    }

    impl MockSemverEngine {
        fn new(results: Vec<Result<(Vec<String>, SemverCheckOutput)>>) -> Self {
            Self {
                results: RefCell::new(results),
                call_count: AtomicUsize::new(0),
            }
        }

        fn success_result() -> Result<(Vec<String>, SemverCheckOutput)> {
            Ok((
                vec!["cargo".to_string(), "semver-checks".to_string()],
                SemverCheckOutput {
                    exit_code: Some(0),
                    success: true,
                    stdout: "No breaking changes detected".to_string(),
                    stderr: String::new(),
                    required_bump: None,
                },
            ))
        }

        fn failure_result() -> Result<(Vec<String>, SemverCheckOutput)> {
            Ok((
                vec!["cargo".to_string(), "semver-checks".to_string()],
                SemverCheckOutput {
                    exit_code: Some(1),
                    success: false,
                    stdout: String::new(),
                    stderr: "Major bump required".to_string(),
                    required_bump: Some(RequiredBump::Major),
                },
            ))
        }

        fn engine_error() -> Result<(Vec<String>, SemverCheckOutput)> {
            Err(SemverguardError::Engine(
                "cargo-semver-checks not found".to_string(),
            ))
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl SemverEngine for MockSemverEngine {
        fn check(&self, _request: SemverCheckRequest) -> Result<(Vec<String>, SemverCheckOutput)> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut results = self.results.borrow_mut();
            if results.is_empty() {
                // Default to success if no more results
                MockSemverEngine::success_result()
            } else {
                results.remove(0)
            }
        }
    }

    // =========================================================================
    // Helper functions
    // =========================================================================

    fn make_package(
        name: &str,
        version: &str,
        root: &str,
        publishable: bool,
        has_lib: bool,
    ) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_string(),
            version: Version::parse(version).unwrap(),
            manifest_path: PathBuf::from(format!("{root}/Cargo.toml")),
            package_root: PathBuf::from(root),
            publishable,
            has_lib,
        }
    }

    fn make_workspace_metadata(packages: Vec<WorkspacePackage>) -> WorkspaceMetadata {
        WorkspaceMetadata {
            workspace_root: PathBuf::from("/workspace"),
            packages,
        }
    }

    fn default_config() -> SemverguardConfig {
        SemverguardConfig {
            baseline: BaselineConfig::default(),
            scope: ScopeConfig {
                mode: ScopeMode::Workspace,
                include: vec![],
                exclude: vec![],
                explicit_packages: vec![],
                skip_publish_false: false,
                skip_no_lib: false,
            },
            features: FeaturesConfig::default(),
            engine: EngineConfig::default(),
            output: OutputConfig::default(),
        }
    }

    // =========================================================================
    // Workspace mode tests
    // =========================================================================

    #[test]
    fn test_workspace_mode_runs_all_packages() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "0.1.0", "/workspace/pkg-c", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.packages.len(), 3);
        assert_eq!(result.summary.total, 3);
        assert_eq!(result.summary.passed, 3);
        assert_eq!(result.summary.failed, 0);
        assert_eq!(result.summary.skipped, 0);
        assert_eq!(engine.call_count(), 3);
    }

    #[test]
    fn test_workspace_mode_with_empty_workspace() {
        let metadata = make_workspace_metadata(vec![]);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.packages.len(), 0);
        assert_eq!(result.summary.total, 0);
        assert_eq!(engine.call_count(), 0);
    }

    // =========================================================================
    // Changed mode tests
    // =========================================================================

    #[test]
    fn test_changed_mode_only_runs_changed_packages() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "0.1.0", "/workspace/pkg-c", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        // Only pkg-a has changed files
        let git = MockGitProvider::new(vec![PathBuf::from("pkg-a/src/lib.rs")]);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // pkg-a should be checked, pkg-b and pkg-c should be skipped as unchanged
        assert_eq!(result.summary.total, 3);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.skipped, 2);
        assert_eq!(engine.call_count(), 1);

        // Verify the skipped packages have correct reason
        let skipped: Vec<_> = result
            .packages
            .iter()
            .filter(|p| p.status == PackageStatus::Skipped)
            .collect();
        assert_eq!(skipped.len(), 2);
        for pkg in skipped {
            assert!(pkg.skip_reason.as_ref().unwrap().contains("unchanged"));
        }
    }

    #[test]
    fn test_changed_mode_workspace_level_changes_trigger_all() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        // A file at workspace level (not in any package) triggers all packages
        let git = MockGitProvider::new(vec![PathBuf::from("Cargo.toml")]);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // All packages should be checked because of workspace-level change
        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.skipped, 0);
        assert_eq!(engine.call_count(), 2);
    }

    #[test]
    fn test_changed_mode_requires_baseline_rev() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let git = MockGitProvider::new(vec![]);
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = None; // Missing rev

        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::InvalidConfig(_)));
        assert!(format!("{err}").contains("baseline.rev"));
    }

    #[test]
    fn test_changed_mode_requires_git_baseline_kind() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let git = MockGitProvider::new(vec![]);
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::CratesIo; // Wrong kind
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::InvalidConfig(_)));
        assert!(format!("{err}").contains("git"));
    }

    #[test]
    fn test_changed_mode_requires_git_provider() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![]);

        // No git provider
        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::InvalidConfig(_)));
        assert!(format!("{err}").contains("GitProvider"));
    }

    // =========================================================================
    // Glob include/exclude filter tests
    // =========================================================================

    #[test]
    fn test_include_glob_filters_packages() {
        let packages = vec![
            make_package("my-lib-core", "1.0.0", "/workspace/my-lib-core", true, true),
            make_package(
                "my-lib-utils",
                "1.0.0",
                "/workspace/my-lib-utils",
                true,
                true,
            ),
            make_package("other-crate", "1.0.0", "/workspace/other-crate", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.include = vec!["my-lib-*".to_string()];

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.skipped, 1);
        assert_eq!(engine.call_count(), 2);

        // Verify other-crate was skipped due to include filter
        let skipped = result
            .packages
            .iter()
            .find(|p| p.name == "other-crate")
            .unwrap();
        assert_eq!(skipped.status, PackageStatus::Skipped);
        assert!(skipped.skip_reason.as_ref().unwrap().contains("filtered"));
    }

    #[test]
    fn test_exclude_glob_filters_packages() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
            make_package(
                "internal-test",
                "1.0.0",
                "/workspace/internal-test",
                true,
                true,
            ),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.exclude = vec!["*-test".to_string()];

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.skipped, 1);
        assert_eq!(engine.call_count(), 2);

        // Verify internal-test was skipped due to exclude filter
        let skipped = result
            .packages
            .iter()
            .find(|p| p.name == "internal-test")
            .unwrap();
        assert_eq!(skipped.status, PackageStatus::Skipped);
    }

    #[test]
    fn test_include_and_exclude_combined() {
        let packages = vec![
            make_package("lib-core", "1.0.0", "/workspace/lib-core", true, true),
            make_package("lib-test", "1.0.0", "/workspace/lib-test", true, true),
            make_package("other", "1.0.0", "/workspace/other", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.include = vec!["lib-*".to_string()]; // Include lib-* packages
        config.scope.exclude = vec!["*-test".to_string()]; // But exclude *-test packages

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // Only lib-core should be checked (matches include, not excluded)
        // lib-test matches include but also matches exclude
        // other doesn't match include
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.skipped, 2);
        assert_eq!(engine.call_count(), 1);
    }

    #[test]
    fn test_invalid_glob_pattern_returns_error() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.include = vec!["[invalid".to_string()]; // Invalid glob

        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::InvalidConfig(_)));
        assert!(format!("{err}").contains("glob"));
    }

    // =========================================================================
    // Publishable filter tests
    // =========================================================================

    #[test]
    fn test_skip_publish_false_filters_unpublishable() {
        let packages = vec![
            make_package("lib-public", "1.0.0", "/workspace/lib-public", true, true),
            make_package(
                "lib-internal",
                "1.0.0",
                "/workspace/lib-internal",
                false,
                true,
            ),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.skipped, 1);
        assert_eq!(engine.call_count(), 1);

        // Verify lib-internal was skipped
        let skipped = result
            .packages
            .iter()
            .find(|p| p.name == "lib-internal")
            .unwrap();
        assert_eq!(skipped.status, PackageStatus::Skipped);
        assert!(skipped
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("publish = false"));
    }

    #[test]
    fn test_skip_publish_false_disabled_includes_unpublishable() {
        let packages = vec![
            make_package("lib-public", "1.0.0", "/workspace/lib-public", true, true),
            make_package(
                "lib-internal",
                "1.0.0",
                "/workspace/lib-internal",
                false,
                true,
            ),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = false;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.skipped, 0);
        assert_eq!(engine.call_count(), 2);
    }

    // =========================================================================
    // Has-lib filter tests
    // =========================================================================

    #[test]
    fn test_skip_no_lib_filters_binary_only() {
        let packages = vec![
            make_package("lib-crate", "1.0.0", "/workspace/lib-crate", true, true),
            make_package("bin-crate", "1.0.0", "/workspace/bin-crate", true, false),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_no_lib = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.skipped, 1);
        assert_eq!(engine.call_count(), 1);

        // Verify bin-crate was skipped
        let skipped = result
            .packages
            .iter()
            .find(|p| p.name == "bin-crate")
            .unwrap();
        assert_eq!(skipped.status, PackageStatus::Skipped);
        assert!(skipped
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("no library target"));
    }

    #[test]
    fn test_skip_no_lib_disabled_includes_binary() {
        let packages = vec![
            make_package("lib-crate", "1.0.0", "/workspace/lib-crate", true, true),
            make_package("bin-crate", "1.0.0", "/workspace/bin-crate", true, false),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_no_lib = false;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.skipped, 0);
        assert_eq!(engine.call_count(), 2);
    }

    // =========================================================================
    // Fail-fast tests
    // =========================================================================

    #[test]
    fn test_fail_fast_stops_on_first_failure() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "0.1.0", "/workspace/pkg-c", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::failure_result(), // Second package fails
            MockSemverEngine::success_result(), // Should not be reached
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.engine.fail_fast = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // Only first two packages should be processed
        assert_eq!(result.packages.len(), 2);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(engine.call_count(), 2);
    }

    #[test]
    fn test_fail_fast_on_engine_error() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::engine_error(),   // First package has engine error
            MockSemverEngine::success_result(), // Should not be reached
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.engine.fail_fast = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // Only first package should be processed
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(engine.call_count(), 1);

        // Verify the error message is captured
        let failed = &result.packages[0];
        assert!(failed
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("engine invocation failed"));
    }

    #[test]
    fn test_without_fail_fast_continues_after_failure() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "2.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "0.1.0", "/workspace/pkg-c", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::failure_result(), // Second package fails
            MockSemverEngine::success_result(), // Should still run
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.engine.fail_fast = false;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // All packages should be processed
        assert_eq!(result.packages.len(), 3);
        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(engine.call_count(), 3);
    }

    // =========================================================================
    // Skipped packages reporting tests
    // =========================================================================

    #[test]
    fn test_skipped_packages_have_zero_duration() {
        let packages = vec![
            make_package("lib-public", "1.0.0", "/workspace/lib-public", true, true),
            make_package(
                "lib-internal",
                "1.0.0",
                "/workspace/lib-internal",
                false,
                true,
            ),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        let skipped = result
            .packages
            .iter()
            .find(|p| p.name == "lib-internal")
            .unwrap();
        assert_eq!(skipped.duration_ms, 0);
        assert!(skipped.command.is_empty());
        assert!(skipped.engine.is_none());
    }

    #[test]
    fn test_skipped_packages_have_correct_status() {
        let packages = vec![
            make_package("pkg", "1.0.0", "/workspace/pkg", false, false), // Both filters apply
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = true;
        config.scope.skip_no_lib = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].status, PackageStatus::Skipped);
        // First matching filter wins (publish = false)
        assert!(result.packages[0]
            .skip_reason
            .as_ref()
            .unwrap()
            .contains("publish = false"));
    }

    // =========================================================================
    // Summary counts tests
    // =========================================================================

    #[test]
    fn test_summary_counts_are_correct() {
        let packages = vec![
            make_package("pkg-pass-1", "1.0.0", "/workspace/pkg-pass-1", true, true),
            make_package("pkg-pass-2", "1.0.0", "/workspace/pkg-pass-2", true, true),
            make_package("pkg-fail", "1.0.0", "/workspace/pkg-fail", true, true),
            make_package("pkg-skip", "1.0.0", "/workspace/pkg-skip", false, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
            MockSemverEngine::failure_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.total, 4);
        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(result.summary.skipped, 1);
    }

    #[test]
    fn test_summary_total_equals_sum_of_statuses() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "1.0.0", "/workspace/pkg-c", false, true),
            make_package("pkg-d", "1.0.0", "/workspace/pkg-d", true, false),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::failure_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.skip_publish_false = true;
        config.scope.skip_no_lib = true;

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(
            result.summary.total,
            result.summary.passed + result.summary.failed + result.summary.skipped
        );
    }

    // =========================================================================
    // Workspace provider error tests
    // =========================================================================

    #[test]
    fn test_workspace_provider_error_propagates() {
        let workspace = FailingWorkspaceProvider {
            error_msg: "failed to parse Cargo.toml".to_string(),
        };
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::Workspace(_)));
        assert!(format!("{err}").contains("failed to parse Cargo.toml"));
    }

    // =========================================================================
    // Git provider error tests
    // =========================================================================

    #[test]
    fn test_git_provider_error_propagates() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let git = FailingGitProvider {
            error_msg: "git rev-parse failed".to_string(),
        };
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::Git(_)));
        assert!(format!("{err}").contains("git rev-parse failed"));
    }

    // =========================================================================
    // Package report content tests
    // =========================================================================

    #[test]
    fn test_passed_package_report_content() {
        let packages = vec![make_package(
            "my-lib",
            "1.2.3",
            "/workspace/my-lib",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        let report = &result.packages[0];
        assert_eq!(report.name, "my-lib");
        assert_eq!(report.version, "1.2.3");
        assert_eq!(
            report.manifest_path,
            PathBuf::from("/workspace/my-lib/Cargo.toml")
        );
        assert_eq!(report.status, PackageStatus::Passed);
        assert!(report.skip_reason.is_none());
        assert!(!report.command.is_empty());
        assert!(report.engine.is_some());
        assert!(report.engine.as_ref().unwrap().success);
    }

    #[test]
    fn test_failed_package_report_content() {
        let packages = vec![make_package(
            "my-lib",
            "1.0.0",
            "/workspace/my-lib",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::failure_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        let report = &result.packages[0];
        assert_eq!(report.status, PackageStatus::Failed);
        assert!(report.skip_reason.is_none());
        assert!(report.engine.is_some());
        assert!(!report.engine.as_ref().unwrap().success);
        assert!(matches!(
            report.inferred_required_bump,
            Some(RequiredBump::Major)
        ));
    }

    // =========================================================================
    // RunArtifacts content tests
    // =========================================================================

    #[test]
    fn test_run_artifacts_workspace_root() {
        let packages = vec![make_package("pkg", "1.0.0", "/workspace/pkg", true, true)];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let config = default_config();
        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.workspace_root, PathBuf::from("/workspace"));
    }

    // =========================================================================
    // build_globset() unit tests
    // =========================================================================

    #[test]
    fn test_build_globset_empty_patterns_returns_none() {
        let result = build_globset(&[]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_globset_single_pattern() {
        let patterns = vec!["foo-*".to_string()];
        let result = build_globset(&patterns).unwrap();
        assert!(result.is_some());

        let set = result.unwrap();
        assert!(set.is_match("foo-bar"));
        assert!(set.is_match("foo-baz"));
        assert!(set.is_match("foo-"));
        assert!(!set.is_match("bar-foo"));
        assert!(!set.is_match("foo"));
    }

    #[test]
    fn test_build_globset_multiple_patterns() {
        let patterns = vec![
            "foo-*".to_string(),
            "bar-*".to_string(),
            "exact".to_string(),
        ];
        let result = build_globset(&patterns).unwrap();
        assert!(result.is_some());

        let set = result.unwrap();
        assert!(set.is_match("foo-lib"));
        assert!(set.is_match("bar-lib"));
        assert!(set.is_match("exact"));
        assert!(!set.is_match("baz-lib"));
        assert!(!set.is_match("exactnot"));
    }

    #[test]
    fn test_build_globset_invalid_glob_pattern_returns_error() {
        let patterns = vec!["[invalid".to_string()];
        let result = build_globset(&patterns);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SemverguardError::InvalidConfig(_)));
        assert!(format!("{err}").contains("bad glob"));
    }

    #[test]
    fn test_build_globset_mixed_valid_and_invalid_stops_at_first_error() {
        let patterns = vec!["valid-*".to_string(), "[invalid".to_string()];
        let result = build_globset(&patterns);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_globset_question_mark_pattern() {
        let patterns = vec!["pkg-?".to_string()];
        let result = build_globset(&patterns).unwrap();
        let set = result.unwrap();
        assert!(set.is_match("pkg-a"));
        assert!(set.is_match("pkg-b"));
        assert!(!set.is_match("pkg-ab"));
        assert!(!set.is_match("pkg-"));
    }

    #[test]
    fn test_build_globset_character_class_pattern() {
        let patterns = vec!["pkg-[abc]".to_string()];
        let result = build_globset(&patterns).unwrap();
        let set = result.unwrap();
        assert!(set.is_match("pkg-a"));
        assert!(set.is_match("pkg-b"));
        assert!(set.is_match("pkg-c"));
        assert!(!set.is_match("pkg-d"));
    }

    // =========================================================================
    // matches_name_filters() unit tests
    // =========================================================================

    #[test]
    fn test_matches_name_filters_no_filters_matches_all() {
        let pkg = make_package("any-name", "1.0.0", "/workspace/any-name", true, true);
        assert!(matches_name_filters(&pkg, &None, &None));
    }

    #[test]
    fn test_matches_name_filters_include_only_matches() {
        let pkg = make_package("foo-lib", "1.0.0", "/workspace/foo-lib", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        assert!(matches_name_filters(&pkg, &include, &None));
    }

    #[test]
    fn test_matches_name_filters_include_only_no_match() {
        let pkg = make_package("bar-lib", "1.0.0", "/workspace/bar-lib", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        assert!(!matches_name_filters(&pkg, &include, &None));
    }

    #[test]
    fn test_matches_name_filters_exclude_only_matches() {
        let pkg = make_package("test-lib", "1.0.0", "/workspace/test-lib", true, true);
        let exclude = build_globset(&["test-*".to_string()]).unwrap();
        assert!(!matches_name_filters(&pkg, &None, &exclude));
    }

    #[test]
    fn test_matches_name_filters_exclude_only_no_match() {
        let pkg = make_package("prod-lib", "1.0.0", "/workspace/prod-lib", true, true);
        let exclude = build_globset(&["test-*".to_string()]).unwrap();
        assert!(matches_name_filters(&pkg, &None, &exclude));
    }

    #[test]
    fn test_matches_name_filters_both_include_and_exclude_match() {
        // Exclude should take precedence (included && !excluded)
        let pkg = make_package("foo-test", "1.0.0", "/workspace/foo-test", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        let exclude = build_globset(&["*-test".to_string()]).unwrap();
        assert!(!matches_name_filters(&pkg, &include, &exclude));
    }

    #[test]
    fn test_matches_name_filters_include_match_exclude_no_match() {
        let pkg = make_package("foo-lib", "1.0.0", "/workspace/foo-lib", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        let exclude = build_globset(&["*-test".to_string()]).unwrap();
        assert!(matches_name_filters(&pkg, &include, &exclude));
    }

    #[test]
    fn test_matches_name_filters_include_no_match_exclude_no_match() {
        let pkg = make_package("bar-lib", "1.0.0", "/workspace/bar-lib", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        let exclude = build_globset(&["test-*".to_string()]).unwrap();
        assert!(!matches_name_filters(&pkg, &include, &exclude));
    }

    #[test]
    fn test_matches_name_filters_include_no_match_exclude_match() {
        // Doesn't match include, so excluded doesn't matter
        let pkg = make_package("bar-test", "1.0.0", "/workspace/bar-test", true, true);
        let include = build_globset(&["foo-*".to_string()]).unwrap();
        let exclude = build_globset(&["*-test".to_string()]).unwrap();
        assert!(!matches_name_filters(&pkg, &include, &exclude));
    }

    // =========================================================================
    // is_under() and path utility unit tests
    // =========================================================================

    #[test]
    fn test_is_under_same_path() {
        assert!(is_under(Path::new("foo"), Path::new("foo")));
    }

    #[test]
    fn test_is_under_subpath() {
        assert!(is_under(Path::new("foo/bar/baz"), Path::new("foo")));
        assert!(is_under(Path::new("foo/bar/baz"), Path::new("foo/bar")));
    }

    #[test]
    fn test_is_under_not_subpath() {
        assert!(!is_under(Path::new("foo"), Path::new("foo/bar")));
        assert!(!is_under(Path::new("bar"), Path::new("foo")));
        assert!(!is_under(Path::new("foobar"), Path::new("foo")));
    }

    #[test]
    fn test_normalize_rel_removes_current_dir() {
        let result = normalize_rel(Path::new("./foo/bar"));
        assert_eq!(result, PathBuf::from("foo/bar"));
    }

    #[test]
    fn test_normalize_rel_preserves_parent_dir() {
        let result = normalize_rel(Path::new("../foo"));
        assert_eq!(result, PathBuf::from("../foo"));
    }

    #[test]
    fn test_normalize_rel_normal_path() {
        let result = normalize_rel(Path::new("foo/bar/baz"));
        assert_eq!(result, PathBuf::from("foo/bar/baz"));
    }

    #[test]
    fn test_path_relative_to_success() {
        let result = path_relative_to(Path::new("/workspace/pkg/src"), Path::new("/workspace"));
        assert_eq!(result, Some(PathBuf::from("pkg/src")));
    }

    #[test]
    fn test_path_relative_to_not_prefix() {
        let result = path_relative_to(Path::new("/other/path"), Path::new("/workspace"));
        assert_eq!(result, None);
    }

    #[test]
    fn test_path_relative_to_same_path() {
        let result = path_relative_to(Path::new("/workspace"), Path::new("/workspace"));
        assert_eq!(result, Some(PathBuf::from("")));
    }

    #[test]
    fn test_is_under_empty_root_matches_all() {
        // Empty root should match any path
        assert!(is_under(Path::new("foo/bar"), Path::new("")));
        assert!(is_under(Path::new("any/path/here"), Path::new("")));
    }

    #[test]
    fn test_is_under_path_shorter_than_root() {
        assert!(!is_under(Path::new("foo"), Path::new("foo/bar")));
    }

    #[test]
    fn test_is_under_similar_prefix_not_subpath() {
        // "foobar" should NOT be under "foo" (not a directory relationship)
        assert!(!is_under(Path::new("foobar/src"), Path::new("foo")));
    }

    #[test]
    fn test_normalize_rel_multiple_current_dirs() {
        let result = normalize_rel(Path::new("./foo/./bar/./baz"));
        assert_eq!(result, PathBuf::from("foo/bar/baz"));
    }

    #[test]
    fn test_normalize_rel_empty_path() {
        let result = normalize_rel(Path::new(""));
        assert_eq!(result, PathBuf::from(""));
    }

    #[test]
    fn test_normalize_rel_only_current_dir() {
        let result = normalize_rel(Path::new("."));
        assert_eq!(result, PathBuf::from(""));
    }

    // =========================================================================
    // changed_packages() unit tests
    // =========================================================================

    #[test]
    fn test_changed_packages_single_package_changed() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let git = MockGitProvider::new(vec![PathBuf::from("pkg-a/src/lib.rs")]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.contains("pkg-a"));
        assert!(!result.contains("pkg-b"));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_changed_packages_multiple_packages_changed() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
            make_package("pkg-c", "1.0.0", "/workspace/pkg-c", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let git = MockGitProvider::new(vec![
            PathBuf::from("pkg-a/src/lib.rs"),
            PathBuf::from("pkg-c/Cargo.toml"),
        ]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.contains("pkg-a"));
        assert!(!result.contains("pkg-b"));
        assert!(result.contains("pkg-c"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_changed_packages_no_changes() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let git = MockGitProvider::new(vec![]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_changed_packages_workspace_level_change_marks_all() {
        // A change outside all package roots marks all packages as changed
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        // Cargo.toml at workspace root is not inside any package
        let git = MockGitProvider::new(vec![PathBuf::from("Cargo.toml")]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.contains("pkg-a"));
        assert!(result.contains("pkg-b"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_changed_packages_ci_config_change_marks_all() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        // .github directory is not inside any package
        let git = MockGitProvider::new(vec![PathBuf::from(".github/workflows/ci.yml")]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        // All packages marked as changed conservatively
        assert!(result.contains("pkg-a"));
    }

    #[test]
    fn test_changed_packages_deeply_nested_file() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        let git = MockGitProvider::new(vec![PathBuf::from(
            "pkg-a/src/module/submodule/deeply/nested.rs",
        )]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.contains("pkg-a"));
    }

    #[test]
    fn test_changed_packages_with_dot_prefix_paths() {
        let packages = vec![make_package(
            "pkg-a",
            "1.0.0",
            "/workspace/pkg-a",
            true,
            true,
        )];
        let metadata = make_workspace_metadata(packages);
        // Git sometimes reports paths with ./ prefix
        let git = MockGitProvider::new(vec![PathBuf::from("./pkg-a/src/lib.rs")]);

        let result =
            changed_packages(&metadata, &git, Path::new("/workspace"), "main", "HEAD").unwrap();

        assert!(result.contains("pkg-a"));
    }

    // =========================================================================
    // summarize() unit tests
    // =========================================================================

    #[test]
    fn test_summarize_empty_reports() {
        let reports: Vec<PackageReport> = vec![];
        let summary = summarize(&reports);
        assert_eq!(summary.total, 0);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_summarize_all_passed() {
        let reports = vec![
            PackageReport {
                name: "a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/a/Cargo.toml"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
            PackageReport {
                name: "b".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/b/Cargo.toml"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 200,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
        ];
        let summary = summarize(&reports);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_summarize_all_failed() {
        let reports = vec![PackageReport {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/a/Cargo.toml"),
            status: PackageStatus::Failed,
            skip_reason: None,
            duration_ms: 100,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
        }];
        let summary = summarize(&reports);
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_summarize_all_skipped() {
        let reports = vec![PackageReport {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            manifest_path: PathBuf::from("/a/Cargo.toml"),
            status: PackageStatus::Skipped,
            skip_reason: Some("excluded".to_string()),
            duration_ms: 0,
            command: vec![],
            engine: None,
            inferred_required_bump: None,
        }];
        let summary = summarize(&reports);
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 1);
    }

    #[test]
    fn test_summarize_mixed_statuses() {
        let reports = vec![
            PackageReport {
                name: "a".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/a/Cargo.toml"),
                status: PackageStatus::Passed,
                skip_reason: None,
                duration_ms: 100,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
            PackageReport {
                name: "b".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/b/Cargo.toml"),
                status: PackageStatus::Failed,
                skip_reason: None,
                duration_ms: 200,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
            PackageReport {
                name: "c".to_string(),
                version: "1.0.0".to_string(),
                manifest_path: PathBuf::from("/c/Cargo.toml"),
                status: PackageStatus::Skipped,
                skip_reason: Some("excluded".to_string()),
                duration_ms: 0,
                command: vec![],
                engine: None,
                inferred_required_bump: None,
            },
        ];
        let summary = summarize(&reports);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
    }

    // =========================================================================
    // Combined filter tests
    // =========================================================================

    #[test]
    fn test_all_filters_combined() {
        let packages = vec![
            // Should pass: publishable, has lib, matches include, not excluded, changed
            make_package("my-lib-core", "1.0.0", "/workspace/my-lib-core", true, true),
            // Should skip: matches exclude pattern
            make_package("my-lib-test", "1.0.0", "/workspace/my-lib-test", true, true),
            // Should skip: not publishable
            make_package(
                "my-lib-internal",
                "1.0.0",
                "/workspace/my-lib-internal",
                false,
                true,
            ),
            // Should skip: no lib target
            make_package("my-lib-cli", "1.0.0", "/workspace/my-lib-cli", true, false),
            // Should skip: doesn't match include pattern
            make_package("other-crate", "1.0.0", "/workspace/other-crate", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let git = MockGitProvider::new(vec![
            PathBuf::from("my-lib-core/src/lib.rs"),
            PathBuf::from("my-lib-test/src/lib.rs"),
            PathBuf::from("my-lib-internal/src/lib.rs"),
            PathBuf::from("my-lib-cli/src/main.rs"),
            PathBuf::from("other-crate/src/lib.rs"),
        ]);
        let engine = MockSemverEngine::new(vec![MockSemverEngine::success_result()]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.scope.include = vec!["my-lib-*".to_string()];
        config.scope.exclude = vec!["*-test".to_string()];
        config.scope.skip_publish_false = true;
        config.scope.skip_no_lib = true;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.total, 5);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.skipped, 4);
        assert_eq!(engine.call_count(), 1);

        // Verify the only passed package
        let passed = result
            .packages
            .iter()
            .find(|p| p.status == PackageStatus::Passed)
            .unwrap();
        assert_eq!(passed.name, "my-lib-core");
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_empty_include_means_include_all() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.include = vec![]; // Empty = include all

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
    }

    #[test]
    fn test_empty_exclude_means_exclude_none() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, None, &engine);
        let mut config = default_config();
        config.scope.exclude = vec![]; // Empty = exclude none

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        assert_eq!(result.summary.passed, 2);
    }

    #[test]
    fn test_changed_mode_with_no_changes() {
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/pkg-a", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/pkg-b", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        let git = MockGitProvider::new(vec![]); // No changes
        let engine = MockSemverEngine::new(vec![]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // All packages should be skipped as unchanged
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.skipped, 2);
        assert_eq!(result.summary.passed, 0);
        assert_eq!(engine.call_count(), 0);
    }

    #[test]
    fn test_multiple_packages_in_same_directory() {
        // While unusual, this tests the path matching logic
        let packages = vec![
            make_package("pkg-a", "1.0.0", "/workspace/shared", true, true),
            make_package("pkg-b", "1.0.0", "/workspace/shared", true, true),
        ];
        let metadata = make_workspace_metadata(packages);
        let workspace = MockWorkspaceProvider::new(metadata);
        // A change in shared directory affects both packages
        let git = MockGitProvider::new(vec![PathBuf::from("shared/src/lib.rs")]);
        let engine = MockSemverEngine::new(vec![
            MockSemverEngine::success_result(),
            MockSemverEngine::success_result(),
        ]);

        let runner = SemverguardRunner::new(&workspace, Some(&git), &engine);
        let mut config = default_config();
        config.scope.mode = ScopeMode::Changed;
        config.baseline.kind = BaselineKind::Git;
        config.baseline.rev = Some("origin/main".to_string());

        let result = runner.run(Path::new("/workspace"), &config).unwrap();

        // Both packages should be checked since they share the same root
        assert_eq!(result.summary.passed, 2);
        assert_eq!(engine.call_count(), 2);
    }
}
