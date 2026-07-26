use llmusage::{
    AppPaths, Dashboard, JobRegistry, JobStatus, QueryFilter, ReportTimezone, Result, SourceKind,
    Store, SyncOptions,
};
use std::process::Command;

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

#[test]
fn cli_sync_uses_shared_stable_validation_codes() {
    let cases: [(&[&str], &str); 3] = [
        (&["sync", "--source", "not-a-source"], "unknown_source"),
        (&["sync", "--recent-days", "0"], "invalid_recent_days"),
        (&["sync", "--parallelism", "0"], "invalid_parallelism"),
    ];
    for (args, code) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_llmusage"))
            .args(args)
            .output()
            .expect("run llmusage");
        assert!(!output.status.success(), "{args:?} must fail");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(code), "stderr={stderr:?}");
    }
}
