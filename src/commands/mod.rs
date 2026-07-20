use std::{future::Future, path::PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{error, info};

use crate::{app::AppContext, models::SourceKind};

pub mod blocks;
pub mod catalog;
pub mod codex_tracer;
pub mod daily;
pub mod dash;
pub mod diagnostics;
pub mod doctor;
pub mod export;
pub mod help;
pub mod hook_run;
pub mod init;
pub mod logs;
pub mod monthly;
pub mod report_args;
pub mod serve;
pub mod session;
pub mod source_status;
pub mod status;
pub mod statusline;
pub mod sync;
pub mod sync_progress;
pub mod sync_summary;
pub mod tui;
pub mod uninstall;

#[derive(Debug, Parser)]
#[command(
    name = "llmusage",
    version,
    about = "本地优先的多 CLI 用量分析工具；无子命令时默认输出 daily 报表"
)]
pub struct Cli {
    /// Override the llmusage runtime root directory (defaults to LLMUSAGE_HOME or ~/.llmusage).
    #[arg(long, global = true, value_name = "PATH")]
    pub home: Option<PathBuf>,

    #[command(flatten)]
    pub default_daily: report_args::DailyArgs,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show daily token/cost usage. This is also the default command.
    Daily(report_args::DailyArgs),
    /// Show monthly token/cost usage.
    Monthly(report_args::MonthlyArgs),
    /// Show per-session token/cost usage.
    Session(report_args::SessionArgs),
    /// Show 5-hour usage blocks and burn-rate projections.
    Blocks(report_args::BlocksArgs),
    /// Print a single statusline-friendly usage summary.
    Statusline(report_args::StatuslineArgs),
    Init,
    Sync {
        /// Rebuild usage rows and buckets from local source files/DBs.
        #[arg(long)]
        rebuild: bool,
        /// Allow `--rebuild` to delete imported usage when original source
        /// files are missing. Without this, lossy rebuilds are refused.
        #[arg(long, requires = "rebuild")]
        allow_lossy_rebuild: bool,
        /// Restrict sync to one local source.
        #[arg(long, value_enum)]
        source: Option<SourceKind>,
        /// Restrict import to a recent-day window. The current parser surface
        /// still scans existing cursors, but this enables RecentReady signalling.
        #[arg(long)]
        recent_days: Option<u32>,
        /// CCR provider activation JSONL used to attribute relay provider labels.
        #[arg(long, value_name = "PATH")]
        provider_map: Option<PathBuf>,
        /// Emit sync lifecycle events as NDJSON on stdout.
        #[arg(long)]
        json_events: bool,
    },
    Status,
    /// Show parser-backed source and monitor-only platform status.
    #[command(name = "source-status")]
    SourceStatus,
    Diagnostics {
        /// Write the diagnostics JSON dump to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Mark a file as `deleted_by_user` and remove its cursor row, then
        /// exit. Pass an absolute file path. Combine with `--source` when
        /// the same path could exist in multiple sources.
        #[arg(long, value_name = "PATH")]
        forget_file: Option<PathBuf>,
        /// Restrict `--forget-file` to one source. Required when the same
        /// `file_path` may appear in multiple sources.
        #[arg(long, value_enum, requires = "forget_file")]
        source: Option<SourceKind>,
    },
    Doctor {
        #[arg(long)]
        json: bool,
        /// Local path to a litellm pricing snapshot. Copies the file into
        /// `~/.llmusage/pricing/` and recomputes per-event cost columns.
        /// llmusage refuses URLs in 0.5.x; remote fetch is a future patch.
        #[arg(long, value_name = "PATH")]
        refresh_pricing: Option<PathBuf>,
    },
    /// Manage the local pricing catalog overlay.
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    /// Query local structured runtime logs and recent run records.
    Logs {
        /// Number of recent log entries and run_log records to return.
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Minimum tracing level to include.
        #[arg(long, value_parser = ["error", "warn", "info", "debug", "trace"])]
        level: Option<String>,
        /// Restrict to one tracked command label, such as `sync`.
        #[arg(long)]
        command: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    Serve {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Interactive terminal dashboard (replaces `tui`).
    Dash,
    /// Deprecated: use `dash` instead.
    #[command(hide = true)]
    Tui,
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    Uninstall {
        #[arg(long)]
        purge: bool,
    },
    /// Codex-specific usage tracker with detailed token accounting and thread tracking.
    #[command(name = "codex-tracer")]
    CodexTracer {
        /// Port to listen on (default: 8765)
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Don't automatically open browser
        #[arg(long)]
        no_open: bool,
        /// Rebuild database from JSONL files
        #[arg(long)]
        rebuild: bool,
    },
    #[command(name = "hook-run", hide = true)]
    HookRun {
        #[arg(long, value_enum)]
        source: SourceKind,
        #[arg(long)]
        trigger: String,
        #[arg(long, default_value_t = false)]
        auto: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ExportCommand {
    Html {
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum CatalogCommand {
    /// Validate and activate a local v2 overlay file.
    Apply {
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Show the base, overlay, and effective catalog layers.
    Status {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Remove the overlay and restore its recorded base catalog.
    Reset,
}

async fn run_tracked<T, Fut, S>(
    store: &crate::store::Store,
    command: &str,
    body: Fut,
    success_summary: S,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
    S: FnOnce(&T) -> Option<String>,
{
    let run_id = store.run_log().record_run_start(command)?;
    info!(command, run_id, "run started");
    match body.await {
        Ok(value) => {
            let summary = success_summary(&value);
            store
                .run_log()
                .finish_run(run_id, "success", summary.as_deref(), None)?;
            info!(
                command,
                run_id,
                status = "success",
                summary = summary.as_deref().unwrap_or(""),
                "run finished"
            );
            Ok(value)
        }
        Err(err) => {
            if let Err(finish_err) =
                store
                    .run_log()
                    .finish_run(run_id, "failed", None, Some(&format!("{err:#}")))
            {
                return Err(err.context(format!(
                    "记录 {command} 失败 run_log 时也失败: {finish_err}"
                )));
            }
            error!(
                command,
                run_id,
                status = "failed",
                error = %err,
                "run failed"
            );
            Err(err)
        }
    }
}

pub async fn dispatch(app: AppContext, cli: Cli) -> Result<()> {
    match cli.command {
        None => daily::run(&app, cli.default_daily).await,
        Some(Commands::Daily(args)) => daily::run(&app, args).await,
        Some(Commands::Monthly(args)) => monthly::run(&app, args).await,
        Some(Commands::Session(args)) => session::run(&app, args).await,
        Some(Commands::Blocks(args)) => blocks::run(&app, args).await,
        Some(Commands::Statusline(args)) => statusline::run(&app, args).await,
        Some(Commands::Init) => init::run(&app).await,
        Some(Commands::Sync {
            rebuild,
            allow_lossy_rebuild,
            source,
            recent_days,
            provider_map,
            json_events,
        }) => {
            sync::run_with_options(
                &app,
                sync::SyncRunOptions {
                    rebuild,
                    source,
                    recent_days,
                    parallelism: None,
                    provider_map,
                    json_events,
                    allow_lossy_rebuild,
                },
            )
            .await
        }
        Some(Commands::Status) => status::run(&app).await,
        Some(Commands::SourceStatus) => source_status::run(&app).await,
        Some(Commands::Diagnostics {
            out,
            forget_file,
            source,
        }) => diagnostics::run(&app, out, forget_file, source).await,
        Some(Commands::Doctor {
            json,
            refresh_pricing,
        }) => doctor::run(&app, json, refresh_pricing).await,
        Some(Commands::Catalog { command }) => match command {
            CatalogCommand::Apply { path } => catalog::apply(&app, &path).await,
            CatalogCommand::Status { json } => catalog::status(&app, json).await,
            CatalogCommand::Reset => catalog::reset(&app).await,
        },
        Some(Commands::Logs {
            limit,
            level,
            command,
            json,
        }) => logs::run(&app, limit, level, command, json).await,
        Some(Commands::Serve { port }) => serve::run(&app, port).await,
        Some(Commands::Dash) => dash::run(&app, false).await,
        Some(Commands::Tui) => dash::run(&app, true).await,
        Some(Commands::Export { command }) => match command {
            ExportCommand::Html { out } => export::run_html(&app, out).await,
        },
        Some(Commands::Uninstall { purge }) => uninstall::run(&app, purge).await,
        Some(Commands::CodexTracer {
            port,
            no_open,
            rebuild,
        }) => codex_tracer::run(&app, port, !no_open, rebuild).await,
        Some(Commands::HookRun {
            source,
            trigger,
            auto,
        }) => hook_run::run(&app, source, &trigger, auto).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::{Cli, Commands};

    #[test]
    fn source_filter_accepts_antigravity_and_rejects_gemini() {
        let cli = Cli::try_parse_from(["llmusage", "sync", "--source", "antigravity"])
            .expect("antigravity should be accepted");
        match cli.command {
            Some(Commands::Sync { source, .. }) => {
                assert_eq!(source.map(|value| value.as_str()), Some("antigravity"));
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let err = Cli::try_parse_from(["llmusage", "sync", "--source", "gemini"])
            .expect_err("gemini source id should be rejected");
        assert!(err.to_string().contains("antigravity"));
    }

    #[test]
    fn source_status_parses_from_args() {
        let cli =
            Cli::try_parse_from(["llmusage", "source-status"]).expect("source-status should parse");
        assert!(matches!(cli.command, Some(Commands::SourceStatus)));
    }

    #[test]
    fn source_status_visible_in_help_text() {
        let help = Cli::command().render_help().to_string();
        assert!(
            help.contains("source-status"),
            "expected `source-status` in help output, got: {help}"
        );
    }

    #[test]
    fn catalog_subcommands_parse() {
        let apply = Cli::try_parse_from(["llmusage", "catalog", "apply", "overlay.json"])
            .expect("catalog apply should parse");
        assert!(matches!(
            apply.command,
            Some(Commands::Catalog {
                command: super::CatalogCommand::Apply { .. }
            })
        ));

        let status = Cli::try_parse_from(["llmusage", "catalog", "status", "--json"])
            .expect("catalog status should parse");
        assert!(matches!(
            status.command,
            Some(Commands::Catalog {
                command: super::CatalogCommand::Status { json: true }
            })
        ));

        let reset = Cli::try_parse_from(["llmusage", "catalog", "reset"])
            .expect("catalog reset should parse");
        assert!(matches!(
            reset.command,
            Some(Commands::Catalog {
                command: super::CatalogCommand::Reset
            })
        ));
    }
}
