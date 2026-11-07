use std::{process::Command, sync::Arc};
use udjat_archive_opencode::OpenCodeArchive;
use udjat_core::ListQuery;
use udjat_fanout::FanOut;

#[test]
fn real_archives_merge_predicates_and_prefer_database_metadata_and_transcript() {
    if std::env::var_os("UDJAT_FANOUT_FIXTURE").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let fan = FanOut {
                harnesses: vec![
                    Arc::new(OpenCodeArchive::database_only()),
                    Arc::new(OpenCodeArchive::legacy_files_only()),
                ],
            };
            for q in [
                ListQuery::default(),
                ListQuery {
                    content: vec!["file needle".into()],
                    ..Default::default()
                },
                ListQuery {
                    content: vec!["database needle".into(), "file needle".into()],
                    ..Default::default()
                },
                ListQuery {
                    content: vec!["database needle".into()],
                    edited_file: Some("/fixture/a.rs".into()),
                    ..Default::default()
                },
            ] {
                let rows = fan.list_matching(&q).await.unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].title, "database title");
            }
            assert!(
                fan.list_matching(&ListQuery {
                    content: vec!["database needle".into(), "absent".into()],
                    ..Default::default()
                })
                .await
                .unwrap()
                .is_empty()
            );
            let entries = fan.transcript("ses_fixture").await.unwrap();
            assert_eq!(
                serde_json::to_value(&entries).unwrap(),
                serde_json::to_value(
                    udjat_archive_opencode::transcript::parse_transcript("ses_fixture").unwrap()
                )
                .unwrap()
            );
            assert!(entries.iter().any(|e| e.content == "file needle"));
        });
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join(".local/share/opencode");
    std::fs::create_dir_all(data.join("storage/session/p")).unwrap();
    std::fs::create_dir_all(data.join("storage/message/ses_fixture")).unwrap();
    std::fs::create_dir_all(data.join("storage/part/msg_file")).unwrap();
    std::fs::write(data.join("storage/session/p/ses_fixture.json"),r#"{"id":"ses_fixture","title":"file title","directory":"/fixture","time":{"created":1000,"updated":2000}}"#).unwrap();
    std::fs::write(
        data.join("storage/message/ses_fixture/msg_file.json"),
        r#"{"id":"msg_file","role":"assistant","time":{"created":2000}}"#,
    )
    .unwrap();
    std::fs::write(
        data.join("storage/part/msg_file/prt_file.json"),
        r#"{"type":"text","text":"file needle"}"#,
    )
    .unwrap();
    std::fs::write(
        data.join("storage/part/msg_file/prt_tool.json"),
        r#"{"type":"tool","tool":"edit","state":{"input":{"filePath":"a.rs"}}}"#,
    )
    .unwrap();
    let conn = rusqlite::Connection::open(data.join("opencode.db")).unwrap();
    conn.execute_batch(indoc::indoc! {r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER);
        CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        INSERT INTO session VALUES ('ses_fixture', NULL, '/fixture', 'database title', 1000, 2000);
        INSERT INTO message VALUES ('msg_db','ses_fixture',1000,1000,'{"id":"msg_db","role":"user","time":{"created":1000}}');
        INSERT INTO part VALUES ('prt_db','msg_db','ses_fixture',1000,1000,'{"type":"text","text":"database needle"}');
    "#}).unwrap();
    drop(conn);
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "real_archives_merge_predicates_and_prefer_database_metadata_and_transcript",
            "--nocapture",
        ])
        .env_clear()
        .env("HOME", root.path())
        .env("UDJAT_FANOUT_FIXTURE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
