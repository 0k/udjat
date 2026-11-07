//! Rich, harness-independent messages and their lossless OpenCode V1 projection.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    #[default]
    Unknown,
    Other(String),
}
impl MessageRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Unknown => "unknown",
            Self::Other(s) => s,
        }
    }
}
impl From<&str> for MessageRole {
    fn from(s: &str) -> Self {
        match s {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "system" => Self::System,
            s => Self::Other(s.into()),
        }
    }
}

/// Token categories use the same semantics as `TokenUsage`, but retain the
/// original numeric values, optional categories and future fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordTokens {
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub reasoning: Option<Value>,
    pub cache_read: Option<Value>,
    pub cache_write: Option<Value>,
    pub cache_creation: Option<Value>,
    pub extra: Map<String, Value>,
    raw: Map<String, Value>,
}
impl RecordTokens {
    pub fn from_value(value: &Value) -> Self {
        let mut result = Self {
            raw: value.as_object().cloned().unwrap_or_default(),
            ..Default::default()
        };
        result.extra = result.raw.clone();
        result.input = result.extra.shift_remove("input");
        result.output = result.extra.shift_remove("output");
        result.reasoning = result.extra.shift_remove("reasoning");
        result.cache_creation = result.extra.shift_remove("cache_creation");
        result.cache_read = value.pointer("/cache/read").cloned();
        result.cache_write = value.pointer("/cache/write").cloned();
        result
    }
    pub fn to_value(&self) -> Value {
        let mut map = self.raw.clone();
        for (key, value) in [
            ("input", &self.input),
            ("output", &self.output),
            ("reasoning", &self.reasoning),
            ("cache_creation", &self.cache_creation),
        ] {
            if let Some(v) = value {
                map.insert(key.into(), v.clone());
            } else {
                map.shift_remove(key);
            }
        }
        for (key, v) in &self.extra {
            map.insert(key.clone(), v.clone());
        }
        if self.cache_read.is_some() || self.cache_write.is_some() {
            let cache = map.entry("cache").or_insert_with(|| json!({}));
            if let Some(cache) = cache.as_object_mut() {
                for (key, val) in [("read", &self.cache_read), ("write", &self.cache_write)] {
                    if let Some(val) = val {
                        cache.insert(key.into(), val.clone());
                    }
                }
            }
        }
        Value::Object(map)
    }
    pub fn usage(&self) -> crate::TokenUsage {
        fn n(v: &Option<Value>) -> u64 {
            v.as_ref().and_then(Value::as_u64).unwrap_or(0)
        }
        crate::TokenUsage {
            input: n(&self.input),
            output: n(&self.output),
            reasoning: n(&self.reasoning),
            cache_read: n(&self.cache_read),
            cache_write: n(&self.cache_write),
            cache_creation: n(&self.cache_creation),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageInfo {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub time_created: i64,
    pub time_completed: Option<i64>,
    pub parent_id: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub mode: Option<String>,
    pub variant: Option<String>,
    pub tokens: Option<RecordTokens>,
    pub cost: Option<Value>,
    pub error: Option<Value>,
    pub extra: Map<String, Value>,
    /// Storage ordering is independent of payload time; never sent on the wire.
    pub storage_time_created: Option<i64>,
    /// Origin selects the historical flat projection, not the wire dialect.
    pub source: Option<crate::HarnessKind>,
    pub source_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    raw: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolState {
    pub status: String,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub time: Option<Value>,
    pub extra: Map<String, Value>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PartKind {
    Text {
        text: String,
        synthetic: Option<bool>,
    },
    Reasoning {
        text: String,
    },
    Tool {
        tool: String,
        call_id: Option<String>,
        state: Box<ToolState>,
    },
    File {
        mime: Option<String>,
        filename: Option<String>,
        url: Option<String>,
    },
    StepStart,
    StepFinish,
    Other {
        type_name: String,
    },
}
impl Default for PartKind {
    fn default() -> Self {
        Self::Other {
            type_name: String::new(),
        }
    }
}
impl PartKind {
    pub fn type_name(&self) -> &str {
        match self {
            Self::Text { .. } => "text",
            Self::Reasoning { .. } => "reasoning",
            Self::Tool { .. } => "tool",
            Self::File { .. } => "file",
            Self::StepStart => "step-start",
            Self::StepFinish => "step-finish",
            Self::Other { type_name } => type_name,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Part {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub kind: PartKind,
    pub extra: Map<String, Value>,
    pub storage_time_created: Option<i64>,
    raw: Option<Value>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageRecord {
    pub info: MessageInfo,
    pub parts: Vec<Part>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}
fn extra(v: &Value, keys: &[&str]) -> Map<String, Value> {
    v.as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| !keys.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
fn parse_info(v: Value) -> MessageInfo {
    MessageInfo {
        id: s(&v, "id").unwrap_or_default(),
        session_id: s(&v, "sessionID").unwrap_or_default(),
        role: MessageRole::from(v.get("role").and_then(Value::as_str).unwrap_or("unknown")),
        time_created: v
            .pointer("/time/created")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        time_completed: v.pointer("/time/completed").and_then(Value::as_i64),
        parent_id: s(&v, "parentID"),
        model_id: s(&v, "modelID").or_else(|| {
            v.pointer("/model/modelID")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }),
        provider_id: s(&v, "providerID").or_else(|| {
            v.pointer("/model/providerID")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }),
        agent: s(&v, "agent"),
        mode: s(&v, "mode"),
        variant: s(&v, "variant"),
        tokens: v.get("tokens").map(RecordTokens::from_value),
        cost: v.get("cost").cloned(),
        error: v.get("error").cloned(),
        extra: extra(
            &v,
            &[
                "id",
                "sessionID",
                "role",
                "parentID",
                "modelID",
                "providerID",
                "agent",
                "mode",
                "variant",
                "tokens",
                "cost",
                "error",
            ],
        ),
        raw: Some(v),
        storage_time_created: None,
        source: None,
        source_timestamp: None,
    }
}
fn parse_part(v: Value) -> Part {
    let name = s(&v, "type").unwrap_or_default();
    let mut keys = vec!["id", "sessionID", "messageID", "type"];
    let kind = match name.as_str() {
        "text" => {
            keys.extend(["text", "synthetic"]);
            PartKind::Text {
                text: s(&v, "text").unwrap_or_default(),
                synthetic: v.get("synthetic").and_then(Value::as_bool),
            }
        }
        "reasoning" => {
            keys.push("text");
            PartKind::Reasoning {
                text: s(&v, "text").unwrap_or_default(),
            }
        }
        "tool" => {
            keys.extend(["tool", "callID", "state"]);
            let state = &v["state"];
            PartKind::Tool {
                tool: s(&v, "tool").unwrap_or_else(|| "unknown".into()),
                call_id: s(&v, "callID"),
                state: Box::new(ToolState {
                    status: s(state, "status").unwrap_or_default(),
                    input: state.get("input").cloned().unwrap_or(Value::Null),
                    output: state.get("output").cloned(),
                    error: state.get("error").cloned(),
                    title: s(state, "title"),
                    metadata: state.get("metadata").cloned(),
                    time: state.get("time").cloned(),
                    extra: extra(
                        state,
                        &[
                            "status", "input", "output", "error", "title", "metadata", "time",
                        ],
                    ),
                }),
            }
        }
        "file" => {
            keys.extend(["mime", "filename", "url"]);
            PartKind::File {
                mime: s(&v, "mime"),
                filename: s(&v, "filename"),
                url: s(&v, "url"),
            }
        }
        "step-start" => PartKind::StepStart,
        "step-finish" => PartKind::StepFinish,
        _ => PartKind::Other { type_name: name },
    };
    Part {
        id: s(&v, "id").unwrap_or_default(),
        message_id: s(&v, "messageID").unwrap_or_default(),
        session_id: s(&v, "sessionID").unwrap_or_default(),
        kind,
        extra: extra(&v, &keys),
        raw: Some(v),
        storage_time_created: None,
    }
}
pub fn from_opencode_v1(v: Value) -> MessageRecord {
    MessageRecord {
        info: parse_info(v.get("info").cloned().unwrap_or_else(|| json!({}))),
        parts: v
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .map(parse_part)
            .collect(),
    }
}

fn put(map: &mut Map<String, Value>, key: &str, val: impl Serialize) {
    map.insert(
        key.into(),
        serde_json::to_value(val).expect("model serializes"),
    );
}
fn opt(map: &mut Map<String, Value>, key: &str, val: &Option<impl Serialize>) {
    if let Some(v) = val {
        put(map, key, v);
    } else {
        map.shift_remove(key);
    }
}
fn info_value(info: &MessageInfo) -> Value {
    let mut m = info.extra.clone();
    put(&mut m, "id", &info.id);
    put(&mut m, "sessionID", &info.session_id);
    put(&mut m, "role", info.role.as_str());
    let time = m.entry("time").or_insert_with(|| json!({}));
    if !time.is_object() {
        *time = json!({});
    }
    time["created"] = json!(info.time_created);
    if let Some(v) = info.time_completed {
        time["completed"] = json!(v);
    } else {
        time.as_object_mut()
            .expect("time object")
            .shift_remove("completed");
    }
    opt(&mut m, "parentID", &info.parent_id);
    if info.role == MessageRole::User && m.contains_key("model") {
        if let Some(model) = m.get_mut("model").and_then(Value::as_object_mut) {
            opt(model, "modelID", &info.model_id);
            opt(model, "providerID", &info.provider_id);
        }
    } else {
        opt(&mut m, "modelID", &info.model_id);
        opt(&mut m, "providerID", &info.provider_id);
    }
    opt(&mut m, "agent", &info.agent);
    opt(&mut m, "mode", &info.mode);
    opt(&mut m, "variant", &info.variant);
    if let Some(v) = &info.tokens {
        m.insert("tokens".into(), v.to_value());
    }
    opt(&mut m, "cost", &info.cost);
    opt(&mut m, "error", &info.error);
    Value::Object(m)
}
fn part_value(p: &Part) -> Value {
    let mut m = p.extra.clone();
    put(&mut m, "id", &p.id);
    put(&mut m, "sessionID", &p.session_id);
    put(&mut m, "messageID", &p.message_id);
    put(&mut m, "type", p.kind.type_name());
    match &p.kind {
        PartKind::Text { text, synthetic } => {
            put(&mut m, "text", text);
            opt(&mut m, "synthetic", synthetic);
        }
        PartKind::Reasoning { text } => put(&mut m, "text", text),
        PartKind::Tool {
            tool,
            call_id,
            state,
        } => {
            put(&mut m, "tool", tool);
            opt(&mut m, "callID", call_id);
            let mut st = state.extra.clone();
            put(&mut st, "status", &state.status);
            put(&mut st, "input", &state.input);
            opt(&mut st, "output", &state.output);
            opt(&mut st, "error", &state.error);
            opt(&mut st, "title", &state.title);
            opt(&mut st, "metadata", &state.metadata);
            opt(&mut st, "time", &state.time);
            m.insert("state".into(), Value::Object(st));
        }
        PartKind::File {
            mime,
            filename,
            url,
        } => {
            opt(&mut m, "mime", mime);
            opt(&mut m, "filename", filename);
            opt(&mut m, "url", url);
        }
        _ => {}
    }
    Value::Object(m)
}
/// Patch only semantic edits into the original ordered shape. This retains
/// historical nulls, missing keys and incorrectly typed fields without making
/// those wire quirks the typed model's representation.
fn patch(original: &Value, before: &Value, after: &Value) -> Value {
    if before == after {
        return original.clone();
    }
    match (original.as_object(), before.as_object(), after.as_object()) {
        (Some(original), Some(before), Some(after)) => {
            let mut out = original.clone();
            for key in before.keys() {
                if !after.contains_key(key) {
                    out.shift_remove(key);
                }
            }
            for (key, v) in after {
                if before.get(key) != Some(v) {
                    let value = match (out.get(key), before.get(key)) {
                        (Some(orig), Some(prev)) => patch(orig, prev, v),
                        _ => v.clone(),
                    };
                    out.insert(key.clone(), value);
                }
            }
            Value::Object(out)
        }
        _ => after.clone(),
    }
}
pub fn to_opencode_v1(m: &MessageRecord) -> Value {
    let info = match &m.info.raw {
        Some(raw) => patch(
            raw,
            &info_value(&parse_info(raw.clone())),
            &info_value(&m.info),
        ),
        None => info_value(&m.info),
    };
    let parts = m
        .parts
        .iter()
        .map(|p| match &p.raw {
            Some(raw) => patch(raw, &part_value(&parse_part(raw.clone())), &part_value(p)),
            None => part_value(p),
        })
        .collect::<Vec<_>>();
    json!({"info":info,"parts":parts})
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagePage {
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Debug, Clone)]
pub struct MessagePageResult {
    pub messages: Vec<MessageRecord>,
    pub next_before: Option<String>,
    pub next_after: Option<String>,
}
impl MessagePageResult {
    pub fn from_messages(mut messages: Vec<MessageRecord>, page: MessagePage) -> Result<Self> {
        ensure!(
            page.before.is_none() || page.after.is_none(),
            "before and after are mutually exclusive"
        );
        messages.sort_by(|a, b| {
            (
                a.info.storage_time_created.unwrap_or(a.info.time_created),
                &a.info.id,
            )
                .cmp(&(
                    b.info.storage_time_created.unwrap_or(b.info.time_created),
                    &b.info.id,
                ))
        });
        let mut lo = 0;
        let mut hi = messages.len();
        if let Some(id) = page
            .before
            .as_ref()
            .or(page.after.as_ref())
            .filter(|id| !id.is_empty())
        {
            let i = messages
                .iter()
                .position(|m| &m.info.id == id)
                .ok_or_else(|| anyhow::anyhow!("message cursor not found: {id}"))?;
            if page.before.is_some() {
                hi = i;
            } else {
                lo = i + 1;
            }
        }
        if let Some(n) = page.limit.filter(|n| *n > 0) {
            if page.after.is_some() {
                hi = hi.min(lo.saturating_add(n));
            } else {
                lo = lo.max(hi.saturating_sub(n));
            }
        }
        let next_before = if lo > 0 && hi > lo {
            Some(messages[lo].info.id.clone())
        } else {
            None
        };
        let next_after = if hi < messages.len() && hi > lo {
            Some(messages[hi - 1].info.id.clone())
        } else {
            None
        };
        Ok(Self {
            messages: messages.drain(lo..hi).collect(),
            next_before,
            next_after,
        })
    }
}
