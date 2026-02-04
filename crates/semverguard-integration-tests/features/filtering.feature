Feature: Package Filtering (Pass 1)
  As a developer
  I want to filter which packages are checked
  So that I can focus on relevant packages

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled

  # =============================================================================
  # Explicit Package Selection
  # =============================================================================

  Scenario: Explicit packages take precedence over globs
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | core   | 1.0.0   | true        | true    |
      | utils  | 1.0.0   | true        | true    |
      | helper | 0.1.0   | true        | true    |
    And config has include patterns ["core*"]
    And explicit packages ["utils"]
    When running check
    Then 1 packages are checked
    And package "utils" is checked

  Scenario: All explicit packages missing returns error
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And explicit packages ["nonexistent", "alsomissing"]
    When running check
    Then error contains "none of the specified packages"

  Scenario: Some explicit packages found succeeds
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | core  | 1.0.0   | true        | true    |
      | utils | 1.0.0   | true        | true    |
    And explicit packages ["core", "nonexistent"]
    When running check
    Then 1 packages are checked
    And package "core" is checked

  # =============================================================================
  # Include/Exclude Glob Patterns
  # =============================================================================

  Scenario: Include glob filters packages
    Given a workspace with packages:
      | name       | version | publishable | has_lib |
      | my-lib-a   | 1.0.0   | true        | true    |
      | my-lib-b   | 1.0.0   | true        | true    |
      | other-pkg  | 1.0.0   | true        | true    |
    And config has include patterns ["my-lib-*"]
    When running check
    Then 2 packages are checked
    And package "my-lib-a" is checked
    And package "my-lib-b" is checked
    And package "other-pkg" is skipped

  Scenario: Exclude glob filters packages
    Given a workspace with packages:
      | name        | version | publishable | has_lib |
      | core        | 1.0.0   | true        | true    |
      | core-test   | 0.1.0   | true        | true    |
      | utils       | 1.0.0   | true        | true    |
    And config has exclude patterns ["*-test"]
    When running check
    Then 2 packages are checked
    And package "core" is checked
    And package "utils" is checked
    And package "core-test" is skipped

  Scenario: Include and exclude combined
    Given a workspace with packages:
      | name       | version | publishable | has_lib |
      | lib-core   | 1.0.0   | true        | true    |
      | lib-test   | 1.0.0   | true        | true    |
      | other      | 1.0.0   | true        | true    |
    And config has include patterns ["lib-*"]
    And config has exclude patterns ["*-test"]
    When running check
    Then 1 packages are checked
    And package "lib-core" is checked
    And package "lib-test" is skipped
    And package "other" is skipped

  # =============================================================================
  # Publishable Filter
  # =============================================================================

  Scenario: Skip non-publishable packages
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | my-lib   | 1.0.0   | true        | true    |
      | internal | 0.1.0   | false       | true    |
    And config has skip_publish_false enabled
    When running check
    Then package "my-lib" is checked
    And package "internal" is skipped with reason "publish = false"

  Scenario: Include non-publishable when filter disabled
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | my-lib   | 1.0.0   | true        | true    |
      | internal | 0.1.0   | false       | true    |
    And config has skip_publish_false disabled
    When running check
    Then 2 packages are checked
    And package "my-lib" is checked
    And package "internal" is checked

  # =============================================================================
  # Library Target Filter
  # =============================================================================

  Scenario: Skip binary-only packages
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | my-lib | 1.0.0   | true        | true    |
      | my-cli | 1.0.0   | true        | false   |
    And config has skip_no_lib enabled
    When running check
    Then package "my-lib" is checked
    And package "my-cli" is skipped with reason "no library target"

  Scenario: Include binary-only when filter disabled
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | my-lib | 1.0.0   | true        | true    |
      | my-cli | 1.0.0   | true        | false   |
    And config has skip_no_lib disabled
    When running check
    Then 2 packages are checked
    And package "my-lib" is checked
    And package "my-cli" is checked

  # =============================================================================
  # Combined Filters
  # =============================================================================

  Scenario: Multiple filters combined
    Given a workspace with packages:
      | name          | version | publishable | has_lib |
      | lib-core      | 1.0.0   | true        | true    |
      | lib-internal  | 0.1.0   | false       | true    |
      | lib-cli       | 1.0.0   | true        | false   |
      | lib-test      | 0.1.0   | true        | true    |
      | other-pkg     | 1.0.0   | true        | true    |
    And config has include patterns ["lib-*"]
    And config has exclude patterns ["*-test"]
    And config has skip_publish_false enabled
    And config has skip_no_lib enabled
    When running check
    Then 1 packages are checked
    And package "lib-core" is checked
    And package "lib-internal" is skipped
    And package "lib-cli" is skipped
    And package "lib-test" is skipped
    And package "other-pkg" is skipped
