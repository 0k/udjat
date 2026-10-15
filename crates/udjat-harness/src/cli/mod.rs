mod def;
pub mod log;

use std::io::Write;
use std::path::Path;

use agent_client_protocol::schema::v1::{SessionId, SessionInfo};
use chrono::{DateTime, Local};
use clap::Parser;
use serde::Serialize;

pub fn run() -> Result<(), String> {
    let cli = def::Args::parse();
    if cli.color {
        colored::control::set_override(true);
    }
    if cli.no_color {
        colored::control::set_override(false);
    }
    let _log = log::setup(&cli.logging)?;
    match cli.action {
        def::Actions::ListSessions { cwd, command, json } => {
            let cwd = cwd
                .map(Ok)
                .unwrap_or_else(std::env::current_dir)
                .and_then(std::path::absolute)
                .map_err(|error| format!("Cannot resolve working directory: {error}"))?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("Cannot start runtime: {error}"))?;
            let sessions = runtime.block_on(crate::acp::list_sessions(&command, cwd))?;
            let output = render(&sessions, json)?;
            std::io::stdout()
                .lock()
                .write_all(output.as_bytes())
                .map_err(|error| format!("Cannot write sessions: {error}"))
        }
    }
}

pub fn render(sessions: &[SessionInfo], json: bool) -> Result<String, String> {
    #[derive(Serialize)]
    struct Record<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        updated: Option<i64>,
        id: &'a SessionId,
        cwd: &'a Path,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<&'a str>,
    }

    let mut output = String::new();
    for session in sessions {
        let updated = session
            .updated_at
            .as_deref()
            .map(DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|error| {
                format!(
                    "Invalid updatedAt {:?} for session {}: {error}",
                    session.updated_at, session.session_id
                )
            })?;
        if json {
            let record = Record {
                updated: updated.map(|timestamp| timestamp.timestamp()),
                id: &session.session_id,
                cwd: &session.cwd,
                title: session.title.as_deref(),
            };
            output.push_str(&serde_json::to_string(&record).map_err(|error| {
                format!("Cannot serialize session {}: {error}", session.session_id)
            })?);
            output.push('\n');
        } else {
            let date = updated
                .map(|timestamp| {
                    timestamp
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_else(|| "-".to_string());
            let title = session
                .title
                .as_deref()
                .unwrap_or("-")
                .replace(['\n', '\r'], " ");
            output.push_str(&format!("{}  {date}  {title}\n", session.session_id));
        }
    }
    Ok(output)
}
