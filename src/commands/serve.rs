use std::process::{Command, Stdio};

use anyhow::{Result, bail};
use tracing::{info, warn};

use crate::{app::AppContext, store::Store, web};

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

    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let run_id = store.run_log().record_run_start("serve")?;
    let addr = web::serve(store.clone(), port).await?;
    store
        .run_log()
        .finish_run(run_id, "success", Some(&format!("listen={addr}")), None)?;

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
                args: vec![
                    "/C".into(),
                    "start".into(),
                    String::new(),
                    url.to_owned(),
                ],
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
        let plan =
            BrowserLaunchPlan::for_platform(BrowserPlatform::Unix, "http://127.0.0.1:37421");
        assert_eq!(plan.program, "xdg-open");
        assert_eq!(plan.args, vec!["http://127.0.0.1:37421".to_string()]);
    }
}
