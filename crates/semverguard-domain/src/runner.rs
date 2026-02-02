use crate::error::{Result, SemverguardError};
use crate::ports::{GitProvider, SemverEngine, WorkspaceProvider};
use globset::{Glob, GlobSet, GlobSetBuilder};
use semverguard_types::{
    PackageReport, PackageStatus, SemverCheckRequest, Summary, WorkspaceMetadata, WorkspacePackage,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
        }
    }

    /// Run semver checks.
    pub fn run(&self, workspace_root: &Path, config: &semverguard_types::SemverguardConfig) -> Result<RunArtifacts> {
        let metadata = self.workspace.load(workspace_root)?;
        let include_set = build_globset(&config.scope.include)?;
        let exclude_set = build_globset(&config.scope.exclude)?;

        let mut reports: Vec<PackageReport> = Vec::new();

        // First pass: apply static filters (publishability, has-lib, name patterns).
        let mut eligible: Vec<WorkspacePackage> = Vec::new();
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

        // Second pass: scope selection (changed vs workspace).
        let eligible = match config.scope.mode {
            semverguard_types::ScopeMode::Workspace => eligible,
            semverguard_types::ScopeMode::Changed => {
                let baseline_rev = config
                    .baseline
                    .rev
                    .as_deref()
                    .ok_or_else(|| SemverguardError::InvalidConfig("scope.mode=changed requires baseline.rev".into()))?;

                if !matches!(config.baseline.kind, semverguard_types::BaselineKind::Git) {
                    return Err(SemverguardError::InvalidConfig(
                        "scope.mode=changed requires baseline.kind = \"git\"".into(),
                    ));
                }

                let git = self.git.ok_or_else(|| {
                    SemverguardError::InvalidConfig("scope.mode=changed requires a GitProvider".into())
                })?;

                let changed = changed_packages(&metadata, git, workspace_root, baseline_rev, "HEAD")?;

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
        for pkg in eligible {
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
                    let status = if output.success {
                        PackageStatus::Passed
                    } else {
                        PackageStatus::Failed
                    };

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

                    if !output.success && config.engine.fail_fast {
                        break;
                    }
                }
                Err(e) => {
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
        Ok(RunArtifacts {
            workspace_root: metadata.workspace_root,
            packages: reports,
            summary,
        })
    }
}

fn build_globset(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).map_err(|e| SemverguardError::InvalidConfig(format!("bad glob '{p}': {e}")))?;
        builder.add(glob);
    }
    let set = builder
        .build()
        .map_err(|e| SemverguardError::InvalidConfig(format!("globset build error: {e}")))?;
    Ok(Some(set))
}

fn matches_name_filters(pkg: &WorkspacePackage, include: &Option<GlobSet>, exclude: &Option<GlobSet>) -> bool {
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
        let rel = path_relative_to(&p.package_root, &metadata.workspace_root).unwrap_or_else(|| p.package_root.clone());
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
