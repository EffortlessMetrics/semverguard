Feature: Mode And Failure Taxonomy
  As a CI operator
  I want mode-aware baseline handling and stable exit-code taxonomy
  So that PR and release lanes gate correctly

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled
    And scope mode is "workspace"
    And output format is "text"
    And warn_as_fail is disabled

  Scenario: Release mode treats baseline failures as hard failures
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "release"
    And engine will return "baseline-fail" for "core"
    When running pipeline check
    Then pipeline exit code is 3
    And pipeline report summary shows 0 passed, 1 failed, 0 skipped
    And package "core" in pipeline report failed
    And package "core" in pipeline report has failure kind "baseline-error"
    And pipeline receipt verdict is "warn"

  Scenario: PR mode downgrades baseline failures to warnings
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "pr"
    And engine will return "baseline-fail" for "core"
    When running pipeline check
    Then pipeline exit code is 0
    And pipeline report summary shows 0 passed, 0 failed, 1 skipped
    And package "core" in pipeline report is skipped with reason "baseline warning"
    And package "core" in pipeline report has failure kind "baseline-error"
    And pipeline receipt verdict is "warn"

  Scenario: warn_as_fail upgrades baseline warnings to exit 3
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "pr"
    And warn_as_fail is enabled
    And engine will return "baseline-fail" for "core"
    When running pipeline check
    Then pipeline exit code is 3
    And pipeline report summary shows 0 passed, 0 failed, 1 skipped
    And package "core" in pipeline report is skipped with reason "baseline warning"
    And package "core" in pipeline report has failure kind "baseline-error"
    And pipeline receipt verdict is "warn"

  Scenario: Baseline missing revision can be configured to skip
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "release"
    And baseline missing_revision action is "skip"
    And engine will return "baseline-fail" for "core"
    When running pipeline check
    Then pipeline exit code is 0
    And pipeline report summary shows 0 passed, 0 failed, 1 skipped
    And package "core" in pipeline report is skipped with reason "baseline skipped"
    And package "core" in pipeline report has no failure kind
    And pipeline receipt verdict is "skip"

  Scenario: Semver violations use exit code 2
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "release"
    And engine will return "fail" for "core"
    When running pipeline check
    Then pipeline exit code is 2
    And pipeline report summary shows 0 passed, 1 failed, 0 skipped
    And package "core" in pipeline report failed
    And package "core" in pipeline report has failure kind "semver-violation"
    And pipeline receipt verdict is "fail"

  Scenario: Tool errors use exit code 1
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "release"
    And engine will return error "engine crashed" for "core"
    When running pipeline check
    Then pipeline exit code is 1
    And pipeline report summary shows 0 passed, 1 failed, 0 skipped
    And package "core" in pipeline report failed
    And package "core" in pipeline report has failure kind "tool-error"
    And pipeline receipt verdict is "fail"
