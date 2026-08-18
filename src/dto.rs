//! Data transfer objects for the Pocket ID REST API.
//!
//! Response types keep every field optional so newer Pocket ID releases that add
//! fields (or omit ones) never break deserialization. Request types mirror the
//! required/optional split of the vendored swagger spec.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MCP envelope
// ---------------------------------------------------------------------------

/// MCP requires `outputSchema` and `structuredContent` to be object-rooted;
/// tools whose natural result is an array or freeform value wrap it as
/// `{"result": ...}` — the same convention the official Python SDK uses.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Enveloped<T> {
    pub result: T,
}

/// Wrap a value for tool return: `.map(enveloped)` mirrors `.map(Json)`.
pub fn enveloped<T>(result: T) -> rmcp::handler::server::wrapper::Json<Enveloped<T>> {
    rmcp::handler::server::wrapper::Json(Enveloped { result })
}

/// Freeform JSON whose declared schema is the unconstrained object `{}`.
/// schemars emits the boolean schema `true` for `serde_json::Value`, but the
/// MCP `Tool` type requires every schema property value to be an object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnyJson(pub serde_json::Value);

impl JsonSchema for AnyJson {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnyJson".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({})
    }

    fn inline_schema() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub current_page: Option<i64>,
    pub items_per_page: Option<i64>,
    pub total_items: Option<i64>,
    pub total_pages: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Paginated<T> {
    pub data: Option<Vec<T>>,
    pub pagination: Option<Pagination>,
}

/// Common list-endpoint inputs, flattened into tool parameters.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListParams {
    /// Page number, starting at 1.
    pub page: Option<i64>,
    /// Items per page.
    pub limit: Option<i64>,
    /// Column to sort by.
    pub sort_column: Option<String>,
    /// Sort direction: "asc" or "desc".
    pub sort_direction: Option<String>,
}

