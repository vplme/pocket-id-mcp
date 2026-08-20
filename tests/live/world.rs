//! Per-scenario state: one spawned MCP server, the "that X" references the
//! steps talk about, and the REST paths to clean up afterwards.

use std::collections::HashMap;

use cucumber::gherkin::Step;
use serde_json::Value;

use crate::common::{LiveEnv, Mcp, Mode, unique};

#[derive(Debug, cucumber::World)]
#[world(init = Self::new)]
pub struct LiveWorld {
    pub env: &'static LiveEnv,
    /// Per-scenario token that `{unique}` expands to.
    pub unique: String,
    pub mcp: Option<Mcp>,
    /// Advertised input schemas by tool name, for typing data-table cells.
    pub schemas: HashMap<String, Value>,
    /// Text of the last tool-level error (steps that expect failure set it).
    pub last_error: Option<String>,
    /// stderr + status of a server started directly (startup scenarios).
    pub process: Option<std::process::Output>,
    // "that ..." references
    pub client_id: Option<String>,
    pub group_id: Option<String>,
    pub user_id: Option<String>,
    pub api_id: Option<String>,
    pub secret: Option<String>,
    pub permission_ids: HashMap<String, String>,
    /// Application configuration as last read (flat key → value) and the
    /// appName found there, so the configuration scenario can restore it.
    pub app_config: Option<serde_json::Map<String, Value>>,
    pub original_app_name: Option<Value>,
    /// REST paths deleted after the scenario (best effort).
    pub cleanup: Vec<String>,
}

impl LiveWorld {
    async fn new() -> Self {
        LiveWorld {
            env: LiveEnv::acquire().await,
            unique: unique("live"),
            mcp: None,
            schemas: HashMap::new(),
            last_error: None,
            process: None,
            client_id: None,
            group_id: None,
            user_id: None,
            api_id: None,
            secret: None,
            permission_ids: HashMap::new(),
            app_config: None,
            original_app_name: None,
            cleanup: Vec::new(),
        }
    }

    /// Spawn (or replace) the scenario's MCP server and cache its schemas.
    pub async fn spawn(&mut self, mode: Mode) {
        if let Some(old) = self.mcp.take() {
            old.shutdown().await;
        }
        let mcp = Mcp::spawn(self.env, mode).await;
        self.schemas = mcp
            .tools()
            .await
            .into_iter()
            .map(|t| {
                let schema = Value::Object((*t.input_schema).clone());
                (t.name.to_string(), schema)
            })
            .collect();
        self.mcp = Some(mcp);
    }

    pub fn mcp(&self) -> &Mcp {
        self.mcp
            .as_ref()
            .expect("an MCP server (use the Background step)")
    }

    /// Expand `{unique}` placeholders in feature text.
    pub fn expand(&self, text: &str) -> String {
        text.replace("{unique}", &self.unique)
    }

    pub fn client_id(&self) -> &str {
        self.client_id
            .as_deref()
            .expect("a client created earlier in the scenario")
    }
    pub fn group_id(&self) -> &str {
        self.group_id
            .as_deref()
            .expect("a group created earlier in the scenario")
    }
    pub fn user_id(&self) -> &str {
        self.user_id
            .as_deref()
            .expect("a user created earlier in the scenario")
    }
    pub fn api_id(&self) -> &str {
        self.api_id
            .as_deref()
            .expect("an API definition created earlier in the scenario")
    }
    pub fn last_error(&self) -> &str {
        self.last_error
            .as_deref()
            .expect("a failed tool call earlier in the scenario")
    }

    /// Turn a two-column data table into tool arguments, typing each cell by
    /// the tool's advertised input schema (booleans, numbers, arrays split on
    /// `, `). Unknown fields fail loudly instead of being sent as strings.
    pub fn args_from_table(&self, tool: &str, step: &Step) -> serde_json::Map<String, Value> {
        let schema = self
            .schemas
            .get(tool)
            .unwrap_or_else(|| panic!("tool {tool} not advertised by the server"));
        let props = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("schema of {tool} has no properties"));
        let mut args = serde_json::Map::new();
        for row in &step.table().expect("a data table").rows {
            let (key, cell) = (row[0].as_str(), self.expand(&row[1]));
            let prop = props.get(key).unwrap_or_else(|| {
                panic!(
                    "{tool} has no parameter `{key}` (schema: {})",
                    Value::Object(props.clone())
                )
            });
            let items = prop["items"]["type"].as_str();
            args.insert(key.to_string(), coerce(prop["type"].as_str(), items, &cell));
        }
        args
    }

    /// Assert every `| field | value |` row of a table against a JSON
    /// record, typing the expected cell by the record's own value.
    pub fn assert_table_matches(&self, record: &Value, step: &Step) {
        for row in &step.table().expect("a data table").rows {
            let (key, cell) = (row[0].as_str(), self.expand(&row[1]));
            let actual = &record[key];
            let expected = match actual {
                Value::Bool(_) => coerce(Some("boolean"), None, &cell),
                Value::Number(_) => coerce(Some("number"), None, &cell),
                Value::Array(_) => coerce(Some("array"), Some("string"), &cell),
                _ => Value::String(cell),
            };
            assert_eq!(
                actual, &expected,
                "field `{key}` in Pocket ID record {record}"
            );
        }
    }

    /// `| key | value |` rows → custom-claim inputs for the claims tools.
    pub fn claims_from_table(&self, step: &Step) -> Vec<Value> {
        step.table()
            .expect("a | key | value | table")
            .rows
            .iter()
            .map(|row| serde_json::json!({"key": row[0], "value": self.expand(&row[1])}))
            .collect()
    }

    /// Assert a record's `customClaims` contains every `| key | value |` row.
    pub fn assert_claims(&self, record: &Value, step: &Step) {
        let claims = record["customClaims"]
            .as_array()
            .unwrap_or_else(|| panic!("no customClaims array in {record}"));
        for row in &step.table().expect("a | key | value | table").rows {
            let (key, value) = (&row[0], self.expand(&row[1]));
            assert!(
                claims
                    .iter()
                    .any(|c| c["key"] == key.as_str() && c["value"] == value.as_str()),
                "claim {key}={value} missing in Pocket ID record: {claims:?}"
            );
        }
    }

    pub async fn teardown(&mut self) {
        if let Some(mcp) = self.mcp.take() {
            mcp.shutdown().await;
        }
        let paths = std::mem::take(&mut self.cleanup);
        self.env.cleanup(&paths).await;
    }
}

fn coerce(ty: Option<&str>, items: Option<&str>, cell: &str) -> Value {
    match ty {
        Some("boolean") => Value::Bool(
            cell.parse()
                .unwrap_or_else(|_| panic!("not a boolean: {cell}")),
        ),
        Some("integer") | Some("number") => {
            serde_json::from_str(cell).unwrap_or_else(|_| panic!("not a number: {cell}"))
        }
        Some("array") => Value::Array(
            cell.split(", ")
                .filter(|s| !s.is_empty())
                .map(|s| coerce(items.or(Some("string")), None, s))
                .collect(),
        ),
        _ => Value::String(cell.to_string()),
    }
}
