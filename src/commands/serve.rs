use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use tracing::{info, warn};

use crate::{app::AppContext, models::SourceKind, store::Store, web};

use super::sync;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TokenAccountingRepairReport {
    pub rebuilt_sources: Vec<SourceKind>,
    pub blocked_sources: Vec<BlockedTokenAccountingSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedTokenAccountingSource {
    pub source: SourceKind,
    pub missing_file_count: u64,
    pub protected_event_count: u64,
}

pub async fn run(app: &AppContext, port: Option<u16>) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：启动本地 Web UI 与 JSON API
     * ========================================================================
     * 目标：
     * 1) 只监听 127.0.0.1
     * 2) 启动固定端口组探测后的本地服务
     * 3) 持续运行到 Ctrl+C，避免任何公网暴露
     */
    info!("开始启动本地 Web UI 服务");

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    repair_legacy_token_accounting(app, &store).await?;
    let addr = super::run_tracked(
        &store,
        "serve",
        async { web::serve(store.clone(), port).await },
        |addr| Some(format!("listen={addr}")),
    )
    .await?;

    /*
     * ========================================================================
     * 步骤2：回显本地地址并尝试打开默认浏览器
     * ========================================================================
     * 目标：
     * 1) 始终保留终端里的手动访问地址
     * 2) 默认帮用户打开本地 dashboard
     * 3) 浏览器启动失败时也不影响本地服务继续运行
     */
    let dashboard_url = format!("http://{addr}");
    println!("Local dashboard: {dashboard_url}");
    if let Err(err) = open_dashboard_in_browser(&dashboard_url) {
        warn!(
            url = %dashboard_url,
            error = %err,
            "打开本地 dashboard 浏览器失败，服务将继续运行"
        );
    }
    tokio::signal::ctrl_c().await?;
    info!("收到 Ctrl+C，准备停止本地 Web UI 服务");
    Ok(())
}

pub async fn repair_legacy_token_accounting(
    app: &AppContext,
    store: &Store,
) -> Result<TokenAccountingRepairReport> {
    let legacy_sources = sync::legacy_token_accounting_sources(store)?;
    let mut report = TokenAccountingRepairReport::default();

    for source in legacy_sources {
        let risk = store.source_files().lossy_rebuild_risk(source)?;
        if risk.has_risk() {
            warn!(
                source = %source,
                missing_files = risk.missing_file_count,
                protected_events = risk.protected_event_count,
                "serve 检测到 legacy token accounting，但自动重建会丢失历史，已跳过该来源"
            );
            eprintln!(
                "Skipped automatic token-accounting rebuild for {source}: missing_files={} protected_events={}. Restore the source files, then run `llmusage sync --rebuild --source {source}`. Historical reports remain available; --allow-lossy-rebuild was not enabled.",
                risk.missing_file_count, risk.protected_event_count
            );
            report.blocked_sources.push(BlockedTokenAccountingSource {
                source,
                missing_file_count: risk.missing_file_count,
                protected_event_count: risk.protected_event_count,
            });
            continue;
        }

        info!(source = %source, "serve 开始自动重建 legacy token accounting 来源");
        eprintln!("Rebuilding legacy token accounting for {source} before starting dashboard...");
        sync::run_with_options(
            app,
            sync::SyncRunOptions {
                rebuild: true,
                source: Some(source),
                allow_lossy_rebuild: false,
                ..Default::default()
            },
        )
        .await
        .with_context(|| {
            format!(
                "Failed to rebuild legacy token accounting for {source}; dashboard startup was stopped. Run `llmusage sync --rebuild --source {source}` after resolving the reported parser or SQLite error."
            )
        })?;
        report.rebuilt_sources.push(source);
        info!(source = %source, "serve 完成 legacy token accounting 自动重建");
    }

    if !report.rebuilt_sources.is_empty() {
        let sources = report
            .rebuilt_sources
            .iter()
            .map(|source| source.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("Token-accounting rebuild completed for: {sources}");
    }

    Ok(report)
}

fn open_dashboard_in_browser(url: &str) -> Result<()> {
    let plan = BrowserLaunchPlan::for_current_platform(url);
    let mut command = plan.command();
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("browser launcher exited with status {status}");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserPlatform {
    Windows,
    MacOs,
    Unix,
}

impl BrowserPlatform {
    fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Unix
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct BrowserLaunchPlan {
    program: &'static str,
    args: Vec<String>,
}

impl BrowserLaunchPlan {
    fn for_current_platform(url: &str) -> Self {
        Self::for_platform(BrowserPlatform::current(), url)
    }

    fn for_platform(platform: BrowserPlatform, url: &str) -> Self {
        match platform {
            BrowserPlatform::Windows => Self {
                program: "cmd",
                args: vec!["/C".into(), "start".into(), String::new(), url.to_owned()],
            },
            BrowserPlatform::MacOs => Self {
                program: "open",
                args: vec![url.to_owned()],
            },
            BrowserPlatform::Unix => Self {
                program: "xdg-open",
                args: vec![url.to_owned()],
            },
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.program);
        command.args(&self.args);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserLaunchPlan, BrowserPlatform};

    #[test]
    fn windows_browser_launch_plan_uses_start_command() {
        let plan =
            BrowserLaunchPlan::for_platform(BrowserPlatform::Windows, "http://127.0.0.1:37421");
        assert_eq!(plan.program, "cmd");
        assert_eq!(
            plan.args,
            vec![
                "/C".to_string(),
                "start".to_string(),
                String::new(),
                "http://127.0.0.1:37421".to_string(),
            ]
        );
    }

    #[test]
    fn macos_browser_launch_plan_uses_open() {
        let plan =
            BrowserLaunchPlan::for_platform(BrowserPlatform::MacOs, "http://127.0.0.1:37421");
        assert_eq!(plan.program, "open");
        assert_eq!(plan.args, vec!["http://127.0.0.1:37421".to_string()]);
    }

    #[test]
    fn unix_browser_launch_plan_uses_xdg_open() {
        let plan = BrowserLaunchPlan::for_platform(BrowserPlatform::Unix, "http://127.0.0.1:37421");
        assert_eq!(plan.program, "xdg-open");
        assert_eq!(plan.args, vec!["http://127.0.0.1:37421".to_string()]);
    }
}
