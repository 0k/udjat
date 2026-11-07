//! Opt-in proof against every installed pi JSONL, including nested sessions.
fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            collect(&e.path(), out)?;
        } else if e.path().extension().is_some_and(|e| e == "jsonl") {
            out.push(e.path());
        }
    }
    Ok(())
}
#[test]
#[ignore = "reads the installed pi corpus"]
fn corpus_projection_parity() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let mut paths = Vec::new();
    collect(&udjat_archive_pi::sessions_dir(), &mut paths)?;
    paths.sort();
    let limit = std::env::var("UDJAT_CORPUS_LIMIT")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(usize::MAX);
    paths.truncate(limit);
    let mut entries = 0;
    for (i, path) in paths.iter().enumerate() {
        let input = std::fs::read_to_string(path)?;
        let rich = udjat_archive_pi::rich::from_jsonl(&input, "corpus")?;
        let actual = serde_json::to_value(udjat_core::transcript_entries(&rich))?;
        let expected = serde_json::to_value(
            udjat_archive_pi::transcript::parse_transcript_from_jsonl(&input)?,
        )?;
        anyhow::ensure!(
            actual == expected,
            "projection mismatch at corpus file index {i}"
        );
        entries += actual.as_array().map_or(0, Vec::len);
        if (i + 1) % 100 == 0 {
            eprintln!(
                "Pi corpus {}/{} entries={} elapsed={:.1}s",
                i + 1,
                paths.len(),
                entries,
                started.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "Pi PASS sessions={} entries={} mismatches=0 elapsed={:.1}s",
        paths.len(),
        entries,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
