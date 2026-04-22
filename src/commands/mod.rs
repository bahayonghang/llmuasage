use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{app::AppContext, models::SourceKind};

pub mod diagnostics;
pub mod doctor;
pub mod export;
pub mod hook_run;
pub mod init;
pub mod serve;
pub mod status;
pub mod sync;
pub mod tui;
pub mod uninstall;

#[derive(Debug, Parser)]
#[command(name = "llmusage", version, about = "本地优先的多 CLI 用量分析工具")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init,
    Sync,
    Status,
    Diagnostics {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Doctor {
        #[arg(long)]
        json: bool,
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

pub async fn dispatch(app: AppContext, cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => init::run(&app).await,
        Commands::Sync => sync::run(&app).await,
        Commands::Status => status::run(&app).await,
        Commands::Diagnostics { out } => diagnostics::run(&app, out).await,
        Commands::Doctor { json } => doctor::run(&app, json).await,
        Commands::Serve { port } => serve::run(&app, port).await,
        Commands::Tui => tui::run(&app).await,
        Commands::Export { command } => match command {
            ExportCommand::Html { out } => export::run_html(&app, out).await,
        },
        Commands::Uninstall { purge } => uninstall::run(&app, purge).await,
        Commands::HookRun {
            source,
            trigger,
            auto,
        } => hook_run::run(&app, source, &trigger, auto).await,
    }
}
