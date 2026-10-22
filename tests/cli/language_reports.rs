use super::*;
use std::{io::Write, process::Stdio};

fn language(fixture: &ReportCliFixture, code: &str) -> Result<()> {
    fs::write(
        fixture.paths.root_dir.join("cli-settings.json"),
        serde_json::to_vec(&serde_json::json!({"language": code}))?,
    )?;
    Ok(())
}

fn text_output(fixture: &ReportCliFixture, args: &[&str]) -> Result<String> {
    let output = fixture.output_with_env(args, &[("COLUMNS", "240"), ("NO_COLOR", "1")])?;
    assert!(output.status.success(), "{args:?}: {output:?}");
    let text = String::from_utf8(output.stdout)?;
    assert!(!text.contains('\x1b'), "{text}");
    Ok(text)
}

fn seeded() -> Result<ReportCliFixture> {
    let fixture = ReportCliFixture::new()?;
    for source in [
        "claude",
        "codex",
        "opencode",
        "antigravity",
        "antigravity_ide",
    ] {
        fixture.seed_event(SeedEvent {
            event_key: &format!("{source}:language:1"),
            source,
            model: "模型-外部-原值",
            event_at: "2026-10-19T01:00:00Z",
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 10,
            cache_creation_tokens: 30,
            total_tokens: 777,
            project_hash: "language-project",
            project_label: "中文项目",
            project_ref: Some("用户/中文项目"),
            session_id: Some("session-language"),
            cost_with_cache_usd: 1.25,
            ..SeedEvent::default()
        })?;
    }
    Ok(fixture)
}

#[test]
fn schema_focused_leaves_have_paired_success_empty_rejection_and_stable_json() -> Result<()> {
    let fixture = seeded()?;
    let schema = llmusage::commands::entrypoint::schema();
    let leaves = schema
        .get_subcommands()
        .filter(|host| {
            host.get_name() != "help"
                && ["daily", "weekly", "monthly", "session"]
                    .iter()
                    .all(|period| host.find_subcommand(period).is_some())
        })
        .flat_map(|host| {
            host.get_subcommands()
                .filter(|period| period.get_name() != "help")
                .map(move |period| (host.get_name().to_string(), period.get_name().to_string()))
        })
        .collect::<Vec<_>>();
    assert_eq!(leaves.len(), 20, "focused leaves: {leaves:?}");
    for (host, period) in leaves {
        let args = [
            host.as_str(),
            period.as_str(),
            "--since",
            "2026-10-19",
            "--until",
            "2026-10-19",
            "--timezone",
            "UTC",
        ];
        let mut json_args = args.to_vec();
        json_args.extend(["--json", "--breakdown", "--by-agent"]);
        language(&fixture, "en")?;
        let en = text_output(&fixture, &args)?;
        assert!(en.contains("Usage Report"), "{host} {period}: {en}");
        assert!(en.contains("模型-外部-原值"), "{en}");
        let en_table = en.split("By host").next().unwrap();
        assert!(
            en_table.contains("160") && !en_table.contains("777"),
            "{en}"
        );
        assert!(!en.contains("Detected:") && !en.contains("│ Agent"), "{en}");
        let en_json = fixture.json(&json_args)?;
        assert_eq!(en_json["totals"]["totalTokens"], 777);
        assert_json_has_no_agent_keys(&en_json);
        language(&fixture, "zh")?;
        let zh = text_output(&fixture, &args)?;
        assert!(
            zh.contains("用量报表") && zh.contains("输入") && zh.contains("合计"),
            "{host} {period}: {zh}"
        );
        assert!(zh.contains("模型-外部-原值"), "{zh}");
        let zh_table = zh.split("按主机统计").next().unwrap();
        assert!(
            zh_table.contains("160") && !zh_table.contains("777"),
            "{zh}"
        );
        assert_eq!(fixture.json(&json_args)?, en_json, "{host} {period}");
        for (code, empty, conflict) in [
            ("en", "No usage data", "conflicts"),
            ("zh", "没有符合", "冲突"),
        ] {
            language(&fixture, code)?;
            let empty_args = [
                host.as_str(),
                period.as_str(),
                "--since",
                "2000-01-01",
                "--until",
                "2000-01-01",
            ];
            assert!(text_output(&fixture, &empty_args)?.contains(empty));
            let source = if host == "claude" { "codex" } else { "claude" };
            let rejected = fixture.output(&[&host, &period, "--source", source])?;
            assert_eq!(rejected.status.code(), Some(1));
            let stderr = String::from_utf8(rejected.stderr)?;
            assert!(
                stderr.contains(conflict) && stderr.contains(source),
                "{stderr}"
            );
        }
    }
    Ok(())
}

