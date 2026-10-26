use super::cli_errors::Diagnostic;
use crate::common::i18n::commands::message;
use anyhow::Result;
use clap::Subcommand;
use tracing::info;

use crate::{
    app::AppContext,
    models::SourceKind,
    remote::{HandshakeResponse, RemoteImporter, ShardSource, SshCommandRunner, SshShardSource},
    store::{HolderKind, LOCAL_HOST_ID, Store},
};

#[derive(Debug, Subcommand)]
pub enum RemoteCommand {
    /// Register an SSH host after a version probe and shard-protocol handshake.
    Add {
        label: String,
        ssh_target: String,
        /// Remote argv prefix. Spaces split into argv; not interpreted by a local shell.
        #[arg(long, default_value = "llmusage")]
        command: String,
    },
    /// List registered hosts.
    List,
    /// Remove a host row. Imported usage is kept unless `--delete-usage --yes`.
    Remove {
        label: String,
        /// Also delete imported usage rows for this host.
        #[arg(long)]
        delete_usage: bool,
        /// Required together with `--delete-usage`.
        #[arg(long)]
        yes: bool,
    },
    /// Import shards from registered SSH hosts. Does not run local parsers.
    Sync {
        #[arg(long)]
        host: Option<String>,
    },
    /// Print local shard protocol and schema versions as JSON.
    #[command(hide = true)]
    Handshake,
}

pub async fn run(app: &AppContext, command: RemoteCommand) -> Result<()> {
    match command {
        RemoteCommand::Handshake => handshake(),
        RemoteCommand::Add {
            label,
            ssh_target,
            command,
        } => add(app, &label, &ssh_target, &command),
        RemoteCommand::List => list(app),
        RemoteCommand::Remove {
            label,
            delete_usage,
            yes,
        } => remove(app, &label, delete_usage, yes),
        RemoteCommand::Sync { host } => {
            sync_remotes_with_source(app, host.as_deref(), &SshShardSource::default())
        }
    }
}

fn handshake() -> Result<()> {
    println!("{}", serde_json::to_string(&HandshakeResponse::local())?);
    Ok(())
}

fn add(app: &AppContext, label: &str, ssh_target: &str, command: &str) -> Result<()> {
    let store = Store::new(&app.paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(30), HolderKind::Cli)?;
    let fenced = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    super::cli_errors::pricing_result(fenced.bootstrap_with_pricing_facts())?;
    let host = crate::remote::register::register_remote_host_with_facts(
        &fenced,
        label,
        ssh_target,
        command,
        &SshCommandRunner::default(),
    )
    .map_err(registration_error)?;
    drop(heartbeat);
    drop(lock);
    println!(
        "{}",
        message!(
            RemoteRegistered,
            host.label,
            host.host_id,
            host.ssh_target.as_deref().unwrap_or("")
        )
    );
    Ok(())
}

fn registration_error(failure: crate::remote::register::RegistrationFailure) -> anyhow::Error {
    let error = anyhow::Error::new(failure.error);
    if let Some(fact) = failure.fact {
        let diagnostic = Diagnostic::with_facts_context(
            &error,
            crate::common::i18n::commands::OperationDiagnostic::Registration { fact },
        );
        error.context(diagnostic)
    } else {
        error
    }
}

fn list(app: &AppContext) -> Result<()> {
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    for host in store.hosts().list()? {
        println!(
            "{}",
            message!(
                RemoteList,
                host.label,
                host.host_id,
                host.transport,
                host.ssh_target.as_deref().unwrap_or("-"),
                host.command,
                host.last_contacted_at.as_deref().unwrap_or("-"),
                host.import_watermark.as_deref().unwrap_or("-"),
                host.last_error.as_deref().unwrap_or("-")
            )
        );
    }
    Ok(())
}

