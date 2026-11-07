use std::process::Command;
use udjat_archive_pi::PiArchive;
use udjat_core::{Capability, Harness, HarnessKind};

#[test]
fn reads_fixture_through_harness() {
    const ID: &str = "019191cd-7be0-7000-8000-000000000001";
    if std::env::var_os("UDJAT_ARCHIVE_FIXTURE").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let archive = PiArchive;
            assert_eq!(archive.kind(), HarnessKind::Pi);
            assert_eq!(archive.capabilities(), &[Capability::Read]);
            let sessions = Harness::list_sessions(&archive).await.unwrap();
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].session_id, ID);
            assert_eq!(sessions[0].project_dir, "/fixture");
            assert_eq!(Harness::messages(&archive,ID,Default::default()).await.unwrap().messages.len(),1);
            assert_eq!(Harness::session_row(&archive,ID).await.unwrap().unwrap().status.last_user_message_text.as_deref(),Some("hello fixture"));
            assert!(Harness::session_row(&archive,"missing").await.unwrap().is_none());
            assert!(Harness::children(&archive,ID).await.unwrap().is_empty());
            assert_eq!(Harness::list_rows(&archive,&Default::default()).await.unwrap().len(),1);
            let entries = Harness::transcript(&archive, ID).await.unwrap();
            assert_eq!(serde_json::to_value(entries).unwrap(), serde_json::json!([
                {"timestamp":"2026-01-01T00:00:01Z","role":"user","entry_type":"text","content":"hello fixture"}
            ]));
        });
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".pi/agent/sessions/--fixture--");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("2026-01-01T00-00-00_{ID}.jsonl"));
    let fixture = indoc::indoc! {r#"
        {"type":"session","version":3,"id":"019191cd-7be0-7000-8000-000000000001","timestamp":"2026-01-01T00:00:00Z","cwd":"/fixture"}
        {"type":"message","id":"u1","timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":[{"type":"text","text":"hello fixture"}]}}
    "#};
    std::fs::write(&path, fixture).unwrap();
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
    assert_eq!(std::fs::read_to_string(path).unwrap(), fixture);
}
