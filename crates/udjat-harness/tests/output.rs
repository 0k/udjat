use agent_client_protocol::schema::v1::SessionInfo;
use udjat_harness::cli::render;

#[test]
fn json_uses_utc_seconds_and_primary_fields_in_order() {
    let session = SessionInfo::new("session-test", "/work")
        .title("A \"quoted\" title\nsecond line")
        .updated_at("2026-01-02T03:04:05.123+02:00");
    assert_eq!(
        render(&[session], true).unwrap(),
        "{\"updated\":1767315845,\"id\":\"session-test\",\"cwd\":\"/work\",\"title\":\"A \\\"quoted\\\" title\\nsecond line\"}\n"
    );
}

#[test]
fn missing_optional_fields_are_dashes_for_humans_and_omitted_in_json() {
    let sessions = [SessionInfo::new("session-test", "/work")];
    assert_eq!(render(&sessions, false).unwrap(), "session-test  -  -\n");
    assert_eq!(
        render(&sessions, true).unwrap(),
        "{\"id\":\"session-test\",\"cwd\":\"/work\"}\n"
    );
}

#[test]
fn invalid_timestamps_fail_instead_of_silently_losing_data() {
    let session = SessionInfo::new("session-test", "/work").updated_at("not-a-timestamp");
    for json in [false, true] {
        let error = render(std::slice::from_ref(&session), json).unwrap_err();
        assert!(
            error.contains("session-test") && error.contains("not-a-timestamp"),
            "{error}"
        );
    }
}
