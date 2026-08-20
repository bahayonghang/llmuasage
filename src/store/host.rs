//! Host registry for local and remote import targets.
//!
//! C1 creates the table and the local row. Remote registration and SSH
//! transport live in later tasks; this module still exposes the write APIs
//! those tasks will call so Store does not grow a second host façade.

use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::error::Result;
use crate::util::now_utc;

/// Stable host_id written by v23 for this machine's own usage rows.
pub const LOCAL_HOST_ID: &str = "local";

/// One row from the `host` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// Internal stable identifier; also used as the event_key prefix.
    pub host_id: String,
    /// User-visible name used by `--host` and status output.
    pub label: String,
    /// `local` or `ssh`.
    pub transport: String,
    /// SSH target for `transport='ssh'` rows.
    pub ssh_target: Option<String>,
    /// Remote command invoked over SSH. Defaults to `llmusage`.
    pub command: String,
    /// RFC 3339 timestamp when the row was inserted.
    pub added_at: String,
    /// RFC 3339 timestamp of the last successful contact, if any.
    pub last_contacted_at: Option<String>,
    /// Last import/contact error. Empty/NULL means the last contact succeeded.
    pub last_error: Option<String>,
    /// Local incremental import watermark (`event_at` of the last committed trailer).
    pub import_watermark: Option<String>,
}

/// Borrowed view onto the `host` surface of [`Store`].
pub struct HostStore<'a> {
    store: &'a Store,
}

impl<'a> HostStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Returns every registered host, ordered by label.
    pub fn list(&self) -> Result<Vec<Host>> {
        let conn = self.store.open_connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                host_id, label, transport, ssh_target, command, added_at,
                last_contacted_at, last_error, import_watermark
            FROM host
            ORDER BY label ASC
            "#,
        )?;
        let rows = stmt.query_map([], map_host_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Looks up one host by its user-visible label.
    pub fn get_by_label(&self, label: &str) -> Result<Option<Host>> {
        let conn = self.store.open_connection()?;
        let row = conn
            .query_row(
                r#"
                SELECT
                    host_id, label, transport, ssh_target, command, added_at,
                    last_contacted_at, last_error, import_watermark
                FROM host
                WHERE label = ?1
                "#,
                [label],
                map_host_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Inserts or updates a host row by `host_id`. `added_at` is kept on conflict.
    pub fn upsert(&self, host: &Host) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
                INSERT INTO host(
                    host_id, label, transport, ssh_target, command, added_at,
                    last_contacted_at, last_error, import_watermark
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(host_id) DO UPDATE SET
                    label = excluded.label,
                    transport = excluded.transport,
                    ssh_target = excluded.ssh_target,
                    command = excluded.command,
                    last_contacted_at = excluded.last_contacted_at,
                    last_error = excluded.last_error,
                    import_watermark = excluded.import_watermark
                "#,
                params![
                    host.host_id,
                    host.label,
                    host.transport,
                    host.ssh_target,
                    host.command,
                    host.added_at,
                    host.last_contacted_at,
                    host.last_error,
                    host.import_watermark,
                ],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Deletes a host row. Usage rows for that host are left in place.
    pub fn remove(&self, host_id: &str) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute("DELETE FROM host WHERE host_id = ?1", [host_id])?;
            Ok(())
        })?;
        Ok(())
    }

    /// Persists the local import watermark for one host.
    pub fn set_watermark(&self, host_id: &str, watermark: Option<&str>) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute(
                "UPDATE host SET import_watermark = ?2 WHERE host_id = ?1",
                params![host_id, watermark],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Records a contact attempt. `last_error = None` means the attempt succeeded.
    pub fn record_contact(&self, host_id: &str, last_error: Option<&str>) -> Result<()> {
        let contacted_at = now_utc();
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
                UPDATE host
                SET last_contacted_at = ?2,
                    last_error = ?3
                WHERE host_id = ?1
                "#,
                params![host_id, contacted_at, last_error],
            )?;
            Ok(())
        })?;
        Ok(())
    }
}

fn map_host_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Host> {
    Ok(Host {
        host_id: row.get(0)?,
        label: row.get(1)?,
        transport: row.get(2)?,
        ssh_target: row.get(3)?,
        command: row.get(4)?,
        added_at: row.get(5)?,
        last_contacted_at: row.get(6)?,
        last_error: row.get(7)?,
        import_watermark: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::paths::AppPaths;
    use crate::store::Store;

    #[test]
    fn bootstrap_seeds_local_host_and_host_store_round_trips() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let listed = store.hosts().list()?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].host_id, LOCAL_HOST_ID);
        assert_eq!(listed[0].label, LOCAL_HOST_ID);
        assert_eq!(listed[0].transport, "local");
        assert_eq!(listed[0].command, "llmusage");

        let local = store.hosts().get_by_label(LOCAL_HOST_ID)?;
        assert_eq!(
            local.as_ref().map(|host| host.host_id.as_str()),
            Some(LOCAL_HOST_ID)
        );

        store.hosts().upsert(&Host {
            host_id: "devbox".to_string(),
            label: "devbox".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("me@devbox".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: None,
        })?;
        store
            .hosts()
            .record_contact("devbox", Some("ssh timed out"))?;
        store
            .hosts()
            .set_watermark("devbox", Some("2026-08-20T01:00:00Z"))?;

        let remote = store.hosts().get_by_label("devbox")?.expect("devbox row");
        assert_eq!(remote.transport, "ssh");
        assert_eq!(remote.last_error.as_deref(), Some("ssh timed out"));
        assert_eq!(
            remote.import_watermark.as_deref(),
            Some("2026-08-20T01:00:00Z")
        );
        assert!(remote.last_contacted_at.is_some());

        store.hosts().remove("devbox")?;
        assert!(store.hosts().get_by_label("devbox")?.is_none());
        assert_eq!(store.hosts().list()?.len(), 1);
        Ok(())
    }
}