fn remove(app: &AppContext, label: &str, delete_usage: bool, yes: bool) -> Result<()> {
    if delete_usage && !yes {
        return Err(Diagnostic::new(
            "refusing to delete imported usage without --yes",
            "未提供 --yes，拒绝删除导入的用量记录",
        )
        .into());
    }
    let store = Store::new(&app.paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(30), HolderKind::Cli)?;
    let fenced = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    super::cli_errors::pricing_result(fenced.bootstrap_with_pricing_facts())?;
    let host = fenced.hosts().get_by_label(label)?.ok_or_else(|| {
        Diagnostic::new(
            format!("host '{label}' is not registered"),
            format!("主机 '{label}' 尚未注册"),
        )
    })?;
    if host.host_id == LOCAL_HOST_ID || host.transport == "local" {
        return Err(Diagnostic::new("cannot remove the local host", "不能移除本地主机").into());
    }
    if delete_usage {
        for kind in [
            SourceKind::Codex,
            SourceKind::Claude,
            SourceKind::Opencode,
            SourceKind::Antigravity,
            SourceKind::AntigravityIde,
            SourceKind::KimiCode,
            SourceKind::Pi,
            SourceKind::Omp,
            SourceKind::Cindy,
            SourceKind::Grok,
            SourceKind::Zcode,
            SourceKind::DeepseekHarness,
        ] {
            fenced.reset_for_source(kind, &host.host_id)?;
        }
    }
    fenced.hosts().remove(&host.host_id)?;
    drop(heartbeat);
    drop(lock);
    if delete_usage {
        println!("{}", message!(RemoteRemovedUsage, label));
    } else {
        println!("{}", message!(RemoteRemoved, label, label));
    }
    Ok(())
}

fn sync_remotes_with_source(
    app: &AppContext,
    host_label: Option<&str>,
    source: &dyn ShardSource,
) -> Result<()> {
    let store = Store::new(&app.paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(30), HolderKind::Cli)?;
    let fenced = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    super::cli_errors::pricing_result(fenced.bootstrap_with_pricing_facts())?;
    let hosts = match host_label {
        Some(label) => {
            let host = fenced.hosts().get_by_label(label)?.ok_or_else(|| {
                Diagnostic::new(
                    format!("host '{label}' is not registered"),
                    format!("主机 '{label}' 尚未注册"),
                )
            })?;
            if host.transport != "ssh" {
                return Err(Diagnostic::new(
                    format!("host '{label}' is not an ssh remote"),
                    format!("主机 '{label}' 不是 SSH 远程主机"),
                )
                .into());
            }
            vec![host]
        }
        None => fenced
            .hosts()
            .list()?
            .into_iter()
            .filter(|host| host.transport == "ssh")
            .collect(),
    };
    if hosts.is_empty() {
        println!("{}", message!(RemoteNone));
        drop(heartbeat);
        drop(lock);
        return Ok(());
    }

    let mut writer = fenced.begin_sync_run()?;
    let mut failed = 0usize;
    for host in &hosts {
        info!(event_code = "commands.remote.01", host_id = %host.host_id, "importing remote shards");
        match RemoteImporter::import_with_facts(host, &fenced, &mut writer, source) {
            Ok(result) => {
                for warning in &result.warnings {
                    eprintln!("{}", import_warning_line(warning));
                }
                let outcome = result.outcome;
                println!(
                    "{}",
                    message!(
                        RemoteImported,
                        host.label,
                        outcome.shards_committed,
                        outcome.skipped_lines
                    )
                );
            }
            Err(failure) => {
                failed += 1;
                let detail = failure.error.to_string();
                let _ = fenced.hosts().record_contact(&host.host_id, Some(&detail));
                let error = import_error(failure);
                eprintln!("{}", import_error_line(&host.label, &error));
                if host_label.is_some() {
                    writer.finish_sync_run()?;
                    drop(heartbeat);
                    drop(lock);
                    return Err(error);
                }
            }
        }
    }
    writer.finish_sync_run()?;
    drop(heartbeat);
    drop(lock);
    if failed > 0 {
        return Err(Diagnostic::new(
            format!("{failed} remote host(s) failed to import"),
            format!("{failed} 台远程主机导入失败"),
        )
        .into());
    }
    Ok(())
}

fn import_error(failure: crate::remote::diagnostic::ImportFailure) -> anyhow::Error {
    let error = anyhow::Error::new(failure.error);
    if let Some(facts) = failure.facts {
        let diagnostic = Diagnostic::with_facts_context(&error, *facts);
        error.context(diagnostic)
    } else {
        error
    }
}

fn import_error_line(host: &str, error: &anyhow::Error) -> String {
    use crate::common::i18n::{current, text};
    format!(
        "{} {host}: {}",
        text("Remote host", "远程主机"),
        super::cli_errors::application_error(error, current())
    )
}

