# identity-tools

## Purpose

MCP tools for identity management in Pocket ID: users, groups, custom claims, passkeys, and onboarding/access-recovery flows.

## Requirements

### Requirement: User management tools
The server SHALL provide tools covering user operations: list users (with pagination/search passthrough), get user by ID, get current user, create user, update user (by ID and `me`), delete user, and profile-picture get/update/reset (by ID and `me`). Deletion is `dangerous` tier; other mutations are `write` tier.

#### Scenario: List and inspect users
- **WHEN** the assistant calls `list_users` then `get_user` with a returned ID
- **THEN** the tools return the API's user records as structured JSON text content

#### Scenario: Delete user is gated
- **WHEN** the server runs with default safety configuration
- **THEN** `delete_user` is not registered

### Requirement: Group management tools
The server SHALL provide tools for user groups: list, get, create, update, delete, set group members, and set a user's group memberships.

#### Scenario: Add user to group
- **WHEN** the assistant updates a group's member list with an additional user ID
- **THEN** the API receives the full updated membership and the tool returns the resulting group

### Requirement: Custom claims tools
The server SHALL provide tools to fetch custom-claim suggestions and to set custom claims for a user or a user group.

#### Scenario: Set claims on a group
- **WHEN** the assistant calls `update_group_custom_claims` with claim key/value pairs
- **THEN** the claims are replaced on that group and returned

### Requirement: Passkey management tools
The server SHALL provide tools to list a user's WebAuthn credentials and to delete a specific credential. Deletion is `dangerous` tier.

#### Scenario: Audit a user's passkeys
- **WHEN** the assistant calls `list_user_passkeys` for a user ID
- **THEN** credential metadata (name, creation date, ID) is returned; no private key material is ever exposed by the API or tool

### Requirement: Onboarding and access-recovery tools
The server SHALL provide tools for signup tokens (list, create, delete) and one-time access (admin token minting, admin email trigger). Signup-token creation, one-time access token minting, and one-time access email triggering are `dangerous` tier; listing is `read` tier.

#### Scenario: One-time access token minting is opt-in
- **WHEN** `POCKET_ID_MCP_ALLOW_DANGEROUS` is not enabled
- **THEN** no tool capable of minting a login credential for another user is registered
