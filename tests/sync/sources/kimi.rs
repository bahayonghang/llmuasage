use super::super::*;

#[test]
fn kimi_first_sync_imports_only_turn_usage_with_raw_model() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤1：首次 sync 只导入 turn-scoped usage.record
     * ========================================================================
     * 目标：
     * 1) 只有 usageScope=turn 的非零 usage.record 成为 kimi_code 事件
     * 2) 四通道 + 饱和 total 正确
     * 3) 原始模型字符串（kimi-code/k3）逐字保留
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-first",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            // session-scoped aggregate is not per-turn usage.
            serde_json::json!({
                "type": "usage.record", "model": "kimi-code/k3",
                "usage": {"inputOther": 999, "output": 999, "inputCacheRead": 0, "inputCacheCreation": 0},
                "usageScope": "session", "time": 1_780_319_378_000i64
            })
            .to_string(),
            // step.end duplicates the turn usage but is not a usage.record.
            serde_json::json!({
                "type": "step.end",
                "usage": {"inputOther": 777, "output": 777, "inputCacheRead": 0, "inputCacheCreation": 0},
                "usageScope": "turn", "time": 1_780_319_379_000i64
            })
            .to_string(),
            // all-zero turn record is skipped.
            kimi_turn_line("kimi-code/k3", 0, 0, 0, 0, 1_780_319_380_000),
            // unrelated line type.
            serde_json::json!({
                "type": "context.append_loop_event",
                "event": {"type": "tool.call"}, "time": 1_780_319_381_000i64
            })
            .to_string(),
            // malformed line must not fail the whole file.
            "not valid json at all".to_string(),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_382_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
            None,
        )
        .await?;

        let stats = &summary.sources[0];
        assert_eq!(stats.source, SourceKind::KimiCode);
        assert_eq!(stats.changed_files, 1);
        assert_eq!(stats.events_seen, 2);
        assert_eq!(stats.events_inserted, 2);
        assert_eq!(stats.stored_events, 2);

        // Only the two turn records survive, with raw model + saturating total.
        let rows = kimi_event_rows(&app.paths.db_path)?;
        assert_eq!(
            rows,
            vec![
                (
                    "kimi-code/k3".to_string(),
                    5102,
                    13312,
                    8,
                    172,
                    5102 + 13312 + 8 + 172,
                ),
                ("kimi-code/k3".to_string(), 100, 0, 0, 50, 150),
            ]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_sync_twice_is_idempotent() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤2：重复 sync 幂等（onboarding gate 核心测试）
     * ========================================================================
     * 目标：二次空跑 changed_files==0、skipped_files>0、事件数不变。
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-hot",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].changed_files, 1);
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert!(second.sources[0].skipped_files > 0);
        assert_eq!(second.sources[0].bytes_scanned, 0);
        assert_eq!(second.sources[0].events_inserted, 0);
        assert_eq!(second.sources[0].stored_events, 2);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_recent_window_preserves_full_history_cursor_and_later_recovers_old_event() -> Result<()> {
    let fixture = Fixture::new()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let old_ms = now_ms - chrono::Duration::days(90).num_milliseconds();
    let recent_ms = now_ms - chrono::Duration::days(1).num_milliseconds();
    fixture.seed_kimi_code(
        "sess-recent-window",
        &[
            kimi_turn_line("kimi-code/k3", 11, 0, 0, 0, old_ms),
            kimi_turn_line("kimi-code/k3", 22, 0, 0, 0, recent_ms),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let bounded = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            recent_days: Some(30),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::KimiCode)?,
            vec![22]
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::KimiCode, "local")?
                .is_empty(),
            "bounded Kimi sync must not advance the full-history cursor"
        );

        let full = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::KimiCode)?,
            vec![11, 22]
        );
        assert_eq!(
            store
                .cursors()
                .load_file_cursors(SourceKind::KimiCode, "local")?
                .len(),
            1
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_append_imports_only_new_record() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤3：追加只导入新增记录（字节偏移事件键幂等）
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-append",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        fixture.append_kimi_code(
            "sess-append",
            &kimi_turn_line("kimi-code/k3", 7, 3, 1, 0, 1_780_319_390_000),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(appended.sources[0].changed_files, 1);
        assert!(appended.sources[0].bytes_scanned > 0);
        assert_eq!(appended.sources[0].events_seen, 1);
        assert_eq!(appended.sources[0].events_inserted, 1);
        // The two earlier events are not duplicated: byte-offset keys hold.
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 3);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_rewrite_resets_and_replaces_old_rows() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤4：改写/截断触发整文件重放，旧行清理后替换
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-rewrite",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        // Rewrite with different, shorter content: fingerprint/size change forces
        // a full reparse whose reset clears the stale rows for this path.
        fixture.seed_kimi_code(
            "sess-rewrite",
            &[kimi_turn_line(
                "kimi-code/k4",
                42,
                8,
                0,
                0,
                1_780_319_400_000,
            )],
        )?;
        let replaced =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(replaced.sources[0].changed_files, 1);
        assert_eq!(replaced.sources[0].events_replayed, 1);

        let rows = kimi_event_rows(&app.paths.db_path)?;
        assert_eq!(rows, vec![("kimi-code/k4".to_string(), 42, 0, 0, 8, 50)]);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_deleted_history_survives_regular_sync_and_blocks_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    let wire_path = fixture.seed_kimi_code(
        "sess-missing-history",
        &[kimi_turn_line(
            "kimi-code/k3",
            100,
            50,
            10,
            0,
            1_780_319_377_000,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);

        fs::remove_file(&wire_path)?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;

        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            store
                .source_files()
                .counts(SourceKind::KimiCode, "local")?
                .missing,
            1
        );
        let risk = store
            .source_files()
            .lossy_rebuild_risk(SourceKind::KimiCode, "local")?;
        assert_eq!(risk.missing_file_count, 1);
        assert_eq!(risk.protected_event_count, 1);

        let blocked = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
        )
        .await;
        let error = blocked.expect_err("missing Kimi history must block a lossy rebuild");
        assert!(
            error.to_string().contains("Refusing lossy sync --rebuild"),
            "{error:#}"
        );
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_missing_root_sync_succeeds_and_status_tracks_passive_data() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤5：缺失根 sync 成功且 source 状态在 passive_no_data/ready 间切换
     * ========================================================================
     */
    let fixture = Fixture::new()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        // No `.kimi-code` root at all: full sync still succeeds and imports zero
        // kimi events without marking other sources missing.
        commands::sync::run(&app).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 0);
        assert_eq!(
            store
                .source_files()
                .counts(SourceKind::Codex, "local")?
                .missing,
            0
        );
        assert_eq!(kimi_capability_status(&app, &store)?, "passive_no_data");

        // Seed one wire.jsonl: passive status flips to ready after import.
        fixture.seed_kimi_code(
            "sess-late",
            &[kimi_turn_line(
                "kimi-code/k3",
                100,
                50,
                0,
                0,
                1_780_319_377_000,
            )],
        )?;
        commands::sync::run(&app).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        assert_eq!(kimi_capability_status(&app, &store)?, "passive_ready");
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_code_home_override_and_raw_models_survive_query_layer() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤6：KIMI_CODE_HOME 覆盖 + 原始模型跨 query 层保留（AC2）
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    let override_root = fixture.home.join("custom-kimi");
    fixture.seed_kimi_code_under(
        &override_root,
        "sess-override",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k4-preview", 10, 5, 0, 0, 1_780_319_380_000),
        ],
    )?;
    unsafe {
        std::env::set_var("KIMI_CODE_HOME", &override_root);
    }

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.sources[0].events_inserted, 2);

        // model_breakdown reads the aggregated buckets: both raw model strings
        // survive event -> bucket -> query without whitelist/normalization.
        let filter = QueryFilter {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };
        let mut models = Dashboard::open(&store)?
            .model_breakdown(&filter)?
            .into_iter()
            .map(|breakdown| breakdown.model)
            .collect::<Vec<_>>();
        models.sort();
        assert_eq!(
            models,
            vec![
                "kimi-code/k3".to_string(),
                "kimi-code/k4-preview".to_string(),
            ]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_first_sync_marks_current_token_accounting() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤7：首次成功 sync 写入记账 marker，二次 sync 不被 legacy 拒绝
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-marker",
        &[kimi_turn_line(
            "kimi-code/k3",
            100,
            50,
            0,
            0,
            1_780_319_377_000,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(
            store.token_accounting_version(SourceKind::KimiCode)?,
            Some(expected_token_accounting_version(SourceKind::KimiCode)),
        );
        assert_eq!(expected_token_accounting_version(SourceKind::KimiCode), 2);
        assert!(!store.has_legacy_token_accounting(SourceKind::KimiCode)?);

        // Current marker keeps normal incremental writes allowed (no refusal).
        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(
            store.token_accounting_version(SourceKind::KimiCode)?,
            Some(2)
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
