use std::process::Command;
use udjat_archive_opencode::OpenCodeArchive;
use udjat_core::{Capability, Harness, HarnessKind};

#[test]
fn reads_fixture_through_harness() {
    const ID: &str = "ses_fixture";
    if std::env::var_os("UDJAT_ARCHIVE_FIXTURE").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let archive = OpenCodeArchive;
            assert_eq!(archive.kind(), HarnessKind::OpenCode);
            assert_eq!(archive.capabilities(), &[Capability::Read]);
            let sessions = Harness::list_sessions(&archive).await.unwrap();
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].session_id, ID);
            assert_eq!(sessions[0].project_dir, "/fixture");
            let rich=Harness::messages(&archive,ID,Default::default()).await.unwrap();
            assert_eq!(rich.messages.len(),1);
            let row=Harness::session_row(&archive,ID).await.unwrap().unwrap();
            assert_eq!(row.status.last_message_role.as_deref(),Some("user"));
            assert!(Harness::session_row(&archive,"ses_missing").await.unwrap().is_none());
            assert!(Harness::children(&archive,ID).await.unwrap().is_empty());
            assert_eq!(Harness::list_rows(&archive,&Default::default()).await.unwrap().len(),1);
            let q = udjat_core::ListQuery {
                session_id: Some(ID.into()), project: Some("/fixture".into()),
                active: Some((1767225600, 1767225600)), content: vec!["hello".into(), "fixture".into()],
                ..Default::default()
            };
            assert_eq!(Harness::list_matching(&archive, &q).await.unwrap().len(), 1);
            let wrong = udjat_core::ListQuery { project: Some("/elsewhere".into()), ..q.clone() };
            assert!(Harness::list_matching(&archive, &wrong).await.unwrap().is_empty());
            let status = udjat_core::ListQuery { statuses: vec!["completed".into()], ..Default::default() };
            assert!(Harness::list_matching(&archive, &status).await.is_err());
            assert_eq!(Harness::list_sessions(&OpenCodeArchive::database_only()).await.unwrap().len(), 1);
            assert!(Harness::list_sessions(&OpenCodeArchive::legacy_files_only()).await.unwrap().is_empty());
            let entries = Harness::transcript(&archive, ID).await.unwrap();
            assert_eq!(serde_json::to_value(entries).unwrap(), serde_json::json!([
                {"timestamp":"2026-01-01T00:00:00Z","role":"user","entry_type":"text","content":"hello fixture"}
            ]));
            let conn = udjat_archive_opencode::db::open_db().unwrap();
            assert_eq!(conn.query_row("PRAGMA query_only", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        });
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".local/share/opencode");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("opencode.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(indoc::indoc! {r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER, project_id TEXT);
            CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
            CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
            INSERT INTO session VALUES ('ses_fixture', NULL, '/fixture', 'fixture', 1767225600000, 1767225600000, 'project');
            INSERT INTO message VALUES ('msg_fixture', 'ses_fixture', 1767225600000, 1767225600000, '{"id":"msg_fixture","role":"user","time":{"created":1767225600000}}');
            INSERT INTO part VALUES ('part_fixture', 'msg_fixture', 'ses_fixture', 1767225600000, 1767225600000, '{"id":"part_fixture","type":"text","text":"hello fixture"}');
        "#}).unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "reads_fixture_through_harness", "--nocapture"])
        .env_clear()
        .env("HOME", root.path())
        .env("UDJAT_ARCHIVE_FIXTURE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
