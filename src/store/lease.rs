use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tracing::info;

use super::{Store, WORKER_LEASE_MINUTES, WORKER_LOCK_NAME, WorkerLock};
use crate::util::now_utc;

impl WorkerLock {
    pub fn refresh(&self) -> Result<()> {
        self.store
            .refresh_worker_lock(&self.lock_name, &self.owner_id)
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
    pub fn acquire_worker_lock(&self) -> Result<Option<WorkerLock>> {
        /*
         * ========================================================================
         * 步骤2：申请 SQLite 租约锁
         * ========================================================================
         * 目标：
         * 1) 用单行租约替代失效的文件锁
         * 2) 只允许一个 sync / hook-run worker 进入
         * 3) 异常退出后依赖租约过期自动恢复
         */
        info!("开始申请 SQLite worker 租约锁");

        // 2.1 用 IMMEDIATE 事务串行化锁竞争
        let owner_id = format!(
            "{}:{}:{}",
            std::process::id(),
            now_utc(),
            self.paths.db_path.display()
        );
        let now = Utc::now();
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = tx
            .query_row(
                r#"
                SELECT owner_id, lease_expires_at
                FROM worker_lease
                WHERE lock_name = ?1
                "#,
                params![WORKER_LOCK_NAME],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let acquired = match existing {
            None => {
                tx.execute(
                    r#"
                    INSERT INTO worker_lease(lock_name, owner_id, lease_expires_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        lease_expires_at(now),
                        now.to_rfc3339(),
                    ],
                )?;
                true
            }
            Some((_owner, expires_at)) if lease_expired(&expires_at, now) => {
                tx.execute(
                    r#"
                    UPDATE worker_lease
                    SET owner_id = ?2, lease_expires_at = ?3, updated_at = ?4
                    WHERE lock_name = ?1
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        lease_expires_at(now),
                        now.to_rfc3339(),
                    ],
                )?;
                true
            }
            Some(_) => false,
        };
        tx.commit()?;

        // 2.2 命中租约才返回 guard
        if !acquired {
            info!("SQLite worker 租约锁已被占用");
            return Ok(None);
        }

        info!("完成 SQLite worker 租约锁申请");
        Ok(Some(WorkerLock {
            store: self.clone(),
            lock_name: WORKER_LOCK_NAME.to_string(),
            owner_id,
        }))
    }

    fn refresh_worker_lock(&self, lock_name: &str, owner_id: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE worker_lease
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
            "DELETE FROM worker_lease WHERE lock_name = ?1 AND owner_id = ?2",
            params![lock_name, owner_id],
        )?;
        Ok(())
    }
}

fn lease_expires_at(now: DateTime<Utc>) -> String {
    (now + ChronoDuration::minutes(WORKER_LEASE_MINUTES)).to_rfc3339()
}

fn lease_expired(raw: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}
