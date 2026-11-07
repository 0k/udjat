use serde_json::json;
use udjat_archive_opencode::{MessageWithParts, latency, transcript};

#[test]
fn json_and_database_share_transcript_message_and_latency_conversion() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(indoc::indoc! {"
        CREATE TABLE session (id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER);
        CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
    "}).unwrap();
    let info = json!({"id":"msg_a","role":"assistant","modelID":"gpt-5","providerID":"openai","time":{"created":1000},"tokens":{"input":3}});
    let part = json!({"id":"part_a","type":"text","text":"hello","time":{"start":1000,"end":1500}});
    conn.execute("INSERT INTO session (id, directory, title, time_created, time_updated) VALUES ('ses_a','/fixture','a',1000,1500)", []).unwrap();
    conn.execute("INSERT INTO message (id,session_id,time_created,time_updated,data) VALUES ('msg_a','ses_a',1000,1000,?)", [info.to_string()]).unwrap();
    conn.execute("INSERT INTO part (id,message_id,session_id,time_created,time_updated,data) VALUES ('part_a','msg_a','ses_a',1000,1500,?)", [part.to_string()]).unwrap();
    let wire: Vec<MessageWithParts> =
        serde_json::from_value(json!([{ "info": info, "parts": [part] }])).unwrap();
    assert_eq!(
        serde_json::to_value(transcript::parse_transcript_from_messages(&wire)).unwrap(),
        serde_json::to_value(transcript::parse_transcript_from_conn(&conn, "ses_a").unwrap())
            .unwrap()
    );
    assert_eq!(
        latency::message_part_latencies_from_messages(&wire),
        latency::message_part_latencies_from_conn(&conn, "ses_a").unwrap()
    );
    assert_eq!(
        udjat_archive_opencode::messages_from_json(&wire, "ses_a")[0]
            .model
            .as_deref(),
        Some("gpt-5")
    );
    assert!(udjat_archive_opencode::messages_contain_text(
        &wire, "hello"
    ));
    assert!(!udjat_archive_opencode::messages_contain_text(
        &wire, "absent"
    ));
}
