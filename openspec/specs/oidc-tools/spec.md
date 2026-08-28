# oidc-tools

## Purpose

MCP tools for Pocket ID's OIDC surface: client management, token introspection, authorized-client visibility, and client API access configuration.

## Requirements

### Requirement: OIDC client management tools
The server SHALL provide tools for OIDC clients: list, get, create, update, delete, list/create/delete client secrets, set allowed user groups, get client metadata, refresh client metadata document, preview client data for a user, and client logo get/update/delete. Client deletion and secret creation/deletion are `write` tier (creating a secret leaves existing secrets usable; deleting one makes it immediately unusable); secret listing is `read` tier and never discloses secret values; client logo upload follows the shared upload input convention (`file_path` or `url`).

#### Scenario: Create client and retrieve secret
- **WHEN** the assistant creates an OIDC client and then calls `create_oidc_client_secret`
- **THEN** the freshly created secret is returned, with the tool description warning that the secret is shown only once, and previously created secrets remain usable

#### Scenario: Restrict client to groups
- **WHEN** the assistant calls `update_oidc_client_allowed_groups` with group IDs
- **THEN** only members of those groups may authorize with that client thereafter

### Requirement: Token introspection tool
The server SHALL provide a `read`-tier tool wrapping `POST /api/oidc/introspect` to inspect a token's validity and claims.

#### Scenario: Inspect an access token
- **WHEN** the assistant submits a token string
- **THEN** the introspection response (active flag, claims) is returned as structured JSON

### Requirement: Authorized-client visibility tools
The server SHALL provide tools to list a user's authorized clients, the current user's authorized clients and accessible clients, and to revoke the current user's authorization for a client.

#### Scenario: Review a user's grants
- **WHEN** the assistant calls `list_user_authorized_clients` with a user ID
- **THEN** the clients that user has authorized are returned

### Requirement: Client API access tools
The server SHALL provide tools over Pocket ID's per-API grant model: list the APIs a client may access (with its grant split into client, user-delegated, and metadata-document access), list APIs still assignable to a client, list an API's clients and its assignable clients, update one client's grant on one API, revoke that grant, and update an API's metadata-document (CIMD) client access — plus tools to manage API definitions (list, get, create, update, delete) and their permissions.

#### Scenario: Grant a client API permissions
- **WHEN** the assistant updates a client's grant on an API with access flags and permission IDs
- **THEN** the API stores the client-permission and user-delegated-permission sets as provided, leaving the client's grants on other APIs untouched
