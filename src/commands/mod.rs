use std::{future::Future, path::PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{app::AppContext, models::SourceKind};

pub mod blocks;
pub mod daily;
pub mod diagnostics;
pub mod doctor;
pub mod export;
pub mod hook_run;
pub mod init;
pub mod monthly;
pub mod report_args;
pub mod serve;
pub mod session;
pub mod status;
pub mod statusline;
pub mod sync;
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
        /// Restrict sync to one local source.
        #[arg(long, value_enum)]
        source: Option<SourceKind>,
        /// Restrict import to a recent-day window. The current parser surface
        /// still scans existing cursors, but this enables RecentReady signalling.
        #[arg(long)]
        recent_days: Option<u32>,
        /// Emit sync lifecycle events as NDJSON on stdout.
        #[arg(long)]
        json_events: bool,
    },
    Status,
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
    Serve {
        #[arg(long)]
        port: Option<u16>,
    },
    Tui,
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    Uninstall {
        #[arg(long)]
        purge: bool,
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
    match body.await {
        Ok(value) => {
            let summary = success_summary(&value);
            store
                .run_log()
                .finish_run(run_id, "success", summary.as_deref(), None)?;
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
            source,
            recent_days,
            json_events,
        }) => {
            sync::run_with_options(
                &app,
                sync::SyncRunOptions {
                    rebuild,
                    source,
                    recent_days,
                    json_events,
                },
            )
            .await
        }
        Some(Commands::Status) => status::run(&app).await,
        Some(Commands::Diagnostics {
            out,
            forget_file,
            source,
        }) => diagnostics::run(&app, out, forget_file, source).await,
        Some(Commands::Doctor {
            json,
            refresh_pricing,
        }) => doctor::run(&app, json, refresh_pricing).await,
        Some(Commands::Serve { port }) => serve::run(&app, port).await,
        Some(Commands::Tui) => tui::run(&app).await,
        Some(Commands::Export { command }) => match command {
            ExportCommand::Html { out } => export::run_html(&app, out).await,
        },
        Some(Commands::Uninstall { purge }) => uninstall::run(&app, purge).await,
        Some(Commands::HookRun {
            source,
            trigger,
            auto,
        }) => hook_run::run(&app, source, &trigger, auto).await,
    }
}
