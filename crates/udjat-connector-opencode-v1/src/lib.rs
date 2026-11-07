//! Read-only connector for the OpenCode V1 audit-list contract.
//!
//! [`OpenCodeV1Harness`] implements [`udjat_core::Harness`] for the
//! OpenCode and pi dialects. List predicates are sent to `GET /session`;
//! transcript reads use OpenCode native messages or pi's lossless raw JSONL
//! endpoint and share the archive converters. HTTP errors are surfaced
//! rather than silently falling back to another storage source.
//!
//! # Server requirements
//!
//! Byte-identical archive output needs the server to return stored message
//! data as stored (historical shapes included) and parts in creation order
//! (`time_created`, then id). These are server-contract requirements, not
//! grounds for client-side storage access or normalization.
//!
//! # Real-server tests
//!
//! `tests/server.rs` runs against real binaries and prints a clear skip
//! when one is unavailable: OpenCode from `UDJAT_TEST_OPENCODE` (default:
//! the installed `opencode`), and the pi test from `UDJAT_TEST_PI_SERVER`
//! (the `cli.js` of an opencode V1 compatible server for pi sessions).
//! Each process gets a cleared environment and temporary HOME/XDG,
//! working and storage directories. OpenCode starts once to migrate its
//! temporary database; the test creates a session over HTTP, stops the
//! process, inserts fixture messages and parts into that database without
//! calling a model, and starts it again. Pi reads fixture JSONL from a
//! temporary agent directory. Cleanup signals only the tracked child, after
//! checking its PID, parent, start time and command.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;
use udjat_archive_opencode::MessageWithParts;
use udjat_core::{
    Capability, Harness, HarnessKind, ListQuery, Message, MessagePartLatencies, ModelAttribution,
    Session, TranscriptEntry,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    #[default]
    OpenCode,
    Pi,
}

pub struct OpenCodeV1Harness {
    pub base_url: Url,
    pub dialect: Dialect,
    pub client: Client,
}

impl OpenCodeV1Harness {
    pub fn new(base_url: &str, dialect: Dialect) -> Result<Self> {
        let base_url = Url::parse(&format!("{}/", base_url.trim_end_matches('/')))?;
        anyhow::ensure!(
            matches!(base_url.scheme(), "http" | "https"),
            "expected HTTP(S) harness URL"
        );
        Ok(Self {
            base_url,
            dialect,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()?,
        })
    }

    fn list_url(&self, q: &ListQuery) -> Result<Url> {
        let mut url = self.base_url.join("session")?;
        let mut params = url.query_pairs_mut();
        params
            .append_pair("all_projects", "true")
            .append_pair("lean", "true")
            .append_pair("limit", "0");
        if self.dialect == Dialect::Pi {
            params.append_pair("archived", "true");
        }
        for (key, value) in [
            ("id", &q.session_id),
            ("parent", &q.parent),
            ("exact_directory", &q.project),
            ("edited_file", &q.edited_file),
        ] {
            if let Some(value) = value {
                params.append_pair(key, value);
            }
        }
        if q.roots_only {
            params.append_pair("roots", "true");
        }
        if let Some((from, to)) = q.active {
            params.append_pair(
                "active_from",
                &from
                    .checked_mul(1000)
                    .context("active_from overflow")?
                    .to_string(),
            );
            params.append_pair(
                "active_to",
                &to.checked_mul(1000)
                    .context("active_to overflow")?
                    .to_string(),
            );
        }
        for needle in &q.content {
            params.append_pair("content", needle);
        }
        if !q.statuses.is_empty() {
            params.append_pair("status", &q.statuses.join(","));
        }
        if let Some((from, to)) = q.last_message_in {
            params
                .append_pair("last_message_from", &from.to_string())
                .append_pair("last_message_to", &to.to_string());
        }
        drop(params);
        Ok(url)
    }

