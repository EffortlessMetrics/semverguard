//! Integration tests for semverguard-git.
//!
//! These tests create temporary git repositories to test the GitCli adapter
//! against real git operations.

use semverguard_domain::GitProvider;
use semverguard_git::GitCli;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run a git command in a directory.
fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git command")
}

/// Helper to run a git command and assert success.
fn git_ok(dir: &std::path::Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a temporary git repository with an initial commit.
fn create_temp_repo() -> TempDir {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path();

    git_ok(path, &["init"]);
    git_ok(path, &["config", "user.email", "test@example.com"]);
    git_ok(path, &["config", "user.name", "Test User"]);
    // Defensive: some hosts set commit.gpgsign globally; force off at repo
    // scope so this test does not depend on signing infrastructure.
    git_ok(path, &["config", "commit.gpgsign", "false"]);
    git_ok(path, &["config", "tag.gpgsign", "false"]);

    // Create initial file and commit
    fs::write(path.join("README.md"), "# Test Repo").expect("failed to write README");
    git_ok(path, &["add", "README.md"]);
    git_ok(path, &["commit", "-m", "Initial commit"]);

    dir
}

#[test]
fn changed_paths_returns_empty_when_no_changes() {
    let repo = create_temp_repo();
    let cli = GitCli::default();

    // Compare HEAD to itself - should have no changes
    let result = cli
        .changed_paths(repo.path(), "HEAD", "HEAD")
        .expect("changed_paths failed");

    assert!(result.is_empty(), "expected no changes, got {:?}", result);
}

#[test]
fn changed_paths_returns_modified_files() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Create a branch point
    git_ok(path, &["checkout", "-b", "feature"]);

    // Modify a file and add a new one
    fs::write(path.join("README.md"), "# Updated").expect("failed to write README");
    fs::write(path.join("new_file.txt"), "new content").expect("failed to write new_file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Feature changes"]);

    // Compare main to feature
    let result = cli
        .changed_paths(path, "main", "feature")
        .expect("changed_paths failed");

    assert!(result.contains(&PathBuf::from("README.md")));
    assert!(result.contains(&PathBuf::from("new_file.txt")));
    assert_eq!(result.len(), 2);
}

#[test]
fn changed_paths_returns_files_in_subdirectories() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Create subdirectory structure
    fs::create_dir_all(path.join("src/lib")).expect("failed to create dirs");
    fs::write(path.join("src/lib/mod.rs"), "// module").expect("failed to write mod.rs");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add source files"]);

    // Create branch and modify nested file
    git_ok(path, &["checkout", "-b", "nested-change"]);
    fs::write(path.join("src/lib/mod.rs"), "// updated module").expect("failed to write mod.rs");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Update nested file"]);

    let result = cli
        .changed_paths(path, "main", "nested-change")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("src/lib/mod.rs")]);
}

#[test]
fn changed_paths_handles_deleted_files() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Add a file to delete later
    fs::write(path.join("to_delete.txt"), "will be deleted").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file to delete"]);

    // Create branch and delete the file
    git_ok(path, &["checkout", "-b", "delete-file"]);
    fs::remove_file(path.join("to_delete.txt")).expect("failed to delete file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Delete file"]);

    let result = cli
        .changed_paths(path, "main", "delete-file")
        .expect("changed_paths failed");

    assert!(result.contains(&PathBuf::from("to_delete.txt")));
}

#[test]
fn changed_paths_handles_renamed_files() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Add a file to rename
    fs::write(path.join("old_name.txt"), "content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file to rename"]);

    // Create branch and rename the file
    git_ok(path, &["checkout", "-b", "rename-file"]);
    git_ok(path, &["mv", "old_name.txt", "new_name.txt"]);
    git_ok(path, &["commit", "-m", "Rename file"]);

    let result = cli
        .changed_paths(path, "main", "rename-file")
        .expect("changed_paths failed");

    // git diff --name-only shows both old and new names for renames
    assert!(
        result.contains(&PathBuf::from("new_name.txt"))
            || result.contains(&PathBuf::from("old_name.txt"))
    );
}

