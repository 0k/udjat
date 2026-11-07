use super::*;
use indoc::indoc;

struct EnvGuard(Option<std::ffi::OsString>);

impl EnvGuard {
    fn new() -> Self {
        Self(std::env::var_os("PI_CODING_AGENT_DIR"))
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
                None => std::env::remove_var("PI_CODING_AGENT_DIR"),
            }
        }
    }
}

#[test]
fn test_pi_data_dir_default() {
    let _lock = TEST_ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::new();
    // Ensure PI_CODING_AGENT_DIR is not set for this test
    // SAFETY: Test is single-threaded for this var.
    unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
    let dir = pi_data_dir();
    assert!(dir.ends_with(".pi/agent"));
}

#[test]
fn test_pi_data_dir_override() {
    let _lock = TEST_ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::new();
    // SAFETY: Test guards by setting then unsetting.
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", "/tmp/custom-pi-dir") };
    let dir = pi_data_dir();
    assert_eq!(dir, PathBuf::from("/tmp/custom-pi-dir"));
    unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
}

#[test]
fn test_sessions_dir_under_data_dir() {
    let _lock = TEST_ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::new();
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", "/tmp/test-pi") };
    assert_eq!(sessions_dir(), PathBuf::from("/tmp/test-pi/sessions"));
    unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
}

#[test]
fn test_resolve_attribution_pi_legacy_modelid_inferred_anthropic() {
    // Pure legacy fixture: only `modelID`, no provider anywhere.
    // Must infer `anthropic` from the `claude-` family and mark
    // the result as inferred.
    let jsonl = indoc! {r#"
        {"type":"session","version":3,"id":"x","timestamp":"2026-04-30T09:36:43Z","cwd":"/tmp"}
        {"type":"message","id":"a1","timestamp":"2026-04-30T09:37:01Z","message":{"role":"assistant","modelID":"claude-opus-4-7","usage":{"input":1,"output":1}}}
    "#};
    let attr = resolve_attribution_from_jsonl(jsonl, "ses").unwrap();
    assert_eq!(attr.llm_provider.as_deref(), Some("anthropic"));
    assert!(attr.llm_provider_inferred);
    assert_eq!(attr.model_id, "claude-opus-4-7");
}

#[test]
fn test_resolve_attribution_pi_unknown_family_errors_loudly() {
    // No provider, model family the inference table cannot map:
    // must error, never invent attribution.
    let jsonl = indoc! {r#"
        {"type":"session","version":3,"id":"x","timestamp":"2026-04-30T09:36:43Z","cwd":"/tmp"}
        {"type":"message","id":"a1","timestamp":"2026-04-30T09:37:01Z","message":{"role":"assistant","modelID":"unknown-model-9000","usage":{"input":1,"output":1}}}
    "#};
    let err = resolve_attribution_from_jsonl(jsonl, "ses")
        .expect_err("must fail for unknown family with no recorded provider");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown-model-9000") || msg.contains("provider"),
        "error should mention the model or missing provider: {msg}"
    );
}

#[test]
fn test_resolve_attribution_pi_no_assistant_errors() {
    // Session with no assistant turn at all.
    let jsonl = indoc! {r#"
        {"type":"session","version":3,"id":"x","timestamp":"2026-04-30T09:36:43Z","cwd":"/tmp"}
        {"type":"message","id":"u1","timestamp":"2026-04-30T09:37:01Z","message":{"role":"user","content":"hello"}}
    "#};
    let err = resolve_attribution_from_jsonl(jsonl, "ses-empty").expect_err("must error");
    assert!(err.to_string().contains("ses-empty"));
}
