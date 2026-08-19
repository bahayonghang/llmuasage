//! Read-only subscription quota fetchers for the dash Usage tab.

mod cache;
mod claude;
mod codex;
mod grok;
mod http;
mod kimi;
mod types;

use std::path::PathBuf;
use std::time::Duration;

pub use types::{
    UsageAccount, UsageFetchDiagnostic, UsageFetchDiagnosticSeverity, UsageFetchReport,
    UsageMetric, UsageOutput, UsageReadiness, capitalize, output_score, readiness_status,
};

#[derive(Debug, Clone)]
pub struct UsageEndpoints {
    pub claude_usage: String,
    pub kimi_usage: String,
    pub grok_subscriptions: String,
    pub grok_task_usage: String,
    pub codex_usage: String,
}

impl UsageEndpoints {
    pub fn production() -> Self {
        Self {
            claude_usage: "https://api.anthropic.com/api/oauth/usage".into(),
            kimi_usage: "https://api.kimi.com/coding/v1/usages".into(),
            grok_subscriptions: "https://grok.com/rest/subscriptions".into(),
            grok_task_usage: "https://grok.com/rest/tasks/usage".into(),
            codex_usage: "https://chatgpt.com/backend-api/wham/usage".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchContext {
    pub endpoints: UsageEndpoints,
    pub user_home: PathBuf,
    pub cache_path: Option<PathBuf>,
    pub timeout: Duration,
}

impl FetchContext {
    pub fn production(user_home: PathBuf, cache_path: PathBuf) -> Self {
        Self {
            endpoints: UsageEndpoints::production(),
            user_home,
            cache_path: Some(cache_path),
            timeout: Duration::from_secs(8),
        }
    }
}

pub async fn fetch_all(ctx: &FetchContext, bypass_cache: bool) -> UsageFetchReport {
    if !bypass_cache
        && let Some(path) = &ctx.cache_path
        && let Some(report) = cache::load(path)
    {
        return report;
    }

    let report = fetch_live(ctx).await;
    if let Some(path) = &ctx.cache_path {
        cache::save(path, &report);
    }
    report
}

async fn fetch_live(ctx: &FetchContext) -> UsageFetchReport {
    let home = ctx.user_home.clone();
    let endpoints = ctx.endpoints.clone();
    let timeout = ctx.timeout;

    let claude = maybe_fetch("Claude", claude::has_credentials(&home), {
        let home = home.clone();
        let url = endpoints.claude_usage.clone();
        async move { claude::fetch(&home, &url, timeout).await }
    });
    let kimi = maybe_fetch("Kimi", kimi::has_credentials(&home), {
        let home = home.clone();
        let url = endpoints.kimi_usage.clone();
        async move { kimi::fetch(&home, &url, timeout).await }
    });
    let grok = maybe_fetch("Grok Build", grok::has_credentials(&home), {
        let home = home.clone();
        let subscriptions = endpoints.grok_subscriptions.clone();
        let tasks = endpoints.grok_task_usage.clone();
        async move { grok::fetch(&home, &subscriptions, &tasks, timeout).await }
    });
    let codex = maybe_fetch("Codex", codex::has_credentials(&home), {
        let home = home.clone();
        let url = endpoints.codex_usage.clone();
        async move { codex::fetch(&home, &url, timeout).await }
    });

    let (claude, kimi, grok, codex) = tokio::join!(claude, kimi, grok, codex);
    let mut report = UsageFetchReport::default();
    for part in [claude, kimi, grok, codex] {
        report.extend(part);
    }
    report
}

async fn maybe_fetch<Fut>(provider: &'static str, enabled: bool, fetch: Fut) -> UsageFetchReport
where
    Fut: std::future::Future<Output = anyhow::Result<UsageOutput>>,
{
    if !enabled {
        return UsageFetchReport::default();
    }
    match fetch.await {
        Ok(output) => UsageFetchReport {
            outputs: vec![output],
            diagnostics: Vec::new(),
        },
        Err(error) => UsageFetchReport {
            outputs: Vec::new(),
            diagnostics: vec![UsageFetchDiagnostic::error(provider, error.to_string())],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use serde_json::json;
    use std::net::SocketAddr;
    use std::path::Path;
    use std::sync::Arc;
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct Fixture {
        claude: StatusCode,
    }

    async fn claude_handler(State(fixture): State<Arc<Fixture>>) -> impl IntoResponse {
        (
            fixture.claude,
            Json(json!({
                "five_hour": { "utilization": 10.0, "resets_at": "2026-08-19T18:00:00Z" },
                "seven_day": { "utilization": 30.0, "resets_at": "2026-08-24T00:18:00Z" }
            })),
        )
    }

    async fn kimi_handler() -> impl IntoResponse {
        Json(json!({
            "user": { "membership": { "level": "LEVEL_INTERMED" } },
            "usage": { "limit": "100", "remaining": "34", "resetTime": "2026-08-19T12:04:00Z" }
        }))
    }

    async fn grok_subs() -> impl IntoResponse {
        Json(json!({
            "subscriptions": [{ "status": "active", "tier": "SUBSCRIPTION_TIER_UNKNOWN" }]
        }))
    }

    async fn grok_tasks() -> impl IntoResponse {
        Json(json!({
            "usage": 30.0,
            "limit": 100.0,
            "resetTime": "2026-08-24T00:18:00Z"
        }))
    }

    async fn codex_handler() -> impl IntoResponse {
        Json(json!({
            "email": "user@example.com",
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": { "used_percent": 40.0, "limit_window_seconds": 18000, "reset_at": 1780000000 },
                "secondary_window": { "used_percent": 20.0, "limit_window_seconds": 604800, "reset_at": 1780000000 }
            }
        }))
    }

    async fn spawn_server(claude_status: StatusCode) -> (String, tokio::task::JoinHandle<()>) {
        let fixture = Arc::new(Fixture {
            claude: claude_status,
        });
        let app = axum::Router::new()
            .route("/oauth/usage", get(claude_handler))
            .route("/coding/v1/usages", get(kimi_handler))
            .route("/rest/subscriptions", get(grok_subs))
            .route("/rest/tasks/usage", get(grok_tasks))
            .route("/backend-api/wham/usage", get(codex_handler))
            .with_state(fixture);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        (format!("http://{addr}"), handle)
    }

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

    fn write_creds(home: &Path) {
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(
            home.join(".claude").join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"claude-token","subscriptionType":"pro"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".kimi-code").join("credentials")).unwrap();
        std::fs::write(
            home.join(".kimi-code")
                .join("credentials")
                .join("kimi-code.json"),
            r#"{"access_token":"kimi-token"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".grok")).unwrap();
        std::fs::write(
            home.join(".grok").join("auth.json"),
            r#"{"auth.x.ai":{"key":"grok-token","email":"grok@example.com"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("auth.json"),
            r#"{"tokens":{"access_token":"codex-token"}}"#,
        )
        .unwrap();
    }

    fn ctx(home: &Path, base: &str) -> FetchContext {
        FetchContext {
            endpoints: UsageEndpoints {
                claude_usage: format!("{base}/oauth/usage"),
                kimi_usage: format!("{base}/coding/v1/usages"),
                grok_subscriptions: format!("{base}/rest/subscriptions"),
                grok_task_usage: format!("{base}/rest/tasks/usage"),
                codex_usage: format!("{base}/backend-api/wham/usage"),
            },
            user_home: home.to_path_buf(),
            cache_path: Some(home.join("cache.json")),
            timeout: Duration::from_secs(2),
        }
    }

    #[tokio::test]
    async fn fetch_all_reads_four_providers_without_writing_credentials() {
        let _lock = ENV_LOCK.lock().await;
        let _env = isolate_credential_env();
        let dir = tempfile::tempdir().unwrap();
        write_creds(dir.path());
        let claude_before = std::fs::read(dir.path().join(".claude/.credentials.json")).unwrap();
        let kimi_before =
            std::fs::read(dir.path().join(".kimi-code/credentials/kimi-code.json")).unwrap();
        let (base, server) = spawn_server(StatusCode::OK).await;
        let report = fetch_all(&ctx(dir.path(), &base), true).await;
        server.abort();

        assert_eq!(report.outputs.len(), 4, "{report:?}");
        assert!(report.diagnostics.is_empty(), "{report:?}");
        assert!(
            report
                .outputs
                .iter()
                .any(|output| output.provider == "Claude")
        );
        assert!(
            report
                .outputs
                .iter()
                .any(|output| output.provider == "Kimi")
        );
        assert!(
            report
                .outputs
                .iter()
                .any(|output| output.provider == "Grok Build")
        );
        assert!(
            report
                .outputs
                .iter()
                .any(|output| output.provider == "Codex")
        );
        assert_eq!(
            std::fs::read(dir.path().join(".claude/.credentials.json")).unwrap(),
            claude_before
        );
        assert_eq!(
            std::fs::read(dir.path().join(".kimi-code/credentials/kimi-code.json")).unwrap(),
            kimi_before
        );
    }

    #[tokio::test]
    async fn one_provider_failure_keeps_other_rows() {
        let _lock = ENV_LOCK.lock().await;
        let _env = isolate_credential_env();
        let dir = tempfile::tempdir().unwrap();
        write_creds(dir.path());
        let claude_before = std::fs::read(dir.path().join(".claude/.credentials.json")).unwrap();
        let (base, server) = spawn_server(StatusCode::TOO_MANY_REQUESTS).await;
        let report = fetch_all(&ctx(dir.path(), &base), true).await;
        server.abort();

        assert_eq!(report.outputs.len(), 3, "{report:?}");
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].provider, "Claude");
        assert!(report.diagnostics[0].message.contains("429"));
        assert_eq!(
            std::fs::read(dir.path().join(".claude/.credentials.json")).unwrap(),
            claude_before
        );
    }

    #[tokio::test]
    async fn missing_credentials_are_skipped() {
        let _lock = ENV_LOCK.lock().await;
        let _env = isolate_credential_env();
        let dir = tempfile::tempdir().unwrap();
        let (base, server) = spawn_server(StatusCode::OK).await;
        let report = fetch_all(&ctx(dir.path(), &base), true).await;
        server.abort();
        assert!(report.outputs.is_empty());
        assert!(report.diagnostics.is_empty());
    }
}
