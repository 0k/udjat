use indoc::indoc;
use serde_json::json;
use udjat_core::{PartKind, transcript_entries};

#[test]
fn preserves_blocks_correlates_results_and_projects_legacy_output() {
    let input = indoc! {r#"
        {"type":"message","id":"a","timestamp":"2024-01-01T00:00:00.123456Z","message":{"role":"assistant","model":"model","provider":"provider","usage":{"input":4,"output":2},"content":[{"type":"thinking","thinking":"why"},{"type":"toolCall","id":"call","name":"read","arguments":{"path":"/home/user/file"}},{"type":"future","data":[1,2]}]}}
        {"type":"message","id":"b","timestamp":"2024-01-01T00:00:01Z","message":{"role":"toolResult","toolCallId":"call","toolName":"read","isError":true,"content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}}
    "#};
    let records = udjat_archive_pi::rich::from_jsonl(input, "session").unwrap();
    assert_eq!(records[0].parts.len(), 3);
    assert_eq!(records[0].info.provider_id.as_deref(), Some("provider"));
    assert_eq!(records[0].info.model_id.as_deref(), Some("model"));
    assert_eq!(records[0].info.tokens.as_ref().unwrap().usage().input, 4);
    assert_eq!(
        records[0].parts[2].extra["native"],
        json!({"type":"future","data":[1,2]})
    );
    assert!(matches!(&records[0].parts[1].kind,PartKind::Tool{state,..} if state.status=="error"));
    assert_eq!(records[0].parts[2].extra["data"], json!([1, 2]));
    assert_eq!(
        serde_json::to_value(transcript_entries(&records)).unwrap(),
        serde_json::to_value(
            udjat_archive_pi::transcript::parse_transcript_from_jsonl(input).unwrap()
        )
        .unwrap()
    );
}
