//! Shared fixture for the live suite: a real Pocket ID (Docker container, or
//! an instance named by env), REST helpers for independent verification, and
//! an MCP client driving the real `pocket-id-mcp` binary over stdio.
//!
//! Configuration (all optional):
//! - `POCKET_ID_LIVE_URL` + `POCKET_ID_LIVE_API_KEY`: use an existing
//!   instance instead of starting Docker.
//! - `POCKET_ID_LIVE_IMAGE`: container image; defaults to the Pocket ID
//!   release the vendored `spec/swagger.yaml` was taken from.
//! - `POCKET_ID_LIVE_PORT`: host port for the container (default 1431).
//!
//! The container (`pocket-id-mcp-live`) is left running after the run so
//! state can be inspected; the next run replaces it.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::{Method, StatusCode, header};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RunningService, ServiceError};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::OnceCell;

pub const CONTAINER_NAME: &str = "pocket-id-mcp-live";
/// Pinned to the Pocket ID release described by the vendored swagger spec.
pub const DEFAULT_IMAGE: &str = "ghcr.io/pocket-id/pocket-id:v2.13.0";
pub const DEFAULT_PORT: u16 = 1431;
/// The real server binary built by cargo for this test run.
pub const BIN: &str = env!("CARGO_BIN_EXE_pocket-id-mcp");

const HEALTH_TIMEOUT: Duration = Duration::from_secs(90);

pub struct LiveEnv {
    pub base_url: String,
    pub api_key: String,
    /// Version expected from `/api/version/current`, derived from the image
    /// tag when we started the container ourselves.
    pub expected_version: Option<String>,
    /// A second API key minted during bootstrap, for tests that need a key to
    /// act upon (Pocket ID refuses API-key-authenticated key creation, so it
    /// can only come from the admin session). `None` for a user-supplied
    /// instance.
    pub spare_api_key: Option<ApiKey>,
}

/// An API key as minted by Pocket ID: record id plus the one-time token.
#[derive(Clone, Debug)]
pub struct ApiKey {
    pub id: String,
    pub token: String,
}

static ENV: OnceCell<LiveEnv> = OnceCell::const_new();

impl LiveEnv {
    pub async fn acquire() -> &'static LiveEnv {
        ENV.get_or_init(Self::init).await
    }

    async fn init() -> LiveEnv {
        if let (Ok(url), Ok(key)) = (
            env::var("POCKET_ID_LIVE_URL"),
            env::var("POCKET_ID_LIVE_API_KEY"),
        ) {
            return LiveEnv {
                base_url: url.trim_end_matches('/').to_string(),
                api_key: key,
                expected_version: None,
                spare_api_key: None,
            };
        }

        let image = env::var("POCKET_ID_LIVE_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());
        let port: u16 = env::var("POCKET_ID_LIVE_PORT")
            .ok()
            .map(|p| {
                p.parse()
                    .expect("POCKET_ID_LIVE_PORT must be a port number")
            })
            .unwrap_or(DEFAULT_PORT);
        // APP_URL must be the host-visible URL, so the port is fixed up front.
        let base_url = format!("http://localhost:{port}");

        // Replace whatever a previous run left behind.
        let _ = docker(&["rm", "-f", CONTAINER_NAME]).await;
        docker(&[
            "run",
            "-d",
            "--name",
            CONTAINER_NAME,
            "-p",
            &format!("127.0.0.1:{port}:1411"),
            "-e",
            &format!("APP_URL={base_url}"),
            "-e",
            "PORT=1411",
            "-e",
            "ENCRYPTION_KEY=pocket-id-mcp-live-tests-32bytes",
            "-e",
            "ANALYTICS_DISABLED=true",
            &image,
        ])
        .await
        .unwrap_or_else(|e| panic!("starting Pocket ID container: {e}"));

        wait_healthy(&base_url).await;
        let (api_key, spare) = bootstrap_api_keys(&base_url).await;
        let expected_version = image
            .rsplit_once(':')
            .map(|(_, tag)| tag.trim_start_matches('v').to_string())
            .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()));
        LiveEnv {
            base_url,
            api_key: api_key.token,
            expected_version,
            spare_api_key: Some(spare),
        }
    }

    // --- REST verification helpers (raw reqwest, independent of our client) ---

    /// A fresh client per call: reqwest pools are bound to the runtime that
    /// created them, and every `#[tokio::test]` runs on its own runtime.
    fn http(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Send an authenticated JSON request; returns status and parsed body
    /// (or the raw text as a JSON string when the body is not JSON).
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.send_with_key(method, path, body, &self.api_key).await
    }

    pub async fn send_with_key(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        api_key: &str,
    ) -> (StatusCode, Value) {
        let mut req = self
            .http()
            .request(method.clone(), self.url(path))
            .header("X-API-KEY", api_key);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .send()
            .await
            .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.send(Method::GET, path, None).await
    }

    /// GET that must succeed; returns the parsed body.
    pub async fn get_ok(&self, path: &str) -> Value {
        let (status, body) = self.get(path).await;
        assert!(status.is_success(), "GET {path} -> {status}: {body}");
        body
    }

    /// GET a binary resource: status, content type, bytes.
    pub async fn get_bytes(&self, path: &str) -> (StatusCode, String, Vec<u8>) {
        let resp = self
            .http()
            .get(self.url(path))
            .header("X-API-KEY", &self.api_key)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {path}: {e}"));
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = resp.bytes().await.expect("body").to_vec();
        (status, content_type, bytes)
    }

    pub async fn delete(&self, path: &str) -> StatusCode {
        self.send(Method::DELETE, path, None).await.0
    }

    /// Best-effort cleanup of several resources; failures are ignored
    /// because the container is fresh per run anyway.
    pub async fn cleanup(&self, paths: &[String]) {
        for p in paths {
            let _ = self.delete(p).await;
        }
    }
}

async fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("cannot run docker ({e}); install Docker or set POCKET_ID_LIVE_URL + POCKET_ID_LIVE_API_KEY"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "docker {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

async fn wait_healthy(base_url: &str) {
    let http = reqwest::Client::new();
    let deadline = Instant::now() + HEALTH_TIMEOUT;
    loop {
        if let Ok(resp) = http.get(format!("{base_url}/healthz")).send().await {
            if resp.status().is_success() {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Pocket ID at {base_url} did not become healthy within {HEALTH_TIMEOUT:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// First-admin bootstrap exactly as a fresh instance allows it: the one-time
/// `/api/signup/setup` call yields an admin session cookie (no passkey
/// needed), which then mints the suite's API key plus a spare one.
async fn bootstrap_api_keys(base_url: &str) -> (ApiKey, ApiKey) {
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{base_url}/api/signup/setup"))
        .json(&json!({"username": "admin", "email": "admin@example.com"}))
        .send()
        .await
        .expect("signup/setup request");
    assert!(
        resp.status().is_success(),
        "signup/setup failed: {} (instance already set up? pass POCKET_ID_LIVE_URL + POCKET_ID_LIVE_API_KEY)",
        resp.status()
    );
    // The cookie is flagged Secure even on a plain-http dev instance, so a
    // cookie jar would drop it; forward it by hand.
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("access_token="))
        .and_then(|c| c.split(';').next())
        .expect("signup/setup returned an access_token cookie")
        .to_string();

    let mint = |name: &'static str| {
        let http = http.clone();
        let cookie = cookie.clone();
        async move {
            let resp = http
                .post(format!("{base_url}/api/api-keys"))
                .header(header::COOKIE, cookie)
                .json(&json!({"name": name, "expiresAt": "2030-01-01T00:00:00Z"}))
                .send()
                .await
                .expect("api-keys request");
            assert!(
                resp.status().is_success(),
                "api key creation failed: {}",
                resp.status()
            );
            let body: Value = resp.json().await.expect("api key json");
            ApiKey {
                id: str_of(&body["apiKey"], "id").to_string(),
                token: str_of(&body, "token").to_string(),
            }
        }
    };
    (mint("live-suite").await, mint("live-suite-spare").await)
}

// ---------------------------------------------------------------------------
// MCP client over the real binary
// ---------------------------------------------------------------------------

/// Safety-tier configuration for a spawned server.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mode {
    pub read_only: bool,
    pub allow_dangerous: bool,
}

impl Mode {
    pub const DEFAULT: Mode = Mode {
        read_only: false,
        allow_dangerous: false,
    };
    pub const READ_ONLY: Mode = Mode {
        read_only: true,
        allow_dangerous: false,
    };
    pub const DANGEROUS: Mode = Mode {
        read_only: false,
        allow_dangerous: true,
    };
}

/// Build the server command with the given environment; shared by the MCP
/// client and the startup-behaviour tests that inspect the process directly.
pub fn server_command(base_url: &str, api_key: &str, mode: Mode) -> Command {
    Command::new(BIN).configure(|cmd| {
        cmd.env("POCKET_ID_URL", base_url)
            .env("POCKET_ID_API_KEY", api_key)
            .env("POCKET_ID_MCP_TRANSPORT", "stdio")
            .env("POCKET_ID_MCP_READ_ONLY", mode.read_only.to_string())
            .env(
                "POCKET_ID_MCP_ALLOW_DANGEROUS",
                mode.allow_dangerous.to_string(),
            )
            .env("RUST_LOG", "pocket_id_mcp=warn");
    })
}

/// An MCP client connected to a freshly spawned `pocket-id-mcp` process.
pub struct Mcp {
    svc: RunningService<RoleClient, ()>,
}

impl Mcp {
    pub async fn spawn(env: &LiveEnv, mode: Mode) -> Mcp {
        let transport = TokioChildProcess::new(server_command(&env.base_url, &env.api_key, mode))
            .expect("spawn pocket-id-mcp");
        let svc = ().serve(transport).await.expect("MCP initialize handshake");
        Mcp { svc }
    }

    pub async fn tool_names(&self) -> BTreeSet<String> {
        self.svc
            .list_all_tools()
            .await
            .expect("tools/list")
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// Raw tool call: protocol-level errors (e.g. unknown tool) surface as `Err`.
    pub async fn try_call(&self, name: &str, args: Value) -> Result<CallToolResult, ServiceError> {
        let mut params = CallToolRequestParams::new(name.to_string());
        match args {
            Value::Null => {}
            Value::Object(map) => params = params.with_arguments(map),
            other => panic!("tool arguments must be a JSON object, got {other}"),
        }
        self.svc.call_tool(params).await
    }

    /// Tool call that must succeed (no protocol error, `isError` not set).
    pub async fn call(&self, name: &str, args: Value) -> CallToolResult {
        let result = self
            .try_call(name, args.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {args}: protocol error: {e}"));
        assert!(
            result.is_error != Some(true),
            "{name} {args} returned an error: {}",
            text_of(&result)
        );
        result
    }

    /// Successful tool call, returning its structured (or JSON-text) payload.
    pub async fn call_json(&self, name: &str, args: Value) -> Value {
        structured(&self.call(name, args).await)
    }

    /// Tool call that must fail at the tool level; returns the error text.
    pub async fn call_err(&self, name: &str, args: Value) -> String {
        let result = self
            .try_call(name, args.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {args}: protocol error: {e}"));
        let text = text_of(&result);
        assert!(
            result.is_error == Some(true),
            "{name} {args} unexpectedly succeeded: {text}"
        );
        text
    }

    pub async fn shutdown(self) {
        let _ = self.svc.cancel().await;
    }
}

/// Concatenated text blocks of a tool result.
pub fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Structured payload of a tool result: `structuredContent` when present,
/// otherwise the first text block parsed as JSON.
pub fn structured(result: &CallToolResult) -> Value {
    if let Some(v) = &result.structured_content {
        return v.clone();
    }
    let text = text_of(result);
    serde_json::from_str(&text)
        .unwrap_or_else(|_| panic!("tool result is neither structured nor JSON text: {text}"))
}

/// Unique, recognisable name so parallel tests never collide.
pub fn unique(prefix: &str) -> String {
    format!("{prefix}-{:08x}", rand::random::<u32>())
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/live/fixtures")
        .join(name)
}

/// `id` values of a list body — a bare array or a paginated `{data: [...]}`.
pub fn ids(list: &Value) -> Vec<&str> {
    let items = list
        .as_array()
        .or_else(|| list["data"].as_array())
        .unwrap_or_else(|| panic!("not a list body: {list}"));
    items.iter().filter_map(|i| i["id"].as_str()).collect()
}

pub fn has_id(list: &Value, id: &str) -> bool {
    ids(list).contains(&id)
}

/// Required string field of a JSON object.
pub fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field `{key}` in {v}"))
}
