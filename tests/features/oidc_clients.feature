@live @oidc
Feature: OIDC client tools
  Every mutation made through a tool must be visible in Pocket ID itself —
  read back over its REST API, never through the server's own client.
  "{unique}" expands to a per-scenario unique token so scenarios can run in parallel.

  Background:
    Given an MCP server with default tiers

  Scenario: Creating a client through the tool stores it in Pocket ID
    When I create a confidential OIDC client "{unique}" with PKCE and callback "https://app.example.com/callback"
    Then Pocket ID has an OIDC client "{unique}" with PKCE enabled and callback "https://app.example.com/callback"
    And that client appears when Pocket ID lists clients matching "{unique}"

  Scenario: Updating a client persists every field
    Given a confidential OIDC client "{unique}"
    When I update that client with:
      | name               | {unique}-renamed               |
      | callbackURLs       | https://new.example.com/cb     |
      | logoutCallbackURLs | https://new.example.com/logout |
      | skipConsent        | true                           |
      | pkceEnabled        | true                           |
    Then Pocket ID's record of that client has:
      | name               | {unique}-renamed               |
      | callbackURLs       | https://new.example.com/cb     |
      | logoutCallbackURLs | https://new.example.com/logout |
      | skipConsent        | true                           |
      | pkceEnabled        | true                           |

  Scenario: A minted secret authenticates the client until it is rotated
    Given a confidential OIDC client "{unique}"
    When I set its secret to "{unique}-chosen-secret"
    Then Pocket ID accepts "{unique}-chosen-secret" as that client's credential
    But Pocket ID rejects "definitely-not-the-secret" as that client's credential
    When I rotate its secret
    Then Pocket ID accepts the new secret as that client's credential
    But Pocket ID rejects "{unique}-chosen-secret" as that client's credential

  Scenario: Restricting a client to a group is visible in Pocket ID
    Given a confidential OIDC client "{unique}"
    And a user group "{unique}-allowed"
    When I restrict that client to that group
    Then Pocket ID's record of that client lists that group as allowed
    When I lift the group restriction on that client
    Then Pocket ID's record of that client lists no allowed groups

  Scenario: Deleting a client removes it from Pocket ID
    Given a confidential OIDC client "{unique}"
    When I delete that client
    Then Pocket ID no longer has that client

  Scenario: API definition permissions can be delegated to a client
    Given a public OIDC client "{unique}"
    And an API definition "{unique}-api" for resource "https://api.example.com/{unique}"
    When I set that API definition's permissions to:
      | read | Read things |
    And I grant that client user-delegated access to permission "read"
    Then Pocket ID's record of that API definition has permission "read"
    And Pocket ID's API access for that client delegates permission "read"
