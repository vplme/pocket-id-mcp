//! Thin authenticated HTTP client for the Pocket ID REST API.

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    /// Transport-level failure (DNS, connect, TLS, timeout).
    #[error("network error calling {operation} on {host}: {source}")]
    Network {
        operation: String,
        host: String,
        #[source]
        source: reqwest::Error,
    },
    /// Non-2xx response from the API.
    #[error("Pocket ID API returned {status} for {operation}: {message}")]
    Api {
        status: StatusCode,
        operation: String,
        message: String,
    },
    /// 2xx response whose body did not match the expected shape.
    #[error("failed to decode response for {operation}: {reason}")]
    Decode { operation: String, reason: String },
    /// Local input problem (bad file path, invalid upload source, ...).
    #[error("{0}")]
    Input(String),
}

/// Source for a file upload: exactly one of `file_path` or `url`.
#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct FileSource {
    /// Absolute path to a local file to upload.
    pub file_path: Option<String>,
    /// HTTPS URL to fetch and re-upload.
    pub url: Option<String>,
}

#[derive(Debug)]
pub struct LoadedFile {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub content_type: String,
}

pub struct BinaryResponse {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

pub struct PocketIdClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl std::fmt::Debug for PocketIdClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // api_key deliberately omitted
        f.debug_struct("PocketIdClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// Extract a human-readable message from a Pocket ID error body without ever
/// echoing credentials. Bodies are JSON like `{"error": "..."}`; fall back to
/// truncated raw text.
fn extract_error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["error", "message", "detail"] {
            if let Some(msg) = v.get(key).and_then(|m| m.as_str()) {
                return msg.to_string();
            }
        }
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "(no error body)".to_string()
    } else {
        trimmed.chars().take(300).collect()
    }
}

impl PocketIdClient {
    pub fn new(base_url: &str, api_key: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("pocket-id-mcp/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client construction cannot fail with static config"),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn host(&self) -> String {
        url::Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| self.base_url.clone())
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
    ) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .http
            .request(method, url)
            .header("X-API-KEY", &self.api_key);
        if !query.is_empty() {
            req = req.query(query);
        }
        req
    }

    async fn execute(
        &self,
        req: reqwest::RequestBuilder,
        operation: &str,
    ) -> Result<reqwest::Response, ApiError> {
        let resp = req.send().await.map_err(|source| ApiError::Network {
            operation: operation.to_string(),
            host: self.host(),
            source,
        })?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(ApiError::Api {
            status,
            operation: operation.to_string(),
            message: extract_error_message(&body),
        })
    }

    async fn decode<T: DeserializeOwned>(
        resp: reqwest::Response,
        operation: &str,
    ) -> Result<T, ApiError> {
        let bytes = resp.bytes().await.map_err(|e| ApiError::Decode {
            operation: operation.to_string(),
            reason: e.to_string(),
        })?;
        serde_json::from_slice(&bytes).map_err(|e| ApiError::Decode {
            operation: operation.to_string(),
            reason: e.to_string(),
        })
    }

    /// JSON-in/JSON-out request. `body` is serialized when provided.
    pub async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&impl serde::Serialize>,
    ) -> Result<T, ApiError> {
        let operation = format!("{method} {path}");
        let mut req = self.request(method, path, query);
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = self.execute(req, &operation).await?;
        Self::decode(resp, &operation).await
    }

    /// Request whose response body is ignored (deletes, syncs, ...).
    pub async fn empty(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&impl serde::Serialize>,
    ) -> Result<(), ApiError> {
        let operation = format!("{method} {path}");
        let mut req = self.request(method, path, query);
        if let Some(body) = body {
            req = req.json(body);
        }
        self.execute(req, &operation).await.map(|_| ())
    }

    /// `application/x-www-form-urlencoded` POST (token introspection).
    pub async fn form<T: DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        let operation = format!("POST {path}");
        let req = self.request(Method::POST, path, &[]).form(form);
        let resp = self.execute(req, &operation).await?;
        Self::decode(resp, &operation).await
    }

    /// Binary GET preserving bytes and content type.
    pub async fn binary(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<BinaryResponse, ApiError> {
        let operation = format!("GET {path}");
        let req = self.request(Method::GET, path, query);
        let resp = self.execute(req, &operation).await?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let bytes = resp.bytes().await.map_err(|e| ApiError::Decode {
            operation,
            reason: e.to_string(),
        })?;
        Ok(BinaryResponse {
            bytes: bytes.to_vec(),
            content_type,
        })
    }

    /// Load upload bytes from a [`FileSource`], enforcing exactly one source.
    pub async fn load_file_source(&self, source: &FileSource) -> Result<LoadedFile, ApiError> {
        match (&source.file_path, &source.url) {
            (Some(_), Some(_)) => Err(ApiError::Input(
                "provide exactly one of file_path or url, not both".to_string(),
            )),
            (None, None) => Err(ApiError::Input(
                "provide exactly one of file_path or url".to_string(),
            )),
            (Some(path), None) => {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| ApiError::Input(format!("cannot read {path}: {e}")))?;
                let file_name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "upload".to_string());
                let content_type = mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string();
                Ok(LoadedFile {
                    bytes,
                    file_name,
                    content_type,
                })
            }
            (None, Some(url)) => {
                let parsed = url::Url::parse(url)
                    .map_err(|e| ApiError::Input(format!("invalid url: {e}")))?;
                if parsed.scheme() != "https" {
                    return Err(ApiError::Input("url uploads must use https".to_string()));
                }
                let operation = format!("fetch {url}");
                let resp = self
                    .http
                    .get(parsed.clone())
                    .send()
                    .await
                    .map_err(|source| ApiError::Network {
                        operation: operation.clone(),
                        host: parsed.host_str().unwrap_or("?").to_string(),
                        source,
                    })?;
                if !resp.status().is_success() {
                    return Err(ApiError::Api {
                        status: resp.status(),
                        operation,
                        message: "fetching upload source failed".to_string(),
                    });
                }
                let content_type = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
                    .filter(|v| !v.is_empty() && v != "application/octet-stream")
                    .unwrap_or_else(|| {
                        mime_guess::from_path(parsed.path())
                            .first_or_octet_stream()
                            .essence_str()
                            .to_string()
                    });
                let file_name = parsed
                    .path_segments()
                    .and_then(|mut s| s.next_back())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("upload")
                    .to_string();
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| ApiError::Input(format!("reading {url} failed: {e}")))?;
                Ok(LoadedFile {
                    bytes: bytes.to_vec(),
                    file_name,
                    content_type,
                })
            }
        }
    }

    /// Multipart upload with a `file` field; ignores any response body.
    pub async fn upload(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        file: LoadedFile,
    ) -> Result<(), ApiError> {
        let operation = format!("{method} {path}");
        let part = reqwest::multipart::Part::bytes(file.bytes)
            .file_name(file.file_name)
            .mime_str(&file.content_type)
            .map_err(|e| ApiError::Input(format!("invalid content type: {e}")))?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let req = self.request(method, path, query).multipart(form);
        self.execute(req, &operation).await.map(|_| ())
    }
}

