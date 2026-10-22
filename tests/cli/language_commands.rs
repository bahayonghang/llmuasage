use super::*;
use llmusage::{common::i18n::Language, runtime::preferences};
use std::process::{Output, Stdio};

fn invoke(
    fixture: &ReportCliFixture,
    language: Language,
    args: &[&str],
    console: &str,
) -> Result<Output> {
    preferences::write(&fixture.paths.root_dir, language)?;
    let mut command = test_process::llmusage_command();
    command
        .arg("--home")
        .arg(&fixture.paths.root_dir)
        .args(args)
        .env_remove("LLMUSAGE_HOME")
        .env("RUST_LOG", console)
        .env("LLMUSAGE_LOG", "off")
        .env("LLMUSAGE_PRICING_REFRESH", "off")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    for key in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "CODEX_HOME",
        "OPENCODE_HOME",
        "OPENCODE_DATA_DIR",
        "OPENCODE_CONFIG_DIR",
        "CLAUDE_CONFIG_DIR",
        "ZCODE_HOME",
        "PI_CODING_AGENT_DIR",
        "OMP_CODING_AGENT_DIR",
        "KIMI_HOME",
        "GROK_HOME",
        "DSH_ROOT",
        "DSH_HOME",
        "GEMINI_CLI_HOME",
        "ANTIGRAVITY_IDE_HOME",
    ] {
        command.env(key, fixture.home.join(key));
    }
    command.output().context("spawn paired operations CLI")
}
fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
fn err(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn checked(output: &Output) {
    assert!(output.status.success(), "{output:?}");
    assert!(!out(output).contains('\u{1b}'));
}

#[test]
fn operational_human_results_follow_language_without_changing_fixed_json() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        input_tokens: 11,
        output_tokens: 1,
        total_tokens: 12,
        ..Default::default()
    })?;
    for (args, english, chinese) in [
        (vec!["init"], "Init finished.", "初始化完成"),
        (vec!["status"], "Status:", "状态："),
        (vec!["source-status"], "Source status:", "来源状态："),
        (vec!["doctor"], "Doctor:", "健康检查："),
        (vec!["catalog", "status"], "Pricing catalog:", "价格目录："),
        (
            vec!["catalog", "reset"],
            "Pricing catalog reset:",
            "已重置价格目录：",
        ),
        (vec!["logs"], "Logs:", "日志："),
        (
            vec!["remote", "sync"],
            "No ssh hosts registered.",
            "尚未注册 SSH 主机",
        ),
    ] {
        for (language, expected) in [(Language::English, english), (Language::Chinese, chinese)] {
            let output = invoke(&fixture, language, &args, "off")?;
            checked(&output);
            assert!(out(&output).contains(expected), "{output:?}");
        }
    }
    // These branches neither modify their canonical payload nor write a run record.
    for args in [
        vec!["doctor", "--json"],
        vec!["logs", "--json"],
        vec!["remote", "handshake"],
    ] {
        let en = invoke(&fixture, Language::English, &args, "off")?;
        let zh = invoke(&fixture, Language::Chinese, &args, "off")?;
        checked(&en);
        checked(&zh);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&en.stdout)?,
            serde_json::from_slice::<serde_json::Value>(&zh.stdout)?
        );
    }
    // Catalog's tracked audit is outside its fixed status DTO.
    let en = invoke(
        &fixture,
        Language::English,
        &["catalog", "status", "--json"],
        "off",
    )?;
    let zh = invoke(
        &fixture,
        Language::Chinese,
        &["catalog", "status", "--json"],
        "off",
    )?;
    checked(&en);
    checked(&zh);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&en.stdout)?,
        serde_json::from_slice::<serde_json::Value>(&zh.stdout)?
    );
    let conn = Connection::open(&fixture.paths.db_path)?;
    assert_eq!(
        conn.query_row("SELECT SUM(total_tokens) FROM usage_event", [], |row| row
            .get::<_, i64>(
            0
        ))?,
        12
    );
    Ok(())
}

