//! Read-only pi session archive.

// Preserve the reader's pre-extraction control flow.
#![allow(clippy::collapsible_if, clippy::filter_next)]

mod harness;
pub mod rich;
pub mod session;
#[cfg(test)]
mod tests;
pub mod transcript;

use anyhow::Result;
use std::path::PathBuf;
use udjat_core::{
    HarnessKind as Provider, Message, MessagePartLatencies, ModelAttribution, Session,
    TranscriptEntry, brand_for_llm_provider,
};

#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn pi_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("PI_CODING_AGENT_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".pi")
        .join("agent")
}

pub fn sessions_dir() -> PathBuf {
    pi_data_dir().join("sessions")
}

pub struct PiArchive;

impl PiArchive {
    pub fn rich_messages(&self, session_id: &str) -> Result<Vec<udjat_core::MessageRecord>> {
        rich::rich_messages(session_id)
    }
    pub fn provider(&self) -> Provider {
        Provider::Pi
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let sessions = session::list_sessions()?;
        Ok(sessions
            .into_iter()
            .map(|s| Session {
                session_id: s.session_id,
                harness: Provider::Pi,
                started_at: s.started_at,
                updated_at: s.updated_at,
                project_dir: s.project_dir,
                title: s.title,
                parent_id: s.parent_id,
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
        let raw = session::read_session_jsonl(session_id)?;
        resolve_attribution_from_jsonl(&raw, session_id)
    }
}

pub fn resolve_attribution_from_jsonl(content: &str, session_id: &str) -> Result<ModelAttribution> {
    let messages = session::parse_messages(content, session_id)?;
    let model_changes = session::parse_model_changes(content);

    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no assistant message found in Pi session {} \
                 — cannot resolve attribution",
                session_id
            )
        })?;

    // Most recent model_change event at or before the assistant turn.
    let prior_change = model_changes
        .iter()
        .filter(|ev| ev.timestamp <= last_assistant.timestamp)
        .next_back();

    let model_id = last_assistant
        .model
        .clone()
        .or_else(|| prior_change.and_then(|ev| ev.model.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Pi session {} has no model id on the last assistant message \
                 nor in any prior model_change event",
                session_id
            )
        })?;

    let mut llm_provider = last_assistant
        .provider_id
        .clone()
        .or_else(|| prior_change.and_then(|ev| ev.provider.clone()));
    let mut llm_provider_inferred = false;
    if llm_provider.is_none() {
        if let Some(inferred) = udjat_core::infer_llm_provider_from_model(&model_id) {
            llm_provider = Some(inferred.to_string());
            llm_provider_inferred = true;
        }
    }

    if let Some(ref pid) = llm_provider {
        if brand_for_llm_provider(pid).is_none() {
            anyhow::bail!(
                "Pi session {} resolved llm_provider {:?} which has no brand mapping; \
                 teach `provider::brand_for_llm_provider` if this is a real provider",
                session_id,
                pid
            );
        }
    } else {
        anyhow::bail!(
            "Pi session {} has model={:?} but no provider field on the assistant \
             message, no prior model_change event, and no inferable family — cannot \
             build a kernel-canonical Assisted-by trailer",
            session_id,
            model_id
        );
    }

    Ok(ModelAttribution {
        harness: Provider::Pi,
        llm_provider,
        llm_provider_inferred,
        model_id,
        access_surface: "Pi".to_string(),
        agent: last_assistant.agent.clone(),
        mode: last_assistant.mode.clone(),
        variant: None,
    })
}
