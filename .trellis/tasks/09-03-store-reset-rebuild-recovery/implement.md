# Implement: reset/rebuild/catalog recovery

1. Delete source_file in `reset_usage_data`; fix tests.
2. Batch rebuild resets in one transaction; failpoint on second source.
3. Recovery reads in-progress catalog identity; add crash-window test (files on disk, meta old, marker set).
4. Run store + sync tests.

Risky files: `store/schema.rs`, `sync/engine.rs`, `store/pricing_catalog.rs`.
