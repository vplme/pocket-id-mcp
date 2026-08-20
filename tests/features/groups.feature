@live @groups
Feature: User-group tools
  Every mutation made through a tool must be visible in Pocket ID itself —
  read back over its REST API, never through the server's own client.

  Background:
    Given an MCP server with default tiers

  Scenario: Creating a group through the tool stores it in Pocket ID
    When I create a user group "{unique}" with friendly name "Live Group"
    Then Pocket ID's record of that group has:
      | name         | {unique}   |
      | friendlyName | Live Group |
    And that group appears when Pocket ID lists groups matching "{unique}"

  Scenario: Updating a group persists
    Given a user group "{unique}"
    When I update that group with:
      | name         | {unique}-renamed |
      | friendlyName | Renamed Group    |
    Then Pocket ID's record of that group has:
      | name         | {unique}-renamed |
      | friendlyName | Renamed Group    |

  Scenario: Members set on the group are visible from both sides, and clearing removes them
    Given a user group "{unique}"
    And a user "{unique}-member"
    When I set that group's members to that user
    Then Pocket ID lists that user among that group's members
    And Pocket ID lists that group among that user's groups
    When I clear that group's members
    Then Pocket ID lists no members for that group

  Scenario: Custom claims set through the tool are stored on the group
    Given a user group "{unique}"
    When I set that group's custom claims to:
      | cost_center | 4711 |
    Then Pocket ID's record of that group has custom claims:
      | cost_center | 4711 |

  Scenario: Deleting a group removes it from Pocket ID
    Given a user group "{unique}"
    When I delete that group
    Then Pocket ID no longer has that group
