use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use llmusage::{Fixture, SourceKind, Store, app::AppContext, subscription::UsageEndpoints};
use llmusage_desktop_lib::startup;

fn suppress_startup_repair(store: &Store) -> anyhow::Result<()> {
    for source in [
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
        store.mark_current_token_accounting(source)?;
    }
    Ok(())
}

fn assert_local_only(endpoints: &UsageEndpoints) {
    for url in [
        &endpoints.claude_usage,
        &endpoints.kimi_usage,
        &endpoints.grok_subscriptions,
        &endpoints.grok_task_usage,
        &endpoints.codex_usage,
    ] {
        assert!(
            url.starts_with("http://127.0.0.1:"),
            "quota tests must use a local listener, got {url}"
        );
        for host in [
            "api.anthropic.com",
            "anthropic.com",
            "grok.com",
            "kimi.com",
            "chatgpt.com",
        ] {
            assert!(
                !url.contains(host),
                "quota tests must not use {host}: {url}"
            );
        }
    }
}

fn local_endpoints(base: &str) -> UsageEndpoints {
    UsageEndpoints {
        claude_usage: format!("{base}/oauth/usage"),
        kimi_usage: format!("{base}/coding/v1/usages"),
        grok_subscriptions: format!("{base}/rest/subscriptions"),
        grok_task_usage: format!("{base}/rest/tasks/usage"),
        codex_usage: format!("{base}/backend-api/wham/usage"),
    }
}

fn write_http(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn handle_conn(mut stream: TcpStream) {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 16_384 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let request = String::from_utf8_lossy(&buf);
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let body = if path.contains("/oauth/usage") {
        r#"{"five_hour":{"utilization":10.0,"resets_at":"2026-08-19T18:00:00Z"},"seven_day":{"utilization":30.0,"resets_at":"2026-08-24T00:18:00Z"}}"#
    } else if path.contains("/coding/v1/usages") {
        r#"{"user":{"membership":{"level":"LEVEL_INTERMED"}},"usage":{"limit":"100","remaining":"34","resetTime":"2026-08-19T12:04:00Z"}}"#
    } else if path.contains("/rest/subscriptions") {
        r#"{"subscriptions":[{"status":"active","tier":"SUBSCRIPTION_TIER_UNKNOWN"}]}"#
    } else if path.contains("/rest/tasks/usage") {
        r#"{"usage":30.0,"limit":100.0,"resetTime":"2026-08-24T00:18:00Z"}"#
    } else if path.contains("/backend-api/wham/usage") {
        r#"{"email":"user@example.com","plan_type":"plus","rate_limit":{"primary_window":{"used_percent":40.0,"limit_window_seconds":18000,"reset_at":1780000000},"secondary_window":{"used_percent":20.0,"limit_window_seconds":604800,"reset_at":1780000000}}}"#
    } else {
        let _ = write_http(&mut stream, "404 Not Found", "{}");
        return;
    };
    let _ = write_http(&mut stream, "200 OK", body);
}

fn spawn_local_quota_server() -> anyhow::Result<(String, thread::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                handle_conn(stream);
            }
        }
    });
    Ok((format!("http://127.0.0.1:{}", addr.port()), handle))
}

fn write_creds(home: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(home.join(".claude"))?;
    std::fs::write(
        home.join(".claude").join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"claude-token","subscriptionType":"pro"}}"#,
    )?;
    std::fs::create_dir_all(home.join(".kimi-code").join("credentials"))?;
    std::fs::write(
        home.join(".kimi-code")
            .join("credentials")
            .join("kimi-code.json"),
        r#"{"access_token":"kimi-token"}"#,
    )?;
    std::fs::create_dir_all(home.join(".grok"))?;
    std::fs::write(
        home.join(".grok").join("auth.json"),
        r#"{"auth.x.ai":{"key":"grok-token","email":"grok@example.com"}}"#,
    )?;
    std::fs::create_dir_all(home.join(".codex"))?;
    std::fs::write(
        home.join(".codex").join("auth.json"),
        r#"{"tokens":{"access_token":"codex-token"}}"#,
    )?;
    Ok(())
}

fn read_cred_bytes(home: &Path) -> anyhow::Result<Vec<(PathBuf, Vec<u8>)>> {
    let paths = [
        home.join(".claude").join(".credentials.json"),
        home.join(".kimi-code")
            .join("credentials")
            .join("kimi-code.json"),
        home.join(".grok").join("auth.json"),
        home.join(".codex").join("auth.json"),
    ];
    paths
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path)?;
            Ok((path, bytes))
        })
        .collect()
}

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn unset(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

fn isolate_credential_env() -> [EnvGuard; 4] {
    [
        EnvGuard::unset("GROK_HOME"),
        EnvGuard::unset("KIMI_CODE_HOME"),
        EnvGuard::unset("CODEX_HOME"),
        EnvGuard::unset("XDG_DATA_HOME"),
    ]
}

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_quota_empty_without_credentials_uses_local_endpoints() -> anyhow::Result<()> {
    let _lock = ENV_LOCK.lock().await;
    let _env = isolate_credential_env();
    let fixture = Fixture::new()?;
    suppress_startup_repair(fixture.store())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let (base, server) = spawn_local_quota_server()?;
    let endpoints = local_endpoints(&base);
    assert_local_only(&endpoints);
    let user_home = tempfile::TempDir::new()?;
    let inject = llmusage_desktop_lib::commands::runtime::QuotaInject {
        user_home: Some(user_home.path().to_path_buf()),
        endpoints: Some(endpoints),
    };
    let first =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, true, inject.clone()).await?;
    assert!(!first.cache_hit);
    assert!(
        first.report.outputs.is_empty(),
        "no credentials should yield empty outputs: {:?}",
        first.report
    );
    let second =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, false, inject).await?;
    assert!(second.cache_hit);
    drop(server);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_quota_fixture_credentials_keep_bytes_and_report_cache_hit() -> anyhow::Result<()> {
    let _lock = ENV_LOCK.lock().await;
    let _env = isolate_credential_env();
    let fixture = Fixture::new()?;
    suppress_startup_repair(fixture.store())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let (base, server) = spawn_local_quota_server()?;
    let endpoints = local_endpoints(&base);
    assert_local_only(&endpoints);
    let user_home = tempfile::TempDir::new()?;
    write_creds(user_home.path())?;
    let before = read_cred_bytes(user_home.path())?;
    let inject = llmusage_desktop_lib::commands::runtime::QuotaInject {
        user_home: Some(user_home.path().to_path_buf()),
        endpoints: Some(endpoints),
    };
    let first =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, true, inject.clone()).await?;
    assert!(!first.cache_hit);
    assert_eq!(first.report.outputs.len(), 4, "{:?}", first.report);
    let after = read_cred_bytes(user_home.path())?;
    assert_eq!(after, before, "credential files must stay byte-identical");
    let cached =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, false, inject.clone()).await?;
    assert!(
        cached.cache_hit,
        "fresh cache and bypass_cache=false must report cache_hit"
    );
    let refreshed =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, true, inject).await?;
    assert!(!refreshed.cache_hit);
    drop(server);
    Ok(())
}
