Feature: List Packages Command
  As a developer
  I want to preview which packages would be checked
  So that I can verify my configuration

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled
    And scope mode is "workspace"

  # =============================================================================
  # Basic Listing
  # =============================================================================

  Scenario: List all packages in workspace mode
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 2.0.0   | true        | true    |
    When running list
    Then would_check has 2 packages
    And would_check contains "pkg-a"
    And would_check contains "pkg-b"
    And would_skip has 0 packages

  Scenario: List empty workspace
    Given an empty workspace
    When running list
    Then would_check has 0 packages
    And would_skip has 0 packages

  # =============================================================================
  # List with Filters
  # =============================================================================

  Scenario: List shows would-skip for publish=false
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | my-lib   | 1.0.0   | true        | true    |
      | internal | 0.1.0   | false       | true    |
    And config has skip_publish_false enabled
    When running list
    Then would_check contains "my-lib"
    And would_skip contains "internal" with reason "publish = false"

  Scenario: List shows would-skip for no library target
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | my-lib | 1.0.0   | true        | true    |
      | my-cli | 1.0.0   | true        | false   |
    And config has skip_no_lib enabled
    When running list
    Then would_check contains "my-lib"
    And would_skip contains "my-cli" with reason "no library target"

  Scenario: List with include filter
    Given a workspace with packages:
      | name       | version | publishable | has_lib |
      | my-lib-a   | 1.0.0   | true        | true    |
      | my-lib-b   | 1.0.0   | true        | true    |
      | other-pkg  | 1.0.0   | true        | true    |
    And config has include patterns ["my-lib-*"]
    When running list
    Then would_check has 2 packages
    And would_check contains "my-lib-a"
    And would_check contains "my-lib-b"
    And would_skip contains "other-pkg"

  Scenario: List with exclude filter
    Given a workspace with packages:
      | name       | version | publishable | has_lib |
      | core       | 1.0.0   | true        | true    |
      | core-test  | 0.1.0   | true        | true    |
    And config has exclude patterns ["*-test"]
    When running list
    Then would_check contains "core"
    And would_skip contains "core-test"

  # =============================================================================
  # List in Changed Mode
  # =============================================================================

  Scenario: List in changed mode shows unchanged as skipped
    Given a workspace with packages:
      | name  | version | publishable | has_lib | path     |
      | pkg-a | 1.0.0   | true        | true    | crates/a |
      | pkg-b | 1.0.0   | true        | true    | crates/b |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files ["crates/a/src/lib.rs"]
    When running list
    Then would_check has 1 packages
    And would_check contains "pkg-a"
    And would_skip contains "pkg-b"

  # =============================================================================
  # Combined Filters
  # =============================================================================

  Scenario: List with multiple skip reasons
    Given a workspace with packages:
      | name          | version | publishable | has_lib |
      | lib-core      | 1.0.0   | true        | true    |
      | lib-internal  | 0.1.0   | false       | true    |
      | lib-cli       | 1.0.0   | true        | false   |
      | other-pkg     | 1.0.0   | true        | true    |
    And config has include patterns ["lib-*"]
    And config has skip_publish_false enabled
    And config has skip_no_lib enabled
    When running list
    Then would_check has 1 packages
    And would_check contains "lib-core"
    And would_skip has 3 packages
