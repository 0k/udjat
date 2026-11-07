//! Lossless block-oriented pi JSONL reader. Flat compatibility lives in core.
use anyhow::Result;
use chrono::DateTime;
use serde_json::{Value, json};
use udjat_core::{HarnessKind, MessageRecord, PartKind, RecordTokens, from_opencode_v1};

pub fn from_jsonl(content: &str, session_id: &str) -> Result<Vec<MessageRecord>> {
    let mut messages = Vec::new();
    let mut model = None;
    let mut provider = None;
    for (ordinal, line) in content.lines().enumerate() {
        let Ok(raw) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if raw["type"] == "model_change" {
            model = raw.get("modelId").or_else(|| raw.get("modelID")).cloned();
            provider = raw.get("provider").cloned();
            continue;
        }
        if raw["type"] != "message" {
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
        if !matches!(role, "user" | "assistant" | "toolResult") {
            continue;
        }
        let id = raw["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{session_id}-{ordinal}"));
        let mut info = json!({"id":id,"sessionID":session_id,"role":if role=="toolResult"{"user"}else{role},"time":{"created":timestamp.timestamp_millis()}});
        for (key, value) in [
            (
                "modelID",
                native
                    .get("model")
                    .or_else(|| native.get("modelID"))
                    .or(model.as_ref()),
            ),
            ("providerID", native.get("provider").or(provider.as_ref())),
            ("parentID", raw.get("parentId")),
            ("error", native.get("error")),
            ("cost", native.pointer("/usage/cost/total")),
        ] {
            if let Some(v) = value {
                info[key] = v.clone();
            }
        }
        let blocks = if role == "toolResult" {
            vec![
                json!({"type":"toolResult","content":native.get("content").unwrap_or(&Value::Null),"toolCallId":native.get("toolCallId"),"toolName":native.get("toolName"),"isError":native.get("isError")}),
            ]
        } else {
            match &native["content"] {
                Value::String(text) => vec![json!({"type":"text","text":text})],
                Value::Array(blocks) => blocks.clone(),
                _ => Vec::new(),
            }
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
                    Some("toolCall") => {
                        part["type"] = json!("tool");
                        part["tool"] = json!(block["name"].as_str().unwrap_or("unknown"));
                        part["callID"] = block["id"].clone();
                        part["state"] = json!({"status":"pending","input":block["arguments"]});
                        if block.get("arguments").is_none() {
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
        record.info.source = Some(HarnessKind::Pi);
        record.info.source_timestamp = Some(timestamp);
        record.info.extra.insert("native".into(), raw.clone());
        if let Some(usage) = native.get("usage") {
            let mut tokens = usage.clone();
            if !tokens.is_object() {
                tokens = json!({"native":usage});
            }
            for (from, to) in [("cacheRead", "read"), ("cacheWrite", "write")] {
                if let Some(v) = usage.get(from) {
                    tokens["cache"][to] = v.clone();
                }
            }
            record.info.tokens = Some(RecordTokens::from_value(&tokens));
        }
        messages.push(record);
    }
    correlate(&mut messages);
    Ok(messages)
}
fn correlate(messages: &mut [MessageRecord]) {
    let mut results = std::collections::HashMap::new();
    for m in messages.iter() {
        for p in &m.parts {
            if matches!(&p.kind,PartKind::Other{type_name} if type_name=="toolResult")
                && let Some(id) = p.extra.get("toolCallId").and_then(Value::as_str)
            {
                results.insert(
                    id.to_owned(),
                    (
                        p.extra.get("content").cloned().unwrap_or(Value::Null),
                        p.extra.get("isError") == Some(&Value::Bool(true)),
                        m.info.time_created,
                    ),
                );
            }
        }
    }
    for m in messages {
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
}
pub fn rich_messages(session_id: &str) -> Result<Vec<MessageRecord>> {
    from_jsonl(&crate::session::read_session_jsonl(session_id)?, session_id)
}