/// Serializable unit for calls without a body, so `Option<&()>::None` isn't
/// needed at call sites.
pub const NO_BODY: Option<&()> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn api_key_header_sent_and_error_mapped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users"))
            .and(header("X-API-KEY", "sekrit"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": "API key lacks permission"
            })))
            .mount(&server)
            .await;

        let client = PocketIdClient::new(&server.uri(), "sekrit".to_string());
        let err = client
            .json::<serde_json::Value>(Method::GET, "/api/users", &[], NO_BODY)
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("403"), "got: {text}");
        assert!(text.contains("API key lacks permission"), "got: {text}");
        assert!(text.contains("GET /api/users"), "got: {text}");
        assert!(!text.contains("sekrit"), "key leaked: {text}");
    }

    #[tokio::test]
    async fn network_error_names_host() {
        // Port 1 on localhost: nothing listens there.
        let client = PocketIdClient::new("http://127.0.0.1:1", "k".to_string());
        let err = client
            .json::<serde_json::Value>(Method::GET, "/api/users", &[], NO_BODY)
            .await
            .unwrap_err();
        match &err {
            ApiError::Network { host, .. } => assert_eq!(host, "127.0.0.1"),
            other => panic!("expected network error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn query_params_forwarded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users"))
            .and(query_param("pagination[page]", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .expect(1)
            .mount(&server)
            .await;

        let client = PocketIdClient::new(&server.uri(), "k".to_string());
        let ok: serde_json::Value = client
            .json(
                Method::GET,
                "/api/users",
                &[("pagination[page]".to_string(), "2".to_string())],
                NO_BODY,
            )
            .await
            .unwrap();
        assert_eq!(ok["data"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn upload_source_validation() {
        let client = PocketIdClient::new("http://127.0.0.1:1", "k".to_string());

        let both = FileSource {
            file_path: Some("/tmp/x.png".into()),
            url: Some("https://example.com/x.png".into()),
        };
        assert!(matches!(
            client.load_file_source(&both).await.unwrap_err(),
            ApiError::Input(_)
        ));

        let neither = FileSource {
            file_path: None,
            url: None,
        };
        assert!(matches!(
            client.load_file_source(&neither).await.unwrap_err(),
            ApiError::Input(_)
        ));

        let http_url = FileSource {
            file_path: None,
            url: Some("http://example.com/x.png".into()),
        };
        let err = client.load_file_source(&http_url).await.unwrap_err();
        assert!(err.to_string().contains("https"));
    }

    #[tokio::test]
    async fn upload_from_file_infers_name_and_type() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("logo.png");
        std::fs::write(&file_path, b"\x89PNG fake").unwrap();

        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/application-images/logo"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = PocketIdClient::new(&server.uri(), "k".to_string());
        let loaded = client
            .load_file_source(&FileSource {
                file_path: Some(file_path.to_string_lossy().to_string()),
                url: None,
            })
            .await
            .unwrap();
        assert_eq!(loaded.file_name, "logo.png");
        assert_eq!(loaded.content_type, "image/png");
        client
            .upload(Method::PUT, "/api/application-images/logo", &[], loaded)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn binary_download_preserves_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/application-images/logo"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/svg+xml; charset=utf-8")
                    .set_body_bytes(b"<svg/>".to_vec()),
            )
            .mount(&server)
            .await;

        let client = PocketIdClient::new(&server.uri(), "k".to_string());
        let bin = client
            .binary("/api/application-images/logo", &[])
            .await
            .unwrap();
        assert_eq!(bin.content_type, "image/svg+xml");
        assert_eq!(bin.bytes, b"<svg/>");
    }

    #[test]
    fn debug_never_prints_key() {
        let client = PocketIdClient::new("https://id.example.com", "super-secret".to_string());
        let debug = format!("{client:?}");
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn error_message_extraction() {
        assert_eq!(extract_error_message(r#"{"error":"nope"}"#), "nope");
        assert_eq!(extract_error_message(r#"{"message":"bad"}"#), "bad");
        assert_eq!(extract_error_message(""), "(no error body)");
        assert_eq!(extract_error_message("plain text"), "plain text");
    }
}
