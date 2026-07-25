use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use tracing::{info, warn};

use super::{
    HolderKind, Store, WORKER_LOCK_LEASE_MINUTES, WORKER_LOCK_NAME, WorkerLock,
    WorkerLockHeartbeat, WorkerLockMeta,
};
use crate::{
    error::{LlmusageError, Result},
    util::now_utc,
};

impl WorkerLock {
    pub fn refresh(&self) -> Result<()> {
        self.store
            .refresh_worker_lock(&self.lock_name, &self.owner_id, self.generation)
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
            .release_worker_lock(&self.lock_name, &self.owner_id);
    }
}

impl Store {
    /// Legacy non-blocking lock acquisition. Hook workers intentionally keep
    /// this path so high-frequency tool signals skip rather than queue.
    #[deprecated(note = "use acquire_worker_lock_with for blocking callers")]
    pub fn acquire_worker_lock(&self) -> Result<Option<WorkerLock>> {
        self.try_acquire_worker_lock(HolderKind::Hook)
    }

    /// Waits until the global worker lock can be acquired or `timeout` elapses.
    pub fn acquire_worker_lock_with(
        &self,
        timeout: Duration,
        kind: HolderKind,
    ) -> Result<WorkerLock> {
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
        info!(holder_kind = %kind, "尝试申请 SQLite worker 锁");

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

        Ok(Some(WorkerLock {
            store: self.clone(),
            lock_name: WORKER_LOCK_NAME.to_string(),
            owner_id,
            generation,
            meta,
        }))
    }

    fn refresh_worker_lock(&self, lock_name: &str, owner_id: &str, generation: u32) -> Result<()> {
        let conn = self.open_connection()?;
        let changed = conn.execute(
            r#"
            UPDATE worker_lock
            SET lease_expires_at = ?4, updated_at = ?5
            WHERE lock_name = ?1 AND owner_id = ?2 AND generation = ?3
            "#,
            params![lock_name, owner_id, generation, lease_expires_at(Utc::now()), now_utc(),],
        )?;
        if changed == 0 {
            return Err(LlmusageError::LockLost.into());
        }
        Ok(())
    }

    fn release_worker_lock(&self, lock_name: &str, owner_id: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM worker_lock WHERE lock_name = ?1 AND owner_id = ?2",
            params![lock_name, owner_id],
        )?;
        Ok(())
    }
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
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
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

    /// CONC-001: a fresh acquisition of an expired lock increments the generation,
    /// causing the old holder's refresh to return LockLost.
    #[test]
    fn stolen_lock_causes_old_owner_refresh_to_fail() -> Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;

        // First acquisition.
        let first = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        let first_gen = first.generation;

        // Manually expire the lease so the second acquisition can steal it.
        {
            let conn = store.open_connection()?;
            conn.execute(
                "UPDATE worker_lock SET lease_expires_at = '2000-01-01T00:00:00Z'",
                [],
            )?;
        }

        // Second acquisition on the same store steals the expired lease.
        let second = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        assert!(
            second.generation > first_gen,
            "new generation ({}) must exceed old generation ({})",
            second.generation,
            first_gen
        );

        // The original holder's refresh now returns LockLost.
        let err = store
            .refresh_worker_lock(&first.lock_name, &first.owner_id, first_gen)
            .unwrap_err();
        assert!(
            matches!(err, LlmusageError::LockLost),
            "expected LockLost after lock theft, got {err:?}"
        );

        // The new holder's refresh still succeeds.
        store.refresh_worker_lock(&second.lock_name, &second.owner_id, second.generation)?;
        Ok(())
    }
}