#[test]
fn report_variants_localize_text_and_preserve_json_locale_cost_and_sections() -> Result<()> {
    let fixture = seeded()?;
    fixture.upsert_host("language-host", "用户主机")?;
    let variants = [
        vec![],
        vec!["daily", "--all"],
        vec!["daily", "--all", "--instances"],
        vec!["weekly"],
        vec!["monthly", "--compact"],
        vec!["session", "--id", "session-language"],
        vec!["blocks", "--token-limit", "max"],
        vec![
            "daily",
            "--all",
            "--sections",
            "daily,weekly,monthly,session",
            "--breakdown",
            "--by-agent",
        ],
        vec![
            "codex",
            "daily",
            "--all",
            "--sections",
            "weekly,monthly,session",
            "--breakdown",
        ],
        vec!["monthly", "--no-cost"],
    ];
    for args in variants {
        language(&fixture, "en")?;
        let en = text_output(&fixture, &args)?;
        language(&fixture, "zh")?;
        let zh = text_output(&fixture, &args)?;
        assert_ne!(en, zh, "{args:?}");
        assert!(
            !zh.contains("No usage data")
                && !zh.contains("Usage Report")
                && !zh.contains("Daily usage by project"),
            "{args:?}: {zh}"
        );
        let mut json_args = args.clone();
        json_args.extend(["--json", "--timezone", "UTC"]);
        let zh_json = fixture.json(&json_args)?;
        language(&fixture, "en")?;
        assert_eq!(fixture.json(&json_args)?, zh_json, "{args:?}");
        if args.contains(&"--no-cost") {
            assert_json_has_no_cost_keys(&zh_json);
        }
        for locale in ["en-US", "zh-CN", "ja-JP"] {
            let mut localized_args = json_args.clone();
            localized_args.extend(["--locale", locale]);
            assert_eq!(
                fixture.json(&localized_args)?,
                zh_json,
                "{args:?} --locale {locale}"
            );
        }
    }
    for (code, expected) in [("en", "unknown host label"), ("zh", "未知主机标签")] {
        language(&fixture, code)?;
        for command in ["daily", "weekly", "monthly", "session", "blocks"] {
            let rejected = fixture.output(&[command, "--host", "missing-user-host"])?;
            assert_eq!(rejected.status.code(), Some(1));
            let stderr = String::from_utf8(rejected.stderr)?;
            assert!(
                stderr.contains(expected)
                    && stderr.contains("用户主机")
                    && stderr.contains("missing-user-host"),
                "{stderr}"
            );
        }
        for args in [
            vec!["daily", "--all", "--since", "2026-01-01"],
            vec!["daily", "--instances", "--sections", "weekly"],
            vec!["session", "--id", "user-id", "--sections", "daily"],
            vec!["codex", "daily", "--instances"],
            vec!["codex", "session", "--id", "user-id", "--sections", "daily"],
        ] {
            let output = fixture.output(&args)?;
            assert_eq!(output.status.code(), Some(1), "{args:?}");
            let stderr = String::from_utf8(output.stderr)?;
            assert!(
                stderr.contains(if code == "zh" { "不能" } else { "cannot" })
                    || stderr.contains(if code == "zh" {
                        "不支持"
                    } else {
                        "not supported"
                    }),
                "{stderr}"
            );
        }
    }
    Ok(())
}