#[test]
fn diagnostics_default_and_out_are_canonical_while_forget_rejections_are_bilingual() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let mut snapshots = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let output = invoke(&fixture, language, &["diagnostics"], "off")?;
        checked(&output);
        let mut value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        // Wall-clock observation is the sole dynamic field under this immutable fixture.
        value.as_object_mut().unwrap().remove("archive_observed_at");
        snapshots.push(value);
        let path = fixture
            .home
            .join(format!("diagnostics-{}.json", language.code()));
        let written = invoke(
            &fixture,
            language,
            &["diagnostics", "--out", path.to_str().unwrap()],
            "off",
        )?;
        checked(&written);
        assert!(out(&written).contains(if language == Language::English {
            "Wrote diagnostics"
        } else {
            "已写出诊断"
        }));
        let mut disk: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        disk.as_object_mut().unwrap().remove("archive_observed_at");
        assert_eq!(disk, snapshots.last().unwrap().clone());
        for console in ["off", "error"] {
            let rejected = invoke(
                &fixture,
                language,
                &["diagnostics", "--forget-file", "fixture.jsonl"],
                console,
            )?;
            assert_eq!(rejected.status.code(), Some(1));
            assert!(
                err(&rejected).contains(if language == Language::English {
                    "No source found"
                } else {
                    "找不到 file_path"
                }),
                "{rejected:?}"
            );
            if language == Language::English {
                assert!(!err(&rejected).contains("找不到"));
            }
        }
    }
    assert_eq!(snapshots[0], snapshots[1]);
    let conn = Connection::open(&fixture.paths.db_path)?;
    for source in ["codex", "claude"] {
        conn.execute("INSERT INTO source_file(host_id,source,file_path,state,last_state_change_at) VALUES ('local',?1,'fixture.jsonl','live','2026-01-01T00:00:00Z')", [source])?;
    }
    for language in [Language::English, Language::Chinese] {
        let rejected = invoke(
            &fixture,
            language,
            &["diagnostics", "--forget-file", "fixture.jsonl"],
            "error",
        )?;
        assert_eq!(rejected.status.code(), Some(1));
        assert!(
            err(&rejected).contains(if language == Language::English {
                "multiple sources"
            } else {
                "多个源"
            }),
            "{rejected:?}"
        );
        let accepted = invoke(
            &fixture,
            language,
            &[
                "diagnostics",
                "--forget-file",
                "fixture.jsonl",
                "--source",
                "codex",
            ],
            "off",
        )?;
        checked(&accepted);
        assert!(out(&accepted).contains("deleted_by_user"));
        assert_eq!(
            conn.query_row(
                "SELECT state FROM source_file WHERE source='codex'",
                [],
                |row| row.get::<_, String>(0)
            )?,
            "deleted_by_user"
        );
        assert_eq!(
            conn.query_row(
                "SELECT state FROM source_file WHERE source='claude'",
                [],
                |row| row.get::<_, String>(0)
            )?,
            "live"
        );
    }
    Ok(())
}

#[test]
fn clean_preview_and_confirmation_preserve_the_same_owned_set_in_both_languages() -> Result<()> {
    for language in [Language::English, Language::Chinese] {
        let fixture = ReportCliFixture::new()?;
        for file in [
            "backups/llmusage.db.pre-fixture",
            "backups/config.json.bak",
            "backups/codex_notify_original.json",
            "baselines/research/data.json",
            "unknown.txt",
            "cli-settings.lock",
        ] {
            let path = fixture.paths.root_dir.join(file);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, b"fixture")?;
        }
        let migration_files = fs::read_dir(&fixture.paths.backups_dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("llmusage.db.pre-")
            })
            .collect::<Vec<_>>();
        let expected_deleted = migration_files.len() + 1; // One regular research baseline.
        let before = fs::read(&fixture.paths.db_path)?;
        let preview = invoke(&fixture, language, &["clean"], "off")?;
        checked(&preview);
        assert!(out(&preview).contains(if language == Language::English {
            "Disk unchanged"
        } else {
            "未修改磁盘"
        }));
        assert!(
            fixture
                .paths
                .root_dir
                .join("backups/llmusage.db.pre-fixture")
                .is_file()
        );
        let cleaned = invoke(&fixture, language, &["clean", "--yes"], "off")?;
        checked(&cleaned);
        assert!(
            out(&cleaned).contains(&if language == Language::English {
                format!("Deleted {expected_deleted} files")
            } else {
                format!("已删除 {expected_deleted} 个文件")
            }),
            "{cleaned:?}"
        );
        assert!(
            !fixture
                .paths
                .root_dir
                .join("backups/llmusage.db.pre-fixture")
                .exists()
        );
        assert!(!fixture.paths.root_dir.join("baselines").exists());
        assert!(migration_files.iter().all(|path| !path.exists()));
        for file in [
            "backups/config.json.bak",
            "backups/codex_notify_original.json",
            "unknown.txt",
            "cli-settings.json",
            "cli-settings.lock",
        ] {
            assert!(fixture.paths.root_dir.join(file).is_file());
        }
        assert_eq!(fs::read(&fixture.paths.db_path)?, before);
        for line in out(&cleaned).lines().filter(|line| line.starts_with('│')) {
            assert_eq!(
                unicode_width::UnicodeWidthStr::width(line),
                unicode_width::UnicodeWidthStr::width(out(&cleaned).lines().next().unwrap())
            );
        }
    }
    Ok(())
}

