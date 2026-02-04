Feature: Config Validation
  As a developer
  I want clear error messages for invalid configuration
  So that I can fix issues quickly

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled

  # =============================================================================
  # Baseline Configuration Errors
  # =============================================================================

  Scenario: Changed mode without baseline rev fails
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And scope mode is "changed"
    And baseline rev is not set
    When validating config
    Then error contains "baseline.rev"

  # =============================================================================
  # Explicit Package Errors
  # =============================================================================

  Scenario: All explicit packages not found
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And explicit packages ["missing-a", "missing-b"]
    When validating config
    Then error contains "none of the specified packages"

  # =============================================================================
  # Valid Configuration
  # =============================================================================

  Scenario: Valid workspace mode config
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And scope mode is "workspace"
    When validating config
    Then run succeeds

  Scenario: Valid changed mode config
    Given a workspace with packages:
      | name | version | publishable | has_lib | path   |
      | core | 1.0.0   | true        | true    | crates/core |
    And scope mode is "changed"
    And baseline rev is "origin/main"
    And changed files ["crates/core/src/lib.rs"]
    When validating config
    Then run succeeds
