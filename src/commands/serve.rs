use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr},
    process::{Command, Stdio},
};

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
    run_with_options(app, port, false, false).await
}

pub(crate) async fn run_with_options(
    app: &AppContext,
    port: Option<u16>,
    public: bool,
    no_open: bool,
) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：启动本地 Web UI 与 JSON API
     * ========================================================================
     * 目标：
     * 1) 默认监听 127.0.0.1；显式公开时监听全部 IPv4 接口
     * 2) 启动固定端口组探测后的本地服务
     * 3) 持续运行到 Ctrl+C；公开监听只能通过显式参数开启
     */
    info!("开始启动本地 Web UI 服务");

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    repair_legacy_token_accounting(app, &store).await?;
    store.run_log().recover_running_runs(&["serve"])?;
    let bind_ip = if public {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    super::run_tracked(
        &store,
        "serve",
        serve_session(store.clone(), port, bind_ip, public, no_open),
        |addr| Some(format!("listen={addr}")),
    )
    .await?;
    Ok(())
}

async fn serve_session(
    store: Store,
    port: Option<u16>,
    bind_ip: IpAddr,
    public: bool,
    no_open: bool,
) -> Result<std::net::SocketAddr> {
    let server = web::bind_server(store, port, bind_ip).await?;
    let addr = server.addr();

    /*
     * ========================================================================
     * 步骤2：回显本地地址并尝试打开默认浏览器
     * ========================================================================
     * 目标：
     * 1) 始终保留终端里的手动访问地址
     * 2) 默认帮用户打开本地 dashboard，但 SSH 与 --no-open 例外
     * 3) 浏览器启动失败时也不影响服务继续运行
     */
    let dashboard_url = local_dashboard_url(addr.port());
    println!("Local dashboard: {dashboard_url}");
    if public {
        eprintln!("Remote dashboard: {}", public_dashboard_url(addr.port()));
        eprintln!(
            "Warning: `--public` exposes the dashboard and JSON API without authentication or TLS. Restrict access with a firewall, SSH tunnel, or reverse proxy."
        );
    }

    let ssh_session = is_ssh_session();
    if ssh_session && !public {
        eprintln!(
            "SSH session detected; browser opening skipped. To open locally, forward the port: ssh -L {port}:127.0.0.1:{port} <user>@<server>",
            port = addr.port()
        );
    }

    match browser_open_decision(no_open, ssh_session) {
        BrowserOpenDecision::Open => {
            if let Err(err) = open_dashboard_in_browser(&dashboard_url) {
                warn!(
                    url = %dashboard_url,
                    error = %err,
                    "打开本地 dashboard 浏览器失败，服务将继续运行"
                );
            }
        }
        BrowserOpenDecision::Disabled => {
            info!("serve 已通过 --no-open 跳过浏览器启动");
        }
        BrowserOpenDecision::SshSession => {
            if public {
                eprintln!("SSH session detected; browser opening skipped.");
            }
            info!("serve 在 SSH 会话中跳过浏览器启动");
        }
    }
    supervise_server(server, tokio::signal::ctrl_c()).await
}

async fn supervise_server<F>(
    mut server: web::BoundWebServer,
    shutdown: F,
) -> Result<std::net::SocketAddr>
where
    F: Future<Output = std::io::Result<()>>,
{
    enum StopReason {
        Shutdown(std::io::Result<()>),
        Server(Result<()>),
    }

    let addr = server.addr();
    let stop_reason = tokio::select! {
        signal = shutdown => StopReason::Shutdown(signal),
        result = server.wait() => StopReason::Server(result),
    };

    match stop_reason {
        StopReason::Shutdown(result) => {
            result.context("Failed to wait for Ctrl+C")?;
            info!(%addr, "收到 Ctrl+C，准备停止本地 Web UI 服务");
            server.shutdown().await?;
            Ok(addr)
        }
        StopReason::Server(result) => {
            result?;
            bail!("Web server at {addr} stopped before a shutdown signal");
        }
    }
}

