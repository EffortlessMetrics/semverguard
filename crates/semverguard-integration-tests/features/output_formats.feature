Feature: Output and Reporting
  As a CI system
  I want structured output for automation
  So that I can integrate with other tools

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled
    And scope mode is "workspace"

  # =============================================================================
  # Summary Output
  # =============================================================================

  Scenario: Run returns accurate summary
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 2.0.0   | true        | true    |
    And all engines return "pass"
    When running check
    Then summary total is 2
    And summary shows 2 passed, 0 failed, 0 skipped

  Scenario: Summary with mixed results
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | passing  | 1.0.0   | true        | true    |
      | failing  | 1.0.0   | true        | true    |
      | skipped  | 0.1.0   | false       | true    |
    And config has skip_publish_false enabled
    And engine will return "pass" for "passing"
    And engine will return "fail" for "failing"
    When running check
    Then summary total is 3
    And summary shows 1 passed, 1 failed, 1 skipped

  # =============================================================================
  # Package Details
  # =============================================================================

  Scenario: Passed package has correct status
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | my-lib | 1.2.3   | true        | true    |
    And all engines return "pass"
    When running check
    Then package "my-lib" passed

  Scenario: Failed package has correct status
    Given a workspace with packages:
      | name   | version | publishable | has_lib |
      | my-lib | 1.0.0   | true        | true    |
    And engine will return "fail" for "my-lib"
    When running check
    Then package "my-lib" failed

  Scenario: Skipped package has reason
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | internal | 0.1.0   | false       | true    |
    And config has skip_publish_false enabled
    When running check
    Then package "internal" is skipped with reason "publish = false"

  # =============================================================================
  # Edge Cases
  # =============================================================================

  Scenario: All packages skipped
    Given a workspace with packages:
      | name     | version | publishable | has_lib |
      | internal | 0.1.0   | false       | true    |
      | cli-only | 0.1.0   | true        | false   |
    And config has skip_publish_false enabled
    And config has skip_no_lib enabled
    When running check
    Then summary shows 0 passed, 0 failed, 2 skipped

  Scenario: All packages fail
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
    And all engines return "fail"
    When running check
    Then summary shows 0 passed, 2 failed, 0 skipped
