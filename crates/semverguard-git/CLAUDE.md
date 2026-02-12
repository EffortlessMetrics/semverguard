# CLAUDE.md - semverguard-git

Concrete adapter wrapping git CLI for detecting changed files between revisions.

## Role in Architecture

Implements the `GitProvider` port trait. This crate knows how to:
- Execute `git diff --name-only` between revisions
- Parse output into a list of changed paths

## Key Components

```rust
pub struct GitCli { /* git binary path */ }

impl GitProvider for GitCli {
    fn changed_paths(&self, workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>>;
}

// Exported for testing
pub fn parse_diff_output(output: &str) -> Vec<PathBuf>;
```

## Git Command Details

Uses **three-dot syntax** for diff:

```bash
git diff --name-only {base}...{head}
```

This matches common PR semantics - it compares the merge-base of `base` and `head` to `head`, showing only files changed in the PR branch.

## Path Handling

- Returns paths **relative to workspace root**
- Handles empty output (no changes)
- Trims whitespace and filters blank lines
- Handles both Unix (`\n`) and Windows (`\r\n`) line endings

## Dependencies

```
semverguard-domain  # GitProvider trait, error types
```

## Testing

Unit tests cover parsing edge cases:
- Empty output
- Single file
- Multiple files
- Whitespace handling
- Windows line endings
