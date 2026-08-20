//! Subject-generic steps: "that client / user / group / API definition" can
//! be read back from Pocket ID, compared with what a read tool reports, and
//! checked for presence in lists.

use std::str::FromStr;

use cucumber::gherkin::Step;
use cucumber::{Parameter, then};
use serde_json::{Value, json};

use crate::common::has_id;
use crate::world::LiveWorld;

#[derive(Debug, Clone, Copy, Parameter)]
#[param(name = "subject", regex = "client|user|group|API definition")]
pub enum Subject {
    Client,
    User,
    Group,
    ApiDefinition,
}

impl FromStr for Subject {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "client" => Subject::Client,
            "user" => Subject::User,
            "group" => Subject::Group,
            "API definition" => Subject::ApiDefinition,
            other => return Err(format!("unknown subject {other}")),
        })
    }
}

impl Subject {
    /// Name of the tool parameter carrying this subject's id.
    pub fn id_param(self) -> &'static str {
        match self {
            Subject::Client => "client_id",
            Subject::User => "user_id",
            Subject::Group => "group_id",
            Subject::ApiDefinition => "api_id",
        }
    }

    /// Pocket ID collection path (REST).
    pub fn collection(self) -> &'static str {
        match self {
            Subject::Client => "/api/oidc/clients",
            Subject::User => "/api/users",
            Subject::Group => "/api/user-groups",
            Subject::ApiDefinition => "/api/apis",
        }
    }

    pub fn path(self, id: &str) -> String {
        format!("{}/{id}", self.collection())
    }
}

impl LiveWorld {
    pub fn id_of(&self, subject: Subject) -> &str {
        match subject {
            Subject::Client => self.client_id(),
            Subject::User => self.user_id(),
            Subject::Group => self.group_id(),
            Subject::ApiDefinition => self.api_id(),
        }
    }

    pub fn name_of(&self, subject: Subject) -> &str {
        let name = match subject {
            Subject::Client => &self.client_name,
            Subject::User => &self.user_name,
            Subject::Group => &self.group_name,
            Subject::ApiDefinition => &self.api_name,
        };
        name.as_deref()
            .unwrap_or_else(|| panic!("no {subject:?} created earlier in the scenario"))
    }

    pub async fn record_of(&self, subject: Subject) -> Value {
        self.env.get_ok(&subject.path(self.id_of(subject))).await
    }
}

/// Every non-null value the tool reported must appear identically in Pocket
/// ID's record (the tool's DTOs may omit fields, never invent them).
fn assert_subset(tool_value: &Value, record: &Value, path: &str) {
    match (tool_value, record) {
        (Value::Null, _) => {}
        (Value::Object(t), Value::Object(r)) => {
            for (k, v) in t {
                assert_subset(v, r.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"));
            }
        }
        (Value::Array(t), Value::Array(r)) => {
            assert_eq!(
                t.len(),
                r.len(),
                "array length at {path}: tool {t:?} vs Pocket ID {r:?}"
            );
            for (i, (tv, rv)) in t.iter().zip(r).enumerate() {
                assert_subset(tv, rv, &format!("{path}[{i}]"));
            }
        }
        (t, r) => assert_eq!(t, r, "value at {path}: tool vs Pocket ID"),
    }
}

#[then(expr = "Pocket ID's record of that {subject} has:")]
async fn record_has(w: &mut LiveWorld, subject: Subject, step: &Step) {
    let record = w.record_of(subject).await;
    w.assert_table_matches(&record, step);
}

#[then(expr = "Pocket ID still has that {subject}")]
async fn still_has(w: &mut LiveWorld, subject: Subject) {
    let (status, body) = w.env.get(&subject.path(w.id_of(subject))).await;
    assert!(status.is_success(), "{subject:?} gone: {status} {body}");
}

#[then(expr = "Pocket ID no longer has that {subject}")]
async fn no_longer_has(w: &mut LiveWorld, subject: Subject) {
    let (status, body) = w.env.get(&subject.path(w.id_of(subject))).await;
    assert_eq!(status, 404, "{subject:?} still present: {body}");
}

#[then(expr = "Pocket ID lists that {subject} when searching for {string}")]
async fn listed_by_search(w: &mut LiveWorld, subject: Subject, search: String) {
    let listed = w
        .env
        .get_ok(&format!(
            "{}?search={}",
            subject.collection(),
            w.expand(&search)
        ))
        .await;
    assert!(
        has_id(&listed, w.id_of(subject)),
        "{subject:?}s listed by Pocket ID: {listed}"
    );
}

/// A read tool called for the subject must agree with Pocket ID's record.
#[then(expr = "{string} for that {subject} agrees with Pocket ID")]
async fn tool_agrees(w: &mut LiveWorld, tool: String, subject: Subject) {
    let id = w.id_of(subject).to_string();
    let reported = w
        .mcp()
        .call_json(&tool, json!({subject.id_param(): id}))
        .await;
    assert_eq!(
        reported["id"], id,
        "{tool} reported a different record: {reported}"
    );
    let record = w.record_of(subject).await;
    assert_subset(&reported, &record, &tool);
}

/// A list tool, searched by the subject's name, must include it.
#[then(expr = "{string} lists that {subject}")]
async fn tool_lists(w: &mut LiveWorld, tool: String, subject: Subject) {
    let search = w.name_of(subject).to_string();
    let listed = w.mcp().call_json(&tool, json!({"search": search})).await;
    assert!(has_id(&listed, w.id_of(subject)), "{tool}: {listed}");
}
