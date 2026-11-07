//! Opt-in, read-only proof against the installed archive; never prints content.
#[test]
#[ignore = "reads the installed Claude Code corpus"]
fn corpus_projection_parity() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let limit = std::env::var("UDJAT_CORPUS_LIMIT")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(usize::MAX);
    let mut paths = Vec::new();
    for project in std::fs::read_dir(udjat_archive_claude_code::projects_dir())? {
        let project = project?;
        if !project.file_type()?.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(project.path())? {
            let file = file?;
            if file.path().extension().is_some_and(|e| e == "jsonl") {
                paths.push(file.path());
            }
        }
    }
    paths.sort();
    paths.truncate(limit);
    let mut entries = 0;
    for (i, path) in paths.iter().enumerate() {
        let input = std::fs::read_to_string(path)?;
        let rich = udjat_archive_claude_code::rich::from_jsonl(&input, "corpus")?;
        let actual = serde_json::to_value(udjat_core::transcript_entries(&rich))?;
        let expected = serde_json::to_value(
            udjat_archive_claude_code::transcript::parse_transcript_from_file(path)?,
        )?;
        anyhow::ensure!(
            actual == expected,
            "projection mismatch at corpus file index {i}"
        );
        entries += actual.as_array().map_or(0, Vec::len);
        if (i + 1) % 100 == 0 {
            eprintln!(
                "Claude corpus {}/{} entries={} elapsed={:.1}s",
                i + 1,
                paths.len(),
                entries,
                started.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "Claude PASS sessions={} entries={} mismatches=0 elapsed={:.1}s",
        paths.len(),
        entries,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
