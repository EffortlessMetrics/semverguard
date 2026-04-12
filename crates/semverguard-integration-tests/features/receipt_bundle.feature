Feature: Receipt Bundle Artifacts
  As a CI system
  I want canonical receipt artifacts with raw evidence
  So that downstream tooling can consume deterministic outputs

  Background:
    Given config has skip_publish_false disabled
    And config has skip_no_lib disabled
    And scope mode is "workspace"
    And run mode is "release"
    And output format is "receipt"
    And warn_as_fail is disabled

  Scenario: Receipt bundle writes canonical artifact structure
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And all engines return "pass"
    And sarif output is enabled
    When running pipeline check
    And writing pipeline receipt bundle
    Then pipeline receipt write succeeds
    And receipt artifact "report.json" exists
    And receipt artifact "comment.md" exists
    And receipt artifact "raw" exists
    And receipt artifact "raw/core-1.0.0.stdout.log" exists
    And receipt artifact "raw/core-1.0.0.stderr.log" exists
    And receipt artifact "sarif.json" exists

  Scenario: Baseline warnings still emit raw logs for evidence
    Given a workspace with packages:
      | name | version | publishable | has_lib |
      | core | 1.0.0   | true        | true    |
    And run mode is "pr"
    And engine will return "baseline-fail" for "core"
    When running pipeline check
    And writing pipeline receipt bundle
    Then pipeline exit code is 0
    And pipeline receipt write succeeds
    And receipt artifact "raw/core-1.0.0.stdout.log" exists
    And receipt artifact "raw/core-1.0.0.stderr.log" exists