    fn session_url(&self, id: &str, endpoint: &str) -> Result<Url> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("invalid base URL"))?
            .pop_if_empty()
            .extend(["session", id]);
        if !endpoint.is_empty() {
            url.path_segments_mut()
                .map_err(|()| anyhow::anyhow!("invalid base URL"))?
                .push(endpoint);
        }
        Ok(url)
    }

    async fn native_messages(
        &self,
        id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessageWithParts>> {
        let mut url = self.session_url(id, "message")?;
        if let Some(limit) = limit {
            url.query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "V1 messages: HTTP {}: {}",
                response.status(),
                response.text().await?
            );
        }
        response.json().await.context("invalid V1 messages")
    }

    async fn raw(&self, id: &str) -> Result<String> {
        Ok(self
            .client
            .get(self.session_url(id, "raw")?)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }
}

#[derive(Deserialize)]
struct LeanRow {
    id: String,
    #[serde(rename = "parentID")]
    parent_id: Option<String>,
    directory: String,
    title: String,
    time: LeanTime,
}
#[derive(Deserialize)]
struct LeanTime {
    created: f64,
    updated: f64,
}

fn timestamp(ms: f64) -> Result<DateTime<Utc>> {
    anyhow::ensure!(ms.is_finite(), "non-finite session timestamp");
    let seconds = (ms / 1000.0).floor() as i64;
    let nanos = ((ms - seconds as f64 * 1000.0) * 1_000_000.0).round() as u32;
    DateTime::from_timestamp(seconds, nanos).context("invalid session timestamp")
}

