Feature: Scope Selection (Pass 2)
  As a CI system
  I want to check only changed packages
  So that builds are faster

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled

  # =============================================================================
  # Workspace Mode
  # =============================================================================

  Scenario: Workspace mode checks all packages
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
      | pkg-c | 1.0.0   | true        | true    |
    And scope mode is "workspace"
    When running check
    Then 3 packages are checked
    And summary shows 3 passed, 0 failed, 0 skipped

  Scenario: Empty workspace succeeds with zero packages
    Given an empty workspace
    And scope mode is "workspace"
    When running check
    Then run succeeds
    And summary shows 0 passed, 0 failed, 0 skipped

  # =============================================================================
  # Changed Mode - Basic
  # =============================================================================

  Scenario: Changed mode checks only modified packages
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
      | pkg-c | 1.0.0   | true        | true    | crates/c |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files ["crates/a/src/lib.rs"]
    When running check
    Then 1 packages are checked
    And package "pkg-a" is checked
    And package "pkg-b" is skipped with reason "unchanged"
    And package "pkg-c" is skipped with reason "unchanged"

  Scenario: Multiple packages changed
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
      | pkg-c | 1.0.0   | true        | true    | crates/c |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files ["crates/a/src/lib.rs", "crates/c/Cargo.toml"]
    When running check
    Then 2 packages are checked
    And package "pkg-a" is checked
    And package "pkg-c" is checked
    And package "pkg-b" is skipped with reason "unchanged"

  Scenario: No changes result in all packages skipped
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And no changed files
    When running check
    Then 0 packages are checked
    And summary shows 0 passed, 0 failed, 2 skipped

  # =============================================================================
  # Changed Mode - Workspace-Level Changes
  # =============================================================================

  Scenario: Workspace-level changes trigger all packages
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files ["Cargo.toml"]
    When running check
    Then 2 packages are checked

  Scenario: CI config changes trigger all packages
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files [".github/workflows/ci.yml"]
    When running check
    Then 2 packages are checked

  # =============================================================================
  # Changed Mode - Validation
  # =============================================================================

  Scenario: Changed mode requires baseline rev
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And scope mode is "changed"
    And baseline rev is not set
    When running check
    Then error contains "baseline.rev"

  # =============================================================================
  # Combined with Filters
  # =============================================================================

  Scenario: Changed mode with include filter
    Given a workspace with packages:
      | name       | version | publishable | has_lib | path        |
      | my-lib-a   | 1.0.0   | true        | true    | crates/a    |
      | my-lib-b   | 1.0.0   | true        | true    | crates/b    |
      | other-pkg  | 1.0.0   | true        | true    | crates/other |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And config has include patterns ["my-lib-*"]
    And changed files ["crates/a/src/lib.rs", "crates/other/src/lib.rs"]
    When running check
    Then 1 packages are checked
    And package "my-lib-a" is checked
    And package "other-pkg" is skipped
