use crate::Session;

/// Audit filters. Activity bounds are inclusive whole epoch seconds;
/// last-message bounds are half-open. Empty fields impose no constraint.
/// Status strings are harness-owned; archives cannot classify them.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub session_id: Option<String>,
    pub parent: Option<String>,
    pub project: Option<String>,
    pub roots_only: bool,
    pub active: Option<(i64, i64)>,
    pub content: Vec<String>,
    pub edited_file: Option<String>,
    pub statuses: Vec<String>,
    pub last_message_in: Option<(i64, i64)>,
}

impl ListQuery {
    pub fn matches_metadata(&self, session: &Session) -> bool {
        self.session_id
            .as_ref()
            .is_none_or(|id| id == &session.session_id)
            && self
                .parent
                .as_ref()
                .is_none_or(|id| session.parent_id.as_ref() == Some(id))
            && self
                .project
                .as_ref()
                .is_none_or(|dir| dir == &session.project_dir)
            && (!self.roots_only || session.parent_id.is_none())
            && self.active.is_none_or(|(from, to)| {
                session.started_at.timestamp() <= to && session.updated_at.timestamp() >= from
            })
    }
}
