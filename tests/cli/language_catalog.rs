use super::*;
use llmusage::{common::i18n::Language, runtime::preferences};
use serde_json::{Value, json};

fn overlay() -> Value {
    json!({"schema_version":2,"kind":"overlay","version":"language-test","models":[{
        "id":"language-model","sources":["codex"],"matches":[{"value":"language-model","mode":"exact"}],
        "rates":{"default":{"input_per_mtok":1.0,"cached_per_mtok":0.5,"output_per_mtok":2.0}}
    }]})
}

fn invoke(
    fixture: &ReportCliFixture,
    language: Language,
    args: &[&str],
) -> Result<std::process::Output> {
    preferences::write(&fixture.paths.root_dir, language)?;
    let args = ["--home", fixture.paths.root_dir.to_str().unwrap()]
        .into_iter()
        .chain(args.iter().copied())
        .collect::<Vec<_>>();
    fixture.output_with_env(
        &args,
        &[
            ("LLMUSAGE_PRICING_REFRESH", "off"),
            ("RUST_LOG", "error"),
            ("LLMUSAGE_LOG", "error"),
            ("NO_COLOR", "1"),
        ],
    )
}

#[test]
fn catalog_validation_families_translate_without_changing_canonical_errors_or_data() -> Result<()> {
    let mut cases = vec![(json!({}), "LiteLLM"), (json!([]), "JSON 对象")];
    for (pointer, value, expected) in [
        ("/kind", json!("base"), "kind"),
        ("/schema_version", json!(3), "schema_version"),
        ("/version", json!(""), "版本不能为空"),
        ("/models/0/id", json!("INVALID"), "ASCII 小写字母"),
        ("/models/0/sources", json!([]), "至少一个来源"),
        (
            "/models/0/sources",
            json!(["codex", "codex"]),
            "来源 `codex` 重复",
        ),
        ("/models/0/matches", json!([]), "至少一个匹配规则"),
        ("/models/0/matches/0/value", json!(""), "空匹配规则"),
        (
            "/models/0/matches",
            json!([{"value":"same","mode":"exact"},{"value":"same","mode":"exact"}]),
            "匹配规则 `same` 重复",
        ),
        (
            "/models/0/rates/default/input_per_mtok",
            json!(-1),
            "input_per_mtok 无效",
        ),
    ] {
        let mut value_doc = overlay();
        *value_doc.pointer_mut(pointer).unwrap() = value;
        cases.push((value_doc, expected));
    }
    let mut doc = overlay();
    doc["models"][0]["context_window"] = json!(0);
    cases.push((doc, "context_window 必须为正数"));
    let mut doc = overlay();
    doc["models"][0]["rates"]["tiers"] = json!([{"name":"","prompt_tokens_above":1,"input_per_mtok":1,"cached_per_mtok":0,"output_per_mtok":2}]);
    cases.push((doc, "分档名称为空或重复"));
    let mut doc = overlay();
    doc["models"][0]["rates"]["tiers"] = json!([{"name":"tier","prompt_tokens_above":0,"input_per_mtok":1,"cached_per_mtok":0,"output_per_mtok":2}]);
    cases.push((doc, "严格递增的正数"));
    let mut doc = overlay();
    doc["remove_models"] = json!(["missing", "missing"]);
    cases.push((doc, "重复移除模型"));
    let mut doc = overlay();
    doc["remove_models"] = json!(["missing"]);
    cases.push((doc, "不存在"));
    let mut doc = overlay();
    let duplicate = doc["models"][0].clone();
    doc["models"].as_array_mut().unwrap().push(duplicate);
    cases.push((doc, "模型 id `language-model` 重复"));
    let mut doc = overlay();
    let mut duplicate = doc["models"][0].clone();
    duplicate["id"] = json!("different-model");
    doc["models"].as_array_mut().unwrap().push(duplicate);
    cases.push((doc, "匹配规则重复"));
    for (index, (document, expected)) in cases.into_iter().enumerate() {
        let fixture = ReportCliFixture::new()?;
        fixture.seed_event(SeedEvent {
            total_tokens: 12,
            ..Default::default()
        })?;
        let path = fixture.home.join(format!("invalid-{index}.json"));
        fs::write(&path, serde_json::to_vec(&document)?)?;
        let store = Store::new(&fixture.paths)?;
        let before = serde_json::to_value(store.pricing_catalog_status()?)?;
        let canonical = store.apply_pricing_overlay(&path).unwrap_err().to_string();
        for language in [Language::English, Language::Chinese] {
            let output = invoke(
                &fixture,
                language,
                &["catalog", "apply", path.to_str().unwrap()],
            )?;
            assert_eq!(output.status.code(), Some(1), "{index}: {output:?}");
            let stderr = String::from_utf8(output.stderr)?;
            if language == Language::Chinese {
                assert!(stderr.contains(expected), "{index}: {stderr}");
                assert!(!stderr.contains("invalid config:"), "{stderr}");
            } else {
                assert!(
                    stderr.contains(
                        canonical
                            .strip_prefix("invalid config: ")
                            .unwrap_or(&canonical)
                    ),
                    "{index}: {stderr}"
                );
            }
            let runs = store.run_log().recent_runs(1)?;
            assert_eq!(
                runs[0].error.as_deref(),
                Some(canonical.as_str()),
                "{index}"
            );
            let entries =
                read_recent_log_entries(&fixture.paths, 10, Some("error"), Some("catalog apply"))?;
            let entry = entries.last().unwrap();
            assert_eq!(entry.fields["error"], canonical);
            assert!(
                entry.fields["diagnostic_code"]
                    .as_str()
                    .unwrap()
                    .starts_with("pricing.")
            );
            assert_eq!(
                serde_json::to_value(store.pricing_catalog_status()?)?,
                before
            );
            assert_eq!(
                Connection::open(&fixture.paths.db_path)?.query_row(
                    "SELECT SUM(total_tokens) FROM usage_event",
                    [],
                    |row| row.get::<_, i64>(0)
                )?,
                12
            );
        }
    }
    Ok(())
}