#[async_trait::async_trait]
impl Harness for OpenCodeV1Harness {
    async fn messages(
        &self,
        id: &str,
        page: udjat_core::MessagePage,
    ) -> Result<udjat_core::MessagePageResult> {
        if self.dialect == Dialect::Pi {
            return udjat_core::MessagePageResult::from_messages(
                udjat_archive_pi::rich::from_jsonl(&self.raw(id).await?, id)?,
                page,
            );
        }
        anyhow::ensure!(
            page.before.is_none() || page.after.is_none(),
            "before and after are mutually exclusive"
        );
        let mut url = self.session_url(id, "message")?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(limit) = page.limit {
                query.append_pair("limit", &limit.to_string());
            }
            if let Some(before) = &page.before {
                query.append_pair("before", before);
            }
            if let Some(after) = &page.after {
                query.append_pair("after", after);
            }
        }
        let response = self.client.get(url).send().await?.error_for_status()?;
        let cursor = response
            .headers()
            .get("x-next-cursor")
            .map(|v| v.to_str().map(str::to_owned))
            .transpose()?;
        let messages = response
            .json::<Vec<serde_json::Value>>()
            .await?
            .into_iter()
            .map(udjat_core::from_opencode_v1)
            .collect();
        Ok(udjat_core::MessagePageResult {
            messages,
            next_before: if page.after.is_none() {
                cursor.clone()
            } else {
                None
            },
            next_after: if page.after.is_some() { cursor } else { None },
        })
    }
    async fn session_row(&self, id: &str) -> Result<Option<udjat_core::SessionRow>> {
        let response = self.client.get(self.session_url(id, "")?).send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let value = response
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let mut row = udjat_core::from_opencode_v1_row(value, self.kind())?;
        if self.dialect == Dialect::Pi {
            let messages = Harness::messages(self, id, Default::default())
                .await?
                .messages;
            row.status =
                udjat_core::StructuralStatus::from_messages(&messages, row.status.child_count);
        }
        Ok(Some(row))
    }
    async fn children(&self, id: &str) -> Result<Vec<Session>> {
        self.list_matching(&ListQuery {
            parent: Some(id.into()),
            ..Default::default()
        })
        .await
    }
    fn kind(&self) -> HarnessKind {
        match self.dialect {
            Dialect::OpenCode => HarnessKind::OpenCode,
            Dialect::Pi => HarnessKind::Pi,
        }
    }
    fn capabilities(&self) -> &[Capability] {
        &[Capability::Read]
    }
    async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.list_matching(&ListQuery::default()).await
    }
    async fn list_matching(&self, q: &ListQuery) -> Result<Vec<Session>> {
        let response = self.client.get(self.list_url(q)?).send().await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "{} session listing: HTTP {}: {}",
                self.kind(),
                response.status(),
                response.text().await?
            );
        }
        let rows: Vec<LeanRow> = response.json().await?;
        rows.into_iter()
            .map(|row| {
                let created = timestamp(row.time.created)?;
                let updated = if self.dialect == Dialect::OpenCode && row.time.updated <= 0.0 {
                    created
                } else {
                    timestamp(row.time.updated)?
                };
                Ok(Session {
                    session_id: row.id,
                    harness: self.kind(),
                    started_at: created,
                    updated_at: updated,
                    project_dir: row.directory,
                    title: row.title,
                    parent_id: row.parent_id,
                })
            })
            .collect()
    }
    async fn session_contains_text(&self, id: &str, needle: &str) -> Result<bool> {
        Ok(!self
            .list_matching(&ListQuery {
                session_id: Some(id.into()),
                content: vec![needle.into()],
                ..Default::default()
            })
            .await?
            .is_empty())
    }
    async fn session_edited_file(&self, id: &str, path: &str) -> Result<bool> {
        Ok(!self
            .list_matching(&ListQuery {
                session_id: Some(id.into()),
                edited_file: Some(path.into()),
                ..Default::default()
            })
            .await?
            .is_empty())
    }
    async fn session_tail_contains_text(
        &self,
        id: &str,
        needle: &str,
        last_n: usize,
    ) -> Result<bool> {
        if last_n == 0 {
            return Ok(false);
        }
        match self.dialect {
            Dialect::OpenCode => Ok(udjat_archive_opencode::messages_contain_text(
                &self.native_messages(id, Some(last_n)).await?,
                needle,
            )),
            Dialect::Pi => Ok(udjat_archive_pi::session::jsonl_tail_contains_text(
                &self.raw(id).await?,
                needle,
                last_n,
            )),
        }
    }
    async fn transcript(&self, id: &str) -> Result<Vec<TranscriptEntry>> {
        match self.dialect {
            Dialect::OpenCode => {
                let messages = self.native_messages(id, None).await?;
                anyhow::ensure!(
                    !messages.is_empty(),
                    "OpenCode session has no messages: {}",
                    id
                );
                Ok(udjat_archive_opencode::transcript::parse_transcript_from_messages(&messages))
            }
            Dialect::Pi => {
                udjat_archive_pi::transcript::parse_transcript_from_jsonl(&self.raw(id).await?)
            }
        }
    }
    async fn list_messages(&self, id: &str) -> Result<Vec<Message>> {
        match self.dialect {
            Dialect::OpenCode => Ok(udjat_archive_opencode::messages_from_json(
                &self.native_messages(id, None).await?,
                id,
            )),
            Dialect::Pi => udjat_archive_pi::session::parse_messages(&self.raw(id).await?, id),
        }
    }
    async fn resolve_attribution(&self, id: &str) -> Result<ModelAttribution> {
        match self.dialect {
            Dialect::OpenCode => udjat_archive_opencode::resolve_attribution_from_messages(
                &self.list_messages(id).await?,
                id,
            ),
            Dialect::Pi => {
                udjat_archive_pi::resolve_attribution_from_jsonl(&self.raw(id).await?, id)
            }
        }
    }
    async fn message_part_latencies(&self, id: &str) -> Result<MessagePartLatencies> {
        match self.dialect {
            Dialect::OpenCode => Ok(
                udjat_archive_opencode::latency::message_part_latencies_from_messages(
                    &self.native_messages(id, None).await?,
                ),
            ),
            Dialect::Pi => Ok(Default::default()),
        }
    }
}
