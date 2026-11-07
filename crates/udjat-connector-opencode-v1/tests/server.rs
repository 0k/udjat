use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use udjat_connector_opencode_v1::{Dialect, OpenCodeV1Harness};
use udjat_core::{Harness, ListQuery};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let status = Command::new("ps")
                .args(["-p", &self.0.id().to_string(), "-o", "pid,ppid,lstart,args"])
                .status()
                .unwrap();
            assert!(status.success());
            assert!(
                Command::new("kill")
                    .args(["-TERM", &self.0.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        self.0.wait().unwrap();
    }
}

async fn ready(command: &mut Command, url: &str) -> Server {
    let mut server = Server(
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::new();
    let start = Instant::now();
    loop {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server exited before readiness"
        );
        if client
            .get(format!(
                "{url}/session?lean=true&all_projects=true&archived=true&limit=0"
            ))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return server;
        }
        assert!(
            start.elapsed() < Duration::from_secs(90),
            "server readiness deadline"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn isolated(exe: &std::path::Path, root: &std::path::Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env_clear()
        .current_dir(root)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    for (key, dir) in [
        ("HOME", "home"),
        ("XDG_DATA_HOME", "share"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("TMPDIR", "tmp"),
    ] {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).unwrap();
        command.env(key, path);
    }
    command
        .env("OPENCODE_DB", root.join("fixture.db"))
        .env("OPENCODE_DISABLE_MODELS_FETCH", "1")
        .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
        .env("OPENCODE_DISABLE_DEFAULT_PLUGINS", "1");
    command
}

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn real_pi_server_filters_and_lossless_parsing() {
    let Some(cli) = std::env::var_os("UDJAT_TEST_PI_SERVER").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: UDJAT_TEST_PI_SERVER (path of the pi server cli.js) is not set");
        return;
    };
    if !cli.is_file() {
        eprintln!("SKIP: {} is not a file", cli.display());
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let agent = root.path().join("agent");
    std::fs::create_dir_all(agent.join("sessions/p")).unwrap();
    let raw = indoc::indoc! {r#"
        {"type":"session","version":3,"id":"019191cd-7be0-7000-8000-000000000001","timestamp":"2026-01-01T00:00:00Z","cwd":"/fixture"}
        {"type":"message","id":"u1","timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"hello fixture"}}
        {"type":"message","id":"a1","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","model":"gpt-5","provider":"openai","content":[{"type":"toolCall","name":"write","arguments":{"path":"a.rs","content":"second needle"}}]}}
    "#};
    std::fs::write(
        agent.join("sessions/p/019191cd-7be0-7000-8000-000000000001.jsonl"),
        raw,
    )
    .unwrap();
    let url = format!("http://127.0.0.1:{}", port());
    let mut cmd = isolated(std::path::Path::new("node"), root.path());
    cmd.arg(cli)
        .arg("--agent-dir")
        .arg(&agent)
        .args(["--bind", url.strip_prefix("http://").unwrap()]);
    let _server = ready(&mut cmd, &url).await;
    let harness = OpenCodeV1Harness::new(&url, Dialect::Pi).unwrap();
    let rows = harness
        .list_matching(&ListQuery {
            content: vec!["hello".into(), "second needle".into()],
            edited_file: Some("/fixture/a.rs".into()),
            project: Some("/fixture".into()),
            active: Some((1767225602, 1767225600)),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let id = &rows[0].session_id;
    assert_eq!(
        serde_json::to_value(harness.transcript(id).await.unwrap()).unwrap(),
        serde_json::to_value(
            udjat_archive_pi::transcript::parse_transcript_from_jsonl(raw).unwrap()
        )
        .unwrap()
    );
    assert_eq!(harness.list_messages(id).await.unwrap().len(), 2);
    assert_eq!(
        harness.resolve_attribution(id).await.unwrap().model_id,
        "gpt-5"
    );
    assert!(
        harness
            .session_tail_contains_text(id, "second needle", 1)
            .await
            .unwrap()
    );
    assert!(
        !harness
            .session_tail_contains_text(id, "hello", 1)
            .await
            .unwrap()
    );
    assert!(
        harness
            .list_matching(&ListQuery {
                statuses: vec!["completed".into()],
                ..Default::default()
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn real_opencode_server_filters_and_shared_conversion() {
    let exe = dirs_home_opencode();
    if !exe.is_file() {
        eprintln!("SKIP: real opencode executable not available");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let url = format!("http://127.0.0.1:{}", port());
    let mut cmd = isolated(&exe, root.path());
    cmd.args([
        "serve",
        "--hostname",
        "127.0.0.1",
        "--port",
        url.rsplit(':').next().unwrap(),
    ]);
    // Bootstrap the real schema and create a session through the real API,
    // then stop, seed parts without a model call, and restart on the same temp DB.
    let server = ready(&mut cmd, &url).await;
    let row: serde_json::Value = reqwest::Client::new()
        .post(format!("{url}/session"))
        .json(&serde_json::json!({"title":"fixture"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = row["id"].as_str().unwrap();
    drop(server);
    let conn = rusqlite::Connection::open(root.path().join("fixture.db")).unwrap();
    conn.execute("INSERT INTO message (id,session_id,time_created,time_updated,data) VALUES ('msg_fixture',?,1000,1000,?)",rusqlite::params![id, r#"{"role":"assistant","parentID":"msg_parent","modelID":"gpt-5","providerID":"openai","mode":"build","agent":"build","path":{"cwd":"/fixture","root":"/fixture"},"cost":0,"tokens":{"input":1,"output":1,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1000,"completed":2000}}"#]).unwrap();
    conn.execute("INSERT INTO part (id,message_id,session_id,time_created,time_updated,data) VALUES ('prt_fixture','msg_fixture',?,1000,2000,?)",rusqlite::params![id,r#"{"type":"text","text":"hello fixture","time":{"start":1000,"end":2000}}"#]).unwrap();
    let expected =
        udjat_archive_opencode::transcript::parse_transcript_from_conn(&conn, id).unwrap();
    drop(conn);
    let _server = ready(&mut cmd, &url).await;
    let harness = OpenCodeV1Harness::new(&url, Dialect::OpenCode).unwrap();
    let q = ListQuery {
        session_id: Some(id.into()),
        content: vec!["hello".into(), "fixture".into()],
        statuses: vec!["completed".into()],
        last_message_in: Some((1, 2)),
        ..Default::default()
    };
    assert_eq!(harness.list_matching(&q).await.unwrap().len(), 1);
    assert_eq!(
        serde_json::to_value(harness.transcript(id).await.unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert!(harness.session_contains_text(id, "hello").await.unwrap());
    assert!(
        !harness
            .session_edited_file(id, "/fixture/a.rs")
            .await
            .unwrap()
    );
    assert!(
        harness
            .session_tail_contains_text(id, "hello", 1)
            .await
            .unwrap()
    );
    assert_eq!(harness.list_messages(id).await.unwrap().len(), 1);
    assert_eq!(
        harness.resolve_attribution(id).await.unwrap().model_id,
        "gpt-5"
    );
    assert_eq!(harness.message_part_latencies(id).await.unwrap().len(), 1);
}

fn dirs_home_opencode() -> std::path::PathBuf {
    std::env::var_os("UDJAT_TEST_OPENCODE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/opencode")
        })
}
