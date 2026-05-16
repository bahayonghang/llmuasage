use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use crate::error::Result;
use tracing::info;

use super::{
    BucketKey, BucketRollup, FileCursor, PricingRollup, ShardCommitStats, Store, SyncRunWriter,
    SyncShard,
};
use crate::{
    error::LlmusageError,
    models::{ProjectInfo, SourceKind, UsageEvent, UsageTokens},
    query::{
        PricingCatalog, pricing,
        pricing::{CostBreakdown, PRICING_MIXED, PRICING_UNPRICED},
    },
    util::now_utc,
};

/// Maximum events committed in a single `usage_event` transaction.
///
/// Owned by the writer side of the protocol so parsers stay agnostic to
/// SQLite batch sizing. Removing this constant is a deletion-test signal:
/// each parser would have to reintroduce its own chunking constant.
const EVENT_WRITE_BATCH_SIZE: usize = 1000;

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
        let raw_archive_enabled = self.raw_archive_enabled()?;
        let pricing_catalog = self.active_pricing_catalog()?;
        info!(raw_archive_enabled, "完成 sync 单写入端建立");
        Ok(SyncRunWriter {
            conn,
            run_started_at: crate::util::now_utc_millis(),
            raw_archive_enabled,
            pricing_catalog,
        })
    }
}

impl Store {
    /// Loads the catalog currently selected by `meta('pricing_catalog_version')`.
    ///
    /// Missing metadata keeps the embedded static catalog. Once metadata points
    /// at a local snapshot, load failures are returned to the caller instead of
    /// silently falling back to `static-v1`; otherwise new sync/recompute work
    /// would be priced against a catalog the user did not select.
    pub fn active_pricing_catalog(&self) -> Result<PricingCatalog> {
        let Some(version) = self.meta_value("pricing_catalog_version")? else {
            return Ok(PricingCatalog::static_v1().clone());
        };
        if version == PricingCatalog::static_v1().version {
            return Ok(PricingCatalog::static_v1().clone());
        }

        let snapshot_path = self
            .paths
            .root_dir
            .join("pricing")
            .join(format!("{version}.json"));
        let catalog = PricingCatalog::load_snapshot(&snapshot_path).map_err(|source| {
            LlmusageError::ConfigInvalid {
                detail: format!(
                    "failed to load active pricing catalog `{version}` from {}: {source}",
                    snapshot_path.display()
                ),
            }
        })?;
        if catalog.version != version {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "pricing catalog metadata points to `{version}` but snapshot {} declares `{}`",
                    snapshot_path.display(),
                    catalog.version
                ),
            });
        }
        Ok(catalog)
    }
}

impl SyncRunWriter {
    fn reset_file_events_batch(
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
                    SUM(cache_read_tokens),
                    SUM(cache_creation_tokens),
                    SUM(output_tokens),
                    SUM(reasoning_output_tokens),
                    SUM(total_tokens),
                    SUM(cost_with_cache_usd),
                    SUM(cost_without_cache_usd),
                    COUNT(*)
                FROM usage_event
                WHERE source = ?1 AND source_path_hash = ?2
                GROUP BY model, hour_start, COALESCE(project_hash, '')
                "#,
            )?;
            let mut update_bucket_stmt = tx.prepare_cached(
                r#"
                UPDATE usage_bucket_30m
                SET
                    input_tokens = input_tokens - ?5,
                    cache_read_tokens = cache_read_tokens - ?6,
                    cache_creation_tokens = cache_creation_tokens - ?7,
                    output_tokens = output_tokens - ?8,
                    reasoning_output_tokens = reasoning_output_tokens - ?9,
                    total_tokens = total_tokens - ?10,
                    cost_with_cache_usd = cost_with_cache_usd - ?11,
                    cost_without_cache_usd = cost_without_cache_usd - ?12,
                    event_count = event_count - ?13,
                    updated_at = ?14
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                "#,
            )?;
            let mut delete_zero_stmt = tx.prepare_cached(
                r#"
                DELETE FROM usage_bucket_30m
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                  AND input_tokens <= 0
                  AND cache_read_tokens <= 0
                  AND cache_creation_tokens <= 0
                  AND output_tokens <= 0
                  AND reasoning_output_tokens <= 0
                  AND total_tokens <= 0
                  AND cost_with_cache_usd <= 0.0
                  AND cost_without_cache_usd <= 0.0
                  AND event_count <= 0
                "#,
            )?;
            let mut delete_event_stmt = tx.prepare_cached(
                "DELETE FROM usage_event WHERE source = ?1 AND source_path_hash = ?2",
            )?;
            let updated_at = now_utc();
            let mut touched_buckets = Vec::new();

