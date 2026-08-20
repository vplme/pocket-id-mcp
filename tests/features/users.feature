@live @users
Feature: User tools
  Every mutation made through a tool must be visible in Pocket ID itself —
  read back over its REST API, never through the server's own client.

  Background:
    Given an MCP server with default tiers

  Scenario: Creating a user through the tool stores it in Pocket ID
    When I create a user "{unique}" with:
      | email     | {unique}@example.com |
      | firstName | Live                 |
      | lastName  | Tester               |
    Then Pocket ID's record of that user has:
      | username  | {unique}             |
      | email     | {unique}@example.com |
      | firstName | Live                 |
      | lastName  | Tester               |
      | isAdmin   | false                |
    And that user appears when Pocket ID lists users matching "{unique}"

  Scenario: Updating a user persists every field
    Given a user "{unique}"
    When I update that user with:
      | username  | {unique}                 |
      | email     | {unique}-new@example.com |
      | firstName | Updated                  |
      | lastName  | Person                   |
      | disabled  | true                     |
    Then Pocket ID's record of that user has:
      | email     | {unique}-new@example.com |
      | firstName | Updated                  |
      | lastName  | Person                   |
      | disabled  | true                     |

  Scenario: Group membership set on the user is visible from both sides
    Given a user "{unique}"
    And a user group "{unique}-members"
    When I put that user in that group
    Then Pocket ID lists that group among that user's groups
    And Pocket ID lists that user among that group's members

  Scenario: Custom claims set through the tool are stored on the user
    Given a user "{unique}"
    When I set that user's custom claims to:
      | department | qa   |
      | tier       | gold |
    Then Pocket ID's record of that user has custom claims:
      | department | qa   |
      | tier       | gold |

  Scenario: Deleting a user needs the dangerous tier
    Given a user "{unique}"
    Then the server does not offer "delete_user"
    And calling "delete_user" on that user is refused by the protocol
    And Pocket ID still has that user
    Given an MCP server with the dangerous tier enabled
    When I delete that user
    Then Pocket ID no longer has that user
