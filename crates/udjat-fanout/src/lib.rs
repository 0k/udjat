//! Ordered multi-harness reads. Metadata is first-wins; output is sorted by
//! creation time, like ai-audit. Errors are surfaced, never partial success.
use anyhow::Result;
use futures::future::try_join_all;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use udjat_core::{Harness, ListQuery, Session, TranscriptEntry, detect_kind, merge_transcripts};

pub struct FanOut {
    pub harnesses: Vec<Arc<dyn Harness>>,
}

impl FanOut {
    async fn union(&self, q: &ListQuery) -> Result<Vec<Session>> {
        let results = try_join_all(self.harnesses.iter().map(|h| h.list_matching(q))).await?;
        let mut seen = HashSet::new();
        Ok(results
            .into_iter()
            .flatten()
            .filter(|s| seen.insert(s.session_id.clone()))
            .collect())
    }

    pub async fn list_matching(&self, q: &ListQuery) -> Result<Vec<Session>> {
        let count = q.content.len() + usize::from(q.edited_file.is_some());
        let mut rows = if count == 0 {
            self.union(q).await?
        } else {
            let cheap = ListQuery {
                content: Vec::new(),
                edited_file: None,
                ..q.clone()
            };
            let mut queries: Vec<ListQuery> = q
                .content
                .iter()
                .map(|needle| ListQuery {
                    content: vec![needle.clone()],
                    ..cheap.clone()
                })
                .collect();
            if let Some(path) = &q.edited_file {
                queries.push(ListQuery {
                    edited_file: Some(path.clone()),
                    ..cheap.clone()
                });
            }
            // Metadata must come from the first owning harness, not the first
            // one that happened to contain a particular needle.
            let mut candidates: HashMap<String, Session> = self
                .union(&cheap)
                .await?
                .into_iter()
                .map(|s| (s.session_id.clone(), s))
                .collect();
            for query in queries {
                let ids: HashSet<_> = self
                    .union(&query)
                    .await?
                    .into_iter()
                    .map(|s| s.session_id)
                    .collect();
                candidates.retain(|id, _| ids.contains(id));
            }
            candidates.into_values().collect()
        };
        rows.sort_by_key(|s| s.started_at);
        Ok(rows)
    }

    pub async fn transcript(&self, id: &str) -> Result<Vec<TranscriptEntry>> {
        let kind = detect_kind(id)?;
        let q = ListQuery {
            session_id: Some(id.into()),
            ..Default::default()
        };
        let owners = try_join_all(self.harnesses.iter().filter(|h| h.kind() == kind).map(|h| {
            let q = &q;
            async move { Ok::<_, anyhow::Error>((h, !h.list_matching(q).await?.is_empty())) }
        }))
        .await?;
        anyhow::ensure!(
            owners.iter().any(|(_, known)| *known),
            "session not found: {}",
            id
        );
        let results = futures::future::join_all(
            owners
                .into_iter()
                .filter(|(_, known)| *known)
                .map(|(h, _)| h.transcript(id)),
        )
        .await;
        let mut merged = Vec::new();
        let mut error = None;
        for result in results {
            match result {
                Ok(entries) => merged = merge_transcripts(merged, entries),
                Err(err) => {
                    if error.is_none() {
                        error = Some(err);
                    }
                }
            }
        }
        if merged.is_empty()
            && let Some(error) = error
        {
            return Err(error);
        }
        Ok(merged)
    }
}