fn import_warning_line(facts: &crate::remote::diagnostic::DiagnosticFacts) -> String {
    use crate::common::i18n::{current, text};
    format!("{}: {}", text("Warning", "警告"), facts.render(current()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{ParseIssues, UsageEvent, UsageTokens},
        paths::AppPaths,
        remote::{MemoryShardSource, ScriptedShardSource, ShardRecord, encode_record},
        store::{FileCursor, SyncShard},
    };
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn registration_rejections_preserve_public_errors_and_translate_typed_private_facts()
    -> anyhow::Result<()> {
        use crate::common::i18n::Language;
        use crate::remote::{CommandOutput, SHARD_PROTOCOL_VERSION, ScriptedCommandRunner};
        let temp = TempDir::new()?;
        let store = Store::new(&AppPaths::with_root(temp.path().to_path_buf())?)?;
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(5), HolderKind::Cli)?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        for (version_status, handshake_status, protocol, en, zh) in [
            (
                127,
                0,
                SHARD_PROTOCOL_VERSION,
                "--version failed",
                "--version 失败",
            ),
            (
                0,
                42,
                SHARD_PROTOCOL_VERSION,
                "handshake failed",
                "握手失败",
            ),
            (0, 0, 99, "shard protocol mismatch", "分片协议不一致"),
        ] {
            let runner = ScriptedCommandRunner {
                version: CommandOutput {
                    status: version_status,
                    stdout: "llmusage test".into(),
                    stderr: "EXTERNAL_DETAIL".into(),
                },
                handshake: CommandOutput {
                    status: handshake_status,
                    stdout: serde_json::to_string(&HandshakeResponse {
                        shard_protocol: protocol,
                        schema_version: 22,
                        llmusage_version: "test".into(),
                    })?,
                    stderr: "EXTERNAL_DETAIL".into(),
                },
            };
            let original = crate::remote::register_remote_host(
                &fenced,
                "devbox",
                "me@devbox",
                "llmusage",
                &runner,
            )
            .expect_err("public rejection");
            let failure = crate::remote::register::register_remote_host_with_facts(
                &fenced,
                "devbox",
                "me@devbox",
                "llmusage",
                &runner,
            )
            .expect_err("private rejection");
            assert_eq!(failure.error.to_string(), original.to_string());
            let error = registration_error(failure);
            assert_eq!(error.to_string(), original.to_string());
            assert!(matches!(
                error.downcast_ref::<crate::LlmusageError>(),
                Some(crate::LlmusageError::ConfigInvalid { .. })
            ));
            for language in [Language::English, Language::Chinese] {
                let rendered = super::super::cli_errors::application_error(&error, language);
                assert!(rendered.contains(language.select(en, zh)), "{rendered}");
                if protocol != 99 {
                    assert!(rendered.contains("EXTERNAL_DETAIL"));
                }
                if language == Language::Chinese {
                    assert!(!rendered.contains(en), "{rendered}");
                }
            }
        }
        assert_eq!(fenced.hosts().list()?.len(), 1);
        Ok(())
    }

    #[test]
    fn remote_remove_keeps_usage_rows_by_default() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(5), HolderKind::Cli)?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        fenced.hosts().upsert(&crate::store::Host {
            host_id: "devbox".to_string(),
            label: "devbox".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("me@devbox".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: None,
        })?;
        let mut shard = SyncShard::new_for_host(SourceKind::Codex, "devbox");
        shard.events.push(UsageEvent {
            event_key: "codex:remote:1".to_string(),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: "gpt-5".to_string(),
            event_at: "2026-08-20T00:00:00Z".to_string(),
            hour_start: "2026-08-20T00:00:00Z".to_string(),
            tokens: UsageTokens {
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
                total_tokens: 2,
            },
            project: None,
            session: None,
            source_cost: None,
        });
        let mut writer = fenced.begin_sync_run()?;
        writer.commit_shard(shard)?;
        writer.finish_sync_run()?;
        fenced.hosts().remove("devbox")?;
        drop(lock);
        let conn = store.open_connection()?;
        let events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(events, 1);
        assert!(store.hosts().get_by_label("devbox")?.is_none());
        Ok(())
    }

    fn usage_event(key: &str) -> UsageEvent {
        UsageEvent {
            event_key: key.to_string(),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: "gpt-5".to_string(),
            event_at: "2026-08-20T00:00:00Z".to_string(),
            hour_start: "2026-08-20T00:00:00Z".to_string(),
            tokens: UsageTokens {
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
                total_tokens: 2,
            },
            project: None,
            session: None,
            source_cost: None,
        }
    }

    fn ssh_host(host_id: &str, label: &str) -> crate::store::Host {
        crate::store::Host {
            host_id: host_id.to_string(),
            label: label.to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some(format!("me@{label}")),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: None,
        }
    }

    fn import_language_fixture(
        watermark: Option<&str>,
    ) -> anyhow::Result<(TempDir, AppContext, Store)> {
        let temp = TempDir::new()?;
        let app = AppContext::with_cli_home(Some(temp.path().join("home")))?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut host = ssh_host("devbox", "devbox");
        host.import_watermark = watermark.map(str::to_string);
        store.hosts().upsert(&host)?;
        Ok((temp, app, store))
    }

    fn import_language_stream(records: &[ShardRecord]) -> anyhow::Result<String> {
        let mut text = String::new();
        for record in records {
            text.push_str(&encode_record(record)?);
            text.push('\n');
        }
        Ok(text)
    }

    #[test]
    fn remote_import_rejections_keep_canonical_errors_contact_data_and_partial_commits()
    -> anyhow::Result<()> {
        use super::super::cli_errors::{application_error, canonical_detail, log_facts};
        use crate::common::i18n::{Language, with_language};
        let header = || {
            ShardRecord::header(
                "2026-08-20T02:00:00Z",
                crate::remote::source_accounting_versions([SourceKind::Codex]),
            )
        };
        let trailer = || ShardRecord::Trailer {
            sources: Vec::new(),
            parse_issues: ParseIssues::default(),
        };
        let shard = || {
            let mut shard = SyncShard::new(SourceKind::Codex);
            shard.events.push(usage_event("codex:partial:1"));
            ShardRecord::Shard { shard }
        };
        let expected_version = crate::store::expected_token_accounting_version(SourceKind::Codex);
        let mut bad_protocol = header();
        if let ShardRecord::Header {
            shard_protocol,
            schema_version,
            ..
        } = &mut bad_protocol
        {
            *shard_protocol = 99;
            *schema_version = 42;
        }
        let cases = [
            (Vec::new(), "remote shard stream from host devbox had no header record: SSH_DETAIL".to_string(), "没有 header 记录", 0),
            (vec![trailer()], "first deserialized shard record must be a header".to_string(), "必须是 header", 0),
            (vec![bad_protocol], "shard protocol mismatch: local=2 remote=99 (remote schema_version=42); upgrade llmusage so both sides share shard_protocol 2".to_string(), "分片协议不一致", 0),
            (vec![ShardRecord::header("fixed", BTreeMap::new())], "remote host devbox shard header is missing source token-accounting versions; upgrade llmusage on that host so both sides share shard_protocol 2 and the header lists each source's token_accounting_version".to_string(), "缺少来源 token 计数版本", 0),
            (vec![ShardRecord::header("fixed", BTreeMap::from([("future_source".to_string(), 1)]))], "remote host devbox shard header lists unknown source future_source; upgrade llmusage so both sides share shard_protocol 2".to_string(), "未知来源 future_source", 0),
            (vec![ShardRecord::header("fixed", BTreeMap::from([("codex".to_string(), expected_version + 1)]))], format!("token accounting mismatch for host devbox source codex: local={expected_version} remote={}; upgrade llmusage on that host so source codex reports token_accounting_version {expected_version} in shard protocol 2", expected_version + 1), "token 计数版本不一致", 0),
            (vec![ShardRecord::header("fixed", crate::remote::source_accounting_versions([SourceKind::Claude])), shard()], "remote host devbox shard source codex is not listed in the header source_accounting_versions map; upgrade llmusage on that host so the header lists every shard source and its token_accounting_version".to_string(), "未列在 header", 0),
            (vec![header(), shard()], "remote shard stream from host devbox ended without a trailer; imported events were kept but the host watermark was not advanced: SSH_DETAIL".to_string(), "未推进主机水位", 1),
        ];
        for (records, canonical, chinese, imported) in cases {
            for language in [Language::English, Language::Chinese] {
                let (_temp, app, store) = import_language_fixture(None)?;
                let source = MemoryShardSource {
                    stdout: import_language_stream(&records)?,
                    stderr: "SSH_DETAIL".into(),
                    status: 1,
                };
                let error = with_language(language, || {
                    sync_remotes_with_source(&app, Some("devbox"), &source)
                })
                .expect_err("fake rejection");
                let expected = format!("invalid config: {canonical}");
                assert_eq!(error.to_string(), expected);
                assert_eq!(canonical_detail(&error), expected);
                assert!(matches!(
                    error.downcast_ref::<crate::LlmusageError>(),
                    Some(crate::LlmusageError::ConfigInvalid { .. })
                ));
                let human = application_error(&error, language);
                let immediate = with_language(language, || import_error_line("devbox", &error));
                for text in [&human, &immediate] {
                    assert!(
                        text.contains(language.select(&canonical, chinese)),
                        "{text}"
                    );
                    if language == Language::Chinese {
                        assert!(!text.contains(&canonical), "{text}");
                    }
                }
                let (code, facts) = log_facts(&error).expect("neutral metadata");
                let fields = serde_json::json!({"event_code":"commands.mod.04","error":expected,"diagnostic_code":code,"diagnostic_facts":facts});
                let replay = crate::common::i18n::logging::project(
                    language,
                    "ERROR",
                    Some("run failed"),
                    &fields,
                    false,
                );
                assert!(
                    replay.contains(language.select(&canonical, chinese)),
                    "{replay}"
                );
                let host = store.hosts().get_by_label("devbox")?.expect("host");
                assert_eq!(host.last_error.as_deref(), Some(expected.as_str()));
                assert!(host.import_watermark.is_none());
                assert_eq!(
                    store.host_source_event_count("devbox", SourceKind::Codex)?,
                    imported
                );
                assert!(
                    store
                        .token_accounting_version_for_host("devbox", SourceKind::Codex)?
                        .is_none()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn remote_import_historical_refusal_preserves_rows_watermark_and_original_cause()
    -> anyhow::Result<()> {
        use crate::common::i18n::{Language, with_language};
        for language in [Language::English, Language::Chinese] {
            let watermark = "2026-08-19T00:00:00Z";
            let (_temp, app, store) = import_language_fixture(Some(watermark))?;
            let mut shard = SyncShard::new_for_host(SourceKind::Codex, "devbox");
            shard.events.push(usage_event("codex:old:1"));
            let mut writer = store.begin_sync_run()?;
            writer.commit_shard(shard)?;
            writer.finish_sync_run()?;
            let source = MemoryShardSource {
                stdout: import_language_stream(&[ShardRecord::header(
                    "fixed",
                    crate::remote::source_accounting_versions([SourceKind::Codex]),
                )])?,
                stderr: String::new(),
                status: 0,
            };
            let error = with_language(language, || {
                sync_remotes_with_source(&app, Some("devbox"), &source)
            })
            .expect_err("untrusted history");
            let expected_version =
                crate::store::expected_token_accounting_version(SourceKind::Codex);
            let canonical = format!(
                "invalid config: remote host devbox source codex already has imported rows without a trusted current token-accounting marker (historical=unknown, current={expected_version}); a full restore is required (not implemented); existing events and watermarks were left unchanged"
            );
            assert_eq!(
                super::super::cli_errors::canonical_detail(&error),
                canonical
            );
            let human = super::super::cli_errors::application_error(&error, language);
            assert!(
                human.contains(language.select(
                    "a full restore is required (not implemented)",
                    "需要完整恢复（尚未实现）"
                )),
                "{human}"
            );
            if language == Language::Chinese {
                assert!(!human.contains("historical=unknown"));
            }
            let host = store.hosts().get_by_label("devbox")?.expect("host");
            assert_eq!(host.last_error.as_deref(), Some(canonical.as_str()));
            assert_eq!(host.import_watermark.as_deref(), Some(watermark));
            assert_eq!(
                store.host_source_event_count("devbox", SourceKind::Codex)?,
                1
            );
        }
        Ok(())
    }

    #[test]
    fn remote_import_warning_facts_translate_without_changing_public_outcome() -> anyhow::Result<()>
    {
        use crate::common::i18n::{Language, with_language};
        for language in [Language::English, Language::Chinese] {
            let (_temp, app, store) = import_language_fixture(None)?;
            let mut shard = SyncShard::new(SourceKind::Codex);
            shard.events.push(usage_event("codex:warning:1"));
            let source = MemoryShardSource {
                stdout: format!(
                    "MOTD\n{}",
                    import_language_stream(&[
                        ShardRecord::header(
                            "fixed",
                            crate::remote::source_accounting_versions([SourceKind::Codex])
                        ),
                        ShardRecord::Shard { shard },
                        ShardRecord::Trailer {
                            sources: vec![crate::parsers::SourceSyncStats {
                                source: SourceKind::Codex,
                                ..Default::default()
                            }],
                            parse_issues: ParseIssues::default()
                        },
                    ])?
                ),
                stderr: String::new(),
                status: 0,
            };
            with_language(language, || {
                sync_remotes_with_source(&app, Some("devbox"), &source)
            })?;
            let host = store.hosts().get_by_label("devbox")?.expect("host");
            let mut writer = store.begin_sync_run()?;
            let result = RemoteImporter::import_with_facts(&host, &store, &mut writer, &source)?;
            writer.finish_sync_run()?;
            assert_eq!(
                result.outcome.warnings,
                ["skipped 1 non-JSON shard lines from host devbox"]
            );
            assert_eq!(result.outcome.skipped_lines, 1);
            assert_eq!(result.outcome.shards_committed, 1);
            let warning = with_language(language, || import_warning_line(&result.warnings[0]));
            assert!(
                warning.contains(language.select(
                    "skipped 1 non-JSON shard lines from host devbox",
                    "已跳过主机 devbox 的 1 行非 JSON 分片内容"
                )),
                "{warning}"
            );
            assert_eq!(
                store.host_source_event_count("devbox", SourceKind::Codex)?,
                1
            );
            assert_eq!(
                host.import_watermark.as_deref(),
                Some("2026-08-20T00:00:00Z")
            );
            assert!(host.last_error.is_none());
        }
        Ok(())
    }

    #[test]
    fn remote_sync_host_filter_imports_only_that_host_without_local_driver() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home)?;
        let app = crate::app::AppContext::with_cli_home(Some(home))?;
        let store = Store::new(&app.paths)?;
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(5), HolderKind::Cli)?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        fenced.hosts().upsert(&ssh_host("devbox", "devbox"))?;
        fenced.hosts().upsert(&ssh_host("other", "other"))?;
        let mut local = SyncShard::new(SourceKind::Codex);
        local.events.push(usage_event("codex:local:1"));
        local.seen_file_paths.push("/local/codex.jsonl".to_string());
        local.cursors.push(FileCursor {
            cursor_key: "/local/codex.jsonl".to_string(),
            file_path: "/local/codex.jsonl".to_string(),
            file_fingerprint: "fp".to_string(),
            file_size: 4,
            file_mtime_ns: 0,
            tail_signature: "tail".to_string(),
            offset: 4,
            last_total: None,
            last_model: None,
            updated_at: "2026-08-20T00:00:00Z".to_string(),
        });
        let mut writer = fenced.begin_sync_run()?;
        writer.commit_shard(local)?;
        writer.finish_sync_run()?;
        drop(lock);

        let mut remote_shard = SyncShard::new(SourceKind::Codex);
        remote_shard.events.push(usage_event("codex:remote:1"));
        let mut stdout = String::new();
        for record in [
            crate::remote::protocol::ShardRecord::header(
                "2026-08-20T02:00:00Z",
                crate::remote::protocol::source_accounting_versions([SourceKind::Codex]),
            ),
            ShardRecord::Shard {
                shard: remote_shard,
            },
            ShardRecord::Trailer {
                sources: vec![crate::parsers::SourceSyncStats {
                    source: SourceKind::Codex,
                    events_seen: 1,
                    events_inserted: 1,
                    ..crate::parsers::SourceSyncStats::default()
                }],
                parse_issues: ParseIssues::default(),
            },
        ] {
            stdout.push_str(&encode_record(&record)?);
            stdout.push('\n');
        }
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "devbox".to_string(),
            Ok(MemoryShardSource {
                stdout,
                stderr: String::new(),
                status: 0,
            }),
        );
        hosts.insert("other".to_string(), Err("should not be opened".to_string()));
        let source = ScriptedShardSource {
            hosts,
            default_error: "unreachable".to_string(),
        };

        sync_remotes_with_source(&app, Some("devbox"), &source)?;

        let store = Store::new(&app.paths)?;
        let conn = store.open_connection()?;
        let remote_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        let other_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'other'",
            [],
            |row| row.get(0),
        )?;
        let local_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'local'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(remote_events, 1);
        assert_eq!(other_events, 0);
        assert_eq!(local_events, 1);
        let other = store.hosts().get_by_label("other")?.expect("other");
        assert!(other.last_error.is_none());
        assert!(other.last_contacted_at.is_none());
        let local_counts = store.source_files().counts(SourceKind::Codex, "local")?;
        assert_eq!(local_counts.live, 1);
        assert_eq!(local_counts.missing, 0);
        Ok(())
    }
}
