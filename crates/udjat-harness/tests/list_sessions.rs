mod support;

use serde_json::{Value, json};
use support::{Isolation, assert_success, create_session_in_child};

#[tokio::test]
async fn lists_a_session_created_through_acp() {
    // Re-enter only after the helper has installed the isolated child environment.
    if std::env::var_os("UDJAT_TEST_CREATE_SESSION").is_some() {
        create_session_in_child().await;
        return;
    }
    let isolated = Isolation::new();
    let id = isolated.create_session().await;
    let output = isolated.list(&["--json"]).await;
    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session: Value = serde_json::from_str(&stdout).expect("exactly one JSON object");
    let title = session["title"].as_str().expect("title must be a string");
    assert!(!title.is_empty(), "title must be non-empty");
    let updated = session["updated"].as_i64().expect("UTC integer seconds");
    assert!(updated > 0, "updated must be positive");
    assert_eq!(
        session,
        json!({"updated": updated, "id": id, "cwd": isolated.cwd(), "title": title})
    );
    assert_eq!(
        stdout,
        format!(
            "{{\"updated\":{updated},\"id\":{},\"cwd\":{},\"title\":{}}}\n",
            serde_json::to_string(&id).unwrap(),
            serde_json::to_string(isolated.cwd()).unwrap(),
            serde_json::to_string(title).unwrap(),
        )
    );

    let human = isolated.list(&[]).await;
    assert_success(&human);
    let local = chrono::DateTime::from_timestamp(updated, 0).unwrap();
    assert_eq!(
        String::from_utf8(human.stdout).unwrap(),
        format!("{id}  {}  {title}\n", local.format("%Y-%m-%d %H:%M"))
    );
}

#[tokio::test]
async fn lists_nothing_for_a_directory_without_sessions() {
    let isolated = Isolation::new();
    for flags in [&["--json"][..], &[][..]] {
        let output = isolated.list(flags).await;
        assert_success(&output);
        assert_eq!(output.stdout, b"");
    }
}

#[tokio::test]
async fn fails_clearly_when_the_harness_cannot_start() {
    let isolated = Isolation::new();
    let output = isolated.list(&["--command", "/nonexistent/harness"]).await;
    assert!(!output.status.success(), "unstartable harness must fail");
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("/nonexistent/harness"),
        "stderr must name the failed command: {stderr}"
    );
}