#[test]
fn remote_remove_requires_yes_and_preserves_local_identity() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for language in [Language::English, Language::Chinese] {
        for (label, target, en, zh) in [
            (
                "codex",
                "unused@example.invalid",
                "collides with source name",
                "与来源名称",
            ),
            (
                "local",
                "unused@example.invalid",
                "already registered",
                "已注册",
            ),
            (
                "devbox",
                "-oProxyCommand=blocked",
                "must not start",
                "不能以",
            ),
        ] {
            let mut args = vec!["remote", "add", label];
            if target.starts_with('-') {
                args.push("--");
            }
            args.push(target);
            let rejected = invoke(&fixture, language, &args, "error")?;
            assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
            assert!(
                err(&rejected).contains(language.select(en, zh)),
                "{rejected:?}"
            );
            if language == Language::Chinese {
                assert!(!err(&rejected).contains(en), "{rejected:?}");
            }
        }
        let refusal = invoke(
            &fixture,
            language,
            &["remote", "remove", "local", "--delete-usage"],
            "off",
        )?;
        assert_eq!(refusal.status.code(), Some(1));
        assert!(err(&refusal).contains("--yes"));
        let local = invoke(
            &fixture,
            language,
            &["remote", "remove", "local", "--delete-usage", "--yes"],
            "off",
        )?;
        assert_eq!(local.status.code(), Some(1));
        assert!(
            err(&local).contains(if language == Language::English {
                "cannot remove the local host"
            } else {
                "不能移除本地主机"
            }),
            "{local:?}"
        );
        let missing = invoke(
            &fixture,
            language,
            &["remote", "sync", "--host", "不存在"],
            "off",
        )?;
        assert_eq!(missing.status.code(), Some(1));
        assert!(err(&missing).contains("不存在"));
    }
    assert!(
        Store::new(&fixture.paths)?
            .hosts()
            .get_by_label("local")?
            .is_some()
    );
    Ok(())
}

fn write_fixture(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, bytes)?;
    Ok(())
}

