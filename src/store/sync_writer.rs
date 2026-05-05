use std::collections::{HashMap, HashSet};

use anyhow::Result;
use tracing::info;

use super::{BucketKey, BucketRollup, FileCursor, Store, SyncRunWriter};
use crate::{
    models::{ProjectInfo, SourceKind, UsageEvent, UsageTokens},
    util::now_utc,
};

impl Store {
    pub fn begin_sync_run(&self) -> Result<SyncRunWriter> {
        /*
         * ========================================================================
         * 步骤3：建立单写入端
         * ========================================================================
         * 目标：
         * 1) 复用单个 SQLite 连接处理批量写
         * 2) 把 event / bucket / project / cursor 写入收敛到一个出口
         * 3) 避免每条 event 单独开连接和事务
         */
        info!("开始建立 sync 单写入端");
        let conn = self.open_connection()?;
        info!("完成 sync 单写入端建立");
        Ok(SyncRunWriter { conn })
    }
}

impl SyncRunWriter {
    pub fn reset_file_events_batch(
        &mut self,
        source: SourceKind,
        path_hashes: &[String],
    ) -> Result<()> {
        if path_hashes.is_empty() {
            return Ok(());
        }

        /*
         * ========================================================================
         * 步骤4：清理需要重放的旧事件
         * ========================================================================
         * 目标：
         * 1) 在整文件重放前先移除旧 event
         * 2) 同步回滚 bucket 聚合，避免双计
         * 3) 保持 path 级别重放的幂等
         */
        info!(source = %source, count = path_hashes.len(), "开始清理重放旧事件");

        // 4.1 在同一事务里扣减 bucket 并删除旧 event
        let mut unique = HashSet::new();
        let tx = self.conn.transaction()?;
        {
            let mut aggregate_stmt = tx.prepare_cached(
                r#"
                SELECT
                    model,
                    hour_start,
                    COALESCE(project_hash, ''),
                    SUM(input_tokens),
                    SUM(cached_input_tokens),
                    SUM(output_tokens),
                    SUM(reasoning_output_tokens),
                    SUM(total_tokens)
                FROM usage_event
                WHERE source = ?1 AND event_key LIKE ?2
                GROUP BY model, hour_start, COALESCE(project_hash, '')
                "#,
            )?;
            let mut update_bucket_stmt = tx.prepare_cached(
                r#"
                UPDATE usage_bucket_30m
                SET
                    input_tokens = input_tokens - ?5,
                    cached_input_tokens = cached_input_tokens - ?6,
                    output_tokens = output_tokens - ?7,
                    reasoning_output_tokens = reasoning_output_tokens - ?8,
                    total_tokens = total_tokens - ?9,
                    updated_at = ?10
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                "#,
            )?;
            let mut delete_zero_stmt = tx.prepare_cached(
                r#"
                DELETE FROM usage_bucket_30m
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                  AND input_tokens <= 0
                  AND cached_input_tokens <= 0
                  AND output_tokens <= 0
                  AND reasoning_output_tokens <= 0
                  AND total_tokens <= 0
                "#,
            )?;
            let mut delete_event_stmt = tx.prepare_cached(
                "DELETE FROM usage_event WHERE source = ?1 AND event_key LIKE ?2",
            )?;
            let updated_at = now_utc();

            for path_hash in path_hashes {
                if !unique.insert(path_hash.clone()) {
                    continue;
                }

                let prefix = format!("{}:{}:%", source.as_str(), path_hash);
                let rows = aggregate_stmt.query_map(
                    rusqlite::params![source.as_str(), prefix],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            UsageTokens {
                                input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                                cached_input_tokens: row
                                    .get::<_, Option<i64>>(4)?
                                    .unwrap_or_default(),
                                output_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                                reasoning_output_tokens: row
                                    .get::<_, Option<i64>>(6)?
                                    .unwrap_or_default(),
                                total_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                            },
                        ))
                    },
                )?;
                let aggregates = rows.collect::<rusqlite::Result<Vec<_>>>()?;

