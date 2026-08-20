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
    And Pocket ID lists that client when searching for "{unique}"
    And "get_oidc_client" for that client agrees with Pocket ID
    And "list_oidc_clients" lists that client
    And get_oidc_client_metadata for that client reports its name and type
    And list_my_accessible_clients lists that client

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

  # Pocket ID's introspection endpoint authenticates with OAuth client
  # credentials, not with an API key, so the introspect_token tool cannot
  # succeed in this server's auth model. Pinned so the tool surface can be
  # documented accurately; if this starts passing, revisit the tool.
  Scenario: introspect_token is refused under API-key authentication
    When I introspect the token "not-a-real-token" through the tool
    Then the tool fails with status 401 and "unauthorized"

  Scenario: Previewing a client for a user reports that user's claims
    Given a confidential OIDC client "{unique}"
    And a user "{unique}-viewer"
    Then preview_oidc_client_for_user for that client and that user reports the user's claims

  Scenario: Restricting a client to a group is visible in Pocket ID
    Given a confidential OIDC client "{unique}"
    And a user group "{unique}-allowed"
    When I restrict that client to that group
    Then Pocket ID's record of that client lists that group as allowed
    When I lift the group restriction on that client
    Then Pocket ID's record of that client lists no allowed groups
    When I allow that group to use that client
    Then Pocket ID's record of that group lists that client as allowed

  Scenario: A client logo round-trips byte for byte and can be removed
    Given a confidential OIDC client "{unique}"
    When I upload "logo.png" as that client's logo
    Then Pocket ID serves that client's logo with exactly the bytes of "logo.png"
    And get_oidc_client_logo returns that client's logo with exactly the bytes of "logo.png"
    And Pocket ID's record of that client has:
      | hasLogo | true |
    When I delete that client's logo
    Then Pocket ID's record of that client has:
      | hasLogo | false |

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
    And get_client_api_access for that client agrees with Pocket ID
    And "get_api_definition" for that API definition agrees with Pocket ID
    And "list_api_definitions" lists that API definition

  Scenario: An API definition can be renamed and deleted
    Given an API definition "{unique}-api" for resource "https://api.example.com/{unique}"
    When I rename that API definition to "{unique}-api-renamed"
    Then Pocket ID's record of that API definition has:
      | name | {unique}-api-renamed |
    When I delete that API definition
    Then Pocket ID no longer has that API definition
