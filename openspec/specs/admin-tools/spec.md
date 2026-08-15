# admin-tools

## Purpose

MCP tools for administering a Pocket ID instance: application images and configuration, audit logs, API keys, SCIM service providers, and instance status.

## Requirements

### Requirement: Application image tools with enum collapsing
The server SHALL expose the 12 application-image operations as exactly three tools — `get_application_image`, `update_application_image`, `delete_application_image` — taking an `image_type` enum (`logo`, `favicon`, `background`, `email`, `default_profile_picture`) and an optional `light` boolean valid only for `logo`.

#### Scenario: Update the dark logo from a local file
- **WHEN** the assistant calls `update_application_image` with `image_type=logo`, `light=false`, and a `file_path`
- **THEN** the file is uploaded as multipart to `PUT /api/application-images/logo?light=false` and the tool reports success

#### Scenario: Light flag rejected for non-logo types
- **WHEN** `light` is supplied with `image_type=favicon`
- **THEN** the tool returns a validation error without calling the API

#### Scenario: Visual verification of an image
- **WHEN** the assistant calls `get_application_image` for any type
- **THEN** the tool result contains an MCP image content block with the image bytes and correct mime type

### Requirement: Application configuration tools
The server SHALL provide tools to read public configuration, read all configuration (admin), update configuration, trigger LDAP sync, and send a test email.

#### Scenario: Update a configuration value
- **WHEN** the assistant calls `update_application_configuration` with key/value updates
- **THEN** the API applies them and the tool returns the updated configuration

### Requirement: Audit log tools
The server SHALL provide `read`-tier tools to list the current user's audit logs and all audit logs (with the API's filtering and pagination parameters exposed as tool inputs), plus the client-name and user filter lookups.

#### Scenario: Investigate sign-in activity
- **WHEN** the assistant lists all audit logs filtered by a user and client name
- **THEN** matching audit entries are returned in structured form

### Requirement: API key tools
The server SHALL provide tools to list API keys, create an API key, renew an API key, and revoke an API key. Revocation is `dangerous` tier (it can sever the server's own access); creation and renewal are `write` tier.

#### Scenario: Rotate a key
- **WHEN** the assistant creates a new API key
- **THEN** the tool returns the key value with a warning that it is shown only once

### Requirement: SCIM service provider tools
The server SHALL provide tools to create, update, delete, and sync SCIM service providers, and to read a client's SCIM service provider.

#### Scenario: Trigger a SCIM sync
- **WHEN** the assistant calls `sync_scim_service_provider` with a provider ID
- **THEN** the API starts a sync and the tool returns the API's response

### Requirement: Instance status tools
The server SHALL provide `read`-tier tools for current version, latest available version, and health check.

#### Scenario: Update check
- **WHEN** the assistant calls the version tools
- **THEN** it can report whether the instance is up to date by comparing current and latest
