use anyhow::{Result, bail};
use clap::Subcommand;
use tracing::info;

use crate::{
    app::AppContext,
    models::SourceKind,
    remote::{
        HandshakeResponse, RemoteImporter, ShardSource, SshCommandRunner, SshShardSource,
        register_remote_host,
    },
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
    fenced.bootstrap()?;
    let host = register_remote_host(
        &fenced,
        label,
        ssh_target,
        command,
        &SshCommandRunner::default(),
    )?;
    drop(heartbeat);
    drop(lock);
    println!(
        "Registered host {} (host_id={}, target={})",
        host.label,
        host.host_id,
        host.ssh_target.as_deref().unwrap_or("")
    );
    Ok(())
}

fn list(app: &AppContext) -> Result<()> {
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    for host in store.hosts().list()? {
        println!(
            "{}\thost_id={}\ttransport={}\ttarget={}\tcommand={}\tlast_contacted={}\twatermark={}\terror={}",
            host.label,
            host.host_id,
            host.transport,
            host.ssh_target.as_deref().unwrap_or("-"),
            host.command,
            host.last_contacted_at.as_deref().unwrap_or("-"),
            host.import_watermark.as_deref().unwrap_or("-"),
            host.last_error.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}

fn remove(app: &AppContext, label: &str, delete_usage: bool, yes: bool) -> Result<()> {
    if delete_usage && !yes {
        bail!("refusing to delete imported usage without --yes");
    }
    let store = Store::new(&app.paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(30), HolderKind::Cli)?;
    let fenced = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    fenced.bootstrap()?;
    let host = fenced
        .hosts()
        .get_by_label(label)?
        .ok_or_else(|| anyhow::anyhow!("host '{label}' is not registered"))?;
    if host.host_id == LOCAL_HOST_ID || host.transport == "local" {
        bail!("cannot remove the local host");
    }
    if delete_usage {
        for kind in [
            SourceKind::Codex,
            SourceKind::Claude,
            SourceKind::Opencode,
            SourceKind::Antigravity,
            SourceKind::KimiCode,
            SourceKind::Pi,
            SourceKind::Omp,
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
        println!("Removed host '{label}' and deleted its imported usage rows.");
    } else {
        println!(
            "Removed host '{label}'. Imported usage rows were kept.\n\
             To delete them, run: llmusage remote remove {label} --delete-usage --yes"
        );
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
    fenced.bootstrap()?;
    let hosts = match host_label {
        Some(label) => {
            let host = fenced
                .hosts()
                .get_by_label(label)?
                .ok_or_else(|| anyhow::anyhow!("host '{label}' is not registered"))?;
            if host.transport != "ssh" {
                bail!("host '{label}' is not an ssh remote");
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
        println!("No ssh hosts registered.");
        drop(heartbeat);
        drop(lock);
        return Ok(());
    }

    let mut writer = fenced.begin_sync_run()?;
    let mut failed = 0usize;
    for host in &hosts {
        info!(host_id = %host.host_id, "importing remote shards");
        match RemoteImporter::import(host, &fenced, &mut writer, source) {
            Ok(outcome) => {
                for warning in &outcome.warnings {
                    eprintln!("warning: {warning}");
                }
                println!(
                    "Imported host {} (shards={}, skipped_lines={})",
                    host.label, outcome.shards_committed, outcome.skipped_lines
                );
            }
            Err(err) => {
                failed += 1;
                let detail = err.to_string();
                let _ = fenced.hosts().record_contact(&host.host_id, Some(&detail));
                eprintln!("error: host {}: {detail}", host.label);
                if host_label.is_some() {
                    writer.finish_sync_run()?;
                    drop(heartbeat);
                    drop(lock);
                    return Err(err.into());
                }
            }
        }
    }
    writer.finish_sync_run()?;
    drop(heartbeat);
    drop(lock);
    if failed > 0 {
        bail!("{failed} remote host(s) failed to import");
    }
    Ok(())
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