#[test]
fn uninstall_translates_private_outcomes_without_changing_cleanup_audits_or_user_files()
-> Result<()> {
    let mut audits = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let fixture = ReportCliFixture::new()?;
        let codex = fixture.home.join("CODEX_HOME/config.toml");
        let claude = fixture.home.join("HOME/.claude/settings.json");
        let antigravity = fixture.home.join("HOME/.gemini/config/hooks.json");
        let plugin = fixture
            .home
            .join("OPENCODE_CONFIG_DIR/plugin/llmusage-tracker.js");
        write_fixture(
            &codex,
            b"model = \"user-model\"\nnotify = [\"llmusage-hook\"]\n",
        )?;
        write_fixture(
            &fixture.paths.backups_dir.join("codex_notify_original.json"),
            br#"{"notify":["echo","user-notify"]}"#,
        )?;
        write_fixture(&claude, br#"{"hooks":{"Stop":[{"hooks":[{"command":"llmusage-hook --source claude"},{"command":"user-notify"}]}]},"user":17}"#)?;
        write_fixture(&antigravity, br#"{"Stop":[{"command":"llmusage-hook --source antigravity"},{"command":"user-notify"}]}"#)?;
        write_fixture(&plugin, b"// LLMUSAGE_LOCAL_PLUGIN\n")?;
        write_fixture(&fixture.paths.hook_sh_path, b"legacy wrapper")?;
        for file in ["history.bak", "llmusage.db.pre-fixture"] {
            write_fixture(&fixture.paths.backups_dir.join(file), b"keep")?;
        }
        let output = invoke(&fixture, language, &["uninstall"], "off")?;
        checked(&output);
        assert!(
            out(&output).contains(language.select("Legacy cleanup finished", "旧版集成清理完成")),
            "{output:?}"
        );
        assert!(out(&output).contains(language.select(
            "removed legacy llmusage OpenCode plugin",
            "已移除旧版 llmusage OpenCode 插件"
        )));
        assert_eq!(
            fs::read_to_string(&codex)?,
            "model = \"user-model\"\nnotify = [\"echo\", \"user-notify\"]\n"
        );
        let claude_data: serde_json::Value = serde_json::from_slice(&fs::read(&claude)?)?;
        assert_eq!(
            claude_data,
            serde_json::json!({"hooks":{"Stop":[{"hooks":[{"command":"user-notify"}]}]},"user":17})
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&antigravity)?)?,
            serde_json::json!({"Stop":[{"command":"user-notify"}]})
        );
        assert!(!plugin.exists() && !fixture.paths.hook_sh_path.exists());
        assert!(
            !fixture
                .paths
                .backups_dir
                .join("codex_notify_original.json")
                .exists()
        );
        for file in ["history.bak", "llmusage.db.pre-fixture"] {
            assert_eq!(fs::read(fixture.paths.backups_dir.join(file))?, b"keep");
        }
        let conn = Connection::open(&fixture.paths.db_path)?;
        let read_audit = || -> Result<Vec<(String, String, String)>> {
            Ok(conn
                .prepare(
                    "SELECT source, status, details_json FROM integration_install ORDER BY source",
                )?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        };
        let first = read_audit()?;
        audits.push(
            first
                .iter()
                .map(|(source, status, raw)| -> Result<_> {
                    let mut value: serde_json::Value = serde_json::from_str(raw)?;
                    // Only the fixture's absolute wrapper path differs across isolated homes.
                    if let Some(detail) = value.get_mut("detail") {
                        *detail = serde_json::json!(
                            detail
                                .as_str()
                                .unwrap()
                                .replace(&fixture.paths.root_dir.display().to_string(), "<root>")
                        );
                    }
                    Ok((source.clone(), status.clone(), value))
                })
                .collect::<Result<Vec<_>>>()?,
        );
        let before = fs::read(&claude)?;
        let noop = invoke(&fixture, language, &["uninstall"], "off")?;
        checked(&noop);
        assert!(
            out(&noop).contains(
                language.select("no legacy Claude hooks found", "未发现旧版 Claude hooks")
            )
        );
        assert_eq!(read_audit()?, first);
        assert_eq!(fs::read(&claude)?, before);
        write_fixture(&plugin, b"// user-owned plugin\n")?;
        write_fixture(
            &plugin
                .parent()
                .unwrap()
                .join(".llmusage-tracker.js.llmusage-tmp.1.2.3"),
            b"residue",
        )?;
        let residue = invoke(&fixture, language, &["uninstall"], "off")?;
        checked(&residue);
        assert!(out(&residue).contains(language.select(
            "recovered residue; preserved unowned",
            "已恢复残留；已保留不属于"
        )));
        assert_eq!(fs::read(&plugin)?, b"// user-owned plugin\n");
        assert!(
            !plugin
                .parent()
                .unwrap()
                .join(".llmusage-tracker.js.llmusage-tmp.1.2.3")
                .exists()
        );
        let final_noop = invoke(&fixture, language, &["uninstall"], "off")?;
        checked(&final_noop);
        assert!(out(&final_noop).contains(language.select(
            "preserved unowned OpenCode plugin",
            "已保留不属于 llmusage 的 OpenCode 插件"
        )));
        drop(conn);
        let purged = invoke(&fixture, language, &["uninstall", "--purge"], "off")?;
        checked(&purged);
        assert!(out(&purged).contains(language.select("Purged", "已清除")));
        assert!(!fixture.paths.root_dir.exists());
        assert_eq!(fs::read(&plugin)?, b"// user-owned plugin\n");
    }
    assert_eq!(audits[0], audits[1]);
    Ok(())
}

