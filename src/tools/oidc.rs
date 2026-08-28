//! OIDC tools: clients, secrets, logos, grants, API access.

use reqwest::Method;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::client::{FileSource, NO_BODY};
use crate::dto::*;
use crate::server::{PocketIdServer, err_str};
use crate::tools::ApiResultExt;
use crate::tools::identity::SearchListParams;
use crate::tools::{client_seg, seg};

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientIdParam {
    /// OIDC client ID.
    pub client_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOidcClientParams {
    /// ID of the OIDC client to update.
    pub client_id: String,
    #[serde(flatten)]
    pub client: OidcClientUpdateInput,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateClientSecretParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Secret to set (min 16 chars). Omit to have the server generate a random one.
    pub secret: Option<String>,
    /// RFC 3339 time after which the secret becomes unusable. Omit for a secret that never expires.
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteClientSecretParams {
    /// OIDC client ID.
    pub client_id: String,
    /// ID of the secret to delete (from list_oidc_client_secrets).
    pub secret_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AllowedGroupsParams {
    /// ID of the OIDC client to restrict.
    pub client_id: String,
    /// Complete set of user-group IDs allowed to use this client.
    pub user_group_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewParams {
    /// OIDC client ID.
    pub client_id: String,
    /// User to preview tokens for.
    pub user_id: String,
    /// Space-separated scopes to include, e.g. "openid profile email groups".
    pub scopes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientLogoParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Light-mode logo variant when true (the API default when omitted); dark-mode when false.
    pub light: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateClientLogoParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Upload as the light-mode logo variant when true (the API default when omitted); dark-mode when false.
    pub light: Option<bool>,
    #[serde(flatten)]
    pub source: FileSource,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserAuthorizedClientsParams {
    /// User ID.
    pub user_id: String,
    /// Only clients with (true) or without (false) a launch URL; omit for all.
    pub has_launch_url: Option<bool>,
    #[serde(flatten)]
    pub list: ListParams,
}

/// List inputs for the current-user client listings, which support the
/// launch-URL filter on top of the common pagination.
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MyClientsListParams {
    /// Only clients with (true) or without (false) a launch URL; omit for all.
    pub has_launch_url: Option<bool>,
    #[serde(flatten)]
    pub list: ListParams,
}

/// Append the `filters[hasLaunchURL]` query parameter when the filter is set.
fn with_launch_url_filter(
    mut query: Vec<(String, String)>,
    has_launch_url: Option<bool>,
) -> Vec<(String, String)> {
    if let Some(v) = has_launch_url {
        query.push(("filters[hasLaunchURL]".to_string(), v.to_string()));
    }
    query
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateClientApiAccessParams {
    /// API definition ID.
    pub api_id: String,
    /// OIDC client ID.
    pub client_id: String,
    /// Whether the client itself may request tokens for this API (client
    /// credentials). Omitted means false upstream: access disabled.
    pub client_access: Option<bool>,
    /// Permission IDs the client itself may use (client credentials).
    pub client_permission_ids: Vec<String>,
    /// Whether users may delegate their access to this API to the client.
    /// Omitted means false upstream: access disabled.
    pub user_delegated_access: Option<bool>,
    /// Permission IDs users may delegate to the client.
    pub user_delegated_permission_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiClientParams {
    /// API definition ID.
    pub api_id: String,
    /// OIDC client ID.
    pub client_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateApiCimdAccessParams {
    /// API definition ID.
    pub api_id: String,
    /// Whether clients registered through a Client ID Metadata Document may
    /// access this API at all. Omitted means false upstream: access disabled.
    pub enabled: Option<bool>,
    /// Permission IDs every CIMD-registered client may request.
    pub permission_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientApiListParams {
    /// OIDC client ID.
    pub client_id: String,
    #[serde(flatten)]
    pub list: SearchListParams,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiDefinitionClientsParams {
    /// API definition ID.
    pub api_id: String,
    #[serde(flatten)]
    pub list: SearchListParams,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateApiDefinitionParams {
    /// Display name (max 50 chars).
    pub name: String,
    /// Resource identifier of the API (max 350 chars).
    pub resource: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiDefinitionIdParam {
    /// API definition ID.
    pub api_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateApiDefinitionParams {
    /// API definition ID.
    pub api_id: String,
    /// New display name.
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetApiPermissionsParams {
    /// API definition ID.
    pub api_id: String,
    /// Complete new permission set for this API.
    pub permissions: Vec<ApiPermissionInput>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupAllowedClientsParams {
    /// User group ID.
    pub group_id: String,
    /// Complete set of OIDC client IDs this group grants access to.
    pub oidc_client_ids: Vec<String>,
}

fn light_query(light: Option<bool>) -> Vec<(String, String)> {
    match light {
        Some(v) => vec![("light".to_string(), v.to_string())],
        None => vec![],
    }
}

// ---------------------------------------------------------------------------
// Read tier
// ---------------------------------------------------------------------------

#[tool_router(router = oidc_read_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(description = "List OIDC clients, with optional search, pagination, and sorting.")]
    pub async fn list_oidc_clients(
        &self,
        Parameters(p): Parameters<SearchListParams>,
    ) -> Result<Json<Paginated<OidcClient>>, String> {
        self.client
            .json(Method::GET, "/api/oidc/clients", &p.to_query(), NO_BODY)
            .await
            .tool_json()
    }

    #[tool(description = "Get an OIDC client by ID, including its allowed user groups.")]
    pub async fn get_oidc_client(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<OidcClient>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/oidc/clients/{}", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(description = "Get an OIDC client's public metadata (name, type, logo flags).")]
    pub async fn get_oidc_client_metadata(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<OidcClientMetaData>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/oidc/clients/{}/meta", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Preview the access token, ID token, and userinfo a given user would receive from an OIDC client — useful to verify claims and scopes without a real login."
    )]
    pub async fn preview_oidc_client_for_user(
        &self,
        Parameters(p): Parameters<PreviewParams>,
    ) -> Result<Json<OidcClientPreview>, String> {
        let query = match &p.scopes {
            Some(s) => vec![("scopes".to_string(), s.clone())],
            None => vec![],
        };
        self.client
            .json(
                Method::GET,
                &format!(
                    "/api/oidc/clients/{}/preview/{}",
                    client_seg(&p.client_id),
                    seg(&p.user_id)
                ),
                &query,
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(description = "Get an OIDC client's logo as an image for visual inspection.")]
    pub async fn get_oidc_client_logo(
        &self,
        Parameters(p): Parameters<ClientLogoParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .client
            .binary(
                &format!("/api/oidc/clients/{}/logo", client_seg(&p.client_id)),
                &light_query(p.light),
            )
            .await
        {
            Ok(bin) => self.binary_result(bin),
            Err(e) => Ok(Self::api_error_result(e)),
        }
    }

    #[tool(description = "List the OIDC clients a user has authorized (granted consent to).")]
    pub async fn list_user_authorized_clients(
        &self,
        Parameters(p): Parameters<UserAuthorizedClientsParams>,
    ) -> Result<Json<Paginated<AuthorizedOidcClient>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/oidc/users/{}/authorized-clients", seg(&p.user_id)),
                &with_launch_url_filter(p.list.to_query(), p.has_launch_url),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(description = "List the OIDC clients the current user has authorized.")]
    pub async fn list_my_authorized_clients(
        &self,
        Parameters(p): Parameters<MyClientsListParams>,
    ) -> Result<Json<Paginated<AuthorizedOidcClient>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/oidc/users/me/authorized-clients",
                &with_launch_url_filter(p.list.to_query(), p.has_launch_url),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(description = "List the OIDC clients the current user can access.")]
    pub async fn list_my_accessible_clients(
        &self,
        Parameters(p): Parameters<MyClientsListParams>,
    ) -> Result<Json<Paginated<AccessibleOidcClient>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/oidc/users/me/clients",
                &with_launch_url_filter(p.list.to_query(), p.has_launch_url),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "List the secrets of an OIDC client (metadata only — the values are never disclosed)."
    )]
    pub async fn list_oidc_client_secrets(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<Enveloped<Vec<OidcClientSecret>>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/oidc/clients/{}/secrets", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_enveloped()
    }

    #[tool(
        description = "List every API an OIDC client may access, with its grant split into client (machine-to-machine), user-delegated, and metadata-document access."
    )]
    pub async fn list_client_accessible_apis(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<Enveloped<Vec<ClientApiGrant>>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/api-access/{}/apis", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_enveloped()
    }

    #[tool(
        description = "List APIs an OIDC client cannot reach yet — candidates for update_client_api_access — with optional search, pagination, and sorting."
    )]
    pub async fn list_client_assignable_apis(
        &self,
        Parameters(p): Parameters<ClientApiListParams>,
    ) -> Result<Json<Paginated<ApiDefinition>>, String> {
        self.client
            .json(
                Method::GET,
                &format!(
                    "/api/api-access/{}/assignable-apis",
                    client_seg(&p.client_id)
                ),
                &p.list.to_query(),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "List the OIDC clients with access to an API, each with its grant split into client (machine-to-machine), user-delegated, and metadata-document access."
    )]
    pub async fn list_api_definition_clients(
        &self,
        Parameters(p): Parameters<ApiDefinitionClientsParams>,
    ) -> Result<Json<Paginated<ApiClientAccess>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/apis/{}/clients", seg(&p.api_id)),
                &p.list.to_query(),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "List OIDC clients that have no grant on an API yet — candidates for update_client_api_access — with optional search, pagination, and sorting."
    )]
    pub async fn list_api_definition_assignable_clients(
        &self,
        Parameters(p): Parameters<ApiDefinitionClientsParams>,
    ) -> Result<Json<Paginated<ApiClientSummary>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/apis/{}/assignable-clients", seg(&p.api_id)),
                &p.list.to_query(),
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(description = "List API definitions, with optional search, pagination, and sorting.")]
    pub async fn list_api_definitions(
        &self,
        Parameters(p): Parameters<SearchListParams>,
    ) -> Result<Json<Paginated<ApiDefinition>>, String> {
        self.client
            .json(Method::GET, "/api/apis", &p.to_query(), NO_BODY)
            .await
            .tool_json()
    }

    #[tool(description = "Get an API definition by ID, including its permissions.")]
    pub async fn get_api_definition(
        &self,
        Parameters(p): Parameters<ApiDefinitionIdParam>,
    ) -> Result<Json<ApiDefinition>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/apis/{}", seg(&p.api_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_json()
    }
}

// ---------------------------------------------------------------------------
// Write tier
// ---------------------------------------------------------------------------

#[tool_router(router = oidc_write_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(
        description = "Create an OIDC client. For confidential clients, call create_oidc_client_secret afterwards to obtain the secret."
    )]
    pub async fn create_oidc_client(
        &self,
        Parameters(p): Parameters<OidcClientInput>,
    ) -> Result<Json<OidcClient>, String> {
        self.client
            .json(Method::POST, "/api/oidc/clients", &[], Some(&p))
            .await
            .tool_json()
    }

    #[tool(
        description = "Update an OIDC client by ID. Supply the full desired state; omitted optional fields are cleared or defaulted by the API. Clients cannot be renamed: the client ID is fixed at creation."
    )]
    pub async fn update_oidc_client(
        &self,
        Parameters(p): Parameters<UpdateOidcClientParams>,
    ) -> Result<Json<OidcClient>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/oidc/clients/{}", client_seg(&p.client_id)),
                &[],
                Some(&p.client),
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Delete an OIDC client. Applications using it will no longer be able to authenticate."
    )]
    pub async fn delete_oidc_client(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/oidc/clients/{}", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("OIDC client {} deleted", p.client_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Add a client secret to an OIDC client, leaving existing secrets usable. Pass `secret` (min 16 chars) to set a chosen value, or omit it to generate a random one; `expires_at` (RFC 3339) makes it expire. The secret value is shown ONLY ONCE in this response and cannot be retrieved later — store it now."
    )]
    pub async fn create_oidc_client_secret(
        &self,
        Parameters(p): Parameters<CreateClientSecretParams>,
    ) -> Result<Json<OidcClientSecret>, String> {
        let mut body = serde_json::Map::new();
        if let Some(secret) = &p.secret {
            body.insert("secret".to_string(), serde_json::json!(secret));
        }
        if let Some(expires_at) = &p.expires_at {
            body.insert("expiresAt".to_string(), serde_json::json!(expires_at));
        }
        let body = (!body.is_empty()).then_some(serde_json::Value::Object(body));
        self.client
            .json(
                Method::POST,
                &format!("/api/oidc/clients/{}/secrets", client_seg(&p.client_id)),
                &[],
                body.as_ref(),
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Delete one secret of an OIDC client, making it immediately unusable. Applications still authenticating with it will fail."
    )]
    pub async fn delete_oidc_client_secret(
        &self,
        Parameters(p): Parameters<DeleteClientSecretParams>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!(
                    "/api/oidc/clients/{}/secrets/{}",
                    client_seg(&p.client_id),
                    seg(&p.secret_id)
                ),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| {
                format!(
                    "secret {} deleted for OIDC client {}",
                    p.secret_id, p.client_id
                )
            })
            .map_err(err_str)
    }

    #[tool(
        description = "Restrict an OIDC client to members of the given user groups (full replacement; empty list removes the restriction)."
    )]
    pub async fn update_oidc_client_allowed_groups(
        &self,
        Parameters(p): Parameters<AllowedGroupsParams>,
    ) -> Result<Json<OidcClient>, String> {
        self.client
            .json(
                Method::PUT,
                &format!(
                    "/api/oidc/clients/{}/allowed-user-groups",
                    client_seg(&p.client_id)
                ),
                &[],
                Some(&serde_json::json!({ "userGroupIds": p.user_group_ids })),
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Re-fetch a federated OIDC client's metadata document (for clients registered via Client ID Metadata Documents)."
    )]
    pub async fn refresh_oidc_client_metadata(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<OidcClient>, String> {
        self.client
            .json(
                Method::POST,
                &format!("/api/oidc/clients/{}/refresh", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Upload an OIDC client's logo from a local file_path or an https url (exactly one). Set light=true for the light-mode variant."
    )]
    pub async fn update_oidc_client_logo(
        &self,
        Parameters(p): Parameters<UpdateClientLogoParams>,
    ) -> Result<String, String> {
        let file = self
            .client
            .load_file_source(&p.source)
            .await
            .map_err(err_str)?;
        self.client
            .upload(
                Method::POST,
                &format!("/api/oidc/clients/{}/logo", client_seg(&p.client_id)),
                &light_query(p.light),
                file,
            )
            .await
            .map(|_| format!("logo updated for OIDC client {}", p.client_id))
            .map_err(err_str)
    }

    #[tool(description = "Delete an OIDC client's logo.")]
    pub async fn delete_oidc_client_logo(
        &self,
        Parameters(p): Parameters<ClientLogoParams>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/oidc/clients/{}/logo", client_seg(&p.client_id)),
                &light_query(p.light),
                NO_BODY,
            )
            .await
            .map(|_| format!("logo deleted for OIDC client {}", p.client_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Replace the set of OIDC clients a user group grants access to (group side of client group restrictions)."
    )]
    pub async fn set_group_allowed_oidc_clients(
        &self,
        Parameters(p): Parameters<GroupAllowedClientsParams>,
    ) -> Result<Json<UserGroup>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/user-groups/{}/allowed-oidc-clients", seg(&p.group_id)),
                &[],
                Some(&serde_json::json!({ "oidcClientIds": p.oidc_client_ids })),
            )
            .await
            .tool_json()
    }

    #[tool(description = "Revoke the current user's authorization (consent) for an OIDC client.")]
    pub async fn revoke_my_authorized_client(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!(
                    "/api/oidc/users/me/authorized-clients/{}",
                    client_seg(&p.client_id)
                ),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("authorization revoked for client {}", p.client_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Replace an OIDC client's grant on one API: access flags and permission IDs for client (machine-to-machine) and user-delegated use. Set client_access/user_delegated_access to true to enable that mode — an omitted flag disables it. Grants on other APIs are untouched."
    )]
    pub async fn update_client_api_access(
        &self,
        Parameters(p): Parameters<UpdateClientApiAccessParams>,
    ) -> Result<Json<ApiClientGrant>, String> {
        let mut body = serde_json::json!({
            "clientPermissionIds": p.client_permission_ids,
            "userDelegatedPermissionIds": p.user_delegated_permission_ids,
        });
        if let Some(v) = p.client_access {
            body["clientAccess"] = v.into();
        }
        if let Some(v) = p.user_delegated_access {
            body["userDelegatedAccess"] = v.into();
        }
        self.client
            .json(
                Method::PUT,
                &format!(
                    "/api/apis/{}/clients/{}",
                    seg(&p.api_id),
                    client_seg(&p.client_id)
                ),
                &[],
                Some(&body),
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Revoke an OIDC client's access to an API: every permission of that API the client was allowed to request is removed."
    )]
    pub async fn revoke_client_api_access(
        &self,
        Parameters(p): Parameters<ApiClientParams>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!(
                    "/api/apis/{}/clients/{}",
                    seg(&p.api_id),
                    client_seg(&p.client_id)
                ),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| {
                format!(
                    "access to API {} revoked for OIDC client {}",
                    p.api_id, p.client_id
                )
            })
            .map_err(err_str)
    }

    #[tool(
        description = "Replace which permissions of an API clients registered through a Client ID Metadata Document may request. Set enabled=true to allow CIMD clients — omitting it disables their access."
    )]
    pub async fn update_api_cimd_access(
        &self,
        Parameters(p): Parameters<UpdateApiCimdAccessParams>,
    ) -> Result<Json<ApiDefinition>, String> {
        let mut body = serde_json::json!({ "permissionIds": p.permission_ids });
        if let Some(v) = p.enabled {
            body["enabled"] = v.into();
        }
        self.client
            .json(
                Method::PUT,
                &format!("/api/apis/{}/cimd-access", seg(&p.api_id)),
                &[],
                Some(&body),
            )
            .await
            .tool_json()
    }

    #[tool(
        description = "Create an API definition (a resource APIs clients can be granted access to)."
    )]
    pub async fn create_api_definition(
        &self,
        Parameters(p): Parameters<CreateApiDefinitionParams>,
    ) -> Result<Json<ApiDefinition>, String> {
        self.client
            .json(
                Method::POST,
                "/api/apis",
                &[],
                Some(&serde_json::json!({ "name": p.name, "resource": p.resource })),
            )
            .await
            .tool_json()
    }

    #[tool(description = "Rename an API definition.")]
    pub async fn update_api_definition(
        &self,
        Parameters(p): Parameters<UpdateApiDefinitionParams>,
    ) -> Result<Json<ApiDefinition>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/apis/{}", seg(&p.api_id)),
                &[],
                Some(&serde_json::json!({ "name": p.name })),
            )
            .await
            .tool_json()
    }

    #[tool(description = "Delete an API definition and its permissions.")]
    pub async fn delete_api_definition(
        &self,
        Parameters(p): Parameters<ApiDefinitionIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/apis/{}", seg(&p.api_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("API definition {} deleted", p.api_id))
            .map_err(err_str)
    }

    #[tool(description = "Replace an API definition's permission set (full replacement).")]
    pub async fn set_api_definition_permissions(
        &self,
        Parameters(p): Parameters<SetApiPermissionsParams>,
    ) -> Result<Json<ApiDefinition>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/apis/{}/permissions", seg(&p.api_id)),
                &[],
                Some(&serde_json::json!({ "permissions": p.permissions })),
            )
            .await
            .tool_json()
    }
}