#[test]
fn changed_paths_errors_on_invalid_ref() {
    let repo = create_temp_repo();
    let cli = GitCli::default();

    let result = cli.changed_paths(repo.path(), "nonexistent-ref", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("git"),
        "error should mention git: {}",
        err
    );
}

#[test]
fn changed_paths_errors_on_invalid_directory() {
    let cli = GitCli::default();

    let result = cli.changed_paths(std::path::Path::new("/nonexistent/path"), "main", "HEAD");

    assert!(result.is_err());
}

#[test]
fn changed_paths_works_with_head_symbolic_ref() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Make a change on current branch
    git_ok(path, &["checkout", "-b", "test-branch"]);
    fs::write(path.join("test.txt"), "test content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add test file"]);

    // Use HEAD as the head ref
    let result = cli
        .changed_paths(path, "main", "HEAD")
        .expect("changed_paths failed");

    assert!(result.contains(&PathBuf::from("test.txt")));
}

#[test]
fn changed_paths_handles_multiple_commits() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "multi-commit"]);

    // First commit
    fs::write(path.join("file1.txt"), "content 1").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file1"]);

    // Second commit
    fs::write(path.join("file2.txt"), "content 2").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file2"]);

    // Third commit
    fs::write(path.join("file3.txt"), "content 3").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file3"]);

    let result = cli
        .changed_paths(path, "main", "multi-commit")
        .expect("changed_paths failed");

    // Should include all files from all commits
    assert!(result.contains(&PathBuf::from("file1.txt")));
    assert!(result.contains(&PathBuf::from("file2.txt")));
    assert!(result.contains(&PathBuf::from("file3.txt")));
    assert_eq!(result.len(), 3);
}

// ===========================================================================
// Three-dot diff behavior tests
// ===========================================================================

#[test]
fn three_dot_diff_excludes_changes_on_base_after_branch_point() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Create a branch from main
    git_ok(path, &["checkout", "-b", "feature"]);
    fs::write(path.join("feature.txt"), "feature content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add feature file"]);

    // Go back to main and make changes there
    git_ok(path, &["checkout", "main"]);
    fs::write(path.join("main_only.txt"), "main only content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add main-only file"]);

    // Three-dot diff should only show changes on feature branch (from merge-base)
    // not the changes on main after the branch point
    let result = cli
        .changed_paths(path, "main", "feature")
        .expect("changed_paths failed");

    assert!(
        result.contains(&PathBuf::from("feature.txt")),
        "should contain feature branch changes"
    );
    // With three-dot diff, changes on main after branch point should NOT appear
    assert!(
        !result.contains(&PathBuf::from("main_only.txt")),
        "should NOT contain main-only changes (three-dot semantics)"
    );
}

#[test]
fn three_dot_diff_shows_only_branch_changes_with_diverged_history() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Add a file on main that will be the branch point
    fs::write(path.join("shared.txt"), "shared content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add shared file"]);

    // Create feature branch
    git_ok(path, &["checkout", "-b", "feature"]);
    fs::write(path.join("feature_a.txt"), "feature a").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Feature commit A"]);

    fs::write(path.join("feature_b.txt"), "feature b").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Feature commit B"]);

    // Go back to main and make different changes
    git_ok(path, &["checkout", "main"]);
    fs::write(path.join("main_a.txt"), "main a").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Main commit A"]);

    fs::write(path.join("main_b.txt"), "main b").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Main commit B"]);

    // Three-dot diff from main to feature
    let result = cli
        .changed_paths(path, "main", "feature")
        .expect("changed_paths failed");

    // Should only see feature branch changes
    assert!(result.contains(&PathBuf::from("feature_a.txt")));
    assert!(result.contains(&PathBuf::from("feature_b.txt")));
    // Should NOT see main branch changes after divergence
    assert!(!result.contains(&PathBuf::from("main_a.txt")));
    assert!(!result.contains(&PathBuf::from("main_b.txt")));
    assert_eq!(result.len(), 2);
}