impl ListParams {
    pub fn to_query(&self) -> Vec<(String, String)> {
        let mut q = Vec::new();
        if let Some(page) = self.page {
            q.push(("pagination[page]".to_string(), page.to_string()));
        }
        if let Some(limit) = self.limit {
            q.push(("pagination[limit]".to_string(), limit.to_string()));
        }
        if let Some(col) = &self.sort_column {
            q.push(("sort[column]".to_string(), col.clone()));
        }
        if let Some(dir) = &self.sort_direction {
            q.push(("sort[direction]".to_string(), dir.clone()));
        }
        q
    }
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomClaim {
    pub key: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomClaimInput {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroupMinimal {
    pub id: Option<String>,
    pub name: Option<String>,
    pub friendly_name: Option<String>,
    pub created_at: Option<String>,
    pub ldap_id: Option<String>,
    pub user_count: Option<i64>,
    pub custom_claims: Option<Vec<CustomClaim>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: Option<String>,
    pub is_admin: Option<bool>,
    pub disabled: Option<bool>,
    pub locale: Option<String>,
    pub ldap_id: Option<String>,
    pub custom_claims: Option<Vec<CustomClaim>>,
    pub user_groups: Option<Vec<UserGroupMinimal>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserInput {
    /// Username (required, max 50 chars).
    pub username: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: Option<String>,
    pub is_admin: Option<bool>,
    pub disabled: Option<bool>,
    pub locale: Option<String>,
    /// Optional explicit user ID.
    pub id: Option<String>,
    pub user_group_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebauthnCredential {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "credentialID")]
    pub credential_id: Option<String>,
    pub attestation_type: Option<String>,
    pub transport: Option<Vec<String>>,
    pub backup_eligible: Option<bool>,
    pub backup_state: Option<bool>,
    pub created_at: Option<String>,
}

// ---------------------------------------------------------------------------
// User groups
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroup {
    pub id: Option<String>,
    pub name: Option<String>,
    pub friendly_name: Option<String>,
    pub created_at: Option<String>,
    pub ldap_id: Option<String>,
    pub custom_claims: Option<Vec<CustomClaim>>,
    pub users: Option<Vec<User>>,
    pub allowed_oidc_clients: Option<Vec<OidcClientMetaData>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroupInput {
    /// Technical name (2–255 chars), used as the claim value.
    pub name: String,
    /// Human-readable name (2–50 chars).
    pub friendly_name: String,
}

// ---------------------------------------------------------------------------
// OIDC clients
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientFederatedIdentity {
    pub issuer: Option<String>,
    pub subject: Option<String>,
    pub audience: Option<String>,
    pub jwks: Option<String>,
    pub replay_protection: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientCredentials {
    pub federated_identities: Option<Vec<OidcClientFederatedIdentity>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClient {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub is_public: Option<bool>,
    pub is_group_restricted: Option<bool>,
    pub pkce_enabled: Option<bool>,
    pub pkce_supported: Option<bool>,
    pub requires_pushed_authorization_requests: Option<bool>,
    pub requires_reauthentication: Option<bool>,
    pub skip_consent: Option<bool>,
    #[serde(rename = "callbackURLs")]
    pub callback_urls: Option<Vec<String>>,
    #[serde(rename = "logoutCallbackURLs")]
    pub logout_callback_urls: Option<Vec<String>>,
    #[serde(rename = "launchURL")]
    pub launch_url: Option<String>,
    pub has_logo: Option<bool>,
    pub has_dark_logo: Option<bool>,
    pub access_token_duration_minutes: Option<i64>,
    pub refresh_token_duration_minutes: Option<i64>,
    pub credentials: Option<OidcClientCredentials>,
    pub allowed_user_groups_count: Option<i64>,
    pub allowed_user_groups: Option<Vec<UserGroupMinimal>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientInput {
    /// Client display name (required, max 50 chars).
    pub name: String,
    pub description: Option<String>,
    /// Optional explicit client ID (2–128 chars); generated when omitted.
    pub id: Option<String>,
    /// Public client (no secret, PKCE required).
    pub is_public: Option<bool>,
    pub is_group_restricted: Option<bool>,
    pub pkce_enabled: Option<bool>,
    pub requires_pushed_authorization_requests: Option<bool>,
    pub requires_reauthentication: Option<bool>,
    pub skip_consent: Option<bool>,
    #[serde(rename = "callbackURLs")]
    pub callback_urls: Option<Vec<String>>,
    #[serde(rename = "logoutCallbackURLs")]
    pub logout_callback_urls: Option<Vec<String>>,
    #[serde(rename = "launchURL", skip_serializing_if = "Option::is_none")]
    pub launch_url: Option<String>,
    /// URL to fetch the light-mode logo from (server-side).
    pub logo_url: Option<String>,
    pub dark_logo_url: Option<String>,
    pub has_logo: Option<bool>,
    pub has_dark_logo: Option<bool>,
    pub access_token_duration_minutes: Option<i64>,
    pub refresh_token_duration_minutes: Option<i64>,
    pub credentials: Option<OidcClientCredentials>,
}

/// Body for updating an OIDC client. Mirrors `OidcClientInput` minus `id`:
/// the upstream update DTO has no `id` field, and a value sent there is
/// silently ignored (clients cannot be renamed).
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientUpdateInput {
    /// Client display name (required, max 50 chars).
    pub name: String,
    pub description: Option<String>,
    /// Public client (no secret, PKCE required).
    pub is_public: Option<bool>,
    pub is_group_restricted: Option<bool>,
    pub pkce_enabled: Option<bool>,
    pub requires_pushed_authorization_requests: Option<bool>,
    pub requires_reauthentication: Option<bool>,
    pub skip_consent: Option<bool>,
    #[serde(rename = "callbackURLs")]
    pub callback_urls: Option<Vec<String>>,
    #[serde(rename = "logoutCallbackURLs")]
    pub logout_callback_urls: Option<Vec<String>>,
    #[serde(rename = "launchURL", skip_serializing_if = "Option::is_none")]
    pub launch_url: Option<String>,
    /// URL to fetch the light-mode logo from (server-side).
    pub logo_url: Option<String>,
    pub dark_logo_url: Option<String>,
    pub has_logo: Option<bool>,
    pub has_dark_logo: Option<bool>,
    pub access_token_duration_minutes: Option<i64>,
    pub refresh_token_duration_minutes: Option<i64>,
    pub credentials: Option<OidcClientCredentials>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientMetaData {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub has_logo: Option<bool>,
    pub has_dark_logo: Option<bool>,
    #[serde(rename = "launchURL")]
    pub launch_url: Option<String>,
    pub requires_reauthentication: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientSecret {
    pub secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientPreview {
    pub access_token: Option<AnyJson>,
    pub id_token: Option<AnyJson>,
    pub user_info: Option<AnyJson>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedOidcClient {
    pub client: Option<OidcClientMetaData>,
    pub scope: Option<String>,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccessibleOidcClient {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub has_logo: Option<bool>,
    pub has_dark_logo: Option<bool>,
    #[serde(rename = "launchURL")]
    pub launch_url: Option<String>,
    pub last_used_at: Option<String>,
    pub requires_reauthentication: Option<bool>,
}

// ---------------------------------------------------------------------------
// API definitions / permissions / client API access
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiPermission {
    pub id: Option<String>,
    pub key: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiPermissionInput {
    /// Permission key (required, max 128 chars).
    pub key: String,
    /// Display name (required, max 50 chars).
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiDefinition {
    pub id: Option<String>,
    pub name: Option<String>,
    pub resource: Option<String>,
    pub created_at: Option<String>,
    pub permissions: Option<Vec<ApiPermission>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientApiAccess {
    pub client_permission_ids: Option<Vec<String>>,
    pub user_delegated_permission_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientApiAccessInput {
    pub client_permission_ids: Vec<String>,
    pub user_delegated_permission_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub expiration_email_sent: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyResponse {
    pub api_key: Option<ApiKey>,
    /// The key value — shown only once, never retrievable again.
    pub token: Option<String>,
}

// ---------------------------------------------------------------------------
// Application configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigVariable {
    pub key: Option<String>,
    #[serde(rename = "type")]
    pub value_type: Option<String>,
    pub value: Option<String>,
    pub is_public: Option<bool>,
}

// ---------------------------------------------------------------------------
// Audit logs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLog {
    pub id: Option<String>,
    pub event: Option<String>,
    #[serde(rename = "userID")]
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub actor_username: Option<String>,
    pub ip_address: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub data: Option<serde_json::Value>,
    pub created_at: Option<String>,
}

// ---------------------------------------------------------------------------
// SCIM
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScimServiceProvider {
    pub id: Option<String>,
    pub endpoint: Option<String>,
    pub token: Option<String>,
    pub oidc_client: Option<OidcClientMetaData>,
    pub created_at: Option<String>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScimServiceProviderInput {
    /// SCIM base endpoint of the downstream provider (required).
    pub endpoint: String,
    /// OIDC client this provider is attached to (required).
    pub oidc_client_id: String,
    /// Bearer token used to authenticate against the SCIM endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Body for updating a SCIM service provider. Unlike the create input,
/// `token` is required: the server overwrites the stored token with whatever
/// the request carries, and an omitted or null token silently clears it.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScimServiceProviderUpdateInput {
    /// SCIM base endpoint of the downstream provider (required).
    pub endpoint: String,
    /// OIDC client this provider is attached to (required).
    pub oidc_client_id: String,
    /// Bearer token used to authenticate against the SCIM endpoint.
    /// Required on update: the server unconditionally stores what is sent,
    /// so resend the current token to keep it (empty string clears it).
    pub token: String,
}

// ---------------------------------------------------------------------------
// Signup tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignupToken {
    pub id: Option<String>,
    pub token: Option<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    pub usage_count: Option<i64>,
    pub usage_limit: Option<i64>,
    pub user_groups: Option<Vec<UserGroupMinimal>>,
}
