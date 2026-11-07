use serde_json::{Value, json};
use udjat_core::{from_opencode_v1, to_opencode_v1};

#[test]
fn database_hydrates_ids_and_preserves_payload_and_storage_order() -> anyhow::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch(indoc::indoc! {"
        CREATE TABLE message(id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        CREATE TABLE part(id TEXT, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
    "})?;
    conn.execute(
        "INSERT INTO message (id,session_id,time_created,time_updated,data) VALUES ('m','s',9,9,?)",
        [r#"{"role":"assistant","time":{"created":1},"summary":{"diffs":"old"}}"#],
    )?;
    conn.execute("INSERT INTO part (id,message_id,session_id,time_created,time_updated,data) VALUES ('p','m','s',8,8,?)",[r#"{"type":"tool","tool":"read","state":{"input":"old","status":"running"}}"#])?;
    let records = udjat_archive_opencode::rich::rich_messages_from_conn(&conn, "s")?;
    assert_eq!(records[0].info.storage_time_created, Some(9));
    assert_eq!(records[0].parts[0].storage_time_created, Some(8));
    assert_eq!(
        to_opencode_v1(&records[0]),
        json!({"info":{"role":"assistant","time":{"created":1},"summary":{"diffs":"old"},"id":"m","sessionID":"s"},"parts":[{"type":"tool","tool":"read","state":{"input":"old","status":"running"},"id":"p","sessionID":"s","messageID":"m"}]})
    );
    Ok(())
}

#[test]
#[ignore = "reads every message of the installed database, bounded to forty minutes"]
fn corpus_roundtrip() -> anyhow::Result<()> {
    let conn = udjat_archive_opencode::db::open_db()?;
    let sessions = udjat_archive_opencode::db::list_sessions_from_conn(&conn)?;
    let limit = std::env::var("UDJAT_CORPUS_LIMIT")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(usize::MAX);
    let total = sessions.len().min(limit);
    let start = std::time::Instant::now();
    let mut count = 0;
    let mut parts = 0;
    let mut malformed_messages = 0;
    let mut malformed_parts = 0;
    for (i, session) in sessions.iter().take(limit).enumerate() {
        let report =
            udjat_archive_opencode::rich::native_report_from_conn(&conn, &session.session_id)?;
        malformed_messages += report.malformed_messages;
        malformed_parts += report.malformed_parts;
        for (wire, _, _) in report.messages {
            let actual = to_opencode_v1(&from_opencode_v1(wire.clone()));
            anyhow::ensure!(
                serde_json::to_string(&actual)? == serde_json::to_string(&wire)?,
                "roundtrip mismatch at session index {i} message {count}"
            );
            count += 1;
            parts += wire["parts"].as_array().map_or(0, Vec::len);
        }
        if (i + 1) % 100 == 0 {
            eprintln!(
                "OpenCode corpus {}/{total} messages={count} parts={parts} elapsed={:.1}s",
                i + 1,
                start.elapsed().as_secs_f64()
            );
        }
        anyhow::ensure!(
            start.elapsed().as_secs() < 2400,
            "corpus exceeded forty minute bound"
        );
    }
    eprintln!(
        "OpenCode PASS sessions={total} messages={count} parts={parts} mismatches=0 malformed_messages={malformed_messages} malformed_parts={malformed_parts} elapsed={:.1}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

#[test]
fn historical_roundtrip_is_serialized_order_exact() {
    let value:Value=serde_json::from_str(r#"{"info":{"role":"system","time":{"created":0},"future":null},"parts":[{"type":"file","mime":"x","url":"https://example.org/file","future":{"a":1}}]}"#).unwrap();
    assert_eq!(
        to_opencode_v1(&from_opencode_v1(value.clone())).to_string(),
        value.to_string()
    );
}

#[test]
fn legacy_files_retain_native_parts_and_metadata() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir_all(root.path().join("message/s"))?;
    std::fs::create_dir_all(root.path().join("part/m"))?;
    let wire = json!({"info":{"id":"m","sessionID":"s","role":"user","time":{"created":1},"summary":"historical"},"parts":[{"id":"p","messageID":"m","sessionID":"s","type":"file","mime":"text/plain","url":"https://example.org/file","filename":"file.txt","source":{"kind":"future"}}]});
    std::fs::write(
        root.path().join("message/s/m.json"),
        serde_json::to_vec(&wire["info"])?,
    )?;
    std::fs::write(
        root.path().join("part/m/p.json"),
        serde_json::to_vec(&wire["parts"][0])?,
    )?;
    let records = udjat_archive_opencode::rich::rich_messages_from_dir(root.path(), "s")?;
    assert_eq!(records.len(), 1);
    assert_eq!(to_opencode_v1(&records[0]).to_string(), wire.to_string());
    assert!(
        udjat_archive_opencode::rich::rich_messages_from_dir(root.path(), "missing")?.is_empty()
    );
    Ok(())
}

#[test]
fn malformed_rows_are_counted_not_silently_normalized() -> anyhow::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch(indoc::indoc! {"
        CREATE TABLE message(id TEXT,session_id TEXT,time_created INTEGER,data TEXT);
        CREATE TABLE part(id TEXT,message_id TEXT,session_id TEXT,time_created INTEGER,data TEXT);
        INSERT INTO message VALUES ('m','s',1,'{');
        INSERT INTO part VALUES ('p','m','s',1,'{');
    "})?;
    let report = udjat_archive_opencode::rich::native_report_from_conn(&conn, "s")?;
    assert_eq!((report.malformed_messages, report.malformed_parts), (1, 1));
    assert!(report.messages.is_empty());
    Ok(())
}
