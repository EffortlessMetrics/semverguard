Feature: Engine Execution (Pass 3)
  As a CI system
  I want consistent execution behavior
  So that I can rely on exit codes

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled
    And scope mode is "workspace"

  # =============================================================================
  # Basic Execution
  # =============================================================================

  Scenario: All packages pass
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
    And all engines return "pass"
    When running check
    Then summary shows 2 passed, 0 failed, 0 skipped

  Scenario: Some packages fail
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
    And engine will return "pass" for "pkg-a"
    And engine will return "fail" for "pkg-b"
    When running check
    Then summary shows 1 passed, 1 failed, 0 skipped
    And package "pkg-a" passed
    And package "pkg-b" failed

  # =============================================================================
  # Fail-Fast Behavior
  # =============================================================================

  Scenario: Fail-fast stops on first failure
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
      | pkg-c | 1.0.0   | true        | true    |
    And fail_fast is enabled
    And engine will return "pass" for "pkg-a"
    And engine will return "fail" for "pkg-b"
    And engine will return "pass" for "pkg-c"
    When running check
    Then 2 packages are in results
    And summary shows 1 passed, 1 failed, 0 skipped

  Scenario: Continue after failure without fail-fast
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
      | pkg-c | 1.0.0   | true        | true    |
    And fail_fast is disabled
    And engine will return "pass" for "pkg-a"
    And engine will return "fail" for "pkg-b"
    And engine will return "pass" for "pkg-c"
    When running check
    Then 3 packages are in results
    And summary shows 2 passed, 1 failed, 0 skipped

  Scenario: Fail-fast on first package failure
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
    And fail_fast is enabled
    And engine will return "fail" for "pkg-a"
    And engine will return "pass" for "pkg-b"
    When running check
    Then 1 packages are in results
    And package "pkg-a" failed

  # =============================================================================
  # Mixed Results
  # =============================================================================

  Scenario: Mixed pass, fail, and skipped
    Given a workspace with packages:
      | name      | version | publishable | has_lib |
      | passing   | 1.0.0   | true        | true    |
      | failing   | 1.0.0   | true        | true    |
      | skipped   | 1.0.0   | false       | true    |
    And config has skip_publish_false enabled
    And engine will return "pass" for "passing"
    And engine will return "fail" for "failing"
    When running check
    Then summary shows 1 passed, 1 failed, 1 skipped

  # =============================================================================
  # Summary Accuracy
  # =============================================================================

  Scenario: Summary counts are accurate
    Given a workspace with packages:
      | name  | version | publishable | has_lib |
      | pkg-a | 1.0.0   | true        | true    |
      | pkg-b | 1.0.0   | true        | true    |
      | pkg-c | 1.0.0   | true        | true    |
      | pkg-d | 1.0.0   | false       | true    |
      | pkg-e | 1.0.0   | true        | false   |
    And config has skip_publish_false enabled
    And config has skip_no_lib enabled
    And engine will return "pass" for "pkg-a"
    And engine will return "pass" for "pkg-b"
    And engine will return "fail" for "pkg-c"
    When running check
    Then summary total is 5
    And summary shows 2 passed, 1 failed, 2 skipped
