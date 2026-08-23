use super::super::*;

#[test]
fn usage_overlay_renders_sync_status_and_platform_monitor_summary() {
    let text = render_sync_status_text(sample_sync_payload(), sample_platform_probes(), 120, 18);

    for expected in [
        "Usage / Sync",
        "Ready to sync",
        "Source Sync",
        "Skipped",
        "codex",
        "8,000",
        "Issues",
        "malformed=2",
        "skipped=4",
        "accounting=1",
        "Platform Monitor",
        "Gemini CLI",
        "blocked-no-samples",
    ] {
        assert!(
            text.contains(expected),
            "sync overlay should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn usage_overlay_uses_compact_columns_on_narrow_widths() {
    let text = render_sync_status_text(sample_sync_payload(), sample_platform_probes(), 52, 12);

    for expected in ["Usage / Sync", "Stored", "codex", "8,000"] {
        assert!(
            text.contains(expected),
            "narrow sync overlay should contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Inserted"),
        "narrow sync overlay should hide wide-only columns: {text}"
    );
}

#[test]
fn usage_overlay_keeps_rebuild_protection_facts_neutral() {
    let mut payload = sample_sync_payload();
    payload.safety.lossy_rebuild_risk = true;
    payload.safety.risk_sources = vec!["claude".to_string()];
    payload.safety.risk_details = vec![llmusage::query::SyncRiskSourcePayload {
        source: "claude".to_string(),
        missing_file_count: 728,
        protected_event_count: 36_495,
    }];
    payload.sources[0].lossy_rebuild_risk = true;
    let text = render_sync_status_text(payload, sample_platform_probes(), 120, 18);

    assert!(text.contains("Ready to sync"));
    assert!(text.contains("rebuild-risk facts"));
    assert!(text.contains("missing=728"));
    assert!(text.contains("protected=36495"));
    assert!(!text.contains("Rebuild risk"));
}

#[test]
fn usage_panel_renders_quota_accounts_and_hides_emails() {
    let text = render_usage_quota_text(sample_quota_report(), true, 140, 32);
    for expected in [
        "Usage",
        "Usage Summary",
        "Accounts",
        "Grok Build",
        "Weekly",
        "70% left",
        "Diagnostics",
        "HTTP 429",
        "[hidden email]",
        "Selected Account",
    ] {
        assert!(
            text.contains(expected),
            "usage quota panel should contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("user@example.com"),
        "hidden emails must not appear: {text}"
    );
    assert!(
        !text.contains("Source Sync"),
        "main usage area must not show Source Sync: {text}"
    );
}

#[test]
fn usage_panel_can_reveal_emails() {
    let text = render_usage_quota_text(sample_quota_report(), false, 140, 32);
    assert!(
        text.contains("user@example.com"),
        "revealed email missing: {text}"
    );
}

#[test]
fn usage_panel_empty_state() {
    let text = render_usage_quota_text(
        llmusage::subscription::UsageFetchReport::default(),
        true,
        80,
        16,
    );
    assert!(
        text.contains("No subscription data available"),
        "empty quota panel missing empty-state copy: {text}"
    );
}
