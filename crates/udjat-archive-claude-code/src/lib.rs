//! Read-only Claude Code session archive.

#[cfg(test)]
mod attribution_tests;
mod harness;
pub mod rich;
pub mod session;
pub mod transcript;

use anyhow::Result;
use std::path::PathBuf;
use udjat_core::{
    HarnessKind as Provider, Message, MessagePartLatencies, ModelAttribution, Session,
    TranscriptEntry, brand_for_llm_provider, infer_llm_provider_from_model,
};

#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn claudecode_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
}

pub fn debug_dir() -> PathBuf {
    crate::claudecode_data_dir().join("debug")
}

pub fn projects_dir() -> PathBuf {
    crate::claudecode_data_dir().join("projects")
}

pub fn resolve_debug_file(session_id: &str) -> PathBuf {
    debug_dir().join(format!("{}.txt", session_id))
}

pub struct ClaudeCodeArchive;

impl ClaudeCodeArchive {
    pub fn rich_messages(&self, session_id: &str) -> Result<Vec<udjat_core::MessageRecord>> {
        rich::rich_messages(session_id)
    }
    pub fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let sessions = session::list_sessions()?;
        Ok(sessions
            .into_iter()
            .map(|s| Session {
                session_id: s.session_id,
                harness: Provider::ClaudeCode,
                started_at: s.started_at,
                updated_at: s.updated_at,
                project_dir: s.project_dir,
                title: s.title,
                parent_id: None,
            })
            .collect())
    }

    pub fn session_contains_text(&self, session_id: &str, needle: &str) -> bool {
        session::session_contains_text(session_id, needle)
    }

    pub fn session_edited_file(&self, session_id: &str, file_path: &str) -> bool {
        session::session_edited_file(session_id, file_path)
    }

    pub fn session_tail_contains_text(
        &self,
        session_id: &str,
        needle: &str,
        last_n: usize,
    ) -> bool {
        session::session_tail_contains_text(session_id, needle, last_n)
    }

    pub fn parse_transcript(&self, session_id: &str) -> Result<Vec<TranscriptEntry>> {
        transcript::parse_transcript(session_id)
    }

    pub fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        session::list_messages(session_id)
    }

    pub fn message_part_latencies(&self, _session_id: &str) -> Result<MessagePartLatencies> {
        Ok(std::collections::HashMap::new())
    }

    pub fn resolve_attribution(&self, session_id: &str) -> Result<ModelAttribution> {
        let messages = session::list_messages(session_id)?;
        resolve_attribution_from_messages(&messages, session_id)
    }
}

pub fn resolve_attribution_from_messages(
    messages: &[Message],
    session_id: &str,
) -> Result<ModelAttribution> {
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant" && m.model.is_some())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no assistant message with a model id found in Claude Code \
                 session {}",
                session_id
            )
        })?;
    let model_id = last_assistant
        .model
        .clone()
        .expect("filtered for model.is_some() above");

    let inferred = infer_llm_provider_from_model(&model_id).ok_or_else(|| {
        anyhow::anyhow!(
            "Claude Code session {} has model={:?}, but its family is not \
             recognised by `provider::infer_llm_provider_from_model` — cannot \
             infer the LLM provider for the Assisted-by trailer",
            session_id,
            model_id
        )
    })?;
    if brand_for_llm_provider(inferred).is_none() {
        anyhow::bail!(
            "Claude Code session {} inferred llm_provider {:?} which has no \
             brand mapping; teach `provider::brand_for_llm_provider`",
            session_id,
            inferred
        );
    }

    Ok(ModelAttribution {
        harness: Provider::ClaudeCode,
        llm_provider: Some(inferred.to_string()),
        llm_provider_inferred: true,
        model_id,
        access_surface: "ClaudeCode".to_string(),
        agent: None,
        mode: None,
        variant: None,
    })
}
