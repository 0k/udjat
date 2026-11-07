//! Acceptance ledger (contract -> automated evidence):
//! - Rich types, unknown fields, historical shapes, key order -> historical_wire_round_trip_preserves_order_nulls_and_shapes.
//! - Cursor ordering/direction/newest limit -> pages_are_oldest_first_and_limit_selects_newest.
//! - Last role/times/error, part/tool counts, sibling null semantics -> structural_counts_errors_siblings_and_storage_tiebreakers.
//! - Incomplete frontier and queued user -> structural_frontier_survives_queued_user_and_completed_turn_clears_it.
//! - Last-user id/text/agent/model/provider and child count -> frontier + user_attribution tests.
//! - Session row wire names and optional metadata -> enriched_rows_keep_archived_project_and_lightweight_metadata.
//! - Claude blocks/UUIDs/usage/correlation -> archive-claude-code tests/rich.rs.
//! - Pi blocks/usage/correlation/unknown data -> archive-pi tests/rich.rs.
//! - DB JSON/order and malformed rows -> archive-opencode tests/rich.rs.
//! - Archive Harness reads and storage immutability -> each archive tests/harness.rs.
//! - HTTP reads, pagination and pi raw conversion -> connector tests/server.rs (isolated real servers).
//! - Every JSONL flat projection -> each JSONL archive tests/corpus.rs (opt-in).
//! - Whole DB exact serialized round trip, forty-minute cap -> archive-opencode corpus_roundtrip (opt-in).
//! - 2,000 enriched rows, at most four live requests -> connector structural_status_live_sample (serial GETs, opt-in).
//! - Unchanged consumer API/output -> external ai-audit output-parity runner; no consumer source edits.
use serde_json::{Value, json};
use udjat_core::*;

#[test]
fn historical_wire_round_trip_preserves_order_nulls_and_shapes() {
    let source = r#"{"info":{"summary":{"diffs":"old string"},"role":"assistant","time":{"completed":null,"created":12,"other":true},"tokens":{"input":2,"cache":{"read":3,"unknown":9},"future":4},"error":null,"id":"m","sessionID":"s"},"parts":[{"state":{"input":"historical string","status":"completed","output":{"structured":true},"unknown":42},"type":"tool","tool":"read","id":"p","sessionID":"s","messageID":"m"},{"type":"future","data":[1,null],"id":"q"}]}"#;
    let value: Value = serde_json::from_str(source).unwrap();
    let record = from_opencode_v1(value.clone());
    assert_eq!(to_opencode_v1(&record).to_string(), value.to_string());
    let mut edited = record;
    edited.info.agent = Some("plan".into());
    assert_eq!(to_opencode_v1(&edited)["info"]["agent"], "plan");
}

