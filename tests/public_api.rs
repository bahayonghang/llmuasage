use llmusage::{
    AppPaths, Dashboard, JobRegistry, JobStatus, QueryFilter, ReportTimezone, Result, SourceKind,
    Store, SyncOptions,
};
use tempfile::TempDir;

#[test]
fn root_facade_opens_store_and_dashboard() -> Result<()> {
    let temp = TempDir::new().expect("create tempdir");
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;

    let filter = QueryFilter {
        source: Some(SourceKind::Codex),
        timezone: ReportTimezone::Utc,
        ..QueryFilter::default()
    };
    let overview = Dashboard::open(&store)?.overview(&filter)?;

    assert_eq!(overview.total.total_tokens, 0);
    Ok(())
}

#[test]
fn root_facade_exposes_sync_job_types() {
    let registry = JobRegistry::default();
    let options = SyncOptions {
        source: Some(SourceKind::Codex.as_str().to_string()),
        ..SyncOptions::default()
    };
    let status = JobStatus::Running;

    assert!(registry.list_recent(1).is_empty());
    assert_eq!(options.source.as_deref(), Some("codex"));
    assert_eq!(status, JobStatus::Running);
}