                for (model, hour_start, project_hash, tokens) in aggregates {
                    update_bucket_stmt.execute(rusqlite::params![
                        source.as_str(),
                        model,
                        hour_start,
                        project_hash,
                        tokens.input_tokens,
                        tokens.cached_input_tokens,
                        tokens.output_tokens,
                        tokens.reasoning_output_tokens,
                        tokens.total_tokens,
                        updated_at,
                    ])?;
                    delete_zero_stmt.execute(rusqlite::params![
                        source.as_str(),
                        model,
                        hour_start,
                        project_hash,
                    ])?;
                }

                let prefix = format!("{}:{}:%", source.as_str(), path_hash);
                delete_event_stmt.execute(rusqlite::params![source.as_str(), prefix])?;
            }
        }
        tx.commit()?;

        info!(source = %source, "完成重放旧事件清理");
        Ok(())
    }

    pub fn write_event_batch(&mut self, events: &[UsageEvent]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        /*
         * ========================================================================
         * 步骤5：批量写入 usage_event 与聚合桶
         * ========================================================================
         * 目标：
         * 1) 批量 INSERT OR IGNORE usage_event
         * 2) 仅对新插入事件更新 project_dim 与 bucket
         * 3) 把每批写入保持在单事务内
         */
        info!(batch = events.len(), "开始批量写入 usage_event");

        // 5.1 在单事务中插入 event，并为新 event 做内存聚合
        let tx = self.conn.transaction()?;
        let now = now_utc();
        let inserted = {
            let mut event_stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    project_hash, project_label, project_ref, path_hash, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                "#,
            )?;
            let mut projects = HashMap::new();
            let mut buckets = HashMap::new();
            let mut inserted = 0usize;

            for event in events {
                let changed = event_stmt.execute(rusqlite::params![
                    event.event_key,
                    event.source.as_str(),
                    event.model,
                    event.event_at,
                    event.hour_start,
                    event.tokens.input_tokens,
                    event.tokens.cached_input_tokens,
                    event.tokens.output_tokens,
                    event.tokens.reasoning_output_tokens,
                    event.tokens.total_tokens,
                    event
                        .project
                        .as_ref()
                        .map(|value| value.project_hash.as_str()),
                    event
                        .project
                        .as_ref()
                        .map(|value| value.project_label.as_str()),
                    event
                        .project
                        .as_ref()
                        .and_then(|value| value.project_ref.as_deref()),
                    event.project.as_ref().map(|value| value.path_hash.as_str()),
                    now,
                ])?;
                if changed == 0 {
                    continue;
                }

                inserted += 1;
                if let Some(project) = &event.project {
                    projects.insert(project.project_hash.clone(), project.clone());
                }
                roll_up_bucket(&mut buckets, event);
            }
            drop(event_stmt);

            // 5.2 将项目维表和 30 分钟桶一次性刷入
            flush_projects_tx(&tx, &projects)?;
            flush_buckets_tx(&tx, &buckets)?;
            inserted
        };
        tx.commit()?;

        info!(batch = events.len(), inserted, "完成批量写入 usage_event");
        Ok(inserted)
    }

    pub fn write_cursor_batch(&mut self, source: SourceKind, cursors: &[FileCursor]) -> Result<()> {
        if cursors.is_empty() {
            return Ok(());
        }

        /*
         * ========================================================================
         * 步骤6：批量刷新增量游标
         * ========================================================================
         * 目标：
         * 1) 只写本轮真正变更的 cursor
         * 2) 把文件签名、offset、累计 token 状态一并持久化
         * 3) 保持每批 cursor 写入在单事务内
         */
        info!(source = %source, count = cursors.len(), "开始批量刷新 cursor");

        // 6.1 用单事务 upsert 本轮发生变化的 cursor
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO source_cursor(
                    source,
                    cursor_key,
                    file_path,
                    file_fingerprint,
                    file_size,
                    file_mtime_ns,
                    tail_signature,
                    offset,
                    last_total_json,
                    last_model,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(source, cursor_key) DO UPDATE SET
                    file_path = excluded.file_path,
                    file_fingerprint = excluded.file_fingerprint,
                    file_size = excluded.file_size,
                    file_mtime_ns = excluded.file_mtime_ns,
                    tail_signature = excluded.tail_signature,
                    offset = excluded.offset,
                    last_total_json = excluded.last_total_json,
                    last_model = excluded.last_model,
                    updated_at = excluded.updated_at
                "#,
            )?;
            for cursor in cursors {
                stmt.execute(rusqlite::params![
                    source.as_str(),
                    cursor.cursor_key,
                    cursor.file_path,
                    cursor.file_fingerprint,
                    cursor.file_size as i64,
                    cursor.file_mtime_ns,
                    cursor.tail_signature,
                    cursor.offset as i64,
                    cursor
                        .last_total
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    cursor.last_model,
                    cursor.updated_at,
                ])?;
            }
        }
        tx.commit()?;

        info!(source = %source, "完成批量刷新 cursor");
        Ok(())
    }

    pub fn finish_sync_run(self) -> Result<()> {
        info!("完成 sync 单写入端收尾");
        Ok(())
    }
}

