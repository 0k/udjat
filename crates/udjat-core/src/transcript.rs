//! Normalized transcript entries.
//!
//! This flat shape is what `ai-audit session transcript` prints; it
//! drops harness detail (message grouping, tool states) that a richer
//! model added later will keep.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Text,
    ToolUse,
    ToolResult,
    /// Failed tool-call response: user refusal, dismissed prompt, tool error,
    /// or any other non-success outcome. Sibling of `ToolResult` (success).
    /// Distinct from `Error`, which is a message-level / API-level failure.
    ToolError,
    Thinking,
    Error,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Text => "text",
            EntryType::ToolUse => "tool_use",
            EntryType::ToolResult => "tool_result",
            EntryType::ToolError => "tool_error",
            EntryType::Thinking => "thinking",
            EntryType::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptEntry {
    pub timestamp: DateTime<Utc>,
    pub role: Role,
    pub entry_type: EntryType,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
}
/// Merge in harness order: preferred entries win on timestamp/role/type/content.
/// Other-source duplicates are deliberately not collapsed amongst themselves.
pub fn merge_transcripts(
    preferred: Vec<TranscriptEntry>,
    other: Vec<TranscriptEntry>,
) -> Vec<TranscriptEntry> {
    use std::collections::HashSet;
    let keys: HashSet<(i64, String, String, String)> = preferred
        .iter()
        .map(|e| {
            (
                e.timestamp.timestamp_millis(),
                e.role.as_str().to_string(),
                e.entry_type.as_str().to_string(),
                e.content.clone(),
            )
        })
        .collect();
    let mut merged = preferred;
    for entry in other {
        let key = (
            entry.timestamp.timestamp_millis(),
            entry.role.as_str().to_string(),
            entry.entry_type.as_str().to_string(),
            entry.content.clone(),
        );
        if !keys.contains(&key) {
            merged.push(entry);
        }
    }
    merged.sort_by_key(|e| e.timestamp);
    merged
}
