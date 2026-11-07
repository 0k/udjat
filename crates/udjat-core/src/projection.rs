//! Compatibility projection to ai-audit's flat transcript.
use crate::{EntryType, MessageRecord, Role, TranscriptEntry, to_opencode_v1};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn transcript_entries(messages: &[MessageRecord]) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();
    for message in messages {
        if matches!(
            message.info.source,
            Some(crate::HarnessKind::ClaudeCode | crate::HarnessKind::Pi)
        ) {
            native_entries(message, &mut entries);
            continue;
        }
        let wire = to_opencode_v1(message);
        let info = &wire["info"];
        let role = match info["role"].as_str() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            Some("system") => Role::System,
            _ => continue,
        };
        let timestamp =
            DateTime::from_timestamp_millis(message.info.time_created).unwrap_or_else(Utc::now);
        if let Some(error) = info.get("error")
            && let Some(text) = error
                .pointer("/data/message")
                .and_then(Value::as_str)
                .or_else(|| error["name"].as_str())
        {
            entries.push(entry(
                timestamp,
                role.clone(),
                EntryType::Error,
                text.into(),
                None,
                None,
            ));
        }
        for part in wire["parts"].as_array().into_iter().flatten() {
            let start = part
                .pointer("/time/start")
                .and_then(Value::as_i64)
                .and_then(DateTime::from_timestamp_millis)
                .unwrap_or(timestamp);
            match part["type"].as_str().unwrap_or("") {
                "text" | "reasoning" => {
                    if let Some(text) = part["text"].as_str().filter(|t| !t.trim().is_empty()) {
                        entries.push(entry(
                            start,
                            role.clone(),
                            if part["type"] == "text" {
                                EntryType::Text
                            } else {
                                EntryType::Thinking
                            },
                            text.into(),
                            None,
                            None,
                        ));
                    }
                }
                "tool" => {
                    let state = &part["state"];
                    entries.push(entry(
                        start,
                        role.clone(),
                        EntryType::ToolUse,
                        String::new(),
                        Some(part["tool"].as_str().unwrap_or("unknown").into()),
                        state.get("input").cloned(),
                    ));
                    let output = state["output"].as_str().unwrap_or("");
                    let error = state["error"].as_str().unwrap_or("");
                    let (kind, text) = if state["status"] == "error" {
                        (
                            EntryType::ToolError,
                            if error.is_empty() { output } else { error },
                        )
                    } else {
                        (EntryType::ToolResult, output)
                    };
                    let end = part
                        .pointer("/time/end")
                        .and_then(Value::as_i64)
                        .and_then(DateTime::from_timestamp_millis)
                        .unwrap_or(start);
                    entries.push(entry(end, role.clone(), kind, text.into(), None, None));
                }
                _ => {}
            }
        }
    }
    if !messages
        .iter()
        .any(|m| m.info.source == Some(crate::HarnessKind::Pi))
    {
        entries.sort_by_key(|e| e.timestamp);
    }
    entries
}
fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
fn native_entries(message: &MessageRecord, entries: &mut Vec<TranscriptEntry>) {
    use crate::{MessageRole, PartKind};
    let timestamp = message.info.source_timestamp.unwrap_or_else(|| {
        DateTime::from_timestamp_millis(message.info.time_created).unwrap_or_default()
    });
    let role = match message.info.role {
        MessageRole::User => Role::User,
        MessageRole::Assistant => Role::Assistant,
        _ => return,
    };
    for part in &message.parts {
        match &part.kind {
            PartKind::Text { text, .. } | PartKind::Reasoning { text } => {
                if !text.trim().is_empty() {
                    entries.push(entry(
                        timestamp,
                        role.clone(),
                        if matches!(part.kind, PartKind::Text { .. }) {
                            EntryType::Text
                        } else {
                            EntryType::Thinking
                        },
                        text.clone(),
                        None,
                        None,
                    ));
                }
            }
            PartKind::Tool { tool, state, .. } => {
                let input = if part.extra.get("nativeInputAbsent") == Some(&Value::Bool(true)) {
                    None
                } else {
                    Some(state.input.clone())
                };
                entries.push(entry(
                    timestamp,
                    role.clone(),
                    EntryType::ToolUse,
                    String::new(),
                    Some(tool.clone()),
                    input,
                ));
            }
            PartKind::Other { type_name }
                if type_name == "tool_result" || type_name == "toolResult" =>
            {
                let pi = message.info.source == Some(crate::HarnessKind::Pi);
                let kind = if !pi && part.extra.get("is_error") == Some(&Value::Bool(true)) {
                    EntryType::ToolError
                } else {
                    EntryType::ToolResult
                };
                let tool = if pi {
                    part.extra
                        .get("toolName")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                } else {
                    None
                };
                entries.push(entry(
                    timestamp,
                    role.clone(),
                    kind,
                    result_text(part.extra.get("content").unwrap_or(&Value::Null)),
                    tool,
                    None,
                ));
            }
            _ => {}
        }
    }
}
fn entry(
    timestamp: DateTime<Utc>,
    role: Role,
    entry_type: EntryType,
    content: String,
    tool_name: Option<String>,
    tool_input: Option<Value>,
) -> TranscriptEntry {
    TranscriptEntry {
        timestamp,
        role,
        entry_type,
        content,
        tool_name,
        tool_input,
    }
}