fn roll_up_bucket(buckets: &mut HashMap<BucketKey, BucketRollup>, event: &UsageEvent) {
    let project_hash = event
        .project
        .as_ref()
        .map(|value| value.project_hash.clone())
        .unwrap_or_default();
    let key = BucketKey {
        source: event.source.as_str().to_string(),
        model: event.model.clone(),
        hour_start: event.hour_start.clone(),
        project_hash,
    };
    let entry = buckets.entry(key).or_insert_with(|| BucketRollup {
        project_label: event
            .project
            .as_ref()
            .map(|value| value.project_label.clone()),
        project_ref: event
            .project
            .as_ref()
            .and_then(|value| value.project_ref.clone()),
        tokens: UsageTokens::default(),
    });
    entry.tokens.input_tokens += event.tokens.input_tokens;
    entry.tokens.cached_input_tokens += event.tokens.cached_input_tokens;
    entry.tokens.output_tokens += event.tokens.output_tokens;
    entry.tokens.reasoning_output_tokens += event.tokens.reasoning_output_tokens;
    entry.tokens.total_tokens += event.tokens.total_tokens;
}

fn flush_projects_tx(
    tx: &rusqlite::Transaction<'_>,
    projects: &HashMap<String, ProjectInfo>,
) -> Result<()> {
    if projects.is_empty() {
        return Ok(());
    }

    let mut stmt = tx.prepare_cached(
        r#"
        INSERT INTO project_dim(
            project_hash, project_label, project_ref, repo_root_hash, path_hash, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(project_hash) DO UPDATE SET
            project_label = excluded.project_label,
            project_ref = excluded.project_ref,
            repo_root_hash = excluded.repo_root_hash,
            path_hash = excluded.path_hash,
            updated_at = excluded.updated_at
        "#,
    )?;
    let updated_at = now_utc();
    for project in projects.values() {
        stmt.execute(rusqlite::params![
            project.project_hash,
            project.project_label,
            project.project_ref,
            project.repo_root_hash,
            project.path_hash,
            updated_at,
        ])?;
    }
    Ok(())
}

fn flush_buckets_tx(
    tx: &rusqlite::Transaction<'_>,
    buckets: &HashMap<BucketKey, BucketRollup>,
) -> Result<()> {
    if buckets.is_empty() {
        return Ok(());
    }

    let mut stmt = tx.prepare_cached(
        r#"
        INSERT INTO usage_bucket_30m(
            source,
            model,
            hour_start,
            project_hash,
            project_label,
            project_ref,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
            project_label = excluded.project_label,
            project_ref = excluded.project_ref,
            input_tokens = usage_bucket_30m.input_tokens + excluded.input_tokens,
            cached_input_tokens = usage_bucket_30m.cached_input_tokens + excluded.cached_input_tokens,
            output_tokens = usage_bucket_30m.output_tokens + excluded.output_tokens,
            reasoning_output_tokens = usage_bucket_30m.reasoning_output_tokens + excluded.reasoning_output_tokens,
            total_tokens = usage_bucket_30m.total_tokens + excluded.total_tokens,
            updated_at = excluded.updated_at
        "#,
    )?;
    let updated_at = now_utc();
    for (key, rollup) in buckets {
        stmt.execute(rusqlite::params![
            key.source,
            key.model,
            key.hour_start,
            key.project_hash,
            rollup.project_label,
            rollup.project_ref,
            rollup.tokens.input_tokens,
            rollup.tokens.cached_input_tokens,
            rollup.tokens.output_tokens,
            rollup.tokens.reasoning_output_tokens,
            rollup.tokens.total_tokens,
            updated_at,
        ])?;
    }
    Ok(())
}
