use llmusage::store::{expected_token_accounting_version, set_rebuild_reset_failpoint};

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
    let source_file_before = count_rows(&store, "source_file", "")?;
    assert!(run_log_before > 0);
    assert!(install_before > 0);
    assert!(
        source_file_before > 0,
        "reset_usage_data must observe source_file rows before deleting them"
    );

    store.reset_usage_data()?;

    assert_eq!(count_rows(&store, "usage_event", "")?, 0);
    assert_eq!(count_rows(&store, "usage_event_raw", "")?, 0);
    assert_eq!(count_rows(&store, "usage_turn", "")?, 0);
    assert_eq!(count_rows(&store, "usage_tool_call", "")?, 0);
    assert_eq!(count_rows(&store, "source_file", "")?, 0);
    assert_eq!(count_rows(&store, "run_log", "")?, run_log_before);
    assert_eq!(
        count_rows(&store, "integration_install", "")?,
        install_before
    );
    Ok(())
}

#[test]
fn rebuild_reset_rolls_back_when_a_later_source_fails() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    seed_resettable_row(&store, SourceKind::Codex, "reset-codex")?;
    seed_resettable_row(&store, SourceKind::Claude, "reset-claude")?;
    store.mark_current_token_accounting(SourceKind::Codex)?;
    store.mark_current_token_accounting(SourceKind::Claude)?;

    let _guard = set_rebuild_reset_failpoint(SourceKind::Claude);
    let error = store
        .reset_for_sources(&[SourceKind::Codex, SourceKind::Claude], "local")
        .expect_err("second-source failpoint must abort the batch reset");
    assert!(
        error.to_string().contains("test failpoint"),
        "unexpected error: {error}"
    );

    assert_eq!(
        count_rows(&store, "usage_event", "WHERE source = 'codex'")?,
        1
    );
    assert_eq!(
        count_rows(&store, "usage_event", "WHERE source = 'claude'")?,
        1
    );
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Codex, "local")?
            .live,
        1
    );
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Claude, "local")?
            .live,
        1
    );
    assert_eq!(
        store.token_accounting_version(SourceKind::Codex)?,
        Some(expected_token_accounting_version(SourceKind::Codex))
    );
    assert_eq!(
        store.token_accounting_version(SourceKind::Claude)?,
        Some(expected_token_accounting_version(SourceKind::Claude))
    );
    Ok(())
}