            for path_hash in path_hashes {
                if !unique.insert(path_hash.clone()) {
                    continue;
                }

                let rows = aggregate_stmt.query_map(
                    rusqlite::params![source.as_str(), path_hash],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            UsageTokens {
                                input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                                cache_read_tokens: row
                                    .get::<_, Option<i64>>(4)?
                                    .unwrap_or_default(),
                                cache_creation_tokens: row
                                    .get::<_, Option<i64>>(5)?
                                    .unwrap_or_default(),
                                output_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                                reasoning_output_tokens: row
                                    .get::<_, Option<i64>>(7)?
                                    .unwrap_or_default(),
                                total_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
                            },
                            row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
                            row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
                            row.get::<_, Option<i64>>(11)?.unwrap_or_default(),
                        ))
                    },
                )?;
                let aggregates = rows.collect::<rusqlite::Result<Vec<_>>>()?;

                for (
                    model,
                    hour_start,
                    project_hash,
                    tokens,
                    cost_with_cache_usd,
                    cost_without_cache_usd,
                    event_count,
                ) in aggregates
                {
                    update_bucket_stmt.execute(rusqlite::params![
                        source.as_str(),
                        &model,
                        &hour_start,
                        &project_hash,
                        tokens.input_tokens,
                        tokens.cache_read_tokens,
                        tokens.cache_creation_tokens,
                        tokens.output_tokens,
                        tokens.reasoning_output_tokens,
                        tokens.total_tokens,
                        cost_with_cache_usd,
                        cost_without_cache_usd,
                        event_count,
                        updated_at,
                    ])?;
                    let deleted_empty_bucket = delete_zero_stmt.execute(rusqlite::params![
                        source.as_str(),
                        &model,
                        &hour_start,
                        &project_hash,
                    ])?;
                    if deleted_empty_bucket == 0 {
                        touched_buckets.push((model, hour_start, project_hash));
                    }
                }

                delete_event_stmt.execute(rusqlite::params![source.as_str(), path_hash])?;
            }

            refresh_bucket_pricing_after_reset_tx(
                &tx,
                source.as_str(),
                &touched_buckets,
                &updated_at,
            )?;
        }
        tx.commit()?;

        info!(source = %source, "完成重放旧事件清理");
        Ok(())
    }

    fn write_event_batch(&mut self, events: &[UsageEvent]) -> Result<usize> {
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
                    input_tokens, cache_read_tokens, cache_creation_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                    project_hash, project_label, project_ref, path_hash,
                    session_id, session_label, source_path_hash,
                    created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
                "#,
            )?;
            let mut projects = HashMap::new();
            let mut buckets = HashMap::new();
            let mut inserted = 0usize;

            for event in events {
                let cost = pricing::compute_cost_with(
                    &self.pricing_catalog,
                    event.source.as_str(),
                    &event.model,
                    pricing::CostTokens {
                        input: event.tokens.input_tokens,
                        cache_read: event.tokens.cache_read_tokens,
                        cache_creation: event.tokens.cache_creation_tokens,
                        output: event.tokens.output_tokens,
                        reasoning_output: event.tokens.reasoning_output_tokens,
                    },
                );
                let changed = event_stmt.execute(rusqlite::params![
                    event.event_key,
                    event.source.as_str(),
                    event.model,
                    event.event_at,
                    event.hour_start,
                    event.tokens.input_tokens,
                    event.tokens.cache_read_tokens,
                    event.tokens.cache_creation_tokens,
                    event.tokens.output_tokens,
                    event.tokens.reasoning_output_tokens,
                    event.tokens.total_tokens,
                    cost.cost_with_cache_usd,
                    cost.cost_without_cache_usd,
                    cost.pricing_status.as_str(),
                    cost.pricing_source,
                    cost.pricing_rate,
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
                    event
                        .session
                        .as_ref()
                        .map(|value| value.session_id.as_str()),
                    event
                        .session
                        .as_ref()
                        .and_then(|value| value.session_label.as_deref()),
                    event
                        .session
                        .as_ref()
                        .and_then(|value| value.source_path_hash.as_deref()),
                    now,
                ])?;
                if changed == 0 {
                    continue;
                }

                inserted += 1;
                if let Some(project) = &event.project {
                    projects.insert(project.project_hash.clone(), project.clone());
                }
                roll_up_bucket(&mut buckets, event, &cost);
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

    fn write_cursor_batch(&mut self, source: SourceKind, cursors: &[FileCursor]) -> Result<()> {
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
                        .transpose()
                        .map_err(|source| LlmusageError::Parse {
                            context: "file cursor token snapshot",
                            source,
                        })?,
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

    pub fn commit_shard(&mut self, shard: SyncShard) -> Result<ShardCommitStats> {
        /*
         * ========================================================================
         * 步骤7：原子化提交单个 shard
         * ========================================================================
         * 目标：
         * 1) 把 reset → write_event(分批) → write_cursor 的隐式协议固化
         * 2) 让 parser 不再关心写入顺序与 batch 大小
         * 3) 统一返回 inserted 数与本次提交耗时
         */
        info!(
            source = %shard.source,
            resets = shard.reset_path_hashes.len(),
            events = shard.events.len(),
            cursors = shard.cursors.len(),
            seen_files = shard.seen_file_paths.len(),
            raw_records = shard.raw_records.len(),
            "开始提交 shard"
        );

        // 7.1 计时入口与累加器
        let started = Instant::now();
        let mut stats = ShardCommitStats::default();

        // 7.2 先清旧 event，再批写 event，最后落 cursor —— 顺序由协议保证
        if !shard.reset_path_hashes.is_empty() {
            self.reset_file_events_batch(shard.source, &shard.reset_path_hashes)?;
        }
        for batch in shard.events.chunks(EVENT_WRITE_BATCH_SIZE) {
            stats.events_inserted += self.write_event_batch(batch)?;
        }
        if !shard.cursors.is_empty() {
            self.write_cursor_batch(shard.source, &shard.cursors)?;
        }
        // 7.3 把本轮看到的候选文件登记为 source_file.state='live'
        //     （D15 / ADR 0006）。OpenCode 等无 file 身份的源传空 vec。
        if !shard.seen_file_paths.is_empty() {
            self.write_source_file_seen(shard.source, &shard.seen_file_paths)?;
        }
        // 7.4 raw archive opt-in（D11 / F1.5）：开关关时丢弃 raw_records，
        //     避免 parser 端必须同步判定开关；开关开时与 event 共享 commit
        //     周期落库（INSERT OR IGNORE 保证 event_key 重复时幂等）。
        if self.raw_archive_enabled && !shard.raw_records.is_empty() {
            self.write_raw_records_batch(&shard.raw_records)?;
        }

        stats.files_seen = shard.seen_file_paths.len();
        stats.write_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        info!(
            source = %shard.source,
            inserted = stats.events_inserted,
            write_ms = stats.write_ms,
            "完成 shard 提交"
        );
        Ok(stats)
    }

    fn write_raw_records_batch(&mut self, records: &[super::RawRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_event_raw(
                    event_key, raw_json, created_at
                ) VALUES (?1, ?2, ?3)
                "#,
            )?;
            let now = now_utc();
            for record in records {
                stmt.execute(rusqlite::params![record.event_key, record.raw_json, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn write_source_file_seen(&mut self, source: SourceKind, file_paths: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        super::source_file::upsert_live_in_tx(
            &tx,
            source.as_str(),
            file_paths,
            &self.run_started_at,
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn roll_up_bucket(
    buckets: &mut HashMap<BucketKey, BucketRollup>,
    event: &UsageEvent,
    cost: &CostBreakdown,
) {
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
        pricing: PricingRollup::default(),
        event_count: 0,
    });
    entry.tokens.input_tokens += event.tokens.input_tokens;
    entry.tokens.cache_read_tokens += event.tokens.cache_read_tokens;
    entry.tokens.cache_creation_tokens += event.tokens.cache_creation_tokens;
    entry.tokens.output_tokens += event.tokens.output_tokens;
    entry.tokens.reasoning_output_tokens += event.tokens.reasoning_output_tokens;
    entry.tokens.total_tokens += event.tokens.total_tokens;
    entry.pricing.add(cost);
    entry.event_count += 1;
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

    // Use a static SQL string with an extra parameter (?20) for the PRICING_MIXED
    // sentinel value, avoiding format! interpolation into SQL text.
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
            cache_read_tokens,
            cache_creation_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
            cost_with_cache_usd,
            cost_without_cache_usd,
            pricing_status,
            pricing_source,
            pricing_rate,
            event_count,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
        ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
            project_label = excluded.project_label,
            project_ref = excluded.project_ref,
            input_tokens = usage_bucket_30m.input_tokens + excluded.input_tokens,
            cache_read_tokens = usage_bucket_30m.cache_read_tokens + excluded.cache_read_tokens,
            cache_creation_tokens = usage_bucket_30m.cache_creation_tokens + excluded.cache_creation_tokens,
            output_tokens = usage_bucket_30m.output_tokens + excluded.output_tokens,
            reasoning_output_tokens = usage_bucket_30m.reasoning_output_tokens + excluded.reasoning_output_tokens,
            total_tokens = usage_bucket_30m.total_tokens + excluded.total_tokens,
            cost_with_cache_usd = usage_bucket_30m.cost_with_cache_usd + excluded.cost_with_cache_usd,
            cost_without_cache_usd = usage_bucket_30m.cost_without_cache_usd + excluded.cost_without_cache_usd,
            pricing_status = CASE
                WHEN usage_bucket_30m.pricing_status = excluded.pricing_status THEN usage_bucket_30m.pricing_status
                ELSE ?20
            END,
            pricing_source = CASE
                WHEN usage_bucket_30m.pricing_source IS excluded.pricing_source THEN usage_bucket_30m.pricing_source
                ELSE ?20
            END,
            pricing_rate = CASE
                WHEN usage_bucket_30m.pricing_rate IS excluded.pricing_rate THEN usage_bucket_30m.pricing_rate
                ELSE ?20
            END,
            event_count = usage_bucket_30m.event_count + excluded.event_count,
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
            rollup.tokens.cache_read_tokens,
            rollup.tokens.cache_creation_tokens,
            rollup.tokens.output_tokens,
            rollup.tokens.reasoning_output_tokens,
            rollup.tokens.total_tokens,
            rollup.pricing.cost_with_cache_usd(),
            rollup.pricing.cost_without_cache_usd(),
            rollup.pricing.pricing_status(),
            rollup.pricing.pricing_source(),
            rollup.pricing.pricing_rate(),
            rollup.event_count,
            updated_at,
            PRICING_MIXED,
        ])?;
    }
    Ok(())
}

fn refresh_bucket_pricing_after_reset_tx(
    tx: &rusqlite::Transaction<'_>,
    source: &str,
    buckets: &[(String, String, String)],
    updated_at: &str,
) -> Result<()> {
    if buckets.is_empty() {
        return Ok(());
    }

    let mut select_stmt = tx.prepare_cached(
        r#"
        SELECT
            cost_with_cache_usd,
            cost_without_cache_usd,
            pricing_status,
            pricing_source,
            pricing_rate
        FROM usage_event
        WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND COALESCE(project_hash, '') = ?4
        "#,
    )?;
    let mut update_stmt = tx.prepare_cached(
        r#"
        UPDATE usage_bucket_30m
        SET pricing_status = ?5,
            pricing_source = ?6,
            pricing_rate = ?7,
            updated_at = ?8
        WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
        "#,
    )?;

    let mut unique = HashSet::new();
    for (model, hour_start, project_hash) in buckets {
        if !unique.insert((model.clone(), hour_start.clone(), project_hash.clone())) {
            continue;
        }

        let rows = select_stmt.query_map(
            rusqlite::params![source, model, hour_start, project_hash],
            |row| {
                Ok(CostBreakdown {
                    cost_with_cache_usd: row.get::<_, Option<f64>>(0)?.unwrap_or_default(),
                    cost_without_cache_usd: row.get::<_, Option<f64>>(1)?.unwrap_or_default(),
                    pricing_status: match row
                        .get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| PRICING_UNPRICED.to_string())
                        .as_str()
                    {
                        "static" => pricing::PricingStatus::Static,
                        "snapshot" => pricing::PricingStatus::Snapshot,
                        _ => pricing::PricingStatus::Unpriced,
                    },
                    pricing_source: row.get(3)?,
                    pricing_rate: row.get(4)?,
                })
            },
        )?;
        let mut pricing = PricingRollup::default();
        for row in rows {
            pricing.add(&row?);
        }

        update_stmt.execute(rusqlite::params![
            source,
            model,
            hour_start,
            project_hash,
            pricing.pricing_status(),
            pricing.pricing_source(),
            pricing.pricing_rate(),
            updated_at,
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{SessionInfo, SourceKind, UsageEvent, UsageTokens},
        paths::AppPaths,
        store::FileCursor,
    };
    use tempfile::TempDir;

    fn build_paths(root: &std::path::Path) -> AppPaths {
        let root_dir = root.to_path_buf();
        AppPaths {
            db_path: root_dir.join("llmusage.db"),
            hook_cmd_path: root_dir.join("hook.cmd"),
            hook_sh_path: root_dir.join("hook.sh"),
            lock_path: root_dir.join("worker.lock"),
            bin_dir: root_dir.join("bin"),
            backups_dir: root_dir.join("backups"),
            exports_dir: root_dir.join("exports"),
            root_dir,
        }
    }

    fn build_event(suffix: &str, path_hash: &str, total: i64) -> UsageEvent {
        UsageEvent {
            event_key: format!("codex:{path_hash}:{suffix}"),
            source: SourceKind::Codex,
            model: "gpt-5".to_string(),
            event_at: "2026-05-01T10:00:00Z".to_string(),
            hour_start: "2026-05-01T10:00:00Z".to_string(),
            tokens: UsageTokens {
                input_tokens: total,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: total,
                reasoning_output_tokens: 0,
                total_tokens: total * 2,
            },
            project: None,
            session: Some(SessionInfo {
                session_id: format!("session:{path_hash}"),
                session_label: None,
                source_path_hash: Some(path_hash.to_string()),
            }),
        }
    }

    fn build_cursor(path_hash: &str) -> FileCursor {
        FileCursor {
            cursor_key: format!("cursor:{path_hash}"),
            file_path: format!("/tmp/{path_hash}.jsonl"),
            file_fingerprint: "fp".to_string(),
            file_size: 1024,
            file_mtime_ns: 0,
            tail_signature: "tail".to_string(),
            offset: 1024,
            last_total: None,
            last_model: Some("gpt-5".to_string()),
            updated_at: "2026-05-01T10:00:00Z".to_string(),
        }
    }

    /// Validates the reset → events → cursor protocol is upheld in a single shard:
    /// 1) seed one event under `path_hash_a` with total=100,
    /// 2) commit a shard that resets `path_hash_a` and writes 5 fresh events
    ///    summing to `2 * (10 + 20 + 30 + 40 + 50)` tokens plus a single cursor row.
    ///
    /// Asserts the seeded event is gone, the bucket reflects only the new events,
    /// and the cursor lands.
    #[test]
    fn commit_shard_runs_reset_then_events_then_cursor() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut writer = store.begin_sync_run()?;

        let seed = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: Vec::new(),
            events: vec![build_event("seed", "pathA", 100)],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;
        assert_eq!(seed.events_inserted, 1);

        let new_events = (0..5)
            .map(|index| build_event(&format!("ev{index}"), "pathA", 10 * (index + 1) as i64))
            .collect::<Vec<_>>();
        let stats = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: vec!["pathA".to_string()],
            events: new_events,
            cursors: vec![build_cursor("pathA")],
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;
        assert_eq!(stats.events_inserted, 5);

        let conn = store.open_connection()?;

        let event_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            event_count, 5,
            "reset 在 events 之前生效，旧 event 应被清理"
        );

        let bucket_total: i64 = conn.query_row(
            "SELECT total_tokens FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        let expected_total: i64 = 2 * (10 + 20 + 30 + 40 + 50);
        assert_eq!(
            bucket_total, expected_total,
            "bucket 总 tokens 应等于第二次写入 events 的总和"
        );

        let cursor_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_cursor WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cursor_count, 1, "cursor 应当落库");

        Ok(())
    }

    #[test]
    fn commit_shard_persists_costs_and_recomputes_bucket_pricing_after_reset() -> anyhow::Result<()>
    {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut writer = store.begin_sync_run()?;
        let seed_events = vec![
            build_event("seed-priced", "pathA", 10),
            UsageEvent {
                model: "unknown-model".to_string(),
                ..build_event("seed-unpriced", "pathA", 20)
            },
        ];
        let seed = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: Vec::new(),
            events: seed_events,
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;
        assert_eq!(seed.events_inserted, 2);

        let conn = store.open_connection()?;
        let (event_cost, event_status, event_source): (f64, String, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, pricing_status, COALESCE(pricing_source, '')
            FROM usage_event
            WHERE event_key = 'codex:pathA:seed-priced'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!(event_cost > 0.0);
        assert_eq!(event_status, "static");
        assert_eq!(event_source, "static-v1");

        let bucket_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bucket_count, 2, "different models keep separate buckets");

        drop(conn);
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: vec!["pathA".to_string()],
            events: vec![build_event("replacement", "pathA", 30)],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;

        let conn = store.open_connection()?;
        let (bucket_cost, bucket_status, bucket_source, event_count): (f64, String, String, i64) =
            conn.query_row(
                r#"
            SELECT
                b.cost_with_cache_usd,
                b.pricing_status,
                COALESCE(b.pricing_source, ''),
                b.event_count
            FROM usage_bucket_30m b
            WHERE b.source = 'codex' AND b.model = 'gpt-5'
            "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let event_cost: f64 = conn.query_row(
            r#"
            SELECT COALESCE(SUM(cost_with_cache_usd), 0.0)
            FROM usage_event
            WHERE source = 'codex' AND model = 'gpt-5'
            "#,
            [],
            |row| row.get(0),
        )?;
        assert!(bucket_cost > 0.0);
        assert!(
            (bucket_cost - event_cost).abs() < 1e-9,
            "bucket cost should match persisted event cost after reset"
        );
        assert_eq!(bucket_status, "static");
        assert_eq!(bucket_source, "static-v1");
        assert_eq!(event_count, 1);

        let unpriced_bucket_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex' AND model = 'unknown-model'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            unpriced_bucket_count, 0,
            "reset removes emptied buckets instead of leaving stale mixed pricing"
        );

        Ok(())
    }

    #[test]
    fn commit_shard_uses_active_local_pricing_snapshot() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let pricing_dir = paths.root_dir.join("pricing");
        std::fs::create_dir_all(&pricing_dir)?;
        std::fs::write(
            pricing_dir.join("litellm-snapshot-2026-05.json"),
            r#"{
                "version": "litellm-snapshot-2026-05",
                "models": [
                    {
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 2.0,
                        "cached_per_mtok": 0.2,
                        "output_per_mtok": 20.0
                    }
                ]
            }"#,
        )?;
        store.set_meta_value("pricing_catalog_version", "litellm-snapshot-2026-05")?;

        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: Vec::new(),
            events: vec![UsageEvent {
                tokens: UsageTokens {
                    input_tokens: 500_000,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                    output_tokens: 100_000,
                    reasoning_output_tokens: 0,
                    total_tokens: 600_000,
                },
                ..build_event("snapshot", "pathSnapshot", 1)
            }],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;

        let conn = store.open_connection()?;
        let (status, source, cost): (String, String, f64) = conn.query_row(
            r#"
            SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
            FROM usage_event
            WHERE event_key = 'codex:pathSnapshot:snapshot'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "snapshot");
        assert_eq!(source, "litellm-snapshot-2026-05");
        assert!((cost - 3.0).abs() < 1e-6);

        Ok(())
    }

    #[test]
    fn commit_shard_prices_claude_cache_creation_and_read_channels() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Claude,
            reset_path_hashes: Vec::new(),
            events: vec![UsageEvent {
                event_key: "claude:pathCache:1".to_string(),
                source: SourceKind::Claude,
                model: "claude-sonnet-4-5".to_string(),
                event_at: "2026-05-01T10:00:00Z".to_string(),
                hour_start: "2026-05-01T10:00:00Z".to_string(),
                tokens: UsageTokens {
                    input_tokens: 1_000_000,
                    cache_read_tokens: 2_000_000,
                    cache_creation_tokens: 3_000_000,
                    output_tokens: 4_000_000,
                    reasoning_output_tokens: 5_000_000,
                    total_tokens: 15_000_000,
                },
                project: None,
                session: Some(SessionInfo {
                    session_id: "session:pathCache".to_string(),
                    session_label: None,
                    source_path_hash: Some("pathCache".to_string()),
                }),
            }],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        })?;

        let conn = store.open_connection()?;
        let (event_cost, without_cache, status, source): (f64, f64, String, String) = conn
            .query_row(
                r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd,
                   pricing_status, COALESCE(pricing_source, '')
            FROM usage_event
            WHERE event_key = 'claude:pathCache:1'
            "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        assert!((event_cost - 72.6).abs() < 1e-9);
        assert!((without_cache - 78.0).abs() < 1e-9);
        assert_eq!(status, "static");
        assert_eq!(source, "static-v1");

        let (bucket_cost, cache_creation, cache_read, bucket_status): (f64, i64, i64, String) =
            conn.query_row(
                r#"
            SELECT cost_with_cache_usd, cache_creation_tokens, cache_read_tokens, pricing_status
            FROM usage_bucket_30m
            WHERE source = 'claude' AND model = 'claude-sonnet-4-5'
            "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        assert!((bucket_cost - event_cost).abs() < 1e-9);
        assert_eq!(cache_creation, 3_000_000);
        assert_eq!(cache_read, 2_000_000);
        assert_eq!(bucket_status, "static");

        Ok(())
    }

    #[test]
    fn begin_sync_run_propagates_invalid_snapshot_error() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let pricing_dir = paths.root_dir.join("pricing");
        std::fs::create_dir_all(&pricing_dir)?;
        std::fs::write(pricing_dir.join("broken-snapshot.json"), "{not-json")?;
        store.set_meta_value("pricing_catalog_version", "broken-snapshot")?;

        let err = match store.begin_sync_run() {
            Ok(_) => panic!("invalid active pricing snapshot must not fall back to static-v1"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("broken-snapshot"),
            "unexpected error shape: {err}"
        );

        Ok(())
    }

    #[test]
    fn active_pricing_catalog_rejects_version_mismatch() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let pricing_dir = paths.root_dir.join("pricing");
        std::fs::create_dir_all(&pricing_dir)?;
        std::fs::write(
            pricing_dir.join("expected-version.json"),
            r#"{
                "version": "actual-version",
                "models": []
            }"#,
        )?;
        store.set_meta_value("pricing_catalog_version", "expected-version")?;

        let err = store
            .active_pricing_catalog()
            .expect_err("metadata/catalog version mismatch must be explicit");
        assert!(
            err.to_string().contains("expected-version")
                && err.to_string().contains("actual-version"),
            "unexpected error: {err}"
        );

        Ok(())
    }
}
