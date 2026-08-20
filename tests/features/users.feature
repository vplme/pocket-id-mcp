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
    And Pocket ID lists that user when searching for "{unique}"
    And "get_user" for that user agrees with Pocket ID
    And "list_users" lists that user
    And list_user_passkeys reports no passkeys for that user

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
    And list_user_groups_of_user lists that group for that user

  Scenario: Custom claims set through the tool are stored on the user
    Given a user "{unique}"
    When I set that user's custom claims to:
      | department | qa   |
      | tier       | gold |
    Then Pocket ID's record of that user has custom claims:
      | department | qa   |
      | tier       | gold |
    And get_custom_claim_suggestions includes "department"

  Scenario: A user's profile picture can be replaced and reset
    Given a user "{unique}"
    When I upload "logo.png" as that user's profile picture
    Then Pocket ID serves a different profile picture for that user than before
    And get_user_profile_picture returns the picture Pocket ID serves for that user
    When I reset that user's profile picture
    Then Pocket ID serves that user's default profile picture again

  Scenario: The current user can be updated through the me-tools
    Given that user is the current user
    When I change the current user's first name to "{unique}"
    Then Pocket ID's record of that user has:
      | firstName | {unique} |
    When I restore the current user's first name
    Then Pocket ID's record of that user has its original first name
    When I upload "logo.png" as the current user's profile picture
    Then Pocket ID serves a different profile picture for that user than before
    When I reset the current user's profile picture
    Then Pocket ID serves that user's default profile picture again

  Scenario: Deleting a user needs the dangerous tier
    Given a user "{unique}"
    Then the server does not offer "delete_user"
    And calling "delete_user" on that user is refused by the protocol
    And Pocket ID still has that user
    Given an MCP server with the dangerous tier enabled
    When I delete that user
    Then Pocket ID no longer has that user

  Scenario: A one-time access token minted for a user can be redeemed once
    Given an MCP server with the dangerous tier enabled
    And a user "{unique}"
    When I mint a one-time access token for that user
    Then Pocket ID redeems that token exactly once

  Scenario: Signup tokens are created, listed and deleted
    Given an MCP server with the dangerous tier enabled
    When I create a signup token valid for "1h" with usage limit 1
    Then list_signup_tokens lists that signup token
    And Pocket ID lists that signup token
    When I delete that signup token
    Then Pocket ID no longer lists that signup token
