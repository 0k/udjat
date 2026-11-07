use super::PiArchive;
use anyhow::Result;
use udjat_core::{
    Capability, Harness, HarnessKind, Message, MessagePartLatencies, ModelAttribution, Session,
    TranscriptEntry,
};

#[async_trait::async_trait]
impl Harness for PiArchive {
    async fn messages(
        &self,
        id: &str,
        page: udjat_core::MessagePage,
    ) -> Result<udjat_core::MessagePageResult> {
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            udjat_core::MessagePageResult::from_messages(Self.rich_messages(&id)?, page)
        })
        .await?
    }
    async fn session_row(&self, id: &str) -> Result<Option<udjat_core::SessionRow>> {
        let sessions = Harness::list_sessions(self).await?;
        let Some(session) = sessions.iter().find(|s| s.session_id == id).cloned() else {
            return Ok(None);
        };
        let count = sessions
            .iter()
            .filter(|s| s.parent_id.as_deref() == Some(id))
            .count();
        let messages = Harness::messages(self, id, Default::default())
            .await?
            .messages;
        Ok(Some(udjat_core::SessionRow::new(
            session,
            udjat_core::StructuralStatus::from_messages(&messages, count),
        )))
    }
    async fn children(&self, id: &str) -> Result<Vec<Session>> {
        Ok(Harness::list_sessions(self)
            .await?
            .into_iter()
            .filter(|s| s.parent_id.as_deref() == Some(id))
            .collect())
    }
    fn kind(&self) -> HarnessKind {
        HarnessKind::Pi
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