fn local_dashboard_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn public_dashboard_url(port: u16) -> String {
    format!("http://<server-host-or-ip>:{port}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserOpenDecision {
    Open,
    Disabled,
    SshSession,
}

fn browser_open_decision(no_open: bool, ssh_session: bool) -> BrowserOpenDecision {
    if no_open {
        BrowserOpenDecision::Disabled
    } else if ssh_session {
        BrowserOpenDecision::SshSession
    } else {
        BrowserOpenDecision::Open
    }
}

fn is_ssh_session() -> bool {
    let connection = std::env::var_os("SSH_CONNECTION");
    let tty = std::env::var_os("SSH_TTY");
    is_ssh_session_from(connection.as_deref(), tty.as_deref())
}

fn is_ssh_session_from(
    connection: Option<&std::ffi::OsStr>,
    tty: Option<&std::ffi::OsStr>,
) -> bool {
    connection.is_some_and(|value| !value.is_empty()) || tty.is_some_and(|value| !value.is_empty())
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
    use std::{ffi::OsStr, future::pending};

    use tempfile::TempDir;

    use crate::{AppPaths, store::Store, web::BoundWebServer};

    use super::{
        BrowserLaunchPlan, BrowserOpenDecision, BrowserPlatform, browser_open_decision,
        is_ssh_session_from, local_dashboard_url, public_dashboard_url, supervise_server,
    };

    #[test]
    fn browser_open_decision_skips_no_open_and_ssh_sessions() {
        assert_eq!(
            browser_open_decision(false, false),
            BrowserOpenDecision::Open
        );
        assert_eq!(
            browser_open_decision(true, false),
            BrowserOpenDecision::Disabled
        );
        assert_eq!(
            browser_open_decision(false, true),
            BrowserOpenDecision::SshSession
        );
        assert_eq!(
            browser_open_decision(true, true),
            BrowserOpenDecision::Disabled
        );
    }

    #[test]
    fn ssh_session_detection_accepts_connection_or_tty() {
        assert!(is_ssh_session_from(
            Some(OsStr::new("192.0.2.4 22 203.0.113.8 49152")),
            None
        ));
        assert!(is_ssh_session_from(None, Some(OsStr::new("/dev/pts/0"))));
        assert!(!is_ssh_session_from(
            Some(OsStr::new("")),
            Some(OsStr::new(""))
        ));
    }

    #[test]
    fn public_listener_uses_loopback_for_browser_and_a_remote_placeholder() {
        assert_eq!(local_dashboard_url(37421), "http://127.0.0.1:37421");
        assert_eq!(
            public_dashboard_url(37421),
            "http://<server-host-or-ip>:37421"
        );
    }

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

    fn make_store() -> anyhow::Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    #[tokio::test]
    async fn supervisor_rejects_early_server_completion_and_panic() {
        let completed = BoundWebServer::from_test_task(tokio::spawn(async { Ok(()) }));
        let err = supervise_server(completed, pending())
            .await
            .expect_err("early completion must fail serve");
        assert!(format!("{err:#}").contains("stopped before a shutdown signal"));

        let panicked: tokio::task::JoinHandle<std::io::Result<()>> =
            tokio::spawn(async { panic!("serve task panic") });
        let err = supervise_server(BoundWebServer::from_test_task(panicked), pending())
            .await
            .expect_err("server panic must fail serve");
        assert!(format!("{err:#}").contains("panicked"));
    }

    #[tokio::test]
    async fn tracked_serve_stays_running_until_clean_shutdown() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let server = crate::web::bind_server(
            store.clone(),
            Some(0),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .await?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let tracked_store = store.clone();
        let task = tokio::spawn(async move {
            super::super::run_tracked(
                &tracked_store,
                "serve",
                async {
                    let _ = started_tx.send(());
                    supervise_server(server, async {
                        shutdown_rx
                            .await
                            .map_err(|_| std::io::Error::other("shutdown sender dropped"))
                    })
                    .await
                },
                |addr| Some(format!("listen={addr}")),
            )
            .await
        });

        started_rx.await.expect("tracked body started");
        let running = store.run_log().recent_runs(1)?;
        assert_eq!(running[0].status, "running");

        shutdown_tx.send(()).expect("shutdown signal");
        task.await??;
        let finished = store.run_log().recent_runs(1)?;
        assert_eq!(finished[0].status, "success");
        assert!(
            finished[0]
                .summary
                .as_deref()
                .is_some_and(|value| value.contains("listen="))
        );
        Ok(())
    }

    #[tokio::test]
    async fn tracked_bind_and_server_failures_finish_as_failed() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let occupied = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let port = occupied.local_addr()?.port();
        let bind_result = super::super::run_tracked(
            &store,
            "serve",
            async {
                crate::web::bind_server(
                    store.clone(),
                    Some(port),
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .await
                .map(|server| server.addr())
            },
            |addr| Some(format!("listen={addr}")),
        )
        .await;
        assert!(bind_result.is_err());
        assert_eq!(store.run_log().recent_runs(1)?[0].status, "failed");

        let failed_task = tokio::spawn(async { Err(std::io::Error::other("server failed")) });
        let server_result = super::super::run_tracked(
            &store,
            "serve",
            supervise_server(BoundWebServer::from_test_task(failed_task), pending()),
            |addr| Some(format!("listen={addr}")),
        )
        .await;
        assert!(server_result.is_err());
        let latest = store.run_log().recent_runs(1)?;
        assert_eq!(latest[0].status, "failed");
        assert!(
            latest[0]
                .error
                .as_deref()
                .is_some_and(|value| value.contains("server failed"))
        );
        Ok(())
    }

    #[test]
    fn stale_serve_run_is_recovered_as_aborted() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        store.run_log().record_run_start("serve")?;
        assert_eq!(store.run_log().recover_running_runs(&["serve"])?, 1);
        let recovered = store.run_log().recent_runs(1)?;
        assert_eq!(recovered[0].status, "aborted");
        Ok(())
    }
}
