//! OIDC tools: clients, secrets, logos, grants, introspection, API access.

use reqwest::Method;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::client::{FileSource, NO_BODY};
use crate::dto::*;
use crate::server::{PocketIdServer, err_str};
use crate::tools::identity::SearchListParams;
use crate::tools::{client_seg, seg};

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ClientIdParam {
    /// OIDC client ID.
    pub client_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateOidcClientParams {
    /// ID of the OIDC client to update.
    pub client_id: String,
    #[serde(flatten)]
    pub client: OidcClientInput,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AllowedGroupsParams {
    /// ID of the OIDC client to restrict.
    pub client_id: String,
    /// Complete set of user-group IDs allowed to use this client.
    pub user_group_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct PreviewParams {
    /// OIDC client ID.
    pub client_id: String,
    /// User to preview tokens for.
    pub user_id: String,
    /// Space-separated scopes to include, e.g. "openid profile email groups".
    pub scopes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ClientLogoParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Dark-mode logo variant when true; light/default when false or omitted.
    pub light: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateClientLogoParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Upload as the light-variant logo when true.
    pub light: Option<bool>,
    #[serde(flatten)]
    pub source: FileSource,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct IntrospectParams {
    /// The token to introspect (access or refresh token issued by this instance).
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UserAuthorizedClientsParams {
    /// User ID.
    pub user_id: String,
    #[serde(flatten)]
    pub list: ListParams,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateClientApiAccessParams {
    /// OIDC client ID.
    pub client_id: String,
    /// Permission IDs the client itself may use (client credentials).
    pub client_permission_ids: Vec<String>,
    /// Permission IDs users may delegate to the client.
    pub user_delegated_permission_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CreateApiDefinitionParams {
    /// Display name (max 50 chars).
    pub name: String,
    /// Resource identifier of the API (max 350 chars).
    pub resource: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ApiDefinitionIdParam {
    /// API definition ID.
    pub api_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateApiDefinitionParams {
    /// API definition ID.
    pub api_id: String,
    /// New display name.
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SetApiPermissionsParams {
    /// API definition ID.
    pub api_id: String,
    /// Complete new permission set for this API.
    pub permissions: Vec<ApiPermissionInput>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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

    #[tool(
        description = "Introspect a token issued by this instance: whether it is active, and its claims."
    )]
    pub async fn introspect_token(
        &self,
        Parameters(p): Parameters<IntrospectParams>,
    ) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .form("/api/oidc/introspect", &[("token", p.token.as_str())])
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
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
                &p.list.to_query(),
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "List the OIDC clients the current user has authorized.")]
    pub async fn list_my_authorized_clients(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<Json<Paginated<AuthorizedOidcClient>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/oidc/users/me/authorized-clients",
                &p.to_query(),
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "List the OIDC clients the current user can access.")]
    pub async fn list_my_accessible_clients(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<Json<Paginated<AccessibleOidcClient>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/oidc/users/me/clients",
                &p.to_query(),
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Get an OIDC client's API access configuration: which API permissions it may use directly or via user delegation."
    )]
    pub async fn get_client_api_access(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<ClientApiAccess>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/api-access/{}", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "List API definitions, with optional search, pagination, and sorting.")]
    pub async fn list_api_definitions(
        &self,
        Parameters(p): Parameters<SearchListParams>,
    ) -> Result<Json<Paginated<ApiDefinition>>, String> {
        self.client
            .json(Method::GET, "/api/apis", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Update an OIDC client by ID. Supply the full desired state; omitted optional fields are cleared or defaulted by the API."
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
            .map(Json)
            .map_err(err_str)
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
        description = "Generate a new client secret for an OIDC client. The secret is shown ONLY ONCE in this response and cannot be retrieved later — store it now. The previous secret is invalidated immediately."
    )]
    pub async fn create_oidc_client_secret(
        &self,
        Parameters(p): Parameters<ClientIdParam>,
    ) -> Result<Json<OidcClientSecret>, String> {
        self.client
            .json(
                Method::POST,
                &format!("/api/oidc/clients/{}/secret", client_seg(&p.client_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
        description = "Replace an OIDC client's API access configuration: permission IDs usable by the client directly and via user delegation."
    )]
    pub async fn update_client_api_access(
        &self,
        Parameters(p): Parameters<UpdateClientApiAccessParams>,
    ) -> Result<Json<ClientApiAccess>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/api-access/{}", client_seg(&p.client_id)),
                &[],
                Some(&serde_json::json!({
                    "clientPermissionIds": p.client_permission_ids,
                    "userDelegatedPermissionIds": p.user_delegated_permission_ids,
                })),
            )
            .await
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
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
            .map(Json)
            .map_err(err_str)
    }
}
