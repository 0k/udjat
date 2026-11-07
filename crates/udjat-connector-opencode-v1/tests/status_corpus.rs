//! Explicitly opted-in read-only comparison against an existing server.
#[tokio::test]
#[ignore = "needs an explicitly selected live server and installed read-only archive"]
async fn structural_status_live_sample() -> anyhow::Result<()> {
    let url = std::env::var("UDJAT_STATUS_SERVER")?;
    let limit = std::env::var("UDJAT_STATUS_SAMPLE")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(2000);
    let conn = udjat_archive_opencode::db::open_db()?;
    let sessions = udjat_archive_opencode::db::list_sessions_from_conn(&conn)?;
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let start = std::time::Instant::now();
    let mut count = 0;
    // Evenly sample the entire history, not only recent live sessions. Serial
    // GETs deliberately stay below the four-request concurrency ceiling.
    for i in 0..limit.min(sessions.len()) {
        let s = &sessions[i * sessions.len() / limit.min(sessions.len())];
        let before = client
            .get(format!(
                "{}/session/{}",
                url.trim_end_matches('/'),
                s.session_id
            ))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let messages = udjat_archive_opencode::rich::rich_messages_from_conn(&conn, &s.session_id)?;
        let children=conn.query_row("SELECT count(*) FROM session c JOIN session p ON c.parent_id=p.id AND c.project_id=p.project_id WHERE p.id=?",[&s.session_id],|r|r.get::<_,i64>(0).map(|n| n as usize))?;
        let actual = udjat_core::StructuralStatus::from_messages(&messages, children);
        let expected: udjat_core::StructuralStatus = serde_json::from_value(before.clone())?;
        if actual != expected {
            let after = client
                .get(format!(
                    "{}/session/{}",
                    url.trim_end_matches('/'),
                    s.session_id
                ))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await?;
            anyhow::ensure!(
                before == after,
                "sample session changed during read at index {i}; rerun stable sample"
            );
            anyhow::bail!(
                "structural mismatch at sample {i}: actual={actual:?} expected={expected:?}"
            );
        }
        count += 1;
        if count % 100 == 0 {
            eprintln!(
                "status sample {count}/{limit} elapsed={:.1}s",
                start.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "status PASS sessions={count} mismatches=0 concurrency=1 elapsed={:.1}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
