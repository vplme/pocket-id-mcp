@live @server
Feature: Server behaviour against a real upstream
  Tier gating over the wire, upstream error mapping, and startup validation —
  things that only show against a real Pocket ID.

  Scenario: Read-only mode hides write tools and refuses them over the wire
    Given a read-only MCP server
    Then the server offers exactly the read-tier tools
    And calling "create_user" with username "{unique}" is refused by the protocol
    And Pocket ID has no user named "{unique}"

  Scenario: Upstream errors surface as tool errors carrying the status
    Given an MCP server with default tiers
    When I call "get_user" for user id "does-not-exist"
    Then the tool fails with status 404 and "not found"

  Scenario: Startup is refused with a bad API key
    When the server is started with API key "not-a-valid-key"
    Then it exits with an error mentioning "API key rejected"

  Scenario: Startup is refused when Pocket ID is unreachable
    When the server is started against "http://127.0.0.1:9"
    Then it exits with an error mentioning "cannot reach Pocket ID"