// ===========================================================================
// Additional error case tests
// ===========================================================================

#[test]
fn changed_paths_errors_on_non_git_directory() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let cli = GitCli::default();

    // This is a valid directory but NOT a git repository
    let result = cli.changed_paths(dir.path(), "main", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("git") || err_str.contains("fatal"),
        "error should indicate git failure: {}",
        err_str
    );
}

#[test]
fn changed_paths_errors_on_invalid_base_reference() {
    let repo = create_temp_repo();
    let cli = GitCli::default();

    // Valid HEAD but invalid base
    let result = cli.changed_paths(repo.path(), "origin/nonexistent-branch", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("git"),
        "error should mention git: {}",
        err
    );
}

#[test]
fn changed_paths_errors_on_invalid_head_reference() {
    let repo = create_temp_repo();
    let cli = GitCli::default();

    // Valid base but invalid head
    let result = cli.changed_paths(repo.path(), "HEAD", "nonexistent-tag");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("git"),
        "error should mention git: {}",
        err
    );
}

// ===========================================================================
// Additional file change type tests
// ===========================================================================

#[test]
fn changed_paths_detects_added_file_only() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "add-only"]);

    // Only add a new file, don't modify existing ones
    fs::write(path.join("brand_new.txt"), "brand new content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add brand new file"]);

    let result = cli
        .changed_paths(path, "main", "add-only")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("brand_new.txt")]);
}

#[test]
fn changed_paths_detects_modified_file_only() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "modify-only"]);

    // Only modify existing README, don't add new files
    fs::write(
        path.join("README.md"),
        "# Modified Title\n\nNew content here.",
    )
    .expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Modify README only"]);

    let result = cli
        .changed_paths(path, "main", "modify-only")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("README.md")]);
}

#[test]
fn changed_paths_detects_deleted_file_only() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // First add a file to delete
    fs::write(path.join("will_delete.txt"), "to be deleted").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file to delete later"]);

    git_ok(path, &["checkout", "-b", "delete-only"]);

    // Only delete, don't add or modify
    fs::remove_file(path.join("will_delete.txt")).expect("failed to delete file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Delete file only"]);

    let result = cli
        .changed_paths(path, "main", "delete-only")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("will_delete.txt")]);
}

#[test]
fn changed_paths_detects_renamed_file_with_content_change() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Add file to rename
    fs::write(path.join("original.txt"), "original content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add original file"]);

    git_ok(path, &["checkout", "-b", "rename-with-change"]);

    // Rename AND modify content
    fs::remove_file(path.join("original.txt")).expect("failed to delete file");
    fs::write(path.join("renamed.txt"), "modified content after rename")
        .expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Rename and modify"]);

    let result = cli
        .changed_paths(path, "main", "rename-with-change")
        .expect("changed_paths failed");

    // Should detect both old and new names since content changed significantly
    assert!(
        result.contains(&PathBuf::from("original.txt"))
            || result.contains(&PathBuf::from("renamed.txt")),
        "should detect rename: {:?}",
        result
    );
}

// ===========================================================================
// GitCli construction tests
// ===========================================================================

#[test]
fn git_cli_new_with_custom_binary() {
    let cli = GitCli::new(Some(PathBuf::from("/usr/bin/git")));
    // We can't easily test the internal state, but we can verify it doesn't panic
    assert!(format!("{:?}", cli).contains("/usr/bin/git"));
}

#[test]
fn git_cli_default_uses_git_from_path() {
    let cli = GitCli::default();
    // Default should use "git" from PATH
    assert!(format!("{:?}", cli).contains("git"));
}

// ===========================================================================
// Edge case tests
// ===========================================================================

#[test]
fn changed_paths_handles_files_with_spaces() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "spaces"]);

    fs::write(path.join("file with spaces.txt"), "content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add file with spaces"]);

    let result = cli
        .changed_paths(path, "main", "spaces")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("file with spaces.txt")]);
}

