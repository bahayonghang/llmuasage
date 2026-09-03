use super::*;

#[test]
fn reset_for_source_codex_keeps_claude_intact() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    seed_resettable_row(&store, SourceKind::Codex, "reset-me")?;
    seed_resettable_row(&store, SourceKind::Claude, "keep-me")?;

    store.reset_for_source(SourceKind::Codex, "local")?;
    let conn = store.open_connection()?;
    let codex_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    let claude_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'claude'",
        [],
        |row| row.get(0),
    )?;
    let codex_raw: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event_raw WHERE event_key LIKE 'local:codex:%'",
        [],
        |row| row.get(0),
    )?;
    let claude_raw: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event_raw WHERE event_key LIKE 'local:claude:%'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(codex_events, 0);
    assert_eq!(codex_raw, 0);
    assert_eq!(
        count_rows(&store, "usage_turn", "WHERE source = 'codex'")?,
        0
    );
    assert_eq!(
        count_rows(&store, "usage_tool_call", "WHERE source = 'codex'")?,
        0
    );
    assert_eq!(claude_events, 1);
    assert_eq!(claude_raw, 1);
    assert_eq!(
        count_rows(&store, "usage_turn", "WHERE source = 'claude'")?,
        1
    );
    assert_eq!(
        count_rows(&store, "usage_tool_call", "WHERE source = 'claude'")?,
        1
    );
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Codex, "local")?
            .live,
        0
    );
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Claude, "local")?
            .live,
        1
    );
    Ok(())
}

#[test]
fn reset_usage_data_clears_behavior_facts() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    seed_resettable_row(&store, SourceKind::Codex, "reset-codex")?;
    seed_resettable_row(&store, SourceKind::Claude, "reset-claude")?;
    store.run_log().record_run_start("sync")?;
    store.integration_state().record_integration_state(
        SourceKind::Codex,
        "plugin",
        "installed",
        None,
        None,
        None,
    )?;
    let run_log_before = count_rows(&store, "run_log", "")?;
    let install_before = count_rows(&store, "integration_install", "")?;
    assert!(run_log_before > 0);
    assert!(install_before > 0);

    store.reset_usage_data()?;

    assert_eq!(count_rows(&store, "usage_event", "")?, 0);
    assert_eq!(count_rows(&store, "usage_event_raw", "")?, 0);
    assert_eq!(count_rows(&store, "usage_turn", "")?, 0);
    assert_eq!(count_rows(&store, "usage_tool_call", "")?, 0);
    assert_eq!(count_rows(&store, "run_log", "")?, run_log_before);
    assert_eq!(
        count_rows(&store, "integration_install", "")?,
        install_before
    );
    Ok(())
}
