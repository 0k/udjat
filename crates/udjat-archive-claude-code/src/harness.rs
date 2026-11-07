use super::ClaudeCodeArchive;
use anyhow::Result;
use udjat_core::{
    Capability, Harness, HarnessKind, Message, MessagePartLatencies, ModelAttribution, Session,
    TranscriptEntry,
};

#[async_trait::async_trait]
impl Harness for ClaudeCodeArchive {
    fn kind(&self) -> HarnessKind {
        HarnessKind::ClaudeCode
    }
    fn capabilities(&self) -> &[Capability] {
        &[Capability::Read]
    }
    async fn list_sessions(&self) -> Result<Vec<Session>> {
        tokio::task::spawn_blocking(|| Self.list_sessions()).await?
    }
    async fn session_contains_text(&self, session_id: &str, needle: &str) -> Result<bool> {
        let (id, needle) = (session_id.to_owned(), needle.to_owned());
        Ok(tokio::task::spawn_blocking(move || Self.session_contains_text(&id, &needle)).await?)
    }
    async fn session_edited_file(&self, session_id: &str, file_path: &str) -> Result<bool> {
        let (id, path) = (session_id.to_owned(), file_path.to_owned());
        Ok(tokio::task::spawn_blocking(move || Self.session_edited_file(&id, &path)).await?)
    }
    async fn session_tail_contains_text(
        &self,
        session_id: &str,
        needle: &str,
        last_n: usize,
    ) -> Result<bool> {
        let (id, needle) = (session_id.to_owned(), needle.to_owned());
        Ok(tokio::task::spawn_blocking(move || {
            Self.session_tail_contains_text(&id, &needle, last_n)
        })
        .await?)
    }
    async fn transcript(&self, session_id: &str) -> Result<Vec<TranscriptEntry>> {
        let id = session_id.to_owned();
        tokio::task::spawn_blocking(move || Self.parse_transcript(&id)).await?
    }
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let id = session_id.to_owned();
        tokio::task::spawn_blocking(move || Self.list_messages(&id)).await?
    }
    async fn message_part_latencies(&self, session_id: &str) -> Result<MessagePartLatencies> {
        let id = session_id.to_owned();
        tokio::task::spawn_blocking(move || Self.message_part_latencies(&id)).await?
    }
    async fn resolve_attribution(&self, session_id: &str) -> Result<ModelAttribution> {
        let id = session_id.to_owned();
        tokio::task::spawn_blocking(move || Self.resolve_attribution(&id)).await?
    }
}