#[test]
fn uninstall_failure_is_bilingual_but_audit_and_run_error_remain_canonical() -> Result<()> {
    for language in [Language::English, Language::Chinese] {
        let fixture = ReportCliFixture::new()?;
        let claude = fixture.home.join("HOME/.claude/settings.json");
        let plugin = fixture
            .home
            .join("OPENCODE_CONFIG_DIR/plugin/llmusage-tracker.js");
        write_fixture(&claude, b"[]")?;
        write_fixture(&plugin, b"// LLMUSAGE_LOCAL_PLUGIN\n")?;
        write_fixture(&fixture.paths.hook_sh_path, b"owned wrapper")?;
        let rejected = invoke(&fixture, language, &["uninstall"], "error")?;
        assert_eq!(rejected.status.code(), Some(1));
        assert!(
            err(&rejected)
                .contains(language.select("legacy hook cleanup failed", "旧版 hook 清理失败")),
            "{rejected:?}"
        );
        assert!(
            err(&rejected)
                .contains(language.select("top level must be an object", "顶层 必须为对象")),
            "{rejected:?}"
        );
        if language == Language::Chinese {
            assert!(!err(&rejected).contains("top level must"), "{rejected:?}");
        }
        assert_eq!(fs::read(&claude)?, b"[]");
        assert!(!plugin.exists() && !fixture.paths.hook_sh_path.exists());
        let conn = Connection::open(&fixture.paths.db_path)?;
        let canonical =
            "legacy hook cleanup failed: claude: Claude settings.json top level must be an object";
        assert_eq!(
            conn.query_row(
                "SELECT error FROM run_log WHERE command='uninstall' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0)
            )?,
            canonical
        );
        let audit: String = conn.query_row(
            "SELECT details_json FROM integration_install WHERE source='claude'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&audit)?["detail"],
            "Claude settings.json top level must be an object"
        );
    }
    Ok(())
}

#[test]
fn logs_replay_uses_ids_and_preserves_unknown_detail_and_disk_bytes() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let entries = [
        serde_json::json!({"timestamp":"2026-01-01T00:00:00Z","level":"INFO","fields":{"event_code":"commands.init.02","message":"完成初始化"}}),
        serde_json::json!({"timestamp":"2026-01-01T00:00:01Z","level":"ERROR","fields":{"event_code":"commands.mod.04","message":"run failed","error":"CANONICAL_OLD","diagnostic_code":"cli.update_eof","diagnostic_facts":"{\"surface\":\"operations\",\"facts\":{\"kind\":\"update_eof\"}}"}}),
        serde_json::json!({"timestamp":"2026-01-01T00:00:02Z","level":"WARN","fields":{"message":"OLD_UNKNOWN_DETAIL","future":17}}),
    ];
    let disk = entries
        .iter()
        .map(|entry| format!("{entry}\n"))
        .collect::<String>();
    write_fixture(&fixture.paths.log_file_path, disk.as_bytes())?;
    let mut jsons = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let output = invoke(&fixture, language, &["logs"], "off")?;
        checked(&output);
        assert!(
            out(&output).contains(
                language.select("local runtime bootstrap completed", "本地运行时初始化完成")
            ),
            "{output:?}"
        );
        assert!(
            out(&output)
                .contains(language.select("confirmation input reached EOF", "确认输入已到 EOF")),
            "{output:?}"
        );
        assert!(!out(&output).contains("CANONICAL_OLD"));
        assert!(out(&output).contains(language.select(
            "Original log: OLD_UNKNOWN_DETAIL",
            "原始日志: OLD_UNKNOWN_DETAIL"
        )));
        let json = invoke(&fixture, language, &["logs", "--json"], "off")?;
        checked(&json);
        jsons.push(serde_json::from_slice::<serde_json::Value>(&json.stdout)?);
        assert_eq!(fs::read(&fixture.paths.log_file_path)?, disk.as_bytes());
    }
    assert_eq!(jsons[0], jsons[1]);
    assert_eq!(jsons[0]["entries"][1]["error"], "CANONICAL_OLD");
    Ok(())
}

