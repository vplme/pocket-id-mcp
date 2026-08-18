//! Admin tools: application images, configuration, audit logs, API keys,
//! SCIM, version/health.

use reqwest::Method;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::client::{FileSource, NO_BODY};
use crate::dto::*;
use crate::server::{PocketIdServer, err_str};
use crate::tools::{client_seg, seg};

/// Application image slot. Maps to `/api/application-images/<slot>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageType {
    /// Main logo. Supports the `light` variant flag.
    Logo,
    Favicon,
    /// Login page background.
    Background,
    /// Image embedded in outgoing emails.
    Email,
    /// Default profile picture for users without one.
    DefaultProfilePicture,
}

impl ImageType {
    fn path(self) -> &'static str {
        match self {
            ImageType::Logo => "logo",
            ImageType::Favicon => "favicon",
            ImageType::Background => "background",
            ImageType::Email => "email",
            ImageType::DefaultProfilePicture => "default-profile-picture",
        }
    }

    fn supports_delete(self) -> bool {
        matches!(
            self,
            ImageType::Background | ImageType::DefaultProfilePicture
        )
    }
}

fn image_query(
    image_type: ImageType,
    light: Option<bool>,
) -> Result<Vec<(String, String)>, String> {
    match (image_type, light) {
        (ImageType::Logo, Some(v)) => Ok(vec![("light".to_string(), v.to_string())]),
        (ImageType::Logo, None) => Ok(vec![]),
        (_, None) => Ok(vec![]),
        (other, Some(_)) => Err(format!(
            "the light flag is only valid for image_type=logo, not {:?}",
            other
        )),
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetImageParams {
    /// Which application image to fetch.
    pub image_type: ImageType,
    /// Light-mode logo variant when true (the API default when omitted); dark-mode when false. Only valid for image_type=logo.
    pub light: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateImageParams {
    /// Which application image to replace.
    pub image_type: ImageType,
    /// Upload as the light-mode logo variant when true (the API default when omitted); dark-mode when false. Only valid for image_type=logo.
    pub light: Option<bool>,
    #[serde(flatten)]
    pub source: FileSource,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteImageParams {
    /// Which application image to reset. Only background and
    /// default_profile_picture can be reset upstream.
    pub image_type: ImageType,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateAppConfigParams {
    /// Complete configuration as a single flat object of camelCase keys to
    /// string values, e.g. {"appName": "...", "sessionDuration": "60", ...}.
    /// Every required key must be present — partial objects are rejected with
    /// 400. Note that get_all_application_configuration returns an ARRAY of
    /// {key, type, value} entries: convert it to a flat {key: value} object,
    /// apply your changes, then submit. Do not send the array itself.
    pub config: serde_json::Value,
}

/// Audit-log network location filter. The upstream server recognizes exactly
/// these values (anything else is silently ignored there), so the closed set
/// is enforced here in the schema. Inlined so the advertised schema is a
/// plain string enum rather than a `$ref`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(inline)]
pub enum AuditLogLocation {
    Internal,
    External,
}

impl AuditLogLocation {
    fn as_str(self) -> &'static str {
        match self {
            AuditLogLocation::Internal => "internal",
            AuditLogLocation::External => "external",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditLogFilterParams {
    /// Filter by user ID.
    pub user_id: Option<String>,
    /// Filter by event type, e.g. SIGN_IN, TOKEN_SIGN_IN, CLIENT_AUTHORIZATION.
    pub event: Option<String>,
    /// Filter by OIDC client name.
    pub client_name: Option<String>,
    /// Filter by network location.
    pub location: Option<AuditLogLocation>,
    #[serde(flatten)]
    pub list: ListParams,
}

impl AuditLogFilterParams {
    fn to_query(&self) -> Vec<(String, String)> {
        let mut q = self.list.to_query();
        // The server matches filter keys against Go field names with only the
        // first letter capitalized, so this must be "userID", not "userId".
        if let Some(v) = &self.user_id {
            q.push(("filters[userID]".to_string(), v.clone()));
        }
        if let Some(v) = &self.event {
            q.push(("filters[event]".to_string(), v.clone()));
        }
        if let Some(v) = &self.client_name {
            q.push(("filters[clientName]".to_string(), v.clone()));
        }
        if let Some(v) = self.location {
            q.push(("filters[location]".to_string(), v.as_str().to_string()));
        }
        q
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKeyParams {
    /// Key name (3–50 chars).
    pub name: String,
    /// Expiry timestamp (RFC 3339, e.g. "2027-01-01T00:00:00Z"). Required.
    pub expires_at: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyIdParam {
    /// API key ID.
    pub key_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScimProviderIdParam {
    /// SCIM service provider ID.
    pub provider_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateScimProviderParams {
    /// SCIM service provider ID.
    pub provider_id: String,
    #[serde(flatten)]
    pub provider: ScimServiceProviderUpdateInput,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientIdOnlyParam {
    /// OIDC client ID.
    pub client_id: String,
}

// ---------------------------------------------------------------------------
// Read tier
// ---------------------------------------------------------------------------

#[tool_router(router = admin_read_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(
        description = "Fetch an application branding image (logo, favicon, background, email, default profile picture) as an image for visual inspection. Use light=true for the light-mode logo."
    )]
    pub async fn get_application_image(
        &self,
        Parameters(p): Parameters<GetImageParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = match image_query(p.image_type, p.light) {
            Ok(q) => q,
            Err(msg) => {
                return Ok(CallToolResult::error(vec![
                    rmcp::model::ContentBlock::text(msg),
                ]));
            }
        };
        match self
            .client
            .binary(
                &format!("/api/application-images/{}", p.image_type.path()),
                &query,
            )
            .await
        {
            Ok(bin) => self.binary_result(bin),
            Err(e) => Ok(Self::api_error_result(e)),
        }
    }

    #[tool(description = "Read the public (unauthenticated) application configuration.")]
    pub async fn get_public_application_configuration(
        &self,
    ) -> Result<Json<Enveloped<Vec<AppConfigVariable>>>, String> {
        self.client
            .json(Method::GET, "/api/application-configuration", &[], NO_BODY)
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(
        description = "Read the complete application configuration, including admin-only settings."
    )]
    pub async fn get_all_application_configuration(
        &self,
    ) -> Result<Json<Enveloped<Vec<AppConfigVariable>>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/application-configuration/all",
                &[],
                NO_BODY,
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(description = "List the current user's own audit log entries.")]
    pub async fn list_my_audit_logs(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<Json<Paginated<AuditLog>>, String> {
        self.client
            .json(Method::GET, "/api/audit-logs", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "List all users' audit log entries, with optional filters by user, event type, client name, and location."
    )]
    pub async fn list_all_audit_logs(
        &self,
        Parameters(p): Parameters<AuditLogFilterParams>,
    ) -> Result<Json<Paginated<AuditLog>>, String> {
        self.client
            .json(Method::GET, "/api/audit-logs/all", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "List client names present in the audit log (for building filters).")]
    pub async fn list_audit_log_client_names(&self) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(
                Method::GET,
                "/api/audit-logs/filters/client-names",
                &[],
                NO_BODY,
            )
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(description = "List users present in the audit log (for building filters).")]
    pub async fn list_audit_log_users(&self) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(Method::GET, "/api/audit-logs/filters/users", &[], NO_BODY)
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(
        description = "List API keys with expiry and last-used metadata (key values are never shown)."
    )]
    pub async fn list_api_keys(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<Json<Paginated<ApiKey>>, String> {
        self.client
            .json(Method::GET, "/api/api-keys", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get the SCIM service provider attached to an OIDC client.")]
    pub async fn get_client_scim_service_provider(
        &self,
        Parameters(p): Parameters<ClientIdOnlyParam>,
    ) -> Result<Json<ScimServiceProvider>, String> {
        self.client
            .json(
                Method::GET,
                &format!(
                    "/api/oidc/clients/{}/scim-service-provider",
                    client_seg(&p.client_id)
                ),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get the Pocket ID version this instance is running.")]
    pub async fn get_current_version(&self) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(Method::GET, "/api/version/current", &[], NO_BODY)
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(description = "Get the latest released Pocket ID version (for update checks).")]
    pub async fn get_latest_version(&self) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(Method::GET, "/api/version/latest", &[], NO_BODY)
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(description = "Check the instance's health endpoint.")]
    pub async fn health_check(&self) -> Result<String, String> {
        let bin = self.client.binary("/healthz", &[]).await.map_err(err_str)?;
        let body = String::from_utf8_lossy(&bin.bytes).trim().to_string();
        Ok(if body.is_empty() {
            "healthy".to_string()
        } else {
            body
        })
    }
}

// ---------------------------------------------------------------------------
// Write tier
// ---------------------------------------------------------------------------

#[tool_router(router = admin_write_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(
        description = "Replace an application branding image from a local file_path or an https url (exactly one). Use light=true for the light-mode logo variant."
    )]
    pub async fn update_application_image(
        &self,
        Parameters(p): Parameters<UpdateImageParams>,
    ) -> Result<String, String> {
        let query = image_query(p.image_type, p.light)?;
        let file = self
            .client
            .load_file_source(&p.source)
            .await
            .map_err(err_str)?;
        self.client
            .upload(
                Method::PUT,
                &format!("/api/application-images/{}", p.image_type.path()),
                &query,
                file,
            )
            .await
            .map(|_| format!("{:?} image updated", p.image_type))
            .map_err(err_str)
    }

    #[tool(
        description = "Reset an application image to its default. Upstream supports this only for background and default_profile_picture."
    )]
    pub async fn delete_application_image(
        &self,
        Parameters(p): Parameters<DeleteImageParams>,
    ) -> Result<String, String> {
        if !p.image_type.supports_delete() {
            return Err(format!(
                "{:?} cannot be reset via the API; only background and default_profile_picture support deletion. To change it, upload a replacement with update_application_image.",
                p.image_type
            ));
        }
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/application-images/{}", p.image_type.path()),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("{:?} image reset to default", p.image_type))
            .map_err(err_str)
    }

    #[tool(
        description = "Update the application configuration. The API expects one complete flat object of camelCase keys to string values with every required key present (partial updates are rejected). Read get_all_application_configuration first, flatten its [{key, value}] array into a {key: value} object, change what you need, and submit that object."
    )]
    pub async fn update_application_configuration(
        &self,
        Parameters(p): Parameters<UpdateAppConfigParams>,
    ) -> Result<Json<Enveloped<Vec<AppConfigVariable>>>, String> {
        self.client
            .json(
                Method::PUT,
                "/api/application-configuration",
                &[],
                Some(&p.config),
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(description = "Trigger an LDAP directory sync now.")]
    pub async fn sync_ldap(&self) -> Result<String, String> {
        self.client
            .empty(
                Method::POST,
                "/api/application-configuration/sync-ldap",
                &[],
                NO_BODY,
            )
            .await
            .map(|_| "LDAP sync triggered".to_string())
            .map_err(err_str)
    }

    #[tool(description = "Send a test email to the current user to verify SMTP settings.")]
    pub async fn send_test_email(&self) -> Result<String, String> {
        self.client
            .empty(
                Method::POST,
                "/api/application-configuration/test-email",
                &[],
                NO_BODY,
            )
            .await
            .map(|_| "test email sent".to_string())
            .map_err(err_str)
    }

    #[tool(
        description = "Create an API key. The key value is shown ONLY ONCE in this response and cannot be retrieved later — store it now."
    )]
    pub async fn create_api_key(
        &self,
        Parameters(p): Parameters<CreateApiKeyParams>,
    ) -> Result<Json<ApiKeyResponse>, String> {
        let mut body = serde_json::json!({
            "name": p.name,
            "expiresAt": p.expires_at,
        });
        if let Some(desc) = &p.description {
            body["description"] = serde_json::json!(desc);
        }
        self.client
            .json(Method::POST, "/api/api-keys", &[], Some(&body))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Renew an API key, extending its expiry. The renewed key value is shown ONLY ONCE in this response."
    )]
    pub async fn renew_api_key(
        &self,
        Parameters(p): Parameters<ApiKeyIdParam>,
    ) -> Result<Json<ApiKeyResponse>, String> {
        self.client
            .json(
                Method::POST,
                &format!("/api/api-keys/{}/renew", seg(&p.key_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Create a SCIM service provider to provision users/groups to a downstream app."
    )]
    pub async fn create_scim_service_provider(
        &self,
        Parameters(p): Parameters<ScimServiceProviderInput>,
    ) -> Result<Json<ScimServiceProvider>, String> {
        self.client
            .json(Method::POST, "/api/scim/service-provider", &[], Some(&p))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Update a SCIM service provider. This is a full replacement: the server stores exactly what is sent, so always resend the current token — a missing or empty token clears the stored one and breaks provisioning."
    )]
    pub async fn update_scim_service_provider(
        &self,
        Parameters(p): Parameters<UpdateScimProviderParams>,
    ) -> Result<Json<ScimServiceProvider>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/scim/service-provider/{}", seg(&p.provider_id)),
                &[],
                Some(&p.provider),
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Delete a SCIM service provider. Stops provisioning to the downstream app."
    )]
    pub async fn delete_scim_service_provider(
        &self,
        Parameters(p): Parameters<ScimProviderIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/scim/service-provider/{}", seg(&p.provider_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("SCIM service provider {} deleted", p.provider_id))
            .map_err(err_str)
    }

    #[tool(description = "Trigger a full SCIM sync for a service provider now.")]
    pub async fn sync_scim_service_provider(
        &self,
        Parameters(p): Parameters<ScimProviderIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::POST,
                &format!("/api/scim/service-provider/{}/sync", seg(&p.provider_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("SCIM sync triggered for provider {}", p.provider_id))
            .map_err(err_str)
    }
}

// ---------------------------------------------------------------------------
// Dangerous tier
// ---------------------------------------------------------------------------

#[tool_router(router = admin_dangerous_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(
        description = "Revoke (delete) an API key immediately. WARNING: revoking the key this server itself uses will sever its access to Pocket ID."
    )]
    pub async fn revoke_api_key(
        &self,
        Parameters(p): Parameters<ApiKeyIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/api-keys/{}", seg(&p.key_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("API key {} revoked", p.key_id))
            .map_err(err_str)
    }
}
