//! Curated MCP prompts encoding multi-step workflows over the primitive tools.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ErrorData as McpError, prompt, prompt_router, schemars};
use serde::{Deserialize, Serialize};

use crate::server::PocketIdServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OnboardOidcClientArgs {
    /// Name of the application being connected.
    pub app_name: String,
    /// OAuth redirect/callback URL(s) of the application, comma-separated if several.
    pub callback_urls: Option<String>,
    /// Group name that should be allowed to use the app (optional).
    pub allowed_group: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuditUserAccessArgs {
    /// Username, email, or user ID to audit.
    pub user: String,
}

#[prompt_router(router = "read_prompts", vis = "pub(crate)")]
impl PocketIdServer {
    /// Audit what a user can access and what they have been doing: profile,
    /// group memberships, authorized OIDC clients, passkeys, and recent
    /// sign-in activity.
    #[prompt(name = "audit-user-access")]
    async fn audit_user_access(
        &self,
        Parameters(args): Parameters<AuditUserAccessArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Audit access for the Pocket ID user \"{user}\". Steps:\n\
                 1. Find the user with list_users (search) and fetch details with get_user.\n\
                 2. List their group memberships (list_user_groups_of_user) and note any \
                 admin-granting groups or custom claims.\n\
                 3. List OIDC clients they have authorized (list_user_authorized_clients).\n\
                 4. List their passkeys (list_user_passkeys) and flag stale or unusual credentials.\n\
                 5. Query recent activity with list_all_audit_logs filtered by their user ID and \
                 summarize sign-ins: clients used, locations, anything anomalous.\n\
                 Finish with a concise access summary and any recommended clean-ups.",
                user = args.user
            ),
        )])
    }

    /// Check instance health and version status: health endpoint, current vs
    /// latest version, and configuration warnings worth surfacing.
    #[prompt(name = "instance-health-check")]
    async fn instance_health_check(&self) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![PromptMessage::new_text(
            Role::User,
            "Run a health check on this Pocket ID instance:\n\
             1. Call health_check and report the result.\n\
             2. Compare get_current_version with get_latest_version and say whether an \
             update is available (include release significance if the gap is large).\n\
             3. Review get_all_application_configuration for risky settings: open signups, \
             unverified emails allowed, SMTP not configured while email features are enabled, \
             or LDAP enabled but failing.\n\
             4. Skim recent list_all_audit_logs for repeated failures or unusual activity.\n\
             Summarize instance status in a few lines with any recommended actions."
                .to_string(),
        )])
    }
}

#[prompt_router(router = "write_prompts", vis = "pub(crate)")]
impl PocketIdServer {
    /// Onboard a new application as an OIDC client: create the client,
    /// restrict it to a group, fetch the secret once, and hand over the
    /// endpoints the app needs.
    #[prompt(name = "onboard-oidc-client")]
    async fn onboard_oidc_client(
        &self,
        Parameters(args): Parameters<OnboardOidcClientArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let callback = args
            .callback_urls
            .unwrap_or_else(|| "ask the user for the app's callback URL(s)".to_string());
        let group_step = match args.allowed_group {
            Some(g) => format!(
                "3. Restrict access to the \"{g}\" group: find its ID with list_groups, then \
                 call update_oidc_client_allowed_groups."
            ),
            None => "3. Ask whether access should be restricted to a user group; if so, use \
                     list_groups and update_oidc_client_allowed_groups."
                .to_string(),
        };
        Ok(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Set up OIDC single sign-on for the application \"{app}\" on this Pocket ID \
                 instance. Steps:\n\
                 1. Create the client with create_oidc_client (name: \"{app}\", callback URLs: \
                 {callback}). Decide public vs confidential based on the app type — confidential \
                 unless it is a SPA/native app that cannot keep a secret; enable PKCE.\n\
                 2. If confidential, call create_oidc_client_secret and hand the secret to the \
                 user immediately — it is shown only once.\n\
                 {group_step}\n\
                 4. Give the user the values their app needs: client ID, secret (if any), and \
                 the issuer/discovery URL of the Pocket ID instance \
                 (<instance>/.well-known/openid-configuration).\n\
                 5. Optionally upload a logo with update_oidc_client_logo and verify it with \
                 get_oidc_client_logo.",
                app = args.app_name,
            ),
        )])
    }
}
