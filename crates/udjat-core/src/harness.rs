//! The [`Harness`] trait and its capability groups.

use anyhow::Result;
use async_trait::async_trait;

use crate::{
    HarnessKind, Message, MessagePartLatencies, ModelAttribution, Session, TranscriptEntry,
};

/// A group of [`Harness`] methods a harness may or may not implement.
///
/// A harness lists the groups it supports in [`Harness::capabilities`];
/// a caller checks the list before using a group, and a method of an
/// unsupported group fails with [`Unsupported`].  Groups are added with
/// their methods (planned: `events`, `status`, `run`, `interactions`,
/// `models`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Capability {
    /// List sessions, search them, read their transcripts and messages.
    Read,
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::Read => "read",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error of a method whose capability group the harness does not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub harness: HarnessKind,
    pub capability: Capability,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "harness {} does not support the {} capability",
            self.harness, self.capability
        )
    }
}

impl std::error::Error for Unsupported {}

/// A harness, however it is reached.
///
/// Every method belongs to a [`Capability`] group.  The `read` group is
/// the contract of the session archives; its methods mirror what
/// `ai-audit` needs, so its output does not depend on how the harness
/// is reached.
#[async_trait]
pub trait Harness: Send + Sync {
    /// The harness program whose sessions this harness serves.
    fn kind(&self) -> HarnessKind;

    /// The capability groups this harness implements.
    fn capabilities(&self) -> &[Capability];

    /// `read`: every session of this harness.
    async fn list_sessions(&self) -> Result<Vec<Session>>;

    /// `read`: archives evaluate locally; network connectors push all predicates
    /// to their server. Status requires harness-specific classification.
    async fn list_matching(&self, q: &crate::ListQuery) -> Result<Vec<Session>> {
        anyhow::ensure!(
            q.statuses.is_empty() && q.last_message_in.is_none(),
            "harness {} does not support status or last-message filtering",
            self.kind()
        );
        let mut result = Vec::new();
        for session in self.list_sessions().await? {
            if !q.matches_metadata(&session) {
                continue;
            }
            if let Some(path) = &q.edited_file
                && !self.session_edited_file(&session.session_id, path).await?
            {
                continue;
            }
            let mut matches = true;
            for needle in &q.content {
                if !self
                    .session_contains_text(&session.session_id, needle)
                    .await?
                {
                    matches = false;
                    break;
                }
            }
            if matches {
                result.push(session);
            }
        }
        Ok(result)
    }

    /// `read`: whether the session's messages contain `needle`.
    async fn session_contains_text(&self, session_id: &str, needle: &str) -> Result<bool>;

    /// `read`: whether the session has a tool call that wrote or edited
    /// `file_path` (see [`crate::file_path_matches`]).
    async fn session_edited_file(&self, session_id: &str, file_path: &str) -> Result<bool>;

    /// `read`: whether the last `last_n` messages of the session contain
    /// `needle`.
    async fn session_tail_contains_text(
        &self,
        session_id: &str,
        needle: &str,
        last_n: usize,
    ) -> Result<bool>;

    /// `read`: the session's transcript, oldest entry first.
    async fn transcript(&self, session_id: &str) -> Result<Vec<TranscriptEntry>>;

    /// `read`: the session's messages in chronological order.  Token data
    /// is present on assistant messages; user messages have
    /// `tokens: None`.
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>>;

    /// Rich messages, oldest first; a limit without a cursor selects newest N.
    async fn messages(
        &self,
        _session_id: &str,
        _page: crate::MessagePage,
    ) -> Result<crate::MessagePageResult> {
        Err(Unsupported {
            harness: self.kind(),
            capability: Capability::Read,
        }
        .into())
    }

    async fn session_row(&self, _id: &str) -> Result<Option<crate::SessionRow>> {
        Err(Unsupported {
            harness: self.kind(),
            capability: Capability::Read,
        }
        .into())
    }

    async fn list_rows(&self, query: &crate::ListQuery) -> Result<Vec<crate::SessionRow>> {
        let mut rows = Vec::new();
        for session in self.list_matching(query).await? {
            if let Some(row) = self.session_row(&session.session_id).await? {
                rows.push(row);
            }
        }
        Ok(rows)
    }

    async fn children(&self, _id: &str) -> Result<Vec<Session>> {
        Err(Unsupported {
            harness: self.kind(),
            capability: Capability::Read,
        }
        .into())
    }

    /// `read`: per-assistant-message latencies from harness-native
    /// part-level timing, keyed by `Message.timestamp`.
    ///
    /// The pair is `(llm_generation_s, tool_latency_s_before)`:
    /// - `llm_generation_s`: wall-clock seconds the LLM spent producing
    ///   the message's text and reasoning, tool execution excluded.
    /// - `tool_latency_s_before`: wall-clock seconds tools ran on behalf
    ///   of the message.
    ///
    /// An empty map means "no part-level data": the caller falls back to
    /// timings walked from the transcript.  A key missing from the map
    /// means "no override" for that message, not "explicit null".
    async fn message_part_latencies(&self, _session_id: &str) -> Result<MessagePartLatencies> {
        Ok(MessagePartLatencies::new())
    }

    /// `read`: who wrote the session's most recent assistant activity.
    ///
    /// Missing model metadata MUST be an error: a Git hook needs to know
    /// that attribution is unknown, and a silent generic value is worse
    /// than no trailer at all.
    async fn resolve_attribution(&self, session_id: &str) -> Result<ModelAttribution>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_names_harness_and_capability() {
        let err = Unsupported {
            harness: HarnessKind::Pi,
            capability: Capability::Read,
        };
        assert_eq!(
            err.to_string(),
            "harness pi does not support the read capability"
        );
    }
}