#[test]
fn rejected_report_logs_project_paired_facts_and_keep_canonical_file_and_run_data() -> Result<()> {
    let canonical = "`codex` source command conflicts with `--source claude`";
    for (code, expected) in [("en", "conflicts"), ("zh", "冲突")] {
        let fixture = ReportCliFixture::new()?;
        language(&fixture, code)?;
        let output = fixture.output_with_env(
            &["codex", "daily", "--source", "claude"],
            &[
                ("RUST_LOG", "error"),
                ("LLMUSAGE_LOG", "error"),
                ("NO_COLOR", "1"),
            ],
        )?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains(expected), "{stderr}");
        if code == "zh" {
            assert!(!stderr.contains(canonical), "{stderr}");
        }
        let entries = read_recent_log_entries(&fixture.paths, 100, Some("error"), Some("codex"))?;
        let entry = entries
            .iter()
            .find(|entry| entry.message.as_deref() == Some("run failed"))
            .expect("original run failure event");
        assert_eq!(
            entry
                .fields
                .get("error")
                .and_then(serde_json::Value::as_str),
            Some(canonical),
            "{entry:?}"
        );
        assert_eq!(
            entry
                .fields
                .get("diagnostic_code")
                .and_then(serde_json::Value::as_str),
            Some("reports.source_conflict"),
            "{entry:?}"
        );
        let runs = Store::new(&fixture.paths)?.run_log().recent_runs(5)?;
        let run = runs
            .iter()
            .find(|run| run.command == "codex")
            .expect("tracked rejection");
        assert_eq!(run.error.as_deref(), Some(canonical));
        let logs = text_output(&fixture, &["logs", "--limit", "20"])?;
        let entries = logs
            .split(if code == "zh" {
                "运行日志："
            } else {
                "Run log:"
            })
            .next()
            .unwrap();
        assert!(entries.contains(expected), "{logs}");
        if code == "zh" {
            assert!(!entries.contains(canonical), "{logs}");
        }
        assert!(
            logs.contains(canonical)
                && logs.contains(if code == "zh" {
                    "原始运行详情"
                } else {
                    "Original run detail"
                }),
            "{logs}"
        );
    }
    Ok(())
}

fn statusline_hook(fixture: &ReportCliFixture, args: &[&str], input: &str) -> Result<String> {
    let mut command = test_process::llmusage_command();
    command
        .arg("--home")
        .arg(&fixture.paths.root_dir)
        .arg("statusline")
        .args(args)
        .env("LLMUSAGE_PRICING_REFRESH", "off")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    child.stdin.take().unwrap().write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    Ok(stdout.trim_end().to_string())
}