#[test]
fn changed_paths_handles_files_with_special_characters() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "special-chars"]);

    // Test with various special characters that are valid in filenames
    fs::write(path.join("file-with-dashes.txt"), "content").expect("failed to write file");
    fs::write(path.join("file_with_underscores.txt"), "content").expect("failed to write file");
    fs::write(path.join("file.multiple.dots.txt"), "content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add files with special chars"]);

    let result = cli
        .changed_paths(path, "main", "special-chars")
        .expect("changed_paths failed");

    assert!(result.contains(&PathBuf::from("file-with-dashes.txt")));
    assert!(result.contains(&PathBuf::from("file_with_underscores.txt")));
    assert!(result.contains(&PathBuf::from("file.multiple.dots.txt")));
    assert_eq!(result.len(), 3);
}

#[test]
fn changed_paths_handles_deeply_nested_files() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    git_ok(path, &["checkout", "-b", "deep-nesting"]);

    // Create deeply nested directory structure
    let deep_path = path.join("a/b/c/d/e/f");
    fs::create_dir_all(&deep_path).expect("failed to create deep dirs");
    fs::write(deep_path.join("deep.txt"), "deep content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "Add deeply nested file"]);

    let result = cli
        .changed_paths(path, "main", "deep-nesting")
        .expect("changed_paths failed");

    assert_eq!(result, vec![PathBuf::from("a/b/c/d/e/f/deep.txt")]);
}

#[test]
fn changed_paths_empty_diff_between_identical_branches() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Create a branch at the same commit as main
    git_ok(path, &["checkout", "-b", "identical"]);

    let result = cli
        .changed_paths(path, "main", "identical")
        .expect("changed_paths failed");

    assert!(result.is_empty());
}

#[test]
fn changed_paths_works_with_commit_sha() {
    let repo = create_temp_repo();
    let path = repo.path();
    let cli = GitCli::default();

    // Get the SHA of the initial commit
    let output = git(path, &["rev-parse", "HEAD"]);
    let initial_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Make a new commit
    fs::write(path.join("new.txt"), "new content").expect("failed to write file");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-m", "New commit"]);

    // Compare using SHA
    let result = cli
        .changed_paths(path, &initial_sha, "HEAD")
        .expect("changed_paths failed");

    assert!(result.contains(&PathBuf::from("new.txt")));
}

// ===========================================================================
// Git binary error tests
// ===========================================================================

#[test]
fn test_git_binary_not_found() {
    // Test behavior when git binary doesn't exist
    let cli = GitCli::new(Some(PathBuf::from("/nonexistent/path/to/git")));
    let repo = create_temp_repo();

    let result = cli.changed_paths(repo.path(), "HEAD", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    // Should indicate it failed to run git
    assert!(
        err_msg.contains("failed to run git"),
        "Error should mention failed to run git: {}",
        err_msg
    );
}

#[test]
fn test_git_invalid_binary_path() {
    // Test with a path that exists but isn't executable (use a directory)
    let cli = GitCli::new(Some(std::env::temp_dir()));
    let repo = create_temp_repo();

    let result = cli.changed_paths(repo.path(), "HEAD", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    // Should indicate it failed to run git
    assert!(
        err_msg.contains("failed to run git"),
        "Error should mention failed to run git: {}",
        err_msg
    );
}

#[test]
fn test_git_binary_with_empty_path() {
    // Test with empty string as git path - should fail to execute
    let cli = GitCli::new(Some(PathBuf::from("")));
    let repo = create_temp_repo();

    let result = cli.changed_paths(repo.path(), "HEAD", "HEAD");

    // Empty path should fail
    assert!(result.is_err());
}

#[test]
fn test_git_error_message_contains_context() {
    // Verify that git errors contain actionable context
    let cli = GitCli::default();
    let repo = create_temp_repo();

    // Use an invalid ref to trigger a git error
    let result = cli.changed_paths(repo.path(), "definitely-not-a-valid-ref", "HEAD");

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Error should contain useful information for debugging
    assert!(
        err_msg.contains("git") || err_msg.contains("command"),
        "Error should mention git or command: {}",
        err_msg
    );
}
