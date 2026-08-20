use std::path::PathBuf;

use super::Store;
use crate::{error::Result, paths::AppPaths};

impl Store {
    /// Parser-facing store for `sync --emit-shards`.
    ///
    /// Cursor reads return empty so every candidate file is parsed. Inventory
    /// and cursor writes are no-ops and never acquire the worker lock. The
    /// store must not open the user `db_path`.
    pub fn new_emit_only() -> Result<Self> {
        Ok(Self {
            paths: AppPaths::with_root(PathBuf::from("llmusage-emit-only-unopened"))?,
            write_permit: None,
            emit_only: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SourceKind;
    use crate::store::{OpencodeCursor, ZcodeCursor};

    #[test]
    fn emit_only_store_returns_empty_cursors_and_skips_writes() -> anyhow::Result<()> {
        crate::store::Store::reset_open_connection_counter();
        let store = Store::new_emit_only()?;
        assert!(store.emit_only());
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Codex, "local")?
                .is_empty()
        );
        store
            .cursors()
            .save_opencode_cursor("local", &OpencodeCursor::default())?;
        store
            .cursors()
            .save_zcode_cursor("local", &ZcodeCursor::default())?;
        store.source_files().mark_inventory_seen(
            SourceKind::Codex,
            "local",
            &["/tmp/session.jsonl".to_string()],
            "2026-08-20T00:00:00Z",
        )?;
        assert_eq!(
            store.source_files().sweep_missing(
                SourceKind::Codex,
                "local",
                "2026-08-20T00:00:00Z"
            )?,
            0
        );
        assert!(!store.raw_archive_enabled()?);
        assert_eq!(Store::open_connection_count(), 0);
        assert!(
            store
                .acquire_worker_lock_with(
                    std::time::Duration::from_secs(1),
                    crate::store::HolderKind::Cli,
                )
                .is_err(),
            "emit-only store must not take the worker lock"
        );
        Ok(())
    }
}