#[test]
fn source_and_doctor_human_diagnostics_follow_language_while_machine_checks_stay_canonical()
-> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let conn = Connection::open(&fixture.paths.db_path)?;
    let samples = (0..10).map(|offset| serde_json::json!({"source":"codex", "path_hash":"PRIVATE_HASH", "offset":offset, "kind":"malformed", "reason":"EXTERNAL_REASON"})).collect::<Vec<_>>();
    let issues = serde_json::json!({"malformed_lines":2, "oversized_lines":1, "skipped_lines":10, "accounting_anomaly_lines":1, "samples":samples, "source_issues":[{"code":"tracked_member_missing","count":3,"observed_at":"2026-01-01T00:00:00Z","scope":"product_group"}]});
    conn.execute("INSERT INTO source_sync_status(host_id,source,files_processed,changed_files,bytes_scanned,events_seen,events_replayed,events_inserted,stored_events,parse_ms,write_ms,lock_wait_ms,updated_at,parse_issues_json) VALUES ('local','codex',1,1,0,0,0,0,0,0,0,0,'2026-01-01T00:00:00Z',?1)", [issues.to_string()])?;
    let mut checks = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let source = invoke(&fixture, language, &["source-status"], "off")?;
        checked(&source);
        let output = out(&source);
        for (en, zh) in [
            (
                "encrypted .pb artifacts and unverified legacy roots are unsupported",
                "不支持加密 .pb 文件及未经验证的旧版根路径",
            ),
            (
                "no candidate roots detected; checked CODEX_HOME",
                "未检测到候选根路径；已检查 CODEX_HOME",
            ),
            (
                "Parse faults: malformed=2 oversized=1",
                "解析故障：格式错误=2 超长=1",
            ),
            (
                "Information: skipped=10 accounting=1",
                "信息提示：跳过=10 计数提示=1",
            ),
            (
                "Samples shown: 8/14; 6 not shown",
                "展示样本：8/14；另有 6 条未展示",
            ),
            (
                "Fault [tracked_member_missing]",
                "故障 [tracked_member_missing]",
            ),
        ] {
            assert!(output.contains(language.select(en, zh)), "{source:?}");
        }
        assert!(!output.contains("PRIVATE_HASH"));
        assert_eq!(output.matches("EXTERNAL_REASON").count(), 8);
        let doctor = invoke(&fixture, language, &["doctor"], "off")?;
        checked(&doctor);
        assert!(
            out(&doctor).contains(
                language.select("codex malformed=2 oversized=1", "codex 格式错误=2 超长=1")
            ),
            "{doctor:?}"
        );
        assert!(out(&doctor).contains("tracked_member_missing"));
        assert!(!out(&doctor).contains("PRIVATE_HASH"));
        let json = invoke(&fixture, language, &["doctor", "--json"], "off")?;
        checked(&json);
        checks.push(serde_json::from_slice::<serde_json::Value>(&json.stdout)?);
    }
    assert_eq!(checks[0], checks[1]);
    assert!(
        checks[0]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["id"] == "parse.issues"
                && check["detail"] == "codex malformed=2 oversized=1")
    );
    assert_eq!(
        conn.query_row(
            "SELECT parse_issues_json FROM source_sync_status",
            [],
            |row| row.get::<_, String>(0)
        )?,
        issues.to_string()
    );
    Ok(())
}

#[test]
fn export_and_tracer_shells_follow_language_without_translating_web_assets() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        input_tokens: 11,
        output_tokens: 1,
        total_tokens: 12,
        ..Default::default()
    })?;
    let mut exports = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let destination = fixture.home.join(format!("export-{}", language.code()));
        let output = invoke(
            &fixture,
            language,
            &["export", "html", "--out", destination.to_str().unwrap()],
            "off",
        )?;
        checked(&output);
        assert!(
            out(&output).contains(language.select("Exported HTML bundle", "已导出 HTML 文件包")),
            "{output:?}"
        );
        assert!(out(&output).contains(&destination.display().to_string()));
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&destination) {
            let entry = entry?;
            if entry.file_type().is_file() && entry.file_name() != "snapshot.json" {
                files.push((
                    entry.path().strip_prefix(&destination)?.to_path_buf(),
                    fs::read(entry.path())?,
                ));
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let snapshot: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("snapshot.json"))?)?;
        let fixed = ["models", "sources", "hosts", "projects", "costs"]
            .map(|key| (key, snapshot[key].clone()));
        exports.push((files, fixed));
    }
    assert_eq!(exports[0], exports[1]);
    for language in [Language::English, Language::Chinese] {
        let rollout = fixture.home.join("CODEX_HOME/rollout");
        if rollout.exists() {
            fs::remove_dir(&rollout)?;
        }
        let missing = invoke(&fixture, language, &["codex-tracer", "--no-open"], "off")?;
        assert_eq!(missing.status.code(), Some(1));
        assert!(
            err(&missing).contains(language.select(
                "Codex rollout directory not found",
                "未找到 Codex rollout 目录"
            )),
            "{missing:?}"
        );
        fs::create_dir_all(&rollout)?;
        let empty = invoke(&fixture, language, &["codex-tracer", "--no-open"], "off")?;
        assert_eq!(empty.status.code(), Some(1));
        assert!(
            err(&empty).contains(language.select("No events found", "未找到事件")),
            "{empty:?}"
        );
    }
    Ok(())
}
