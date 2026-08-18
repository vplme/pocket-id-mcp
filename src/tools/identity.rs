//! Identity tools: users, groups, custom claims, passkeys, signup tokens,
//! one-time access.

use reqwest::Method;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::client::{FileSource, NO_BODY};
use crate::dto::*;
use crate::server::{PocketIdServer, err_str};
use crate::tools::seg;

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SearchListParams {
    /// Free-text search filter.
    pub search: Option<String>,
    #[serde(flatten)]
    pub list: ListParams,
}

impl SearchListParams {
    pub(crate) fn to_query(&self) -> Vec<(String, String)> {
        let mut q = self.list.to_query();
        if let Some(search) = &self.search {
            q.push(("search".to_string(), search.clone()));
        }
        q
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UserIdParam {
    /// User ID.
    pub user_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GroupIdParam {
    /// User group ID.
    pub group_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateUserParams {
    /// ID of the user to update.
    pub user_id: String,
    #[serde(flatten)]
    pub user: UserInput,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UserProfilePictureParams {
    /// ID of the user whose profile picture to replace.
    pub user_id: String,
    #[serde(flatten)]
    pub source: FileSource,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SetUserGroupsParams {
    /// ID of the user whose group memberships to replace.
    pub user_id: String,
    /// Complete new set of group IDs for this user.
    pub user_group_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateGroupParams {
    /// ID of the group to update.
    pub group_id: String,
    #[serde(flatten)]
    pub group: UserGroupInput,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SetGroupUsersParams {
    /// ID of the group whose member list to replace.
    pub group_id: String,
    /// Complete new set of member user IDs.
    pub user_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UserClaimsParams {
    /// ID of the user whose custom claims to replace.
    pub user_id: String,
    /// Complete new set of claims (replaces existing ones).
    pub claims: Vec<CustomClaimInput>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GroupClaimsParams {
    /// ID of the group whose custom claims to replace.
    pub group_id: String,
    /// Complete new set of claims (replaces existing ones).
    pub claims: Vec<CustomClaimInput>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct DeletePasskeyParams {
    /// ID of the user owning the credential.
    pub user_id: String,
    /// ID of the WebAuthn credential to delete.
    pub credential_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CreateSignupTokenParams {
    /// Token lifetime as a Go duration string, e.g. "1h", "24h", "168h"
    /// (= 7 days). Only ns/us/ms/s/m/h units are accepted — "7d" is rejected.
    pub ttl: String,
    /// How many signups this token allows (1–100).
    pub usage_limit: i64,
    /// Groups new users are added to on signup.
    pub user_group_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SignupTokenIdParam {
    /// Signup token ID.
    pub token_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct OneTimeAccessEmailAdminParams {
    /// ID of the user to send the login email to.
    pub user_id: String,
    /// Optional token lifetime as a Go duration string, e.g. "1h", "30m".
    /// Only ns/us/ms/s/m/h units are accepted — day units like "7d" are rejected.
    pub ttl: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct OneTimeAccessEmailParams {
    /// Email address of the account requesting a login link.
    pub email: String,
    /// Path to redirect to after login.
    pub redirect_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct VerifyEmailParams {
    /// Verification token from the email.
    pub token: String,
}

// ---------------------------------------------------------------------------
// Read tier
// ---------------------------------------------------------------------------

#[tool_router(router = identity_read_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(description = "List users, with optional search, pagination, and sorting.")]
    pub async fn list_users(
        &self,
        Parameters(p): Parameters<SearchListParams>,
    ) -> Result<Json<Paginated<User>>, String> {
        self.client
            .json(Method::GET, "/api/users", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get a user by ID.")]
    pub async fn get_user(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<Json<User>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/users/{}", seg(&p.user_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get the user account associated with the configured API key.")]
    pub async fn get_current_user(&self) -> Result<Json<User>, String> {
        self.client
            .json(Method::GET, "/api/users/me", &[], NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "List the groups a user is a member of.")]
    pub async fn list_user_groups_of_user(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<Json<Enveloped<Vec<UserGroup>>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/users/{}/groups", seg(&p.user_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(
        description = "Get a user's profile picture as an image (PNG), so it can be visually inspected."
    )]
    pub async fn get_user_profile_picture(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .client
            .binary(
                &format!("/api/users/{}/profile-picture.png", seg(&p.user_id)),
                &[],
            )
            .await
        {
            Ok(bin) => self.binary_result(bin),
            Err(e) => Ok(Self::api_error_result(e)),
        }
    }

    #[tool(description = "List user groups, with optional search, pagination, and sorting.")]
    pub async fn list_groups(
        &self,
        Parameters(p): Parameters<SearchListParams>,
    ) -> Result<Json<Paginated<UserGroupMinimal>>, String> {
        self.client
            .json(Method::GET, "/api/user-groups", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get a user group by ID, including members and custom claims.")]
    pub async fn get_group(
        &self,
        Parameters(p): Parameters<GroupIdParam>,
    ) -> Result<Json<UserGroup>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/user-groups/{}", seg(&p.group_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Get suggested custom-claim keys already in use on this instance.")]
    pub async fn get_custom_claim_suggestions(&self) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(Method::GET, "/api/custom-claims/suggestions", &[], NO_BODY)
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(
        description = "List a user's passkeys (WebAuthn credentials): names, creation dates, IDs."
    )]
    pub async fn list_user_passkeys(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<Json<Enveloped<Vec<WebauthnCredential>>>, String> {
        self.client
            .json(
                Method::GET,
                &format!("/api/users/{}/webauthn-credentials", seg(&p.user_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(description = "List signup tokens with usage counts and expiry.")]
    pub async fn list_signup_tokens(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<Json<Paginated<SignupToken>>, String> {
        self.client
            .json(Method::GET, "/api/signup-tokens", &p.to_query(), NO_BODY)
            .await
            .map(Json)
            .map_err(err_str)
    }
}

// ---------------------------------------------------------------------------
// Write tier
// ---------------------------------------------------------------------------

#[tool_router(router = identity_write_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(description = "Create a user.")]
    pub async fn create_user(
        &self,
        Parameters(p): Parameters<UserInput>,
    ) -> Result<Json<User>, String> {
        self.client
            .json(Method::POST, "/api/users", &[], Some(&p))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Update a user by ID. Supply the full desired state; omitted optional fields are cleared or defaulted by the API."
    )]
    pub async fn update_user(
        &self,
        Parameters(p): Parameters<UpdateUserParams>,
    ) -> Result<Json<User>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/users/{}", seg(&p.user_id)),
                &[],
                Some(&p.user),
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Update the current user (the account behind the API key).")]
    pub async fn update_current_user(
        &self,
        Parameters(p): Parameters<UserInput>,
    ) -> Result<Json<User>, String> {
        self.client
            .json(Method::PUT, "/api/users/me", &[], Some(&p))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Replace a user's profile picture from a local file_path or an https url (exactly one)."
    )]
    pub async fn update_user_profile_picture(
        &self,
        Parameters(p): Parameters<UserProfilePictureParams>,
    ) -> Result<String, String> {
        let file = self
            .client
            .load_file_source(&p.source)
            .await
            .map_err(err_str)?;
        self.client
            .upload(
                Method::PUT,
                &format!("/api/users/{}/profile-picture", seg(&p.user_id)),
                &[],
                file,
            )
            .await
            .map(|_| format!("profile picture updated for user {}", p.user_id))
            .map_err(err_str)
    }

    #[tool(description = "Reset a user's profile picture to the default.")]
    pub async fn reset_user_profile_picture(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/users/{}/profile-picture", seg(&p.user_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("profile picture reset for user {}", p.user_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Replace the current user's profile picture from a local file_path or an https url (exactly one)."
    )]
    pub async fn update_current_user_profile_picture(
        &self,
        Parameters(p): Parameters<FileSource>,
    ) -> Result<String, String> {
        let file = self.client.load_file_source(&p).await.map_err(err_str)?;
        self.client
            .upload(Method::PUT, "/api/users/me/profile-picture", &[], file)
            .await
            .map(|_| "profile picture updated".to_string())
            .map_err(err_str)
    }

    #[tool(description = "Reset the current user's profile picture to the default.")]
    pub async fn reset_current_user_profile_picture(&self) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                "/api/users/me/profile-picture",
                &[],
                NO_BODY,
            )
            .await
            .map(|_| "profile picture reset".to_string())
            .map_err(err_str)
    }

    #[tool(description = "Send a verification email for the current user's email address.")]
    pub async fn send_current_user_email_verification(&self) -> Result<String, String> {
        self.client
            .empty(
                Method::POST,
                "/api/users/me/send-email-verification",
                &[],
                NO_BODY,
            )
            .await
            .map(|_| "verification email sent".to_string())
            .map_err(err_str)
    }

    #[tool(
        description = "Verify the current user's email address with a token from the verification email."
    )]
    pub async fn verify_current_user_email(
        &self,
        Parameters(p): Parameters<VerifyEmailParams>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::POST,
                "/api/users/me/verify-email",
                &[],
                Some(&serde_json::json!({ "token": p.token })),
            )
            .await
            .map(|_| "email verified".to_string())
            .map_err(err_str)
    }

    #[tool(
        description = "Replace a user's group memberships with the given set of group IDs (full replacement, not additive)."
    )]
    pub async fn set_user_groups(
        &self,
        Parameters(p): Parameters<SetUserGroupsParams>,
    ) -> Result<Json<User>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/users/{}/user-groups", seg(&p.user_id)),
                &[],
                Some(&serde_json::json!({ "userGroupIds": p.user_group_ids })),
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Create a user group.")]
    pub async fn create_group(
        &self,
        Parameters(p): Parameters<UserGroupInput>,
    ) -> Result<Json<UserGroup>, String> {
        self.client
            .json(Method::POST, "/api/user-groups", &[], Some(&p))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Update a user group's name and friendly name.")]
    pub async fn update_group(
        &self,
        Parameters(p): Parameters<UpdateGroupParams>,
    ) -> Result<Json<UserGroup>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/user-groups/{}", seg(&p.group_id)),
                &[],
                Some(&p.group),
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Delete a user group. Members are not deleted.")]
    pub async fn delete_group(
        &self,
        Parameters(p): Parameters<GroupIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/user-groups/{}", seg(&p.group_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("group {} deleted", p.group_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Replace a group's member list with the given set of user IDs (full replacement, not additive)."
    )]
    pub async fn set_group_users(
        &self,
        Parameters(p): Parameters<SetGroupUsersParams>,
    ) -> Result<Json<UserGroup>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/user-groups/{}/users", seg(&p.group_id)),
                &[],
                Some(&serde_json::json!({ "userIds": p.user_ids })),
            )
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(
        description = "Replace a user's custom claims (key/value pairs included in tokens) with the given set."
    )]
    pub async fn update_user_custom_claims(
        &self,
        Parameters(p): Parameters<UserClaimsParams>,
    ) -> Result<Json<Enveloped<Vec<CustomClaim>>>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/custom-claims/user/{}", seg(&p.user_id)),
                &[],
                Some(&p.claims),
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }

    #[tool(
        description = "Replace a user group's custom claims (key/value pairs included in tokens for members) with the given set."
    )]
    pub async fn update_group_custom_claims(
        &self,
        Parameters(p): Parameters<GroupClaimsParams>,
    ) -> Result<Json<Enveloped<Vec<CustomClaim>>>, String> {
        self.client
            .json(
                Method::PUT,
                &format!("/api/custom-claims/user-group/{}", seg(&p.group_id)),
                &[],
                Some(&p.claims),
            )
            .await
            .map(enveloped)
            .map_err(err_str)
    }
}

// ---------------------------------------------------------------------------
// Dangerous tier
// ---------------------------------------------------------------------------

#[tool_router(router = identity_dangerous_tools, vis = "pub(crate)")]
impl PocketIdServer {
    #[tool(description = "Permanently delete a user and all their credentials. Irreversible.")]
    pub async fn delete_user(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/users/{}", seg(&p.user_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("user {} deleted", p.user_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Delete one of a user's passkeys (WebAuthn credentials). If it is their only credential, the user may lose access to their account."
    )]
    pub async fn delete_user_passkey(
        &self,
        Parameters(p): Parameters<DeletePasskeyParams>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!(
                    "/api/users/{}/webauthn-credentials/{}",
                    seg(&p.user_id),
                    seg(&p.credential_id)
                ),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("passkey {} deleted for user {}", p.credential_id, p.user_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Create a signup token that lets holders self-register accounts on this instance."
    )]
    pub async fn create_signup_token(
        &self,
        Parameters(p): Parameters<CreateSignupTokenParams>,
    ) -> Result<Json<SignupToken>, String> {
        let mut body = serde_json::json!({
            "ttl": p.ttl,
            "usageLimit": p.usage_limit,
        });
        if let Some(groups) = &p.user_group_ids {
            body["userGroupIds"] = serde_json::json!(groups);
        }
        self.client
            .json(Method::POST, "/api/signup-tokens", &[], Some(&body))
            .await
            .map(Json)
            .map_err(err_str)
    }

    #[tool(description = "Delete (invalidate) a signup token.")]
    pub async fn delete_signup_token(
        &self,
        Parameters(p): Parameters<SignupTokenIdParam>,
    ) -> Result<String, String> {
        self.client
            .empty(
                Method::DELETE,
                &format!("/api/signup-tokens/{}", seg(&p.token_id)),
                &[],
                NO_BODY,
            )
            .await
            .map(|_| format!("signup token {} deleted", p.token_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Mint a one-time login token for a user. WARNING: whoever holds this token can log in as that user (impersonation)."
    )]
    pub async fn create_one_time_access_token(
        &self,
        Parameters(p): Parameters<UserIdParam>,
    ) -> Result<Json<Enveloped<AnyJson>>, String> {
        self.client
            .json(
                Method::POST,
                &format!("/api/users/{}/one-time-access-token", seg(&p.user_id)),
                &[],
                Some(&serde_json::json!({})),
            )
            .await
            .map(|v| enveloped(AnyJson(v)))
            .map_err(err_str)
    }

    #[tool(
        description = "Email a one-time login link to a user, as an admin action. The link grants access to that user's account."
    )]
    pub async fn send_one_time_access_email(
        &self,
        Parameters(p): Parameters<OneTimeAccessEmailAdminParams>,
    ) -> Result<String, String> {
        let body = match &p.ttl {
            Some(ttl) => serde_json::json!({ "ttl": ttl }),
            None => serde_json::json!({}),
        };
        self.client
            .empty(
                Method::POST,
                &format!("/api/users/{}/one-time-access-email", seg(&p.user_id)),
                &[],
                Some(&body),
            )
            .await
            .map(|_| format!("one-time access email sent to user {}", p.user_id))
            .map_err(err_str)
    }

    #[tool(
        description = "Request a one-time login email for an account by email address (the instance's \"email me a login link\" flow)."
    )]
    pub async fn request_one_time_access_email(
        &self,
        Parameters(p): Parameters<OneTimeAccessEmailParams>,
    ) -> Result<String, String> {
        let mut body = serde_json::json!({ "email": p.email });
        if let Some(path) = &p.redirect_path {
            body["redirectPath"] = serde_json::json!(path);
        }
        self.client
            .empty(Method::POST, "/api/one-time-access-email", &[], Some(&body))
            .await
            .map(|_| format!("one-time access email requested for {}", p.email))
            .map_err(err_str)
    }
}
