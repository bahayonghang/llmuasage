use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use tracing::{info, warn};

use super::{
    HolderKind, Store, WORKER_LOCK_LEASE_MINUTES, WORKER_LOCK_NAME, WorkerLock,
    WorkerLockHeartbeat, WorkerLockMeta, WriteOperation, WritePermit,
};
use crate::{
    error::{LlmusageError, Result},
    util::now_utc,
};

impl WorkerLock {
    pub fn refresh(&self) -> Result<()> {
        self.permit.ensure_not_lost()?;
        match self
            .store
            .refresh_worker_lock(&self.lock_name, &self.owner_id, self.generation)
        {
            Err(LlmusageError::LockLost) => {
                self.permit.mark_lost();
                Err(LlmusageError::LockLost)
            }
            result => result,
        }
    }

    /// Returns a store clone whose writes are fenced by this lock generation.
    pub fn fenced_store(&self) -> Store {
        Store {
            paths: self.store.paths.clone(),
            write_permit: Some(self.permit.clone()),
            emit_only: self.store.emit_only,
        }
    }

    /// Starts a background heartbeat that refreshes this lock until the
    /// returned guard is dropped.
    pub fn start_heartbeat(&self, interval: Duration) -> WorkerLockHeartbeat {
        let interval = if interval.is_zero() {
            Duration::from_millis(1)
        } else {
            interval
        };
        let store = self.store.clone();
        let lock_name = self.lock_name.clone();
        let owner_id = self.owner_id.clone();
        let generation = self.generation;
        let permit = self.permit.clone();
        let (stop_tx, stop_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        match store.refresh_worker_lock(&lock_name, &owner_id, generation) {
                            Ok(()) => {}
                            Err(LlmusageError::LockLost) => {
                                // Our generation no longer matches the row — the lease
                                // expired and was stolen by a new owner. Log at error so
                                // ops/debuggers can trace dual-writer incidents.
                                tracing::error!(
                                    "SQLite worker 锁已被其他进程抢占（fencing generation \
                                     不匹配），当前 worker 应停止写入"
                                );
                                permit.mark_lost();
                                break;
                            }
                            Err(err) => {
                                warn!(error = %err, "SQLite worker 锁 heartbeat 续租失败");
                            }
                        }
                    }
                }
            }
        });
        WorkerLockHeartbeat {
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }

    /// Starts the default heartbeat cadence for production sync runs.
    pub fn start_default_heartbeat(&self) -> WorkerLockHeartbeat {
        let lease_seconds = WORKER_LOCK_LEASE_MINUTES.max(1) as u64 * 60;
        self.start_heartbeat(Duration::from_secs((lease_seconds / 3).max(1)))
    }

    /// Metadata captured when this guard acquired the lock.
    pub fn meta(&self) -> &WorkerLockMeta {
        &self.meta
    }
}

impl WritePermit {
    pub(crate) fn validate_in_transaction(&self, tx: &Transaction<'_>) -> Result<()> {
        self.ensure_not_lost()?;
        let current = tx
            .query_row(
                r#"
                SELECT owner_id, generation, lease_expires_at
                FROM worker_lock
                WHERE lock_name = ?1
                "#,
                params![self.lock_name],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let valid = current.is_some_and(|(owner_id, generation, expires_at)| {
            owner_id == self.owner_id
                && generation == self.generation
                && !lease_expired(&expires_at, Utc::now())
        });
        if !valid {
            self.mark_lost();
            return Err(LlmusageError::LockLost);
        }
        Ok(())
    }
}

impl Drop for WorkerLockHeartbeat {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WorkerLock {
    fn drop(&mut self) {
        let _ = self
            .store
            .release_worker_lock(&self.lock_name, &self.owner_id, self.generation);
    }
}

impl Store {
    /// Returns a fenced store for one complete mutation operation. Existing
    /// fenced clones reuse their permit; compatibility callers acquire and own
    /// a short-lived lock automatically.
    pub(crate) fn write_operation(&self, kind: HolderKind) -> Result<WriteOperation> {
        if self.emit_only {
            return Err(LlmusageError::ConfigInvalid {
                detail: "emit-only store does not acquire the worker lock".to_string(),
            });
        }
        if let Some(permit) = self.write_permit.as_ref() {
            permit.ensure_not_lost()?;
            return Ok(WriteOperation {
                store: self.clone(),
                _heartbeat: None,
                _lock: None,
            });
        }

        let lock = self.acquire_worker_lock_with(Duration::from_secs(30), kind)?;
        let fenced = lock.fenced_store();
        let heartbeat = lock.start_default_heartbeat();
        Ok(WriteOperation {
            store: fenced,
            _heartbeat: Some(heartbeat),
            _lock: Some(lock),
        })
    }

