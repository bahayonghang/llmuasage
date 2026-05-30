use std::{
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use tracing::info;

use super::{
    HolderKind, Store, WORKER_LOCK_LEASE_MINUTES, WORKER_LOCK_NAME, WorkerLock, WorkerLockMeta,
};
use crate::{
    error::{LlmusageError, Result},
    util::now_utc,
};

impl WorkerLock {
    pub fn refresh(&self) -> Result<()> {
        self.store
            .refresh_worker_lock(&self.lock_name, &self.owner_id)
    }

    /// Metadata captured when this guard acquired the lock.
    pub fn meta(&self) -> &WorkerLockMeta {
        &self.meta
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
        let acquired = match existing {
            None => {
                tx.execute(
                    r#"
                    INSERT INTO worker_lock(
                        lock_name, owner_id, lease_expires_at, updated_at,
                        holder_pid, holder_kind, acquired_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        meta.lease_expires_at,
                        meta.updated_at,
                        meta.holder_pid,
                        meta.holder_kind,
                        meta.acquired_at,
                    ],
                )?;
                true
            }
            Some(existing) if lease_expired(&existing.lease_expires_at, now) => {
                tx.execute(
                    r#"
                    UPDATE worker_lock
                    SET owner_id = ?2,
                        lease_expires_at = ?3,
                        updated_at = ?4,
                        holder_pid = ?5,
                        holder_kind = ?6,
                        acquired_at = ?7
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
                    ],
                )?;
                true
            }
            Some(_) => false,
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
            meta,
        }))
    }

    fn refresh_worker_lock(&self, lock_name: &str, owner_id: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE worker_lock
            SET lease_expires_at = ?3, updated_at = ?4
            WHERE lock_name = ?1 AND owner_id = ?2
            "#,
            params![lock_name, owner_id, lease_expires_at(Utc::now()), now_utc(),],
        )?;
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

fn load_worker_lock_for_update(tx: &Transaction<'_>) -> Result<Option<WorkerLockMeta>> {
    tx.query_row(
        r#"
        SELECT holder_pid, holder_kind, acquired_at, lease_expires_at, updated_at
        FROM worker_lock
        WHERE lock_name = ?1
        "#,
        params![WORKER_LOCK_NAME],
        worker_lock_meta_from_row,
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
