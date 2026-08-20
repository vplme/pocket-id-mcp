@live @admin
Feature: Admin tools
  API keys, branding images, application configuration and status tools,
  verified against Pocket ID directly.

  # Pocket ID (2.13) refuses API-key authentication for key creation and
  # renewal (403 api_key_auth_not_allowed) — those need an admin session —
  # while listing and revocation are allowed. This pins that upstream
  # contract so the tool surface can be documented accurately; if creation
  # ever starts succeeding here, update the tool descriptions and README.
  Scenario: Pocket ID refuses API-key creation under API-key authentication
    Given an MCP server with default tiers
    When I try to create an API key "{unique}"
    Then the tool fails with status 403 and "not allowed"
    And Pocket ID has no API key named "{unique}"

  @needs-bootstrap
  Scenario: A revoked API key stops authenticating
    Given an MCP server with the dangerous tier enabled
    And a spare API key minted at bootstrap
    Then the spare API key appears in the tool's API key list
    And Pocket ID accepts the spare API key as a credential
    When I revoke the spare API key
    Then Pocket ID rejects the spare API key as a credential
    And Pocket ID no longer lists the spare API key

  Scenario: An uploaded logo is served back byte for byte
    Given an MCP server with default tiers
    When I upload "logo.png" as the dark-mode logo
    Then Pocket ID serves the dark-mode logo as image/png with exactly the bytes of "logo.png"
    And the get_application_image tool returns the dark-mode logo with exactly the bytes of "logo.png"

  Scenario: Application configuration changes persist and can be restored
    Given an MCP server with default tiers
    When I change the application name to "{unique}"
    Then Pocket ID's public configuration has appName "{unique}"
    When I restore the original application name
    Then Pocket ID's public configuration has the original appName

  Scenario: Status tools report the real instance
    Given a read-only MCP server
    Then get_current_version reports Pocket ID's own version
    And health_check succeeds
    And list_all_audit_logs returns a page
    And list_users finds the user that get_current_user reports
