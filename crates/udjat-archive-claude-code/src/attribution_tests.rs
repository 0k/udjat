use super::*;
use chrono::TimeZone;
use udjat_core::TokenUsage;

fn assistant(model: &str, ts_secs: i64) -> Message {
    Message {
        message_id: format!("msg-{}", ts_secs),
        session_id: "ses".to_string(),
        harness: Provider::ClaudeCode,
        role: "assistant".to_string(),
        model: Some(model.to_string()),
        provider_id: None,
        agent: None,
        mode: None,
        timestamp: chrono::Utc.timestamp_opt(ts_secs, 0).unwrap(),
        tokens: Some(TokenUsage::default()),
    }
}

#[test]
fn test_resolve_attribution_claude_picks_most_recent_assistant() {
    // Older assistant turn is ignored when a newer one exists.
    let msgs = vec![
        assistant("claude-haiku-3-5", 1_000),
        assistant("claude-sonnet-4-5", 2_000),
    ];
    let attr = resolve_attribution_from_messages(&msgs, "ses").unwrap();
    assert_eq!(attr.model_id, "claude-sonnet-4-5");
}

#[test]
fn test_resolve_attribution_claude_unknown_family_errors() {
    // Non-Claude model in a Claude Code session — we must NOT
    // hand-wave attribution.  Loud failure is the contract.
    let msgs = vec![assistant("custom-fork-of-something", 1_000)];
    let err = resolve_attribution_from_messages(&msgs, "ses").expect_err("must error");
    assert!(err.to_string().contains("custom-fork-of-something"));
}

#[test]
fn test_resolve_attribution_claude_no_assistant_errors() {
    let err = resolve_attribution_from_messages(&[], "ses-x").expect_err("must error");
    assert!(err.to_string().contains("ses-x"));
}
