use indoc::indoc;
use udjat_core::{PartKind, transcript_entries};

#[test]
fn uuid_blocks_error_correlation_and_flat_projection() {
    let input = indoc! {r#"
        {"type":"assistant","uuid":"a","timestamp":"2024-01-01T00:00:00.123456Z","message":{"role":"assistant","model":"model","usage":{"input_tokens":4,"output_tokens":2},"content":[{"type":"thinking","thinking":"why"},{"type":"tool_use","id":"call","name":"read","input":{"path":"/home/user/file"}}]}}
        {"type":"user","uuid":"b","timestamp":"2024-01-01T00:00:01Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","is_error":true,"content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}]}}
    "#};
    let records = udjat_archive_claude_code::rich::from_jsonl(input, "session").unwrap();
    assert_eq!(records[0].info.id, "a");
    assert_eq!(records[0].info.model_id.as_deref(), Some("model"));
    assert_eq!(records[0].info.tokens.as_ref().unwrap().usage().input, 4);
    assert_eq!(records[0].parts[1].id, "a-1");
    assert!(matches!(&records[0].parts[1].kind,PartKind::Tool{state,..} if state.status=="error"));
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), input).unwrap();
    assert_eq!(
        serde_json::to_value(transcript_entries(&records)).unwrap(),
        serde_json::to_value(
            udjat_archive_claude_code::transcript::parse_transcript_from_file(temp.path()).unwrap()
        )
        .unwrap()
    );
}
