use super::OpenCodeArchive;
use anyhow::Result;
use udjat_core::{
    Capability, Harness, HarnessKind, Message, MessagePartLatencies, ModelAttribution, Session,
    TranscriptEntry,
};

#[async_trait::async_trait]
impl Harness for OpenCodeArchive {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenCode
    }
    fn capabilities(&self) -> &[Capability] {
        &[Capability::Read]
    }
    async fn list_sessions(&self) -> Result<Vec<Session>> {
        let archive = *self;
        tokio::task::spawn_blocking(move || archive.list_sessions()).await?
    }
    async fn session_contains_text(&self, session_id: &str, needle: &str) -> Result<bool> {
        let (id, needle) = (session_id.to_owned(), needle.to_owned());
        let archive = *self;
        Ok(
            tokio::task::spawn_blocking(move || archive.session_contains_text(&id, &needle))
                .await?,
        )
    }
    async fn session_edited_file(&self, session_id: &str, file_path: &str) -> Result<bool> {
        let (id, path) = (session_id.to_owned(), file_path.to_owned());
        let archive = *self;
        Ok(tokio::task::spawn_blocking(move || archive.session_edited_file(&id, &path)).await?)
    }
    async fn session_tail_contains_text(
        &self,
        session_id: &str,
        needle: &str,
        last_n: usize,
    ) -> Result<bool> {
        let (id, needle) = (session_id.to_owned(), needle.to_owned());
        let archive = *self;
        Ok(tokio::task::spawn_blocking(move || {
            archive.session_tail_contains_text(&id, &needle, last_n)
        })
        .await?)
    }
    async fn transcript(&self, session_id: &str) -> Result<Vec<TranscriptEntry>> {
        let id = session_id.to_owned();
        let archive = *self;
        tokio::task::spawn_blocking(move || archive.parse_transcript(&id)).await?
    }
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let id = session_id.to_owned();
        let archive = *self;
        tokio::task::spawn_blocking(move || archive.list_messages(&id)).await?
    }
    async fn message_part_latencies(&self, session_id: &str) -> Result<MessagePartLatencies> {
        let id = session_id.to_owned();
        let archive = *self;
        tokio::task::spawn_blocking(move || archive.message_part_latencies(&id)).await?
    }
    async fn resolve_attribution(&self, session_id: &str) -> Result<ModelAttribution> {
        let id = session_id.to_owned();
        let archive = *self;
        tokio::task::spawn_blocking(move || archive.resolve_attribution(&id)).await?
    }
}