    /// Runs one mutation transaction under the current generation fence.
    /// Validation happens after `BEGIN IMMEDIATE` and immediately before
    /// commit, so a stale holder cannot begin or finish another transaction.
    pub(crate) fn write_transaction<T>(
        &self,
        write: impl FnOnce(&Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let operation = self.write_operation(HolderKind::Library)?;
        let permit = operation.store.write_permit()?.clone();
        let mut conn = operation.store.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        permit.validate_in_transaction(&tx)?;
        let value = write(&tx)?;
        permit.validate_in_transaction(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    pub(crate) fn validate_write_transaction(&self, tx: &Transaction<'_>) -> Result<()> {
        self.write_permit()?.validate_in_transaction(tx)
    }

    /// Waits until the global worker lock can be acquired or `timeout` elapses.
    pub fn acquire_worker_lock_with(
        &self,
        timeout: Duration,
        kind: HolderKind,
    ) -> Result<WorkerLock> {
        if self.emit_only {
            return Err(LlmusageError::ConfigInvalid {
                detail: "emit-only store does not acquire the worker lock".to_string(),
            });
        }
        info!(holder_kind = %kind, timeout_ms = timeout.as_millis(), "开始等待 SQLite worker 锁");
        let started = Instant::now();
        loop {
            if let Some(lock) = self.try_acquire_worker_lock(kind)? {
                info!(
                    holder = %lock.meta().holder_identity(),
                    wait_ms = started.elapsed().as_millis(),
                    "完成 SQLite worker 锁申请"
                );
                return Ok(lock);
            }

            if started.elapsed() >= timeout {
                let holder = self
                    .current_worker_lock()?
                    .map(|meta| meta.holder_identity())
                    .unwrap_or_default();
                return Err(LlmusageError::LockBusy { holder });
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Attempts to acquire the global worker lock once without sleeping.
    ///
    /// JobRegistry uses this to build a cancellation-aware async wait loop
    /// without spawning unbounded blocking waiters.
    pub(crate) fn try_acquire_worker_lock_once(
        &self,
        kind: HolderKind,
    ) -> Result<Option<WorkerLock>> {
        self.try_acquire_worker_lock(kind)
    }

    /// Returns the current non-expired worker lock holder, if any.
    pub fn current_worker_lock(&self) -> Result<Option<WorkerLockMeta>> {
        let conn = self.open_connection()?;
        Self::current_worker_lock_with_conn(&conn)
    }

    /// Returns the current non-expired worker lock holder using an existing
    /// connection.
    pub(crate) fn current_worker_lock_with_conn(
        conn: &rusqlite::Connection,
    ) -> Result<Option<WorkerLockMeta>> {
        let meta = conn
            .query_row(
                r#"
                SELECT holder_pid, holder_kind, acquired_at, lease_expires_at, updated_at
                FROM worker_lock
                WHERE lock_name = ?1
                "#,
                params![WORKER_LOCK_NAME],
                worker_lock_meta_from_row,
            )
            .optional()?;
        Ok(meta.filter(|item| !lease_expired(&item.lease_expires_at, Utc::now())))
    }

    fn try_acquire_worker_lock(&self, kind: HolderKind) -> Result<Option<WorkerLock>> {
        match self.try_acquire_worker_lock_inner(kind) {
            Err(error) if sqlite_lock_contention(&error) => {
                info!(holder_kind = %kind, "SQLite worker 锁协调表正被并发更新");
                Ok(None)
            }
            result => result,
        }
    }

    fn try_acquire_worker_lock_inner(&self, kind: HolderKind) -> Result<Option<WorkerLock>> {
        info!(holder_kind = %kind, "尝试申请 SQLite worker 锁");

        self.ensure_worker_lock_table()?;

        let owner_id = format!(
            "{}:{}:{}",
            std::process::id(),
            now_utc(),
            self.paths.db_path.display()
        );
        let now = Utc::now();
        let acquired_at = now.to_rfc3339();
        let holder_pid = i64::from(std::process::id());
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_worker_lock_for_update(&tx)?;

        let meta = WorkerLockMeta {
            holder_pid,
            holder_kind: kind.as_str().to_string(),
            acquired_at: acquired_at.clone(),
            lease_expires_at: lease_expires_at(now),
            updated_at: now.to_rfc3339(),
        };
        let generation: u32;
        let acquired = match existing {
            None => {
                generation = 1;
                tx.execute(
                    r#"
                    INSERT INTO worker_lock(
                        lock_name, owner_id, lease_expires_at, updated_at,
                        holder_pid, holder_kind, acquired_at, generation
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        meta.lease_expires_at,
                        meta.updated_at,
                        meta.holder_pid,
                        meta.holder_kind,
                        meta.acquired_at,
                        generation,
                    ],
                )?;
                true
            }
            Some((ref existing_meta, existing_generation))
                if lease_expired(&existing_meta.lease_expires_at, now) =>
            {
                generation = existing_generation.saturating_add(1);
                tx.execute(
                    r#"
                    UPDATE worker_lock
                    SET owner_id = ?2,
                        lease_expires_at = ?3,
                        updated_at = ?4,
                        holder_pid = ?5,
                        holder_kind = ?6,
                        acquired_at = ?7,
                        generation = ?8
                    WHERE lock_name = ?1
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        meta.lease_expires_at,
                        meta.updated_at,
                        meta.holder_pid,
                        meta.holder_kind,
                        meta.acquired_at,
                        generation,
                    ],
                )?;
                true
            }
            Some(_) => {
                generation = 0; // not acquired; value unused
                false
            }
        };
        tx.commit()?;

        if !acquired {
            info!("SQLite worker 锁已被占用");
            return Ok(None);
        }

        let permit = WritePermit {
            lock_name: WORKER_LOCK_NAME.to_string(),
            owner_id: owner_id.clone(),
            generation,
            lost: Arc::new(AtomicBool::new(false)),
        };
        Ok(Some(WorkerLock {
            store: self.clone(),
            lock_name: WORKER_LOCK_NAME.to_string(),
            owner_id,
            generation,
            permit,
            meta,
        }))
    }

    fn refresh_worker_lock(&self, lock_name: &str, owner_id: &str, generation: u32) -> Result<()> {
        let now = Utc::now();
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current_expiry = tx
            .query_row(
                r#"
                SELECT lease_expires_at
                FROM worker_lock
                WHERE lock_name = ?1 AND owner_id = ?2 AND generation = ?3
                "#,
                params![lock_name, owner_id, generation],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if current_expiry
            .as_deref()
            .is_none_or(|expires_at| lease_expired(expires_at, now))
        {
            return Err(LlmusageError::LockLost);
        }
        let changed = tx.execute(
            r#"
            UPDATE worker_lock
            SET lease_expires_at = ?4, updated_at = ?5
            WHERE lock_name = ?1 AND owner_id = ?2 AND generation = ?3
            "#,
            params![
                lock_name,
                owner_id,
                generation,
                lease_expires_at(now),
                now.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(LlmusageError::LockLost);
        }
        tx.commit()?;
        Ok(())
    }

    fn release_worker_lock(&self, lock_name: &str, owner_id: &str, generation: u32) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM worker_lock WHERE lock_name = ?1 AND owner_id = ?2 AND generation = ?3",
            params![lock_name, owner_id, generation],
        )?;
        Ok(())
    }

    /// Creates only the coordination table needed to acquire the first fence.
    /// Full schema migration still runs after the lock has been acquired.
    fn ensure_worker_lock_table(&self) -> Result<()> {
        std::fs::create_dir_all(&self.paths.root_dir)?;
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS worker_lock (
                lock_name TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                holder_pid INTEGER,
                holder_kind TEXT,
                acquired_at TEXT,
                updated_at TEXT NOT NULL,
                generation INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        ensure_worker_lock_column(&tx, "holder_pid", "INTEGER")?;
        ensure_worker_lock_column(&tx, "holder_kind", "TEXT")?;
        ensure_worker_lock_column(&tx, "acquired_at", "TEXT")?;
        ensure_worker_lock_column(&tx, "generation", "INTEGER NOT NULL DEFAULT 0")?;
        tx.commit()?;
        Ok(())
    }
}

fn sqlite_lock_contention(error: &LlmusageError) -> bool {
    matches!(
        error,
        LlmusageError::Db(rusqlite::Error::SqliteFailure(code, _))
            if matches!(
                code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

fn ensure_worker_lock_column(
    conn: &rusqlite::Connection,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(worker_lock)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !columns.iter().any(|candidate| candidate == column) {
        conn.execute(
            &format!("ALTER TABLE worker_lock ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn load_worker_lock_for_update(tx: &Transaction<'_>) -> Result<Option<(WorkerLockMeta, u32)>> {
    tx.query_row(
        r#"
        SELECT holder_pid, holder_kind, acquired_at, lease_expires_at, updated_at,
               COALESCE(generation, 0)
        FROM worker_lock
        WHERE lock_name = ?1
        "#,
        params![WORKER_LOCK_NAME],
        |row| {
            let meta = WorkerLockMeta {
                holder_pid: row.get(0)?,
                holder_kind: row.get(1)?,
                acquired_at: row.get(2)?,
                lease_expires_at: row.get(3)?,
                updated_at: row.get(4)?,
            };
            let generation: u32 = row.get(5)?;
            Ok((meta, generation))
        },
    )
    .optional()
    .map_err(Into::into)
}

fn worker_lock_meta_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerLockMeta> {
    Ok(WorkerLockMeta {
        holder_pid: row.get(0)?,
        holder_kind: row.get(1)?,
        acquired_at: row.get(2)?,
        lease_expires_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn lease_expires_at(now: DateTime<Utc>) -> String {
    (now + ChronoDuration::minutes(WORKER_LOCK_LEASE_MINUTES)).to_rfc3339()
}

fn lease_expired(raw: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{error::LlmusageError, paths::AppPaths};

    fn test_store(temp: &TempDir) -> Result<Store> {
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok(store)
    }

    /// CONC-001: a refresh that matches owner_id but the wrong generation returns
    /// LockLost rather than silently succeeding.
    #[test]
    fn refresh_with_wrong_generation_returns_lock_lost() -> Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;

        // Acquire the lock.
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        let correct_generation = lock.generation;

        // A stale generation (e.g. after a re-acquire by another process) should fail.
        let stale_generation = correct_generation.wrapping_add(1);
        let err = store
            .refresh_worker_lock(&lock.lock_name, &lock.owner_id, stale_generation)
            .unwrap_err();
        assert!(
            matches!(err, LlmusageError::LockLost),
            "expected LockLost, got {err:?}"
        );
        Ok(())
    }

    #[test]
    fn refresh_rejects_expired_lease_without_reviving_it() -> Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        let expired_at = "2000-01-01T00:00:00Z";
        store.open_connection()?.execute(
            "UPDATE worker_lock SET lease_expires_at = ?1 WHERE lock_name = ?2",
            params![expired_at, WORKER_LOCK_NAME],
        )?;

        let error = lock
            .refresh()
            .expect_err("an expired generation must not revive its lease");
        assert!(matches!(error, LlmusageError::LockLost));
        let persisted_expiry = store.open_connection()?.query_row(
            "SELECT lease_expires_at FROM worker_lock WHERE lock_name = ?1",
            [WORKER_LOCK_NAME],
            |row| row.get::<_, String>(0),
        )?;
        assert_eq!(persisted_expiry, expired_at);
        Ok(())
    }

    #[test]
    fn concurrent_legacy_coordination_upgrade_is_serialized() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        std::fs::create_dir_all(&paths.root_dir)?;
        rusqlite::Connection::open(&paths.db_path)?.execute_batch(
            r#"
            CREATE TABLE worker_lock (
                lock_name TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;

        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let store = Store::new(&paths)?;
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                store
                    .try_acquire_worker_lock_once(HolderKind::Cli)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }));
        }
        for handle in handles {
            let result = handle.join().expect("coordination upgrade thread panicked");
            assert!(result.is_ok(), "coordination upgrade failed: {result:?}");
        }

        let conn = rusqlite::Connection::open(&paths.db_path)?;
        let generation_exists = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('worker_lock') WHERE name = 'generation'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        assert_eq!(generation_exists, 1);
        Ok(())
    }

    /// CONC-001: a fresh acquisition of an expired lock increments the generation,
    /// causing the old holder's refresh to return LockLost.
    #[test]
    fn stolen_lock_causes_old_owner_refresh_to_fail() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let first_store = Store::new(&paths)?;
        first_store.bootstrap()?;
        let second_store = Store::new(&paths)?;

        // First acquisition.
        let first = first_store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        let first_gen = first.generation;
        let stale_store = first.fenced_store();

        // Expire through the second Store so both lock owners use independently
        // opened SQLite connections to the shared database.
        {
            let conn = second_store.open_connection()?;
            conn.execute(
                "UPDATE worker_lock SET lease_expires_at = '2000-01-01T00:00:00Z'",
                [],
            )?;
        }

        let second = second_store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        assert!(
            second.generation > first_gen,
            "new generation ({}) must exceed old generation ({})",
            second.generation,
            first_gen
        );

        // The original holder's refresh now returns LockLost.
        let err = first.refresh().unwrap_err();
        assert!(
            matches!(err, LlmusageError::LockLost),
            "expected LockLost after lock theft, got {err:?}"
        );
        let err = stale_store
            .set_meta_value("stale-writer", "must-not-commit")
            .expect_err("refresh loss must propagate to every fenced store clone");
        assert!(matches!(err, LlmusageError::LockLost));

        // The new holder's refresh still succeeds.
        second_store.refresh_worker_lock(&second.lock_name, &second.owner_id, second.generation)?;
        Ok(())
    }

    #[test]
    fn write_transaction_rolls_back_when_closure_returns_err() -> Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;

        let error = store
            .write_transaction(|tx| -> Result<()> {
                tx.execute(
                    "INSERT INTO meta(key, value) VALUES (?1, ?2)",
                    params!["rollback-probe", "must-not-commit"],
                )?;
                Err(LlmusageError::ConfigInvalid {
                    detail: "forced rollback".to_string(),
                })
            })
            .expect_err("closure Err must surface without committing");
        assert!(matches!(error, LlmusageError::ConfigInvalid { .. }));

        let count: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM meta WHERE key = ?1",
            ["rollback-probe"],
            |row| row.get(0),
        )?;
        assert_eq!(count, 0);
        Ok(())
    }
}
