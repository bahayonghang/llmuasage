use std::{
    fs,
    net::TcpListener,
    time::{Duration, Instant},
};

use llmusage::{
    Dashboard, Fixture, HolderKind, QueryFilter, SourceKind, Store, app::AppContext,
    subscription::UsageEndpoints,
};
use llmusage_desktop_lib::{
    FilterDto, InteractiveRequest, SyncStartDto, convert_filter, run_query, startup,
};
use serde_json::Value;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

fn event_count(store: &Store) -> anyhow::Result<i64> {
    let conn = store.open_connection()?;
    Ok(conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?)
}

fn strip_generated_at(value: Value) -> Value {
    fn walk(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .filter(|(key, _)| key != "generated_at")
                    .map(|(key, child)| (key, walk(child)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(walk).collect()),
            other => other,
        }
    }
    walk(value)
}

async fn wait_inflight_zero(state: &llmusage_desktop_lib::AppState) {
    let started = Instant::now();
    loop {
        if state.supervisor.snapshot().inflight == 0 {
            return;
        }
        if started.elapsed() > Duration::from_secs(5) {
            panic!(
                "supervisor inflight did not settle: {:?}",
                state.supervisor.snapshot()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac1_empty_temp_root_startup_and_independent_crate() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let app = AppContext::with_cli_home(Some(temp.path().to_path_buf()))?;
    let state = startup(app).await?;
    let snapshot = llmusage_desktop_lib::commands::query::dashboard_interactive(
        &state,
        InteractiveRequest {
            request_id: 1,
            filter: FilterDto::default(),
            window: "all".to_string(),
        },
    )
    .await?;
    let json = serde_json::to_value(&snapshot)?;
    assert!(json.get("overview").is_some());
    assert!(json.get("models").is_some());
    assert_eq!(state.paths.root_dir, temp.path());

    let desktop_manifest = fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )?;
    assert!(
        desktop_manifest.contains("path = \"../..\""),
        "desktop crate must path-depend on the root crate"
    );
    let root_manifest = fs::read_to_string(repo_root().join("Cargo.toml"))?;
    assert!(
        !root_manifest.contains("desktop/src-tauri"),
        "root Cargo.toml must not list desktop as a workspace member"
    );
    assert!(
        !root_manifest.contains("[workspace]"),
        "root crate stays a single package, not a workspace that includes desktop"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac2_fixture_snapshot_matches_interactive_snapshot() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(12)?;
    suppress_startup_repair(fixture.store())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let filter_dto = FilterDto::default();
    let filter = convert_filter(&filter_dto)?;
    let got = llmusage_desktop_lib::commands::query::dashboard_interactive(
        &state,
        InteractiveRequest {
            request_id: 2,
            filter: filter_dto,
            window: "all".to_string(),
        },
    )
    .await?;
    let expected = Dashboard::open(fixture.store())?.interactive_snapshot(&filter, "all")?;
    assert_eq!(
        strip_generated_at(serde_json::to_value(&got)?),
        strip_generated_at(serde_json::to_value(&expected)?)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac3_lock_busy_and_runtime_info() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(8)?;
    suppress_startup_repair(fixture.store())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let other = Store::new(&state.paths)?;
    let lock = other.acquire_worker_lock_with(Duration::from_secs(1), HolderKind::Cli)?;
    let before = event_count(&state.store)?;
    let error = llmusage_desktop_lib::commands::jobs::start_sync(
        &state,
        SyncStartDto {
            source: Some("codex".to_string()),
            recent_days: Some(7),
        },
    )
    .expect_err("held lock must map to lock_busy");
    assert_eq!(error.code, "lock_busy");
    assert!(error.holder.is_some());
    assert_eq!(event_count(&state.store)?, before);
    let info = llmusage_desktop_lib::commands::runtime::runtime_info(&state)?;
    assert_eq!(info.root_dir, state.paths.root_dir);
    assert!(
        info.lock.is_some(),
        "runtime_info must include lock summary"
    );
    drop(lock);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac4_schema_too_new_does_not_migrate() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let future = llmusage::store::latest_schema_version() + 1;
    fixture
        .store()
        .set_meta_value("schema_version", &future.to_string())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let error = match startup(app).await {
        Err(error) => error,
        Ok(_) => panic!("future schema must map to schema_too_new"),
    };
    assert_eq!(error.code, "schema_too_new");
    assert_eq!(
        fixture.store().meta_value("schema_version")?.as_deref(),
        Some(future.to_string().as_str())
    );
    Ok(())
}

#[test]
fn ac5_root_src_and_ci_gate_untouched() -> anyhow::Result<()> {
    let root = repo_root();
    let query = fs::read_to_string(root.join("src/query/mod.rs"))?;
    assert!(query.contains("pub fn interrupt_handle"));
    assert!(!query.contains("pub(crate) fn interrupt_handle"));
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml"))?;
    assert!(ci.contains("name: CI gate"));
    let serve = fs::read_to_string(root.join("src/commands/serve.rs"))?;
    assert!(serve.contains("pub async fn run("));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac6_cancel_first_request_second_completes() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(8)?;
    suppress_startup_repair(fixture.store())?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let first_state = state.clone();
    let first = tokio::spawn(async move {
        run_query(&first_state, 11, Duration::from_secs(30), |_dashboard| {
            std::thread::sleep(Duration::from_millis(800));
            Ok::<_, llmusage::LlmusageError>(serde_json::json!({ "ok": true }))
        })
        .await
    });
    let started = Instant::now();
    loop {
        if state.supervisor.snapshot().inflight >= 1 {
            break;
        }
        if started.elapsed() > Duration::from_secs(2) {
            panic!("first query never registered inflight");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    llmusage_desktop_lib::commands::runtime::cancel_queries(
        &state,
        llmusage_desktop_lib::CancelQueriesDto {
            request_ids: vec![11],
        },
    );
    let first_result = first.await?;
    let first_error = first_result.expect_err("cancelled request must fail");
    assert!(
        first_error.code == "cancelled" || first_error.code == "timeout",
        "first request code={}",
        first_error.code
    );
    run_query(&state, 12, Duration::from_secs(5), |dashboard| {
        dashboard.home_overview(&QueryFilter::default())
    })
    .await
    .expect("second request must complete");
    wait_inflight_zero(&state).await;
    assert_eq!(state.supervisor.snapshot().inflight, 0);
    Ok(())
}

#[test]
fn ac7_convert_sync_rebuild_stays_false() {
    let options = llmusage_desktop_lib::convert_sync(&SyncStartDto {
        source: Some("codex".to_string()),
        recent_days: Some(7),
    })
    .expect("valid sync dto");
    let validated = llmusage::ValidatedSyncRequest::new(options.clone()).expect("validated");
    assert!(!options.rebuild);
    assert!(!validated.rebuild());
    assert_eq!(validated.recent_days(), Some(7));
    let error = llmusage_desktop_lib::convert_sync(&SyncStartDto {
        source: None,
        recent_days: Some(0),
    })
    .expect_err("recent_days 0");
    assert_eq!(error.code, "invalid_request");
    let dto: SyncStartDto = serde_json::from_value(serde_json::json!({
        "source": "codex",
        "recent_days": 7,
        "rebuild": true
    }))
    .expect("raw json");
    let options = llmusage_desktop_lib::convert_sync(&dto).expect("ignore rebuild field");
    assert!(!options.rebuild);
}

#[test]
fn ac8_single_instance_plugin_configured() -> anyhow::Result<()> {
    let conf_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let conf: Value = serde_json::from_str(&fs::read_to_string(conf_path)?)?;
    assert_eq!(conf["identifier"], "com.bahayonghang.llmusage");
    assert!(
        conf["plugins"].get("single-instance").is_some(),
        "tauri.conf.json must enable the single-instance plugin"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac9_fetch_quota_injected_local_endpoints_and_cache_hit() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::with_cli_home(Some(fixture.paths().root_dir.clone()))?;
    let state = startup(app).await?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let base = format!("http://127.0.0.1:{}", addr.port());
    let endpoints = UsageEndpoints {
        claude_usage: format!("{base}/oauth/usage"),
        kimi_usage: format!("{base}/coding/v1/usages"),
        grok_subscriptions: format!("{base}/rest/subscriptions"),
        grok_task_usage: format!("{base}/rest/tasks/usage"),
        codex_usage: format!("{base}/backend-api/wham/usage"),
    };
    assert!(endpoints.claude_usage.starts_with("http://127.0.0.1"));
    let user_home = tempfile::TempDir::new()?;
    let inject = llmusage_desktop_lib::commands::runtime::QuotaInject {
        user_home: Some(user_home.path().to_path_buf()),
        endpoints: Some(endpoints),
    };
    let first =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, true, inject.clone()).await?;
    assert!(!first.cache_hit);
    let second =
        llmusage_desktop_lib::commands::runtime::fetch_quota(&state, false, inject).await?;
    assert!(
        second.cache_hit,
        "valid cache and bypass_cache=false must report cache_hit"
    );
    drop(listener);
    Ok(())
}