#[test]
fn report_uninitialized_failures_keep_exit_code_and_pair_own_context() -> Result<()> {
    let temp = TempDir::new()?;
    let root = temp.path().join("profile");
    fs::create_dir_all(&root)?;
    let schema = llmusage::commands::entrypoint::schema();
    let mut paths = [
        "daily",
        "weekly",
        "monthly",
        "session",
        "blocks",
        "statusline",
    ]
    .into_iter()
    .map(|command| vec![command.to_string()])
    .collect::<Vec<_>>();
    paths.extend(
        schema
            .get_subcommands()
            .filter(|host| {
                host.get_name() != "help"
                    && ["daily", "weekly", "monthly", "session"]
                        .iter()
                        .all(|period| host.find_subcommand(period).is_some())
            })
            .flat_map(|host| {
                host.get_subcommands()
                    .filter(|period| period.get_name() != "help")
                    .map(move |period| {
                        vec![host.get_name().to_string(), period.get_name().to_string()]
                    })
            }),
    );
    for (code, expected) in [("en", "Not initialized"), ("zh", "尚未初始化")] {
        fs::write(
            root.join("cli-settings.json"),
            format!(r#"{{"language":"{code}"}}"#),
        )?;
        for path in &paths {
            let output = test_process::llmusage_command()
                .arg("--home")
                .arg(&root)
                .args(path)
                .env("LLMUSAGE_PRICING_REFRESH", "off")
                .env("NO_COLOR", "1")
                .stdin(Stdio::null())
                .output()?;
            assert_eq!(output.status.code(), Some(1), "{path:?}: {output:?}");
            let stderr = String::from_utf8(output.stderr)?;
            assert!(
                stderr.contains(expected) && stderr.contains("llmusage init"),
                "{path:?}: {stderr}"
            );
        }
    }
    Ok(())
}

#[test]
fn statusline_keeps_cost_policy_single_line_and_language_cache_isolation() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let cache = fixture.paths.root_dir.join("statusline-cache");
    fs::create_dir_all(&cache)?;
    fs::write(cache.join("latest.txt"), "OLD LANGUAGE CACHE")?;
    for (code, today, db, hook, inactive) in [
        ("en", "today", "db", "hook", "no active block"),
        ("zh", "今日", "数据库", "钩子", "无活跃用量块"),
    ] {
        language(&fixture, code)?;
        for (policy, expected) in [
            ("llmusage", "$0.00"),
            ("hook", "$9.25"),
            ("both", "$9.25"),
            ("auto", "$9.25"),
        ] {
            let line = statusline_hook(
                &fixture,
                &["--cost-source", policy],
                r#"{"model":"用户模型","costUSD":9.25}"#,
            )?;
            assert!(
                line.contains("用户模型")
                    && line.contains(today)
                    && line.contains(inactive)
                    && line.contains(expected),
                "{line}"
            );
            if policy == "both" || policy == "auto" {
                assert!(line.contains(db) && line.contains(hook), "{line}");
            }
            assert_eq!(
                fs::read_to_string(cache.join(format!("latest.{code}.txt")))?,
                line
            );
        }
        let absent = statusline_hook(
            &fixture,
            &["--no-cache", "--cost-source", "hook"],
            "invalid JSON",
        )?;
        assert!(absent.contains(hook) && absent.contains("n/a"), "{absent}");
    }
    assert_eq!(
        fs::read_to_string(cache.join("latest.txt"))?,
        "OLD LANGUAGE CACHE"
    );
    let en_cache = fs::read_to_string(cache.join("latest.en.txt"))?;
    let zh_cache = fs::read_to_string(cache.join("latest.zh.txt"))?;
    Connection::open(&fixture.paths.db_path)?.execute_batch("DROP TABLE usage_bucket_30m;")?;
    language(&fixture, "en")?;
    assert_eq!(statusline_hook(&fixture, &[], "")?, en_cache);
    language(&fixture, "zh")?;
    assert_eq!(statusline_hook(&fixture, &[], "")?, zh_cache);
    fs::remove_file(cache.join("latest.zh.txt"))?;
    let line = statusline_hook(&fixture, &[], "")?;
    assert!(
        line.contains("不可用") && !line.contains("OLD LANGUAGE CACHE") && !line.contains("today"),
        "{line}"
    );
    Ok(())
}

#[test]
fn statusline_active_labels_and_io_failures_are_paired() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    fixture.seed_event(SeedEvent {
        event_key: "codex:statusline-language:1",
        source: "codex",
        model: "gpt-5",
        event_at: &now,
        input_tokens: 777,
        total_tokens: 777,
        project_hash: "statusline-language",
        project_label: "用户项目",
        session_id: Some("active-language-session"),
        ..SeedEvent::default()
    })?;
    fs::write(
        fixture.paths.root_dir.join("statusline-cache"),
        "cache directory is blocked",
    )?;
    for (code, block, projected, failed_cache, failed_read) in [
        (
            "en",
            "block 777",
            "projected",
            "failed to create statusline cache directory",
            "failed to read statusline hook input",
        ),
        (
            "zh",
            "用量块 777",
            "预测",
            "无法创建状态栏缓存目录",
            "无法读取状态栏钩子输入",
        ),
    ] {
        language(&fixture, code)?;
        let line = statusline_hook(
            &fixture,
            &["--no-cache"],
            r#"{"workspace":{"model":"用户模型\n第二行"}}"#,
        )?;
        assert!(
            line.contains(block) && line.contains(projected) && line.contains("用户模型 第二行"),
            "{line}"
        );
        let failed = fixture.output(&["statusline"])?;
        assert_eq!(failed.status.code(), Some(1));
        assert!(String::from_utf8(failed.stderr)?.contains(failed_cache));
        let mut command = test_process::llmusage_command();
        let mut child = command
            .arg("--home")
            .arg(&fixture.paths.root_dir)
            .arg("statusline")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().unwrap().write_all(&[0xff])?;
        let failed = child.wait_with_output()?;
        assert_eq!(failed.status.code(), Some(1));
        assert!(String::from_utf8(failed.stderr)?.contains(failed_read));
    }
    Ok(())
}
