use std::time::Duration;

use rusqlite::Connection;

use super::Store;
use crate::{error::Result, paths::AppPaths};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static OPEN_CONNECTION_CALLS: AtomicUsize = AtomicUsize::new(0);

impl Store {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        Ok(Self {
            paths: paths.clone(),
        })
    }

    pub fn open_connection(&self) -> Result<Connection> {
        self.open_connection_with_busy_timeout(Duration::from_secs(30))
    }

    /// Opens a connection with a caller-chosen `busy_timeout`.
    ///
    /// The default [`Store::open_connection`] keeps a 30s busy timeout for
    /// sync writers and migrations. Web/API read paths use a much shorter
    /// timeout so lock waits fall into the existing dashboard timeout /
    /// degraded-section flow instead of blocking far beyond the request
    /// budget.
    pub fn open_connection_with_busy_timeout(&self, busy_timeout: Duration) -> Result<Connection> {
        #[cfg(test)]
        OPEN_CONNECTION_CALLS.fetch_add(1, Ordering::Relaxed);

        let conn = Connection::open(&self.paths.db_path)?;
        conn.busy_timeout(busy_timeout)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;
        Ok(conn)
    }

    #[cfg(test)]
    pub(crate) fn reset_open_connection_counter() {
        OPEN_CONNECTION_CALLS.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn open_connection_count() -> usize {
        OPEN_CONNECTION_CALLS.load(Ordering::Relaxed)
    }
}