#[test]
fn pages_are_oldest_first_and_limit_selects_newest() {
    let records = (0..5)
        .map(|i| {
            from_opencode_v1(json!({
                "info":{"id":format!("m{i}"),"role":"user","time":{"created":i}},"parts":[]
            }))
        })
        .collect::<Vec<_>>();
    let page = MessagePageResult::from_messages(
        records.clone(),
        MessagePage {
            limit: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        page.messages
            .iter()
            .map(|m| m.info.id.as_str())
            .collect::<Vec<_>>(),
        ["m3", "m4"]
    );
    assert_eq!(page.next_before.as_deref(), Some("m3"));
    let page = MessagePageResult::from_messages(
        records.clone(),
        MessagePage {
            limit: Some(2),
            before: Some("m3".into()),
            after: None,
        },
    )
    .unwrap();
    assert_eq!(
        page.messages
            .iter()
            .map(|m| m.info.id.as_str())
            .collect::<Vec<_>>(),
        ["m1", "m2"]
    );
    let page = MessagePageResult::from_messages(
        records.clone(),
        MessagePage {
            limit: Some(2),
            after: Some("m1".into()),
            before: None,
        },
    )
    .unwrap();
    assert_eq!(
        page.messages
            .iter()
            .map(|m| m.info.id.as_str())
            .collect::<Vec<_>>(),
        ["m2", "m3"]
    );
    assert!(
        MessagePageResult::from_messages(
            records,
            MessagePage {
                before: Some("missing".into()),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn structural_frontier_survives_queued_user_and_completed_turn_clears_it() {
    let mut records = vec![
        from_opencode_v1(
            json!({"info":{"id":"a","role":"assistant","parentID":"u","time":{"created":1,"completed":2}},"parts":[]}),
        ),
        from_opencode_v1(
            json!({"info":{"id":"b","role":"assistant","parentID":"u","time":{"created":3}},"parts":[{"id":"p","type":"tool","tool":"question","state":{"status":"running","input":{}}}]}),
        ),
        from_opencode_v1(
            json!({"info":{"id":"c","role":"user","time":{"created":4},"agent":"build","model":{"providerID":"provider","modelID":"model"}},"parts":[{"id":"x","type":"text","text":"first"},{"id":"y","type":"text","text":"second"}]}),
        ),
    ];
    let status = serde_json::to_value(StructuralStatus::from_messages(&records, 7)).unwrap();
    assert_eq!(
        status,
        json!({"lastMessageRole":"user","lastMessageTimeCreated":4,"lastMessageErrored":false,"partsTotal":2,"runningTools":0,"interruptedTools":0,"stuckTools":0,"lastIncompleteAssistantLastPart":{"type":"tool","toolName":"question","toolStatus":"running"},"priorCompletedAssistantSiblings":0,"lastUserMessageId":"c","lastUserMessageText":"first\nsecond","lastUserMessageAgent":"build","lastUserMessageProviderID":"provider","lastUserMessageModelID":"model","childCount":7})
    );
    records.push(from_opencode_v1(
        json!({"info":{"id":"d","role":"assistant","time":{"created":5,"completed":6}},"parts":[]}),
    ));
    assert!(
        StructuralStatus::from_messages(&records, 0)
            .last_incomplete_assistant_last_part
            .is_none()
    );
}

#[test]
fn structural_counts_errors_siblings_and_storage_tiebreakers() {
    let mut records = vec![
        from_opencode_v1(
            json!({"info":{"id":"b","role":"assistant","parentID":"u","time":{"created":2,"completed":3}},"parts":[]}),
        ),
        from_opencode_v1(
            json!({"info":{"id":"c","role":"assistant","parentID":"u","time":{"created":2,"completed":3},"error":{"name":"Error"}},"parts":[]}),
        ),
        from_opencode_v1(
            json!({"info":{"id":"d","role":"assistant","parentID":"u","time":{"created":2}},"parts":[
                {"id":"p1","type":"tool","tool":"read","state":{"status":"running"}},
                {"id":"p2","type":"tool","tool":"read","state":{"status":"pending"}},
                {"id":"p3","type":"tool","tool":"read","state":{"status":"error","metadata":{"interrupted":true}}},
                {"id":"p4","type":"tool","tool":"read","state":{"status":"error","metadata":{"interrupted":1}}},
                {"id":"p5","type":"tool","tool":"read","state":{"status":"error","metadata":{"interrupted":false}}},
                {"id":"p6","type":"text","text":"last"}
            ]}),
        ),
    ];
    // Storage time, not JSON time or vector position, selects the final part.
    records[2].parts[0].storage_time_created = Some(9);
    let s = StructuralStatus::from_messages(&records, 2);
    assert_eq!(
        (
            s.parts_total,
            s.running_tools,
            s.interrupted_tools,
            s.stuck_tools
        ),
        (6, 2, 2, 4)
    );
    assert_eq!(s.prior_completed_assistant_siblings, 1);
    assert_eq!(s.last_message_time_created, Some(2));
    assert_eq!(s.last_message_role.as_deref(), Some("assistant"));
    assert!(!s.last_message_errored);
    assert_eq!(
        s.last_incomplete_assistant_last_part,
        Some(IncompleteAssistantLastPart {
            type_name: "tool".into(),
            tool_name: Some("read".into()),
            tool_status: Some("running".into())
        })
    );
    records[0].info.storage_time_created = Some(20);
    let s = StructuralStatus::from_messages(&records, 0);
    assert_eq!(s.last_message_time_created, Some(20));
    assert_eq!(s.last_message_time_completed, Some(3));
    assert!(s.last_incomplete_assistant_last_part.is_none());
    records[0].info.parent_id = None;
    assert_eq!(
        StructuralStatus::from_messages(&records, 0).prior_completed_assistant_siblings,
        0
    );
    assert_eq!(
        StructuralStatus::from_messages(&[], 4),
        StructuralStatus {
            child_count: 4,
            ..Default::default()
        }
    );
}

#[test]
fn enriched_rows_keep_archived_project_and_lightweight_metadata() {
    let value = json!({"id":"s","title":"title","directory":"/home/user/project","parentID":"parent","projectID":"project","lightweight":true,"time":{"created":1,"updated":2,"archived":3},"partsTotal":0,"runningTools":0,"interruptedTools":0,"stuckTools":0,"priorCompletedAssistantSiblings":0,"childCount":0,"lastMessageErrored":false});
    let row = from_opencode_v1_row(value.clone(), HarnessKind::OpenCode).unwrap();
    assert_eq!(to_opencode_v1_row(&row), value);
}

#[test]
fn user_attribution_uses_nested_model_and_empty_text_is_not_missing() {
    let messages = vec![from_opencode_v1(
        json!({"info":{"id":"u","role":"user","providerID":"ignore","modelID":"ignore","time":{"created":1}},"parts":[{"id":"p","type":"text","text":""}]}),
    )];
    let status = StructuralStatus::from_messages(&messages, 0);
    assert_eq!(status.last_user_message_text.as_deref(), Some(""));
    assert!(status.last_user_message_provider_id.is_none());
    assert!(status.last_user_message_model_id.is_none());
}
