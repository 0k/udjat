//! Native OpenCode messages, with storage ordering kept outside wire JSON.
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path};
use udjat_core::{MessageRecord, from_opencode_v1};

pub fn session_row_from_conn(
    conn: &Connection,
    id: &str,
) -> Result<Option<udjat_core::SessionRow>> {
    let mut statement = conn.prepare("SELECT * FROM session WHERE id = ?")?;
    let mut rows = statement.query([id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let ms = |key: &str| -> Result<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::from_timestamp_millis(row.get::<_, i64>(key)?)
            .ok_or_else(|| anyhow::anyhow!("invalid session timestamp {key}"))
    };
    let created = ms("time_created")?;
    let session = udjat_core::Session {
        session_id: row.get("id")?,
        harness: udjat_core::HarnessKind::OpenCode,
        started_at: created,
        updated_at: if row.get::<_, i64>("time_updated")? > 0 {
            ms("time_updated")?
        } else {
            created
        },
        project_dir: row
            .get::<_, Option<String>>("directory")?
            .unwrap_or_default(),
        title: row.get::<_, Option<String>>("title")?.unwrap_or_default(),
        parent_id: row.get("parent_id")?,
    };
    let project = row.get::<_, Option<String>>("project_id")?;
    let children = conn.query_row(
        "SELECT count(*) FROM session WHERE parent_id=? AND project_id=?",
        rusqlite::params![id, project],
        |r| r.get::<_, i64>(0).map(|n| n as usize),
    )?;
    let messages = rich_messages_from_conn(conn, id)?;
    let mut result = udjat_core::SessionRow::new(
        session,
        udjat_core::StructuralStatus::from_messages(&messages, children),
    );
    if let Some(project) = project {
        result.extra.insert("projectID".into(), json!(project));
    }
    if let Ok(index) = row.as_ref().column_index("time_archived")
        && let Some(archived) = row.get::<_, Option<i64>>(index)?
    {
        result
            .extra
            .insert("time".into(), json!({"archived":archived}));
    }
    if let Ok(index) = row.as_ref().column_index("lightweight_system")
        && row.get::<_, Option<String>>(index)?.is_some()
    {
        result.extra.insert("lightweight".into(), json!(true));
    }
    Ok(Some(result))
}

pub type NativeStoredMessage = (Value, i64, Vec<i64>);
#[derive(Debug, Default)]
pub struct NativeReadReport {
    pub messages: Vec<NativeStoredMessage>,
    pub malformed_messages: usize,
    pub malformed_parts: usize,
}
pub fn native_messages_from_conn(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<NativeStoredMessage>> {
    Ok(native_report_from_conn(conn, session_id)?.messages)
}
pub fn native_report_from_conn(conn: &Connection, session_id: &str) -> Result<NativeReadReport> {
    let mut report = NativeReadReport::default();
    let mut part_stmt=conn.prepare("SELECT id, message_id, session_id, time_created, data FROM part WHERE session_id = ? ORDER BY time_created, id")?;
    let mut parts: HashMap<String, Vec<(Value, i64)>> = HashMap::new();
    for row in part_stmt.query_map([session_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (id, mid, sid, time, data) = row?;
        let Ok(mut v) = serde_json::from_str::<Value>(&data) else {
            report.malformed_parts += 1;
            continue;
        };
        if !v.is_object() {
            report.malformed_parts += 1;
            continue;
        }
        v["id"] = json!(id);
        v["sessionID"] = json!(sid);
        v["messageID"] = json!(mid);
        parts.entry(mid).or_default().push((v, time));
    }
    let mut stmt = conn.prepare(
        "SELECT id, time_created, data FROM message WHERE session_id = ? ORDER BY time_created, id",
    )?;
    let mut messages = Vec::new();
    for row in stmt.query_map([session_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, time, data) = row?;
        let Ok(mut info) = serde_json::from_str::<Value>(&data) else {
            report.malformed_messages += 1;
            continue;
        };
        if !info.is_object() {
            report.malformed_messages += 1;
            continue;
        }
        info["id"] = json!(id);
        info["sessionID"] = json!(session_id);
        let (parts, times): (Vec<_>, Vec<_>) =
            parts.remove(&id).unwrap_or_default().into_iter().unzip();
        messages.push((json!({"info":info,"parts":parts}), time, times));
    }
    if report.malformed_messages > 0 || report.malformed_parts > 0 {
        log::warn!(
            "OpenCode rich reader skipped malformed rows: messages={} parts={}",
            report.malformed_messages,
            report.malformed_parts
        );
    }
    report.messages = messages;
    Ok(report)
}
pub fn rich_messages_from_conn(conn: &Connection, session_id: &str) -> Result<Vec<MessageRecord>> {
    Ok(native_messages_from_conn(conn, session_id)?
        .into_iter()
        .map(|(wire, time, part_times)| {
            let mut record = from_opencode_v1(wire);
            record.info.storage_time_created = Some(time);
            for (part, time) in record.parts.iter_mut().zip(part_times) {
                part.storage_time_created = Some(time);
            }
            record
        })
        .collect())
}
pub fn rich_messages_from_dir(storage: &Path, session_id: &str) -> Result<Vec<MessageRecord>> {
    let path = storage.join("message").join(session_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut files = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    files.sort_by_key(|e| e.file_name());
    let mut messages = Vec::new();
    for file in files {
        if file.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let info: Value = serde_json::from_str(&std::fs::read_to_string(file.path())?)?;
        let id = info["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("legacy message missing id"))?;
        let dir = storage.join("part").join(id);
        let mut parts = Vec::new();
        if dir.exists() {
            let mut files = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
            files.sort_by_key(|e| e.file_name());
            for f in files {
                if f.path().extension().is_some_and(|e| e == "json") {
                    parts.push(serde_json::from_str::<Value>(&std::fs::read_to_string(
                        f.path(),
                    )?)?);
                }
            }
        }
        messages.push(from_opencode_v1(json!({"info":info,"parts":parts})));
    }
    Ok(messages)
}
