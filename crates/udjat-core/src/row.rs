//! Structural fields computed with the OpenCode enriched-row SQL semantics.
use crate::{HarnessKind, MessageRecord, MessageRole, Part, PartKind, Session};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub session: Session,
    pub status: StructuralStatus,
    /// Session fields absent from the legacy Session API, including archived
    /// time, project identity and lightweight-session metadata.
    pub extra: serde_json::Map<String, Value>,
}
impl SessionRow {
    pub fn new(session: Session, status: StructuralStatus) -> Self {
        Self {
            session,
            status,
            extra: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompleteAssistantLastPart {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StructuralStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_time_created: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_time_completed: Option<i64>,
    pub last_message_errored: bool,
    pub parts_total: usize,
    pub running_tools: usize,
    pub interrupted_tools: usize,
    pub stuck_tools: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_incomplete_assistant_last_part: Option<IncompleteAssistantLastPart>,
    pub prior_completed_assistant_siblings: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_user_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_user_message_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_user_message_agent: Option<String>,
    #[serde(
        rename = "lastUserMessageProviderID",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_user_message_provider_id: Option<String>,
    #[serde(
        rename = "lastUserMessageModelID",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_user_message_model_id: Option<String>,
    pub child_count: usize,
}
fn message_key(m: &MessageRecord) -> (i64, &str) {
    (
        m.info.storage_time_created.unwrap_or(m.info.time_created),
        &m.info.id,
    )
}
fn part_key(p: &Part) -> (i64, &str) {
    (p.storage_time_created.unwrap_or(0), &p.id)
}
impl StructuralStatus {
    pub fn from_messages(messages: &[MessageRecord], child_count: usize) -> Self {
        let mut status = Self {
            child_count,
            ..Default::default()
        };
        let Some(last) = messages.iter().max_by_key(|m| message_key(m)) else {
            return status;
        };
        let info = &last.info;
        if matches!(info.role, MessageRole::User | MessageRole::Assistant) {
            status.last_message_role = Some(info.role.as_str().into());
        }
        status.last_message_time_created = Some(message_key(last).0);
        status.last_message_time_completed = info.time_completed;
        status.last_message_errored = info.error.as_ref().is_some_and(|v| !v.is_null());
        status.parts_total = last.parts.len();
        for part in &last.parts {
            if let PartKind::Tool { state, .. } = &part.kind {
                if matches!(state.status.as_str(), "running" | "pending") {
                    status.running_tools += 1;
                }
                if state.status == "error"
                    && state
                        .metadata
                        .as_ref()
                        .and_then(|v| v.get("interrupted"))
                        .is_some_and(|v| v == true || v == 1)
                {
                    status.interrupted_tools += 1;
                }
            }
        }
        status.stuck_tools = status.running_tools + status.interrupted_tools;
        status.prior_completed_assistant_siblings = messages
            .iter()
            .filter(|m| {
                m.info.id != info.id
                    && m.info.role == MessageRole::Assistant
                    && m.info.time_completed.is_some()
                    && m.info.error.as_ref().is_none_or(Value::is_null)
                    && info.parent_id.is_some()
                    && m.info.parent_id == info.parent_id
            })
            .count();
        let incomplete = messages
            .iter()
            .filter(|m| m.info.role == MessageRole::Assistant && m.info.time_completed.is_none())
            .max_by_key(|m| message_key(m));
        if let Some(incomplete) = incomplete {
            let progressed = messages.iter().any(|m| {
                m.info.role == MessageRole::Assistant
                    && m.info.time_completed.is_some()
                    && message_key(m) > message_key(incomplete)
            });
            if !progressed && let Some(part) = incomplete.parts.iter().max_by_key(|p| part_key(p)) {
                let (tool_name, tool_status) = match &part.kind {
                    PartKind::Tool { tool, state, .. } => {
                        (Some(tool.clone()), Some(state.status.clone()))
                    }
                    _ => (None, None),
                };
                status.last_incomplete_assistant_last_part = Some(IncompleteAssistantLastPart {
                    type_name: part.kind.type_name().into(),
                    tool_name,
                    tool_status,
                });
            }
        }
        if let Some(user) = messages
            .iter()
            .filter(|m| m.info.role == MessageRole::User)
            .max_by_key(|m| message_key(m))
        {
            status.last_user_message_id = Some(user.info.id.clone());
            let mut parts = user.parts.iter().collect::<Vec<_>>();
            parts.sort_by_key(|p| part_key(p));
            let texts = parts
                .iter()
                .filter_map(|p| match &p.kind {
                    PartKind::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !texts.is_empty() {
                status.last_user_message_text = Some(texts.join("\n"));
            }
            status.last_user_message_agent = user.info.agent.clone();
            if user.info.source.is_some() {
                status.last_user_message_provider_id = user.info.provider_id.clone();
                status.last_user_message_model_id = user.info.model_id.clone();
            } else {
                // The SQL reads the USER message's nested model, not the
                // assistant-style top-level attribution fields.
                let model = user.info.extra.get("model");
                status.last_user_message_provider_id = model
                    .and_then(|m| m.get("providerID"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                status.last_user_message_model_id = model
                    .and_then(|m| m.get("modelID"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
        status
    }
}
pub fn to_opencode_v1_row(row: &SessionRow) -> Value {
    let s = &row.session;
    let mut v = Value::Object(row.extra.clone());
    v["id"] = json!(s.session_id);
    v["title"] = json!(s.title);
    v["directory"] = json!(s.project_dir);
    if !v["time"].is_object() {
        v["time"] = json!({});
    }
    v["time"]["created"] = json!(s.started_at.timestamp_millis());
    v["time"]["updated"] = json!(s.updated_at.timestamp_millis());
    if let Some(parent) = &s.parent_id {
        v["parentID"] = json!(parent);
    }
    v.as_object_mut().expect("row object").extend(
        serde_json::to_value(&row.status)
            .expect("status serializes")
            .as_object()
            .expect("status object")
            .clone(),
    );
    v
}
pub fn from_opencode_v1_row(v: Value, harness: HarnessKind) -> Result<SessionRow> {
    let text = |key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| format!("session row missing {key}"))
    };
    let time = |key: &str| {
        v.get("time")
            .and_then(|t| t.get(key))
            .and_then(Value::as_i64)
            .and_then(chrono::DateTime::from_timestamp_millis)
            .with_context(|| format!("invalid row time.{key}"))
    };
    let session = Session {
        session_id: text("id")?,
        harness,
        title: text("title")?,
        project_dir: text("directory")?,
        parent_id: v.get("parentID").and_then(Value::as_str).map(str::to_owned),
        started_at: time("created")?,
        updated_at: time("updated")?,
    };
    let status: StructuralStatus = serde_json::from_value(v.clone())?;
    let mut extra = v
        .as_object()
        .context("session row is not an object")?
        .clone();
    for key in ["id", "title", "directory", "parentID"] {
        extra.shift_remove(key);
    }
    for key in serde_json::to_value(&status)?
        .as_object()
        .expect("status object")
        .keys()
    {
        extra.shift_remove(key);
    }
    Ok(SessionRow {
        session,
        status,
        extra,
    })
}