#[test]
fn catalog_local_path_rejections_and_doctor_snapshot_reasons_are_paired() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let bad = fixture.home.join("bad-snapshot.json");
    fs::write(&bad, b"{}")?;
    for language in [Language::English, Language::Chinese] {
        for (args, expected) in [
            (
                vec!["catalog", "apply", "https://example.invalid/pricing.json"],
                language.select("URLs are not supported", "不支持 URL"),
            ),
            (
                vec!["catalog", "apply", "missing-language-catalog.json"],
                language.select("path does not exist", "路径不存在"),
            ),
            (
                vec!["doctor", "--refresh-pricing", bad.to_str().unwrap()],
                language.select("LiteLLM rows", "LiteLLM 记录"),
            ),
        ] {
            let output = invoke(&fixture, language, &args)?;
            assert_eq!(output.status.code(), Some(1));
            let stderr = String::from_utf8(output.stderr)?;
            assert!(stderr.contains(expected), "{stderr}");
            if language == Language::Chinese {
                assert!(
                    !stderr.contains("must be a local file")
                        && !stderr.contains("did not contain any"),
                    "{stderr}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn paired_catalog_apply_reset_keeps_identical_status_and_token_values() -> Result<()> {
    let mut results = Vec::new();
    for language in [Language::English, Language::Chinese] {
        let fixture = ReportCliFixture::new()?;
        fixture.seed_event(SeedEvent {
            model: "language-model",
            input_tokens: 10,
            total_tokens: 12,
            ..Default::default()
        })?;
        let path = fixture.home.join("overlay.json");
        fs::write(&path, serde_json::to_vec(&overlay())?)?;
        let applied = invoke(
            &fixture,
            language,
            &["catalog", "apply", path.to_str().unwrap()],
        )?;
        assert!(applied.status.success(), "{applied:?}");
        let apply_status = fixture.json(&["catalog", "status", "--json"])?;
        let reset = invoke(&fixture, language, &["catalog", "reset"])?;
        assert!(reset.status.success(), "{reset:?}");
        let reset_status = fixture.json(&["catalog", "status", "--json"])?;
        let conn = Connection::open(&fixture.paths.db_path)?;
        let tokens: i64 =
            conn.query_row("SELECT SUM(total_tokens) FROM usage_event", [], |row| {
                row.get(0)
            })?;
        results.push((apply_status, reset_status, tokens));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].2, 12);
    Ok(())
}

#[test]
fn corrupt_recompute_marker_keeps_public_bootstrap_error_and_pairs_cli_reason() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let conn = Connection::open(&fixture.paths.db_path)?;
    conn.execute(
        "INSERT OR REPLACE INTO meta(key,value) VALUES ('pricing_recompute_in_progress','{')",
        [],
    )?;
    let canonical = Store::new(&fixture.paths)?
        .bootstrap()
        .unwrap_err()
        .to_string();
    assert!(canonical.contains("pricing recompute in-progress marker is invalid: EOF"));
    let path = fixture.home.join("valid-overlay.json");
    fs::write(&path, serde_json::to_vec(&overlay())?)?;
    for language in [Language::English, Language::Chinese] {
        for args in [
            vec!["catalog", "reset"],
            vec!["catalog", "apply", path.to_str().unwrap()],
            vec!["doctor", "--refresh-pricing", path.to_str().unwrap()],
            vec!["init"],
            vec!["uninstall"],
            vec!["sync"],
            vec!["sync", "--json-events"],
        ] {
            let output = invoke(&fixture, language, &args)?;
            assert_eq!(output.status.code(), Some(1));
            if args.contains(&"--json-events") {
                for line in output
                    .stdout
                    .split(|byte| *byte == b'\n')
                    .filter(|line| !line.is_empty())
                {
                    let event: Value = serde_json::from_slice(line)?;
                    assert_ne!(event["event"], "source_started", "{event}");
                }
            }
            let stderr = String::from_utf8(output.stderr)?;
            assert!(
                stderr.contains(language.select(
                    "pricing recompute in-progress marker is invalid",
                    "价格重算进行中标记无效"
                )),
                "{stderr}"
            );
            assert!(stderr.contains("EOF while parsing an object"), "{stderr}");
            if language == Language::Chinese {
                assert!(!stderr.contains("marker is invalid"), "{stderr}");
            }
            if args[0] == "catalog" {
                assert_eq!(
                    Store::new(&fixture.paths)?.run_log().recent_runs(1)?[0]
                        .error
                        .as_deref(),
                    Some(canonical.as_str())
                );
            }
        }
    }
    assert_eq!(
        Store::new(&fixture.paths)?
            .bootstrap()
            .unwrap_err()
            .to_string(),
        canonical
    );
    assert_eq!(
        conn.query_row(
            "SELECT value FROM meta WHERE key='pricing_recompute_in_progress'",
            [],
            |row| row.get::<_, String>(0)
        )?,
        "{"
    );
    Ok(())
}
