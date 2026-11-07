//! Rich Claude Code turns, retaining every content block and native envelope.
use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::{Value, json};
use udjat_core::{HarnessKind, MessageRecord, PartKind, RecordTokens, from_opencode_v1};

pub fn from_jsonl(content: &str, session_id: &str) -> Result<Vec<MessageRecord>> {
    let mut messages = Vec::new();
    for (ordinal, line) in content.lines().enumerate() {
        let Ok(raw) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !matches!(raw["type"].as_str(), Some("user" | "assistant")) {
            continue;
        }
        let Some(timestamp) = raw["timestamp"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.to_utc())
        else {
            continue;
        };
        let native = &raw["message"];
        let role = native["role"].as_str().unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let id = raw["uuid"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{session_id}-{ordinal}"));
        let mut info = json!({"id":id,"sessionID":session_id,"role":role,"time":{"created":timestamp.timestamp_millis()}});
        for (key, value) in [
            ("modelID", native.get("model")),
            ("parentID", raw.get("parentUuid")),
            ("error", native.get("error")),
            ("cost", raw.get("costUSD")),
        ] {
            if let Some(v) = value {
                info[key] = v.clone();
            }
        }
        let blocks = match &native["content"] {
            Value::String(text) => vec![json!({"type":"text","text":text})],
            Value::Array(blocks) => blocks.clone(),
            _ => Vec::new(),
        };
        let parts = blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let mut part = block.clone();
                if !part.is_object() {
                    part = json!({"type":"unknown","data":block});
                }
                match block["type"].as_str() {
                    Some("thinking") => {
                        part["type"] = json!("reasoning");
                        part["text"] = block["thinking"].clone();
                    }
                    Some("tool_use") => {
                        part["type"] = json!("tool");
                        part["tool"] = json!(block["name"].as_str().unwrap_or("unknown"));
                        part["callID"] = block["id"].clone();
                        part["state"] = json!({"status":"pending","input":block["input"]});
                        if block.get("input").is_none() {
                            part["nativeInputAbsent"] = json!(true);
                        }
                    }
                    _ => {}
                }
                part["id"] = json!(format!("{id}-{index}"));
                part["native"] = block.clone();
                part["sessionID"] = json!(session_id);
                part["messageID"] = json!(id);
                part
            })
            .collect::<Vec<_>>();
        let mut record = from_opencode_v1(json!({"info":info,"parts":parts}));
        record.info.source = Some(HarnessKind::ClaudeCode);
        record.info.source_timestamp = Some(timestamp);
        record.info.extra.insert("native".into(), raw.clone());
        if let Some(usage) = native.get("usage") {
            let mut tokens = usage.clone();
            if !tokens.is_object() {
                tokens = json!({"native":usage});
            }
            for (from, to) in [
                ("input_tokens", "input"),
                ("output_tokens", "output"),
                ("cache_creation_input_tokens", "cache_creation"),
            ] {
                if let Some(v) = usage.get(from) {
                    tokens[to] = v.clone();
                }
            }
            if let Some(v) = usage.get("cache_read_input_tokens") {
                tokens["cache"] = json!({"read":v});
            }
            record.info.tokens = Some(RecordTokens::from_value(&tokens));
        }
        messages.push(record);
    }
    let mut results = std::collections::HashMap::new();
    for m in &messages {
        for p in &m.parts {
            if matches!(&p.kind,PartKind::Other{type_name} if type_name=="tool_result")
                && let Some(id) = p.extra.get("tool_use_id").and_then(Value::as_str)
            {
                results.insert(
                    id.to_owned(),
                    (
                        p.extra.get("content").cloned().unwrap_or(Value::Null),
                        p.extra.get("is_error") == Some(&Value::Bool(true)),
                        m.info.time_created,
                    ),
                );
            }
        }
    }
    for m in &mut messages {
        for p in &mut m.parts {
            if let PartKind::Tool {
                call_id: Some(id),
                state,
                ..
            } = &mut p.kind
                && let Some((content, error, end)) = results.get(id)
            {
                state.status = if *error { "error" } else { "completed" }.into();
                if *error {
                    state.error = Some(content.clone());
                } else {
                    state.output = Some(content.clone());
                }
                state.time = Some(json!({"start":m.info.time_created,"end":end}));
            }
        }
    }
    Ok(messages)
}
pub fn rich_messages(session_id: &str) -> Result<Vec<MessageRecord>> {
    let path = crate::session::find_session_file(session_id)
        .with_context(|| format!("Session file not found for: {session_id}"))?;
    from_jsonl(&std::fs::read_to_string(path)?, session_id)
}
