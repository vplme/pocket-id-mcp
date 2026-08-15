# spec-drift-ci

## Purpose

CI automation that keeps the vendored Pocket ID OpenAPI spec honest: scheduled upstream drift detection and a build/test workflow that enforces spec coverage.

## Requirements

### Requirement: Scheduled upstream spec diff
A CI workflow SHALL run on a weekly schedule and on manual dispatch, download the current upstream `swagger.yaml` from pocket-id.org, and compare its operation set (path + method + parameters) against the vendored copy.

#### Scenario: Upstream adds an endpoint
- **WHEN** the scheduled workflow finds operations upstream that are absent from the vendored spec
- **THEN** it opens (or updates a single existing) GitHub issue listing the added, removed, and changed operations

#### Scenario: No drift
- **WHEN** the operation sets match
- **THEN** the workflow succeeds silently without creating issues

### Requirement: Build and test workflow
A CI workflow SHALL build the crate and run the test suite (including the spec coverage test) on every push and pull request to `main`.

#### Scenario: Unmapped operation blocks merge
- **WHEN** a PR updates the vendored spec without reconciling the tool surface or exclusion list
- **THEN** the coverage test fails and the workflow reports failure on the PR
