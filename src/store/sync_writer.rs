use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use crate::error::Result;
use rusqlite::{Transaction, TransactionBehavior};
use tracing::info;

use super::{
    BucketKey, BucketRollup, FileCursor, HolderKind, LOCAL_HOST_ID, PricingRollup,
    ShardCommitStats, Store, SyncRunWriter, SyncShard,
    cursor::{persist_opencode_cursor_tx, persist_zcode_cursor_tx},
    schema::{
        omp_split_migrated_key, read_meta_value, reset_for_source_tx,
        token_accounting_key_for_host, write_meta_value,
    },
};
use crate::{
    domain::{
        pricing::{self, CostBreakdown, PRICING_MIXED, PRICING_UNPRICED},
        pricing_catalog::PricingCatalog,
        provider_map::ProviderIndex,
    },
    error::LlmusageError,
    models::{ProjectInfo, SourceKind, UsageEvent, UsageTokens, UsageToolCall, UsageTurn},
    util::now_utc,
};

// Tests can observe exclusive stages without changing the production protocol.
macro_rules! writer_stage {
    ($name:literal, $body:expr) => {{
        #[cfg(test)]
        let _stage = profiling::Stage::enter($name);
        $body
    }};
}

#[cfg(test)]
pub(crate) mod profiling;

fn audit_now() -> String {
    #[cfg(test)]
    if let Some(now) = profiling::audit_now() {
        return now;
    }
    now_utc()
}

fn audit_run_started_at() -> String {
    #[cfg(test)]
    if let Some(now) = profiling::audit_now() {
        return now;
    }
    crate::util::now_utc_millis()
}

/// Maximum events per insert batch within the shard's single transaction.
///
/// Owned by the writer side of the protocol so parsers stay agnostic to
/// SQLite batch sizing. Removing this constant is a deletion-test signal:
/// each parser would have to reintroduce its own chunking constant.
const EVENT_WRITE_BATCH_SIZE: usize = 1000;

// Choose the smaller indexed candidate set without changing path or host
// identity, aggregation order, or deletion order.
const RESET_EVENT_AGGREGATE_SQL: &str = r#"
                SELECT
                    COALESCE(provider_label, ''),
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
                FROM usage_event INDEXED BY idx_usage_event_source_path_hash
                WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3
                GROUP BY COALESCE(provider_label, ''), model, hour_start, COALESCE(project_hash, '')
                "#;
const RESET_HOST_COUNT_SQL: &str = "SELECT COUNT(*) FROM usage_event INDEXED BY idx_usage_event_host_source_event_at WHERE host_id = ?1 AND source = ?2";
const RESET_PATH_COUNT_SQL: &str = "SELECT COUNT(*) FROM (SELECT 1 FROM usage_event INDEXED BY idx_usage_event_source_path_hash WHERE source = ?1 AND source_path_hash = ?2 LIMIT ?3)";

const RESET_EVENT_DELETE_SQL: &str = "DELETE FROM usage_event INDEXED BY idx_usage_event_source_path_hash WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3";

const RESET_EVENT_AGGREGATE_DEFAULT_SQL: &str = r#"
                SELECT
                    COALESCE(provider_label, ''),
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
                WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3
                GROUP BY COALESCE(provider_label, ''), model, hour_start, COALESCE(project_hash, '')
                "#;
const RESET_EVENT_DELETE_DEFAULT_SQL: &str =
    "DELETE FROM usage_event WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3";

const RESET_BUCKET_PRICING_SELECT_SQL: &str = r#"
    SELECT
        b.provider_label,
        b.model,
        b.hour_start,
        b.project_hash,
        e.cost_with_cache_usd,
        e.cost_without_cache_usd,
        e.pricing_status,
        e.pricing_source,
        e.pricing_rate
    FROM usage_event AS e INDEXED BY idx_usage_event_source_path_hash
    JOIN temp.llmusage_reset_bucket AS b
      ON b.provider_label = COALESCE(e.provider_label, '')
     AND b.model = e.model
     AND b.hour_start = e.hour_start
     AND b.project_hash = COALESCE(e.project_hash, '')
    WHERE e.source = ?1
      AND e.host_id = ?2
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShardCommitFailpoint {
    Reset,
    Events,
    Cursor,
    SourceFile,
    Raw,
    BehaviorReset,
    Turns,
    ToolCalls,
}

fn fail_shard_commit_at(
    active: Option<ShardCommitFailpoint>,
    target: ShardCommitFailpoint,
) -> Result<()> {
    if active == Some(target) {
        return Err(LlmusageError::ConfigInvalid {
            detail: format!("test failpoint during shard commit: {target:?}"),
        });
    }
    Ok(())
}

type AfterCommitShardHook = Box<dyn FnMut(&Store, &SyncShard) + 'static>;

thread_local! {
    static AFTER_COMMIT_SHARD: std::cell::RefCell<Option<AfterCommitShardHook>> =
        const { std::cell::RefCell::new(None) };
}

/// Guard that clears the `commit_shard` post-commit hook on drop.
#[must_use]
#[doc(hidden)]
pub struct AfterCommitShardGuard;

impl Drop for AfterCommitShardGuard {
    fn drop(&mut self) {
        AFTER_COMMIT_SHARD.with(|slot| *slot.borrow_mut() = None);
    }
}

/// Installs a hook that runs after a successful persisted [`SyncRunWriter::commit_shard`].
///
/// Test-only observer of the shipped commit path: a shard with events/tool
/// calls but no SQLite cursor is visible here if those writes were split
/// across transactions.
#[doc(hidden)]
pub fn set_after_commit_shard_hook(
    hook: impl FnMut(&Store, &SyncShard) + 'static,
) -> AfterCommitShardGuard {
    AFTER_COMMIT_SHARD.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
    AfterCommitShardGuard
}

fn invoke_after_commit_shard(store: &Store, shard: &SyncShard) {
    AFTER_COMMIT_SHARD.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(store, shard);
        }
    });
}

impl Store {
    pub fn begin_sync_run(&self) -> Result<SyncRunWriter> {
        self.begin_sync_run_with_provider_index(None)
    }

    pub fn begin_sync_run_with_provider_index(
        &self,
        provider_index: Option<ProviderIndex>,
    ) -> Result<SyncRunWriter> {
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
            store: self.clone(),
            conn: Some(conn),
            permit: self.write_permit.clone(),
            run_started_at: audit_run_started_at(),
            raw_archive_enabled,
            pricing_catalog,
            provider_index,
            collect_sink: None,
        })
    }

    /// Collects shards through `commit_shard` without writing SQLite.
    ///
    /// Used by `sync --emit-shards`. The callback receives each shard in commit
    /// order. Host prefixes are not applied here; the local importer owns that.
    pub fn begin_collect_run<F>(&self, on_shard: F) -> Result<SyncRunWriter>
    where
        F: FnMut(SyncShard) -> Result<()> + Send + 'static,
    {
        Ok(SyncRunWriter {
            store: self.clone(),
            conn: None,
            permit: None,
            run_started_at: audit_run_started_at(),
            raw_archive_enabled: false,
            pricing_catalog: PricingCatalog::embedded().clone(),
            provider_index: None,
            collect_sink: Some(Box::new(on_shard)),
        })
    }
}

impl SyncRunWriter {
    fn reset_file_events_batch_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
        path_hashes: &[String],
    ) -> Result<()> {
        #[cfg(test)]
        if profiling::is_baseline() {
            return profiling::reset_file_events_baseline_tx(tx, source, host_id, path_hashes);
        }
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

        // 4.1 在 shard 事务里扣减 bucket 并删除旧 event
        #[cfg(test)]
        profiling::record_reset("adaptive");
        let mut unique = HashSet::new();
        let use_adaptive_plan = path_hashes
            .iter()
            .skip(1)
            .any(|path| path != &path_hashes[0]);
        {
            let mut aggregate_path_stmt = use_adaptive_plan
                .then(|| tx.prepare_cached(RESET_EVENT_AGGREGATE_SQL))
                .transpose()?;
            let mut aggregate_default_stmt =
                tx.prepare_cached(RESET_EVENT_AGGREGATE_DEFAULT_SQL)?;
            // Keep the original plan for one distinct path. Multiple paths
            // share one host/source count; deletions reduce each probe's bound.
            let mut host_candidates: Option<i64> = if use_adaptive_plan {
                Some(writer_stage!(
                    "reset_selectivity",
                    tx.query_row(
                        RESET_HOST_COUNT_SQL,
                        rusqlite::params![host_id, source.as_str()],
                        |row| row.get(0),
                    )
                )?)
            } else {
                None
            };
            let mut path_count_stmt = use_adaptive_plan
                .then(|| tx.prepare_cached(RESET_PATH_COUNT_SQL))
                .transpose()?;
            let mut update_bucket_stmt = tx.prepare_cached(
                r#"
                UPDATE usage_bucket_30m
                SET
                    input_tokens = input_tokens - ?7,
                    cache_read_tokens = cache_read_tokens - ?8,
                    cache_creation_tokens = cache_creation_tokens - ?9,
                    output_tokens = output_tokens - ?10,
                    reasoning_output_tokens = reasoning_output_tokens - ?11,
                    total_tokens = total_tokens - ?12,
                    cost_with_cache_usd = cost_with_cache_usd - ?13,
                    cost_without_cache_usd = cost_without_cache_usd - ?14,
                    event_count = event_count - ?15,
                    updated_at = ?16
                WHERE host_id = ?1
                  AND source = ?2
                  AND provider_label = ?3
                  AND model = ?4
                  AND hour_start = ?5
                  AND project_hash = ?6
                "#,
            )?;
            let mut delete_zero_stmt = tx.prepare_cached(
                r#"
                DELETE FROM usage_bucket_30m
                WHERE host_id = ?1
                  AND source = ?2
                  AND provider_label = ?3
                  AND model = ?4
                  AND hour_start = ?5
                  AND project_hash = ?6
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
            let mut delete_path_stmt = use_adaptive_plan
                .then(|| tx.prepare_cached(RESET_EVENT_DELETE_SQL))
                .transpose()?;
            let mut delete_default_stmt = tx.prepare_cached(RESET_EVENT_DELETE_DEFAULT_SQL)?;
            let updated_at = audit_now();
            let mut touched_buckets = Vec::new();

            for path_hash in path_hashes {
                if !unique.insert(path_hash.clone()) {
                    continue;
                }

                let prefer_path = match host_candidates {
                    Some(bound) if bound > 0 => {
                        let path_candidates: i64 = writer_stage!(
                            "reset_selectivity",
                            path_count_stmt
                                .as_mut()
                                .expect("adaptive reset owns a path count statement")
                                .query_row(
                                    rusqlite::params![source.as_str(), path_hash, bound],
                                    |row| row.get(0),
                                )
                        )?;
                        path_candidates < bound
                    }
                    _ => false,
                };
                #[cfg(test)]
                profiling::count(
                    if prefer_path {
                        "reset_path_index_paths"
                    } else {
                        "reset_default_plan_paths"
                    },
                    1,
                );
                let aggregate_stmt = if prefer_path {
                    aggregate_path_stmt
                        .as_mut()
                        .expect("adaptive reset owns a path aggregate statement")
                } else {
                    &mut aggregate_default_stmt
                };
                let delete_event_stmt = if prefer_path {
                    delete_path_stmt
                        .as_mut()
                        .expect("adaptive reset owns a path delete statement")
                } else {
                    &mut delete_default_stmt
                };
                #[cfg(test)]
                let aggregate_profile = profiling::Stage::enter("reset_aggregate");
                let rows = aggregate_stmt.query_map(
                    rusqlite::params![source.as_str(), host_id, path_hash],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            UsageTokens {
                                input_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                                cache_read_tokens: row
                                    .get::<_, Option<i64>>(5)?
                                    .unwrap_or_default(),
                                cache_creation_tokens: row
                                    .get::<_, Option<i64>>(6)?
                                    .unwrap_or_default(),
                                output_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                                reasoning_output_tokens: row
                                    .get::<_, Option<i64>>(8)?
                                    .unwrap_or_default(),
                                total_tokens: row.get::<_, Option<i64>>(9)?.unwrap_or_default(),
                            },
                            row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
                            row.get::<_, Option<f64>>(11)?.unwrap_or_default(),
                            row.get::<_, Option<i64>>(12)?.unwrap_or_default(),
                        ))
                    },
                )?;
                let aggregates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                #[cfg(test)]
                drop(aggregate_profile);

                for (
                    provider_label,
                    model,
                    hour_start,
                    project_hash,
                    tokens,
                    cost_with_cache_usd,
                    cost_without_cache_usd,
                    event_count,
                ) in aggregates
                {
                    #[cfg(test)]
                    let _bucket_profile = profiling::Stage::enter("reset_bucket_update");
                    update_bucket_stmt.execute(rusqlite::params![
                        host_id,
                        source.as_str(),
                        &provider_label,
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
                        host_id,
                        source.as_str(),
                        &provider_label,
                        &model,
                        &hour_start,
                        &project_hash,
                    ])?;
                    if deleted_empty_bucket == 0 {
                        touched_buckets.push((provider_label, model, hour_start, project_hash));
                    }
                }

                let deleted = writer_stage!(
                    "reset_delete",
                    delete_event_stmt.execute(rusqlite::params![
                        source.as_str(),
                        host_id,
                        path_hash
                    ])
                )?;
                if let Some(bound) = host_candidates.as_mut() {
                    *bound -= deleted as i64;
                }
                #[cfg(test)]
                profiling::count("events_deleted", deleted);
                #[cfg(not(test))]
                let _ = deleted;
            }

            writer_stage!(
                "reset_pricing",
                refresh_bucket_pricing_after_reset_tx(
                    tx,
                    source.as_str(),
                    host_id,
                    &touched_buckets,
                    &updated_at,
                )
            )?;
            #[cfg(test)]
            profiling::count("touched_bucket_candidates", touched_buckets.len());
        }
        info!(source = %source, "完成重放旧事件清理");
        Ok(())
    }

    fn write_event_batch_tx(
        tx: &Transaction<'_>,
        pricing_catalog: &PricingCatalog,
        host_id: &str,
        events: &[UsageEvent],
    ) -> Result<usize> {
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

        // 5.1 在 shard 事务中插入 event，并为新 event 做内存聚合
        let now = audit_now();
        let inserted = {
            let mut event_stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_event(
                    event_key, host_id, source, provider_label, model, event_at, hour_start,
                    input_tokens, cache_read_tokens, cache_creation_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                    project_hash, project_label, project_ref, path_hash,
                    session_id, session_label, source_path_hash,
                    created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)
                "#,
            )?;
            let mut projects = HashMap::new();
            let mut buckets = HashMap::new();
            let mut inserted = 0usize;

            for event in events {
                let cost = writer_stage!(
                    "pricing",
                    pricing::cost_for_event(
                        pricing_catalog,
                        event.source.as_str(),
                        &event.model,
                        pricing::CostTokens {
                            input: event.tokens.input_tokens,
                            cache_read: event.tokens.cache_read_tokens,
                            cache_creation: event.tokens.cache_creation_tokens,
                            output: event.tokens.output_tokens,
                            reasoning_output: event.tokens.reasoning_output_tokens,
                        },
                        event.source_cost.as_ref(),
                    )
                );
                let changed = writer_stage!(
                    "event_insert",
                    event_stmt.execute(rusqlite::params![
                        event.event_key,
                        host_id,
                        event.source.as_str(),
                        event.provider_label,
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
                    ])
                )?;
                if changed == 0 {
                    continue;
                }

                inserted += 1;
                if let Some(project) = &event.project {
                    projects.insert(project.project_hash.clone(), project.clone());
                }
                roll_up_bucket(&mut buckets, host_id, event, &cost);
            }
            drop(event_stmt);

            // 5.2 将项目维表和 30 分钟桶一次性刷入
            writer_stage!("projects", flush_projects_tx(tx, &projects))?;
            writer_stage!("buckets", flush_buckets_tx(tx, &buckets))?;
            inserted
        };
        info!(batch = events.len(), inserted, "完成批量写入 usage_event");
        Ok(inserted)
    }

    fn write_cursor_batch_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
        cursors: &[FileCursor],
    ) -> Result<()> {
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

        // 6.1 用 shard 事务 upsert 本轮发生变化的 cursor
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO source_cursor(
                    host_id,
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
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(host_id, source, cursor_key) DO UPDATE SET
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
                #[cfg(test)]
                let fixed_audit = profiling::audit_now();
                #[cfg(test)]
                let updated_at = fixed_audit.as_deref().unwrap_or(&cursor.updated_at);
                #[cfg(not(test))]
                let updated_at = &cursor.updated_at;
                stmt.execute(rusqlite::params![
                    host_id,
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
                    updated_at,
                ])?;
            }
        }
        info!(source = %source, "完成批量刷新 cursor");
        Ok(())
    }

    pub fn finish_sync_run(self) -> Result<()> {
        info!("完成 sync 单写入端收尾");
        Ok(())
    }

    pub fn commit_shard(&mut self, shard: SyncShard) -> Result<ShardCommitStats> {
        if self.collect_sink.is_some() {
            let stats = ShardCommitStats {
                events_inserted: shard.events.len(),
                write_ms: 0,
                files_seen: shard.seen_file_paths.len(),
                turns_inserted: shard.turns.len(),
                tool_calls_inserted: shard.tool_calls.len(),
            };
            if let Some(sink) = self.collect_sink.as_mut() {
                sink(shard)?;
            }
            return Ok(stats);
        }

        let hook_shard = AFTER_COMMIT_SHARD
            .with(|slot| slot.borrow().is_some())
            .then(|| shard.clone());
        let stats = self.commit_shard_inner(shard, None)?;
        if let Some(hook_shard) = hook_shard.as_ref() {
            invoke_after_commit_shard(&self.store, hook_shard);
        }
        Ok(stats)
    }

    /// Atomically commits complete Antigravity source-family snapshots.
    ///
    /// The caller must finish discovery and decoding, reject incomplete/missing
    /// members, and check cancellation before calling. Bounded imports carry
    /// no resets/cursors; certification covers token semantics, not history
    /// completeness. A nonempty path-reset list denotes a complete ordinary
    /// source snapshot: its cursors and seen paths replace that source/host's
    /// entire membership. Explicit rebuild additionally replaces all attributed
    /// history for the selected source/host. Hook-era
    /// unattributed rows always survive. Accounting markers commit with usage.
    /// Collect mode emits ordinary shards, never a destructive rebuild flag.
    pub fn commit_antigravity_snapshot(
        &mut self,
        shards: Vec<SyncShard>,
        rebuild: bool,
    ) -> Result<Vec<ShardCommitStats>> {
        self.commit_antigravity_snapshot_inner(shards, rebuild, None)
    }

    fn commit_antigravity_snapshot_inner(
        &mut self,
        mut shards: Vec<SyncShard>,
        rebuild: bool,
        failpoint: Option<(usize, ShardCommitFailpoint)>,
    ) -> Result<Vec<ShardCommitStats>> {
        let mut selected = HashSet::new();
        for shard in &shards {
            if !matches!(
                shard.source,
                SourceKind::Antigravity | SourceKind::AntigravityIde
            ) || !selected.insert((shard.source, shard.host_id.clone()))
            {
                return Err(LlmusageError::ConfigInvalid {
                    detail: "Antigravity snapshots require one shard per selected Antigravity source and host".to_string(),
                });
            }
        }
        if shards.is_empty() {
            return Ok(Vec::new());
        }
        if self.collect_sink.is_some() {
            return shards
                .into_iter()
                .map(|shard| self.commit_shard(shard))
                .collect();
        }

        #[cfg(test)]
        let profile = profiling::Scope::start(
            "antigravity_transaction",
            &shards.iter().collect::<Vec<_>>(),
        );
        for shard in &mut shards {
            writer_stage!("pre_host_prefix", apply_host_prefix(shard));
            writer_stage!("pre_behavior_dedupe", dedupe_behavior_facts(shard));
            writer_stage!("pre_provider", {
                if let Some(index) = &self.provider_index {
                    for event in &mut shard.events {
                        if event.provider_label.is_empty() {
                            event.provider_label = index.label_for(event.source, &event.event_at);
                        }
                    }
                }
            });
        }
        #[cfg(test)]
        let transaction_started = Instant::now();
        let operation = writer_stage!(
            "operation",
            if self.permit.is_none() {
                Some(self.store.write_operation(HolderKind::Library)?)
            } else {
                None
            }
        );
        let permit = match self.permit.as_ref() {
            Some(permit) => permit.clone(),
            None => operation
                .as_ref()
                .expect("writer owns a temporary operation")
                .store
                .write_permit()?
                .clone(),
        };
        let tx = writer_stage!(
            "begin",
            self.conn
                .as_mut()
                .expect("persist writer has a connection")
                .transaction_with_behavior(TransactionBehavior::Immediate)
        )?;
        writer_stage!("fence_before", permit.validate_in_transaction(&tx))?;
        // Reset every selected owner before inserting any winners, so an event
        // moving between CLI and IDE cannot collide with its previous owner.
        writer_stage!("group_reset", {
            for shard in &shards {
                if rebuild {
                    Self::reset_antigravity_attributed_history_tx(
                        &tx,
                        shard.source,
                        &shard.host_id,
                    )?;
                } else {
                    Self::reset_file_events_batch_tx(
                        &tx,
                        shard.source,
                        &shard.host_id,
                        &shard.reset_path_hashes,
                    )?;
                    Self::reset_behavior_facts_batch_tx(
                        &tx,
                        shard.source,
                        &shard.host_id,
                        &shard.reset_path_hashes,
                    )?;
                }
                if rebuild || !shard.reset_path_hashes.is_empty() {
                    Self::reset_antigravity_membership_tx(&tx, shard.source, &shard.host_id)?;
                }
            }
        });
        let mut results = Vec::with_capacity(shards.len());
        for (index, shard) in shards.iter().enumerate() {
            let started = Instant::now();
            let active_failpoint = failpoint
                .filter(|(target, _)| *target == index)
                .map(|(_, point)| point);
            let mut stats = Self::apply_shard_tx(
                &tx,
                &self.pricing_catalog,
                self.raw_archive_enabled,
                &self.run_started_at,
                shard,
                active_failpoint,
                false,
            )?;
            writer_stage!(
                "marker",
                write_meta_value(
                    &tx,
                    &token_accounting_key_for_host(&shard.host_id, shard.source),
                    &super::expected_token_accounting_version(shard.source).to_string(),
                )
            )?;
            stats.files_seen = shard.seen_file_paths.len();
            let elapsed = started.elapsed();
            stats.write_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
            #[cfg(test)]
            profiling::source_apply(elapsed);
            results.push(stats);
        }
        writer_stage!("fence_after", permit.validate_in_transaction(&tx))?;
        writer_stage!("commit", tx.commit())?;
        #[cfg(test)]
        profile.finish(transaction_started.elapsed());
        for shard in &shards {
            invoke_after_commit_shard(&self.store, shard);
        }
        Ok(results)
    }

    fn reset_antigravity_attributed_history_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
    ) -> Result<()> {
        let path_hashes = {
            let mut statement = tx.prepare(
                "SELECT DISTINCT source_path_hash FROM usage_event WHERE source = ?1 AND host_id = ?2 AND COALESCE(source_path_hash, '') <> ''",
            )?;
            statement
                .query_map(rusqlite::params![source.as_str(), host_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        tx.execute(
            "DELETE FROM usage_event_raw WHERE event_key IN (SELECT event_key FROM usage_event WHERE source = ?1 AND host_id = ?2 AND COALESCE(source_path_hash, '') <> '')",
            rusqlite::params![source.as_str(), host_id],
        )?;
        Self::reset_file_events_batch_tx(tx, source, host_id, &path_hashes)?;
        Self::reset_behavior_facts_batch_tx(tx, source, host_id, &path_hashes)?;
        tx.execute(
            "DELETE FROM source_sync_status WHERE source = ?1 AND host_id = ?2",
            rusqlite::params![source.as_str(), host_id],
        )?;
        Ok(())
    }

    fn reset_antigravity_membership_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
    ) -> Result<()> {
        tx.execute(
            "DELETE FROM source_cursor WHERE source = ?1 AND host_id = ?2",
            rusqlite::params![source.as_str(), host_id],
        )?;
        super::source_file::delete_for_source_in_tx(tx, source.as_str(), host_id)
    }

    #[cfg(test)]
    fn commit_shard_with_failpoint(
        &mut self,
        shard: SyncShard,
        failpoint: ShardCommitFailpoint,
    ) -> Result<ShardCommitStats> {
        self.commit_shard_inner(shard, Some(failpoint))
    }

    fn commit_shard_inner(
        &mut self,
        mut shard: SyncShard,
        failpoint: Option<ShardCommitFailpoint>,
    ) -> Result<ShardCommitStats> {
        /*
         * ========================================================================
         * 步骤7：原子化提交单个 shard
         * ========================================================================
         * 目标：
         * 1) 把 reset → write_event(分批) → write_cursor 的隐式协议固化
         * 2) 让 parser 不再关心写入顺序与 batch 大小
         * 3) 统一返回 inserted 数与本次提交耗时
         */
        #[cfg(test)]
        let profile = profiling::Scope::start("shard", &[&shard]);
        writer_stage!("pre_host_prefix", apply_host_prefix(&mut shard));
        writer_stage!("pre_behavior_dedupe", dedupe_behavior_facts(&mut shard));
        #[cfg(test)]
        {
            profiling::count("turns_deduped", shard.turns.len());
            profiling::count("tools_deduped", shard.tool_calls.len());
        }
        info!(
            source = %shard.source,
            resets = shard.reset_path_hashes.len(),
            events = shard.events.len(),
            cursors = shard.cursors.len(),
            seen_files = shard.seen_file_paths.len(),
            raw_records = shard.raw_records.len(),
            turns = shard.turns.len(),
            tool_calls = shard.tool_calls.len(),
            "开始提交 shard"
        );

        // 7.1 计时入口与累加器
        let started = Instant::now();
        writer_stage!("provider", {
            if let Some(index) = self.provider_index.as_ref() {
                for event in &mut shard.events {
                    if event.provider_label.is_empty() {
                        event.provider_label = index.label_for(event.source, &event.event_at);
                    }
                }
            }
        });
        let pricing_catalog = &self.pricing_catalog;
        let raw_archive_enabled = self.raw_archive_enabled;
        let run_started_at = self.run_started_at.clone();
        let host_id = shard.host_id.clone();
        let operation = writer_stage!(
            "operation",
            if self.permit.is_none() {
                Some(self.store.write_operation(HolderKind::Library)?)
            } else {
                None
            }
        );
        let permit = match self.permit.as_ref() {
            Some(permit) => permit.clone(),
            None => operation
                .as_ref()
                .expect("unfenced writer must own a temporary operation")
                .store
                .write_permit()?
                .clone(),
        };
        let conn = self
            .conn
            .as_mut()
            .expect("persist writer keeps a SQLite connection");
        let tx = writer_stage!(
            "begin",
            conn.transaction_with_behavior(TransactionBehavior::Immediate)
        )?;
        writer_stage!("fence_before", permit.validate_in_transaction(&tx))?;
        writer_stage!(
            "migration",
            Self::migrate_omp_split_if_needed_tx(&tx, shard.source, &host_id, &run_started_at)
        )?;

        let mut stats = Self::apply_shard_tx(
            &tx,
            pricing_catalog,
            raw_archive_enabled,
            &run_started_at,
            &shard,
            failpoint,
            true,
        )?;
        writer_stage!("fence_after", permit.validate_in_transaction(&tx))?;
        writer_stage!("commit", tx.commit())?;

        stats.files_seen = shard.seen_file_paths.len();
        let elapsed = started.elapsed();
        stats.write_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
        #[cfg(test)]
        {
            profiling::count("events_inserted", stats.events_inserted);
            profiling::count("turns_inserted", stats.turns_inserted);
            profiling::count("tools_inserted", stats.tool_calls_inserted);
            profile.finish(elapsed);
        }
        info!(
            source = %shard.source,
            inserted = stats.events_inserted,
            turns_inserted = stats.turns_inserted,
            tool_calls_inserted = stats.tool_calls_inserted,
            write_ms = stats.write_ms,
            "完成 shard 提交"
        );
        Ok(stats)
    }

    fn apply_shard_tx(
        tx: &Transaction<'_>,
        pricing_catalog: &PricingCatalog,
        raw_archive_enabled: bool,
        run_started_at: &str,
        shard: &SyncShard,
        failpoint: Option<ShardCommitFailpoint>,
        reset_paths: bool,
    ) -> Result<ShardCommitStats> {
        let mut stats = ShardCommitStats::default();
        let host_id = &shard.host_id;

        // 7.2 先清旧 event，再批写 event，最后落 cursor —— 顺序由协议保证
        if reset_paths && !shard.reset_path_hashes.is_empty() {
            writer_stage!(
                "reset_events",
                Self::reset_file_events_batch_tx(
                    tx,
                    shard.source,
                    host_id,
                    &shard.reset_path_hashes
                )
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::Reset)?;
        for batch in shard.events.chunks(EVENT_WRITE_BATCH_SIZE) {
            stats.events_inserted += writer_stage!(
                "event_work",
                Self::write_event_batch_tx(tx, pricing_catalog, host_id, batch)
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::Events)?;
        writer_stage!("cursor", {
            if !shard.cursors.is_empty() {
                Self::write_cursor_batch_tx(tx, shard.source, host_id, &shard.cursors)?;
            }
            if let Some(cursor) = shard.opencode_cursor.as_deref() {
                persist_opencode_cursor_tx(tx, host_id, cursor)?;
            }
            if let Some(cursor) = shard.zcode_cursor.as_deref() {
                persist_zcode_cursor_tx(tx, host_id, cursor)?;
            }
        });
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::Cursor)?;
        // 7.3 把本轮看到的候选文件登记为 source_file.state='live'
        //     （D15 / ADR 0006）。OpenCode 等无 file 身份的源传空 vec。
        if !shard.seen_file_paths.is_empty() {
            writer_stage!(
                "inventory",
                Self::write_source_file_seen_tx(
                    tx,
                    shard.source,
                    host_id,
                    &shard.seen_file_paths,
                    run_started_at,
                )
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::SourceFile)?;
        // 7.4 raw archive opt-in（D11 / F1.5）：开关关时丢弃 raw_records，
        //     避免 parser 端必须同步判定开关；开关开时与 event 共享 commit
        //     周期落库（INSERT OR IGNORE 保证 event_key 重复时幂等）。
        if raw_archive_enabled && !shard.raw_records.is_empty() {
            writer_stage!(
                "raw",
                Self::write_raw_records_batch_tx(tx, &shard.raw_records)
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::Raw)?;
        // 7.5 行为事实是 usage_event/bucket 之外的独立 normalized 表。
        //     reset 同源文件时先清掉旧 path_hash 关联事实，随后 INSERT OR IGNORE
        //     新事实；未支持行为提取的 parser 可继续传空 vec。
        if reset_paths && !shard.reset_path_hashes.is_empty() {
            writer_stage!(
                "behavior_reset",
                Self::reset_behavior_facts_batch_tx(
                    tx,
                    shard.source,
                    host_id,
                    &shard.reset_path_hashes,
                )
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::BehaviorReset)?;
        if !shard.turns.is_empty() {
            stats.turns_inserted += writer_stage!(
                "turns",
                Self::write_turn_batch_tx(tx, host_id, &shard.turns)
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::Turns)?;
        if !shard.tool_calls.is_empty() {
            stats.tool_calls_inserted += writer_stage!(
                "tools",
                Self::write_tool_call_batch_tx(tx, host_id, &shard.tool_calls)
            )?;
        }
        fail_shard_commit_at(failpoint, ShardCommitFailpoint::ToolCalls)?;
        Ok(stats)
    }

    fn migrate_omp_split_if_needed_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
        run_started_at: &str,
    ) -> Result<()> {
        if source != SourceKind::Omp || host_id == LOCAL_HOST_ID {
            return Ok(());
        }
        let key = omp_split_migrated_key(host_id);
        if read_meta_value(tx, &key)?.is_some() {
            return Ok(());
        }
        info!(
            host_id,
            "resetting pre-split pi rows before first omp shard"
        );
        reset_for_source_tx(tx, SourceKind::Pi, host_id)?;
        write_meta_value(tx, &key, run_started_at)?;
        Ok(())
    }

    fn reset_behavior_facts_batch_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
        path_hashes: &[String],
    ) -> Result<()> {
        if path_hashes.is_empty() {
            return Ok(());
        }

        tx.execute_batch(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS llmusage_reset_path(
                path_hash TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            DELETE FROM temp.llmusage_reset_path;
            "#,
        )?;
        {
            let mut insert_stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO temp.llmusage_reset_path(path_hash) VALUES (?1)",
            )?;
            for path_hash in path_hashes {
                insert_stmt.execute([path_hash])?;
            }
        }
        tx.execute(
            r#"
            DELETE FROM usage_tool_call
            WHERE source = ?1
              AND host_id = ?2
              AND source_path_hash IN (SELECT path_hash FROM temp.llmusage_reset_path)
            "#,
            rusqlite::params![source.as_str(), host_id],
        )?;
        tx.execute(
            r#"
            DELETE FROM usage_turn
            WHERE source = ?1
              AND host_id = ?2
              AND source_path_hash IN (SELECT path_hash FROM temp.llmusage_reset_path)
            "#,
            rusqlite::params![source.as_str(), host_id],
        )?;
        Ok(())
    }

    fn write_turn_batch_tx(
        tx: &Transaction<'_>,
        host_id: &str,
        turns: &[UsageTurn],
    ) -> Result<usize> {
        let inserted = {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_turn(
                    turn_key, host_id, source, session_id, source_path_hash, project_hash,
                    primary_model, started_at, category, has_edits, retries,
                    one_shot, call_count, input_tokens, cache_read_tokens,
                    cache_creation_tokens, output_tokens, reasoning_output_tokens,
                    total_tokens, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
                "#,
            )?;
            let now = audit_now();
            let mut inserted = 0usize;
            for turn in turns {
                inserted += stmt.execute(rusqlite::params![
                    turn.turn_key,
                    host_id,
                    turn.source.as_str(),
                    turn.session_id,
                    turn.source_path_hash,
                    turn.project_hash,
                    turn.primary_model,
                    turn.started_at,
                    turn.category.as_str(),
                    bool_to_i64(turn.has_edits),
                    turn.retries,
                    bool_to_i64(turn.one_shot),
                    turn.call_count,
                    turn.tokens.input_tokens,
                    turn.tokens.cache_read_tokens,
                    turn.tokens.cache_creation_tokens,
                    turn.tokens.output_tokens,
                    turn.tokens.reasoning_output_tokens,
                    turn.tokens.total_tokens,
                    now,
                ])?;
            }
            inserted
        };
        Ok(inserted)
    }

    fn write_tool_call_batch_tx(
        tx: &Transaction<'_>,
        host_id: &str,
        tool_calls: &[UsageToolCall],
    ) -> Result<usize> {
        let inserted = {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, host_id, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint,
                    safe_preview, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          ?13, ?14, ?15, ?16, ?17)
                "#,
            )?;
            let now = audit_now();
            let mut inserted = 0usize;
            for call in tool_calls {
                inserted += stmt.execute(rusqlite::params![
                    call.tool_call_key,
                    call.turn_key,
                    call.event_key,
                    host_id,
                    call.source.as_str(),
                    call.session_id,
                    call.source_path_hash,
                    call.project_hash,
                    call.model,
                    call.occurred_at,
                    call.tool_name,
                    call.tool_kind.as_str(),
                    call.mcp_server,
                    call.mcp_tool,
                    call.input_fingerprint,
                    call.safe_preview,
                    now,
                ])?;
            }
            inserted
        };
        Ok(inserted)
    }

    fn write_raw_records_batch_tx(
        tx: &Transaction<'_>,
        records: &[super::RawRecord],
    ) -> Result<()> {
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_event_raw(
                    event_key, raw_json, created_at
                ) VALUES (?1, ?2, ?3)
                "#,
            )?;
            let now = audit_now();
            for record in records {
                stmt.execute(rusqlite::params![record.event_key, record.raw_json, now])?;
            }
        }
        Ok(())
    }

    fn write_source_file_seen_tx(
        tx: &Transaction<'_>,
        source: SourceKind,
        host_id: &str,
        file_paths: &[String],
        run_started_at: &str,
    ) -> Result<()> {
        super::source_file::upsert_live_in_tx(
            tx,
            source.as_str(),
            host_id,
            file_paths,
            run_started_at,
        )?;
        Ok(())
    }
}

fn apply_host_prefix(shard: &mut SyncShard) {
    if shard.host_prefix_applied {
        return;
    }
    let host_id = shard.host_id.as_str();
    let source = shard.source.as_str();
    for event in &mut shard.events {
        event.event_key = format!("{host_id}:{}", event.event_key);
    }
    for turn in &mut shard.turns {
        turn.turn_key = prefix_turn_key(host_id, &turn.turn_key);
    }
    for call in &mut shard.tool_calls {
        if let Some(event_key) = &mut call.event_key {
            *event_key = format!("{host_id}:{event_key}");
        }
        if let Some(turn_key) = &mut call.turn_key {
            *turn_key = prefix_turn_key(host_id, turn_key);
        }
        call.tool_call_key = prefix_tool_call_key(source, host_id, &call.tool_call_key);
    }
    for record in &mut shard.raw_records {
        record.event_key = format!("{host_id}:{}", record.event_key);
    }
    shard.host_prefix_applied = true;
}

fn prefix_turn_key(host_id: &str, turn_key: &str) -> String {
    let rest = turn_key.strip_prefix("turn:").unwrap_or(turn_key);
    format!("turn:{host_id}:{rest}")
}

fn prefix_tool_call_key(source: &str, host_id: &str, tool_call_key: &str) -> String {
    let prefix = format!("tool:{source}:");
    let rest = tool_call_key.strip_prefix(&prefix).unwrap_or(tool_call_key);
    format!("tool:{source}:{host_id}:{rest}")
}

fn dedupe_behavior_facts(shard: &mut SyncShard) {
    let mut turn_keys = HashSet::new();
    shard
        .turns
        .retain(|turn| turn_keys.insert(turn.turn_key.clone()));
    let mut tool_keys = HashSet::new();
    shard
        .tool_calls
        .retain(|call| tool_keys.insert(call.tool_call_key.clone()));
}

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn roll_up_bucket(
    buckets: &mut HashMap<BucketKey, BucketRollup>,
    host_id: &str,
    event: &UsageEvent,
    cost: &CostBreakdown,
) {
    let project_hash = event
        .project
        .as_ref()
        .map(|value| value.project_hash.clone())
        .unwrap_or_default();
    let key = BucketKey {
        host_id: host_id.to_string(),
        source: event.source.as_str().to_string(),
        provider_label: event.provider_label.clone(),
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
    let updated_at = audit_now();
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

    // Use a static SQL string with an extra parameter (?21) for the PRICING_MIXED
    // sentinel value, avoiding format! interpolation into SQL text.
    let mut stmt = tx.prepare_cached(
        r#"
        INSERT INTO usage_bucket_30m(
            host_id,
            source,
            provider_label,
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
        ON CONFLICT(host_id, source, provider_label, model, hour_start, project_hash) DO UPDATE SET
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
                ELSE ?22
            END,
            pricing_source = CASE
                WHEN usage_bucket_30m.pricing_source IS excluded.pricing_source THEN usage_bucket_30m.pricing_source
                ELSE ?22
            END,
            pricing_rate = CASE
                WHEN usage_bucket_30m.pricing_rate IS excluded.pricing_rate THEN usage_bucket_30m.pricing_rate
                ELSE ?22
            END,
            event_count = usage_bucket_30m.event_count + excluded.event_count,
            updated_at = excluded.updated_at
        "#,
    )?;
    let updated_at = audit_now();
    for (key, rollup) in buckets {
        stmt.execute(rusqlite::params![
            key.host_id,
            key.source,
            key.provider_label,
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
    host_id: &str,
    buckets: &[(String, String, String, String)],
    updated_at: &str,
) -> Result<()> {
    if buckets.is_empty() {
        return Ok(());
    }

    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS llmusage_reset_bucket(
            provider_label TEXT NOT NULL,
            model TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            project_hash TEXT NOT NULL,
            PRIMARY KEY(provider_label, model, hour_start, project_hash)
        ) WITHOUT ROWID;
        DELETE FROM temp.llmusage_reset_bucket;
        "#,
    )?;
    {
        let mut insert_stmt = tx.prepare_cached(
            r#"
            INSERT OR IGNORE INTO temp.llmusage_reset_bucket(
                provider_label, model, hour_start, project_hash
            ) VALUES (?1, ?2, ?3, ?4)
            "#,
        )?;
        for (provider_label, model, hour_start, project_hash) in buckets {
            insert_stmt.execute(rusqlite::params![
                provider_label,
                model,
                hour_start,
                project_hash
            ])?;
        }
    }

    let mut pricing_by_bucket = HashMap::new();
    {
        let mut select_stmt = tx.prepare_cached(RESET_BUCKET_PRICING_SELECT_SQL)?;
        let rows = select_stmt.query_map(rusqlite::params![source, host_id], |row| {
            Ok((
                (
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ),
                CostBreakdown {
                    cost_with_cache_usd: row.get::<_, Option<f64>>(4)?.unwrap_or_default(),
                    cost_without_cache_usd: row.get::<_, Option<f64>>(5)?.unwrap_or_default(),
                    pricing_status: pricing::PricingStatus::from_stored(
                        row.get::<_, Option<String>>(6)?
                            .unwrap_or_else(|| PRICING_UNPRICED.to_string())
                            .as_str(),
                    ),
                    pricing_source: row.get(7)?,
                    pricing_rate: row.get(8)?,
                },
            ))
        })?;
        for row in rows {
            let (key, cost) = row?;
            pricing_by_bucket
                .entry(key)
                .or_insert_with(PricingRollup::default)
                .add(&cost);
        }
    }

    let mut update_stmt = tx.prepare_cached(
        r#"
        UPDATE usage_bucket_30m
        SET pricing_status = ?7,
            pricing_source = ?8,
            pricing_rate = ?9,
            updated_at = ?10
        WHERE host_id = ?1
          AND source = ?2
          AND provider_label = ?3
          AND model = ?4
          AND hour_start = ?5
          AND project_hash = ?6
        "#,
    )?;

    for ((provider_label, model, hour_start, project_hash), pricing) in pricing_by_bucket {
        update_stmt.execute(rusqlite::params![
            host_id,
            source,
            &provider_label,
            &model,
            &hour_start,
            &project_hash,
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
        models::{
            ActivityCategory, SessionInfo, SourceCost, SourceKind, ToolKind, UsageEvent,
            UsageTokens, UsageToolCall, UsageTurn,
        },
        paths::AppPaths,
        store::{BootstrapOptions, FileCursor, OpencodeCursor, ZcodeCursor},
    };
    use tempfile::TempDir;

    fn build_paths(root: &std::path::Path) -> AppPaths {
        AppPaths::with_root(root.to_path_buf()).expect("test paths")
    }

    fn build_event(suffix: &str, path_hash: &str, total: i64) -> UsageEvent {
        UsageEvent {
            event_key: format!("codex:{path_hash}:{suffix}"),
            source: SourceKind::Codex,
            provider_label: String::new(),
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
            source_cost: None,
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

    fn antigravity_shard(source: SourceKind, path: &str, value: i64) -> SyncShard {
        let mut event = build_event("generation", path, value);
        event.source = source;
        event.event_key = format!("{}:{path}:generation", source.as_str());
        SyncShard {
            events: vec![event],
            cursors: vec![build_cursor(path)],
            seen_file_paths: vec![format!("/tmp/{path}.jsonl")],
            ..SyncShard::new(source)
        }
    }

    fn source_total(store: &Store, source: SourceKind, host: &str) -> Result<(i64, i64)> {
        Ok(store.open_connection()?.query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_event WHERE source = ?1 AND host_id = ?2",
            rusqlite::params![source.as_str(), host],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    #[test]
    fn antigravity_snapshot_rebuild_preserves_hooks_and_other_hosts() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        store.set_raw_archive(true)?;
        let mut writer = store.begin_sync_run()?;
        let mut old = antigravity_shard(SourceKind::Antigravity, "old-cli", 100);
        let mut hook = build_event("hook", "unused", 5);
        hook.source = SourceKind::Antigravity;
        hook.event_key = "antigravity:hook".to_string();
        hook.session = None;
        let mut empty_hook = hook.clone();
        empty_hook.event_key = "antigravity:empty-hook".to_string();
        empty_hook.tokens.total_tokens = 14;
        empty_hook.tokens.input_tokens = 7;
        empty_hook.tokens.output_tokens = 7;
        empty_hook.session = Some(SessionInfo {
            session_id: "old-hook".to_string(),
            session_label: None,
            source_path_hash: Some(String::new()),
        });
        old.raw_records = vec![
            super::super::RawRecord {
                event_key: old.events[0].event_key.clone(),
                raw_json: "{}".to_string(),
            },
            super::super::RawRecord {
                event_key: hook.event_key.clone(),
                raw_json: "{}".to_string(),
            },
        ];
        old.events.extend([hook, empty_hook]);
        writer.commit_shard(old)?;
        writer.commit_shard(antigravity_shard(SourceKind::AntigravityIde, "old-ide", 13))?;
        let mut remote = antigravity_shard(SourceKind::Antigravity, "old-cli", 19);
        remote.host_id = "remote".to_string();
        writer.commit_shard(remote)?;
        writer.commit_shard(SyncShard {
            events: vec![build_event("unrelated", "codex", 23)],
            ..SyncShard::new(SourceKind::Codex)
        })?;
        store.set_meta_value("token_accounting_version.antigravity", "2")?;
        store.set_meta_value("token_accounting_version.antigravity_ide", "2")?;

        writer.commit_antigravity_snapshot(
            vec![
                antigravity_shard(SourceKind::Antigravity, "new-cli", 11),
                antigravity_shard(SourceKind::AntigravityIde, "new-ide", 17),
            ],
            true,
        )?;
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "local")?,
            (3, 46)
        );
        assert_eq!(
            source_total(&store, SourceKind::AntigravityIde, "local")?,
            (1, 34)
        );
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "remote")?,
            (1, 38)
        );
        assert_eq!(source_total(&store, SourceKind::Codex, "local")?, (1, 46));
        assert_eq!(store.retained_antigravity_history_count("local")?, 2);
        assert_eq!(store.retained_antigravity_history_count("remote")?, 0);
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(3)
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::AntigravityIde)?,
            Some(3)
        );
        assert!(!store.has_legacy_token_accounting(SourceKind::Antigravity)?);
        let cursors = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?;
        assert_eq!(cursors.len(), 1);
        assert!(cursors.contains_key("cursor:new-cli"));
        assert_eq!(
            store
                .source_files()
                .tracked_paths(SourceKind::Antigravity, "local")?,
            vec!["/tmp/new-cli.jsonl"]
        );
        let conn = store.open_connection()?;
        let bucket_total: i64 = conn.query_row("SELECT SUM(total_tokens) FROM usage_bucket_30m WHERE source = 'antigravity' AND host_id = 'local'", [], |row| row.get(0))?;
        assert_eq!(bucket_total, 46);
        let raw_keys = conn
            .prepare("SELECT event_key FROM usage_event_raw ORDER BY event_key")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert_eq!(raw_keys, vec!["local:antigravity:hook"]);
        drop(conn);
        store
            .sync_status()
            .mark_recent_completed(SourceKind::Antigravity, "local", now_utc())?;
        let reopened = Store::new(&paths)?;
        let statuses = reopened.sync_status().load_source_sync_statuses("local")?;
        let status = statuses
            .iter()
            .find(|row| row.source == "antigravity")
            .expect("source status");
        assert_eq!(status.token_accounting_version, Some(3));
        assert!(!status.legacy_token_accounting);
        assert!(
            status
                .token_accounting_warning
                .as_deref()
                .is_some_and(|warning| warning.contains("2 hook-era"))
        );
        Ok(())
    }

    #[test]
    fn antigravity_snapshot_failure_rolls_back_every_source_and_marker() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = Store::new(&build_paths(temp.path()))?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        for source in [SourceKind::Antigravity, SourceKind::AntigravityIde] {
            writer.commit_shard(antigravity_shard(source, "old", 10))?;
            store.set_meta_value(&token_accounting_key_for_host("local", source), "2")?;
        }
        let error = writer
            .commit_antigravity_snapshot_inner(
                vec![
                    antigravity_shard(SourceKind::Antigravity, "new", 20),
                    antigravity_shard(SourceKind::AntigravityIde, "new", 30),
                ],
                true,
                Some((1, ShardCommitFailpoint::Events)),
            )
            .expect_err("second-source failure");
        assert!(error.to_string().contains("failpoint"));
        for source in [SourceKind::Antigravity, SourceKind::AntigravityIde] {
            assert_eq!(source_total(&store, source, "local")?, (1, 20));
            assert_eq!(store.token_accounting_version(source)?, Some(2));
            let cursors = store.cursors().load_file_cursors(source, "local")?;
            assert!(cursors.contains_key("cursor:old"));
            assert!(!cursors.contains_key("cursor:new"));
            assert_eq!(
                store.source_files().tracked_paths(source, "local")?,
                vec!["/tmp/old.jsonl"]
            );
        }
        Ok(())
    }

    #[test]
    fn antigravity_snapshot_resets_all_owners_before_inserting_a_transfer() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let transferred_path = temp.path().join("transferred.db");
        std::fs::write(&transferred_path, b"test artifact")?;
        let transferred = transferred_path.to_string_lossy().to_string();
        let store = Store::new(&build_paths(temp.path()))?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        let mut old = antigravity_shard(SourceKind::AntigravityIde, "ide-group", 10);
        old.events[0].event_key = "antigravity:shared-response".to_string();
        old.cursors[0].cursor_key = transferred.clone();
        old.cursors[0].file_path = transferred.clone();
        old.seen_file_paths = vec![transferred.clone()];
        writer.commit_shard(old)?;
        let mut cli = antigravity_shard(SourceKind::Antigravity, "cli-group", 20);
        cli.events[0].event_key = "antigravity:shared-response".to_string();
        cli.cursors[0].cursor_key = transferred.clone();
        cli.cursors[0].file_path = transferred.clone();
        cli.seen_file_paths = vec![transferred.clone()];
        cli.reset_path_hashes = vec!["cli-group".to_string()];
        let ide = SyncShard {
            reset_path_hashes: vec!["ide-group".to_string()],
            ..SyncShard::new(SourceKind::AntigravityIde)
        };
        writer.commit_antigravity_snapshot(vec![cli, ide], false)?;
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "local")?,
            (1, 40)
        );
        assert_eq!(
            source_total(&store, SourceKind::AntigravityIde, "local")?,
            (0, 0)
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::AntigravityIde, "local")?
                .is_empty()
        );
        assert!(
            store
                .source_files()
                .tracked_paths(SourceKind::AntigravityIde, "local")?
                .is_empty()
        );
        assert_eq!(
            store
                .source_files()
                .tracked_paths(SourceKind::Antigravity, "local")?,
            vec![transferred.clone()]
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Antigravity, "local")?
                .contains_key(&transferred)
        );
        std::fs::remove_file(&transferred_path)?;
        let former_owner = store
            .source_files()
            .lossy_rebuild_risk(SourceKind::AntigravityIde, "local")?;
        assert_eq!(
            former_owner.missing_file_count, 0,
            "a moved member must not block its former owner after deletion"
        );
        assert!(
            store
                .source_files()
                .lossy_rebuild_risk(SourceKind::Antigravity, "local")?
                .has_risk()
        );
        Ok(())
    }

    #[test]
    fn antigravity_bounded_snapshot_keeps_full_history_cursor() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = Store::new(&build_paths(temp.path()))?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(antigravity_shard(SourceKind::Antigravity, "old", 10))?;
        let mut bounded = antigravity_shard(SourceKind::Antigravity, "recent", 20);
        bounded.cursors.clear();
        writer.commit_antigravity_snapshot(vec![bounded], false)?;
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "local")?,
            (2, 60)
        );
        let cursors = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?;
        assert_eq!(cursors.len(), 1);
        assert!(cursors.contains_key("cursor:old"));
        assert_eq!(
            store
                .source_files()
                .tracked_paths(SourceKind::Antigravity, "local")?,
            vec!["/tmp/old.jsonl", "/tmp/recent.jsonl"]
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(3)
        );
        Ok(())
    }

    #[test]
    fn antigravity_remote_legacy_rows_do_not_block_empty_local_source() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = Store::new(&build_paths(temp.path()))?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        let mut remote = antigravity_shard(SourceKind::Antigravity, "remote-history", 10);
        remote.host_id = "remote".to_string();
        writer.commit_shard(remote)?;
        store.set_meta_value("token_accounting_version.remote.antigravity", "2")?;
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        assert!(!store.has_legacy_token_accounting(SourceKind::Antigravity)?);
        writer.commit_antigravity_snapshot(
            vec![antigravity_shard(
                SourceKind::Antigravity,
                "local-first",
                20,
            )],
            false,
        )?;
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(3)
        );
        assert_eq!(
            store.token_accounting_version_for_host("remote", SourceKind::Antigravity)?,
            Some(2)
        );
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "remote")?,
            (1, 20)
        );
        Ok(())
    }

    #[test]
    fn antigravity_collect_snapshot_has_no_local_rebuild_effects() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = Store::new(&build_paths(temp.path()))?;
        store.bootstrap()?;
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = captured.clone();
        let mut writer = store.begin_collect_run(move |shard| {
            sink.lock().expect("capture lock").push(shard);
            Ok(())
        })?;
        writer.commit_antigravity_snapshot(
            vec![antigravity_shard(SourceKind::Antigravity, "emitted", 10)],
            true,
        )?;
        assert_eq!(
            source_total(&store, SourceKind::Antigravity, "local")?,
            (0, 0)
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Antigravity, "local")?
                .is_empty()
        );
        let shards = captured.lock().expect("capture lock");
        assert_eq!(shards.len(), 1);
        assert_eq!(
            shards[0].events[0].event_key,
            "antigravity:emitted:generation"
        );
        assert!(shards[0].reset_path_hashes.is_empty());
        Ok(())
    }

    fn build_tool_call(event: &UsageEvent, tool_name: &str) -> UsageToolCall {
        UsageToolCall {
            tool_call_key: format!("tool:{}:{tool_name}", event.event_key),
            turn_key: Some(format!("turn:{}", event.event_key)),
            event_key: Some(event.event_key.clone()),
            source: event.source,
            session_id: event
                .session
                .as_ref()
                .map(|session| session.session_id.clone()),
            source_path_hash: event
                .session
                .as_ref()
                .and_then(|session| session.source_path_hash.clone()),
            project_hash: event
                .project
                .as_ref()
                .map(|project| project.project_hash.clone()),
            model: Some(event.model.clone()),
            occurred_at: event.event_at.clone(),
            tool_name: tool_name.to_string(),
            tool_kind: ToolKind::Read,
            mcp_server: None,
            mcp_tool: None,
            input_fingerprint: Some(format!("fp:{tool_name}")),
            safe_preview: Some(format!("{tool_name} preview")),
        }
    }

    fn build_behavior_turn(event: &UsageEvent, category: ActivityCategory) -> UsageTurn {
        UsageTurn {
            category,
            ..UsageTurn::from_event(event, category)
        }
    }

    fn build_full_replacement_shard(path_hash: &str) -> SyncShard {
        let replacement = build_event("replacement", path_hash, 20);
        SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec![path_hash.to_string()],
            events: vec![replacement.clone()],
            cursors: vec![build_cursor(path_hash)],
            seen_file_paths: vec![format!("/tmp/{path_hash}.jsonl")],
            raw_records: vec![super::super::RawRecord {
                event_key: replacement.event_key.clone(),
                raw_json: r#"{"replacement":true}"#.to_string(),
            }],
            turns: vec![UsageTurn {
                has_edits: true,
                ..build_behavior_turn(&replacement, ActivityCategory::Coding)
            }],
            tool_calls: vec![UsageToolCall {
                tool_kind: ToolKind::Edit,
                ..build_tool_call(&replacement, "Edit")
            }],
            opencode_cursor: None,
            zcode_cursor: None,
        }
    }

    fn build_activity_index_benchmark_shards() -> Vec<SyncShard> {
        const SHARDS: usize = 8;
        const EVENTS_PER_SHARD: usize = 500;

        (0..SHARDS)
            .map(|shard_index| {
                let path_hash = format!("activity-index-benchmark-{shard_index:02}");
                let events = (0..EVENTS_PER_SHARD)
                    .map(|event_index| {
                        build_event(
                            &format!("{event_index:04}"),
                            &path_hash,
                            100 + (event_index % 17) as i64,
                        )
                    })
                    .collect();
                SyncShard {
                    source: SourceKind::Codex,
                    host_id: "local".to_string(),
                    host_prefix_applied: false,
                    reset_path_hashes: Vec::new(),
                    events,
                    cursors: vec![build_cursor(&path_hash)],
                    seen_file_paths: vec![format!("/tmp/{path_hash}.jsonl")],
                    raw_records: Vec::new(),
                    turns: Vec::new(),
                    tool_calls: Vec::new(),
                    opencode_cursor: None,
                    zcode_cursor: None,
                }
            })
            .collect()
    }

    fn measure_activity_index_sync(indexed: bool) -> anyhow::Result<std::time::Duration> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        if !indexed {
            store
                .open_connection()?
                .execute_batch("DROP INDEX idx_usage_event_activity_cost;")?;
        }
        let shards = build_activity_index_benchmark_shards();
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
        let fenced_store = lock.fenced_store();

        let started = Instant::now();
        let mut writer = fenced_store.begin_sync_run()?;
        for shard in shards {
            writer.commit_shard(shard)?;
        }
        writer.finish_sync_run()?;
        let elapsed = started.elapsed();
        let event_count: i64 =
            store
                .open_connection()?
                .query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        assert_eq!(event_count, 4_000, "benchmark must persist every event");
        drop(lock);
        Ok(elapsed)
    }

    fn median_duration(samples: &mut [std::time::Duration]) -> std::time::Duration {
        samples.sort_unstable();
        samples[samples.len() / 2]
    }

    #[test]
    #[ignore = "explicit single-thread production D1 throughput acceptance benchmark"]
    fn activity_cost_index_sync_throughput_regression_stays_within_ten_percent()
    -> anyhow::Result<()> {
        const ROUNDS: usize = 7;
        const MAX_REGRESSION_RATIO: f64 = 1.10;

        let mut baseline = Vec::with_capacity(ROUNDS);
        let mut indexed = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            if round % 2 == 0 {
                baseline.push(measure_activity_index_sync(false)?);
                indexed.push(measure_activity_index_sync(true)?);
            } else {
                indexed.push(measure_activity_index_sync(true)?);
                baseline.push(measure_activity_index_sync(false)?);
            }
        }

        let baseline_median = median_duration(&mut baseline);
        let indexed_median = median_duration(&mut indexed);
        let ratio = indexed_median.as_secs_f64() / baseline_median.as_secs_f64();
        eprintln!(
            "activity index sync throughput: baseline_median_ms={:.3} indexed_median_ms={:.3} ratio={ratio:.6} rounds={ROUNDS}",
            baseline_median.as_secs_f64() * 1_000.0,
            indexed_median.as_secs_f64() * 1_000.0,
        );
        assert!(
            ratio <= MAX_REGRESSION_RATIO,
            "Activity covering index sync regression ratio {ratio:.6} exceeds {MAX_REGRESSION_RATIO:.2}"
        );
        Ok(())
    }

    fn measure_top_sessions_cover_index_sync(indexed: bool) -> anyhow::Result<std::time::Duration> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        if !indexed {
            store
                .open_connection()?
                .execute_batch("DROP INDEX IF EXISTS idx_usage_event_top_sessions_cover;")?;
        }
        let shards = build_activity_index_benchmark_shards();
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
        let fenced_store = lock.fenced_store();

        let started = Instant::now();
        let mut writer = fenced_store.begin_sync_run()?;
        for shard in shards {
            writer.commit_shard(shard)?;
        }
        writer.finish_sync_run()?;
        let elapsed = started.elapsed();
        let event_count: i64 =
            store
                .open_connection()?
                .query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        assert_eq!(event_count, 4_000, "benchmark must persist every event");
        drop(lock);
        Ok(elapsed)
    }

    #[test]
    #[ignore = "explicit single-thread production D2 write-throughput acceptance benchmark"]
    fn top_sessions_cover_index_sync_throughput_regression_stays_within_ten_percent()
    -> anyhow::Result<()> {
        const ROUNDS: usize = 7;
        const MAX_REGRESSION_RATIO: f64 = 1.10;

        let mut baseline = Vec::with_capacity(ROUNDS);
        let mut indexed = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            if round % 2 == 0 {
                baseline.push(measure_top_sessions_cover_index_sync(false)?);
                indexed.push(measure_top_sessions_cover_index_sync(true)?);
            } else {
                indexed.push(measure_top_sessions_cover_index_sync(true)?);
                baseline.push(measure_top_sessions_cover_index_sync(false)?);
            }
        }

        let baseline_median = median_duration(&mut baseline);
        let indexed_median = median_duration(&mut indexed);
        let ratio = indexed_median.as_secs_f64() / baseline_median.as_secs_f64();
        eprintln!(
            "Top Sessions covering index sync throughput: baseline_median_ms={:.3} indexed_median_ms={:.3} ratio={ratio:.6} rounds={ROUNDS}",
            baseline_median.as_secs_f64() * 1_000.0,
            indexed_median.as_secs_f64() * 1_000.0,
        );
        assert!(
            ratio <= MAX_REGRESSION_RATIO,
            "Top Sessions covering index sync regression ratio {ratio:.6} exceeds {MAX_REGRESSION_RATIO:.2}"
        );
        Ok(())
    }

    #[test]
    fn stale_generation_cannot_commit_next_shard_transaction() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let first_store = Store::new(&paths)?;
        first_store.bootstrap()?;
        let second_store = Store::new(&paths)?;

        let first = first_store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Cli)?;
        let fenced_first = first.fenced_store();
        let mut writer = fenced_first.begin_sync_run()?;

        let conn = second_store.open_connection()?;
        conn.execute(
            "UPDATE worker_lock SET lease_expires_at = '2000-01-01T00:00:00Z'",
            [],
        )?;
        drop(conn);
        let second = second_store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;

        let event = build_event("stale", "stale-generation", 10);
        let event_key = event.event_key.clone();
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard.events.push(event);
        let error = writer
            .commit_shard(shard)
            .expect_err("a stolen generation must fence the stale writer");
        assert!(matches!(error, LlmusageError::LockLost));

        let conn = second_store.open_connection()?;
        let persisted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE event_key = ?1",
            [event_key],
            |row| row.get(0),
        )?;
        assert_eq!(persisted, 0, "stale shard must not commit any event");
        let error = writer
            .commit_antigravity_snapshot(
                vec![antigravity_shard(
                    SourceKind::Antigravity,
                    "stale-snapshot",
                    10,
                )],
                true,
            )
            .expect_err("a stolen generation must fence snapshots too");
        assert!(matches!(error, LlmusageError::LockLost));
        assert_eq!(
            source_total(&second_store, SourceKind::Antigravity, "local")?,
            (0, 0)
        );
        assert_eq!(
            second_store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        drop(second);
        drop(first);
        Ok(())
    }

    fn assert_seed_only_after_failed_shard(store: &Store, path_hash: &str) -> anyhow::Result<()> {
        let conn = store.open_connection()?;
        let seed_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE event_key = ?1",
            [format!("local:codex:{path_hash}:seed")],
            |row| row.get(0),
        )?;
        let replacement_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE event_key = ?1",
            [format!("local:codex:{path_hash}:replacement")],
            |row| row.get(0),
        )?;
        let (bucket_count, bucket_total): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let cursor_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_cursor WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        let source_file_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_file WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        let replacement_raw_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event_raw WHERE event_key = ?1",
            [format!("local:codex:{path_hash}:replacement")],
            |row| row.get(0),
        )?;
        let turn_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_turn WHERE source_path_hash = ?1",
            [path_hash],
            |row| row.get(0),
        )?;
        let edit_turn_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_turn WHERE source_path_hash = ?1 AND category = 'coding'",
            [path_hash],
            |row| row.get(0),
        )?;
        let tool_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_tool_call WHERE source_path_hash = ?1",
            [path_hash],
            |row| row.get(0),
        )?;
        let edit_tool_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_tool_call WHERE source_path_hash = ?1 AND tool_kind = 'edit'",
            [path_hash],
            |row| row.get(0),
        )?;

        assert_eq!(seed_events, 1, "seed event must remain after rollback");
        assert_eq!(
            replacement_events, 0,
            "replacement event must not survive failed shard"
        );
        assert_eq!(bucket_count, 1, "seed bucket must remain");
        assert_eq!(bucket_total, 20, "bucket should still reflect seed only");
        assert_eq!(cursor_count, 0, "cursor write must roll back");
        assert_eq!(source_file_count, 0, "source_file write must roll back");
        assert_eq!(replacement_raw_count, 0, "raw write must roll back");
        assert_eq!(turn_count, 1, "seed turn must remain after rollback");
        assert_eq!(edit_turn_count, 0, "replacement turn must roll back");
        assert_eq!(tool_count, 1, "seed tool call must remain after rollback");
        assert_eq!(edit_tool_count, 0, "replacement tool call must roll back");
        Ok(())
    }

    #[test]
    fn commit_shard_rolls_back_every_stage_on_failure() -> anyhow::Result<()> {
        let failpoints = [
            ShardCommitFailpoint::Reset,
            ShardCommitFailpoint::Events,
            ShardCommitFailpoint::Cursor,
            ShardCommitFailpoint::SourceFile,
            ShardCommitFailpoint::Raw,
            ShardCommitFailpoint::BehaviorReset,
            ShardCommitFailpoint::Turns,
            ShardCommitFailpoint::ToolCalls,
        ];

        for failpoint in failpoints {
            let temp = TempDir::new()?;
            let paths = build_paths(temp.path());
            let store = Store::new(&paths)?;
            store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
            let path_hash = "pathAtomic";
            let seed = build_event("seed", path_hash, 10);
            let mut writer = store.begin_sync_run()?;
            writer.commit_shard(SyncShard {
                source: SourceKind::Codex,
                host_id: "local".to_string(),
                host_prefix_applied: false,
                reset_path_hashes: Vec::new(),
                events: vec![seed.clone()],
                cursors: Vec::new(),
                seen_file_paths: Vec::new(),
                raw_records: vec![super::super::RawRecord {
                    event_key: seed.event_key.clone(),
                    raw_json: r#"{"seed":true}"#.to_string(),
                }],
                turns: vec![build_behavior_turn(&seed, ActivityCategory::Exploration)],
                tool_calls: vec![build_tool_call(&seed, "Read")],
                opencode_cursor: None,
                zcode_cursor: None,
            })?;

            let err = writer
                .commit_shard_with_failpoint(build_full_replacement_shard(path_hash), failpoint)
                .expect_err("test failpoint should abort shard");
            assert!(
                err.to_string().contains("test failpoint"),
                "unexpected error for {failpoint:?}: {err}"
            );
            drop(writer);

            assert_seed_only_after_failed_shard(&store, path_hash)
                .map_err(|err| err.context(format!("failpoint {failpoint:?}")))?;
        }

        Ok(())
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
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![build_event("seed", "pathA", 100)],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        assert_eq!(seed.events_inserted, 1);

        let new_events = (0..5)
            .map(|index| build_event(&format!("ev{index}"), "pathA", 10 * (index + 1) as i64))
            .collect::<Vec<_>>();
        let stats = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec!["pathA".to_string()],
            events: new_events,
            cursors: vec![build_cursor("pathA")],
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
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
    fn commit_shard_persists_sqlite_cursors_with_events_in_one_transaction() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut opencode_event = build_event("oc", "pathOc", 10);
        opencode_event.source = SourceKind::Opencode;
        opencode_event.event_key = "opencode:oc".to_string();
        let opencode_cursor = OpencodeCursor {
            last_time_created: 42,
            last_processed_ids: vec!["oc".to_string()],
            last_part_rowid: 7,
            sqlite_status: "ok".to_string(),
            updated_at: "2026-05-01T10:00:00Z".to_string(),
            ..OpencodeCursor::default()
        };
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            events: vec![opencode_event],
            opencode_cursor: Some(Box::new(opencode_cursor.clone())),
            ..SyncShard::new(SourceKind::Opencode)
        })?;
        drop(writer);

        let loaded = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(loaded.last_time_created, 42);
        assert_eq!(loaded.last_processed_ids, vec!["oc".to_string()]);
        assert_eq!(loaded.last_part_rowid, 7);
        let event_count: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'opencode'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(event_count, 1);

        let mut zcode_event = build_event("zc", "pathZc", 20);
        zcode_event.source = SourceKind::Zcode;
        zcode_event.event_key = "zcode:zc".to_string();
        let zcode_cursor = ZcodeCursor {
            last_completed_at: 99,
            last_processed_ids: vec!["zc".to_string()],
            sqlite_status: "ok".to_string(),
            updated_at: "2026-05-01T10:00:00Z".to_string(),
            ..ZcodeCursor::default()
        };
        let mut writer = store.begin_sync_run()?;
        // Cursor failpoint runs after sqlite cursor persist and before
        // `tx.commit()`, so a split write would leave events or the cursor.
        let err = writer
            .commit_shard_with_failpoint(
                SyncShard {
                    events: vec![zcode_event],
                    zcode_cursor: Some(Box::new(zcode_cursor)),
                    ..SyncShard::new(SourceKind::Zcode)
                },
                ShardCommitFailpoint::Cursor,
            )
            .expect_err("cursor failpoint must abort after sqlite cursor persist");
        assert!(err.to_string().contains("test failpoint"));
        drop(writer);

        let zcode_loaded = store.cursors().load_zcode_cursor("local")?;
        assert_eq!(zcode_loaded.last_completed_at, 0);
        assert!(zcode_loaded.last_processed_ids.is_empty());
        let zcode_events: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'zcode'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(zcode_events, 0);
        Ok(())
    }

    #[test]
    fn commit_shard_writes_behavior_facts_and_resets_them_by_source_path() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut writer = store.begin_sync_run()?;
        let first_event = build_event("first", "pathBehavior", 10);
        let first_turn = UsageTurn {
            category: ActivityCategory::Exploration,
            ..UsageTurn::from_event(&first_event, ActivityCategory::Exploration)
        };
        let first_tool = build_tool_call(&first_event, "Read");
        let stats = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![first_event],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: vec![first_turn],
            tool_calls: vec![first_tool],
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        assert_eq!(stats.events_inserted, 1);
        assert_eq!(stats.turns_inserted, 1);
        assert_eq!(stats.tool_calls_inserted, 1);

        let conn = store.open_connection()?;
        let turn_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_turn WHERE source_path_hash = 'pathBehavior'",
            [],
            |row| row.get(0),
        )?;
        let tool_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_tool_call WHERE source_path_hash = 'pathBehavior'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(turn_count, 1);
        assert_eq!(tool_count, 1);
        drop(conn);

        let replacement_event = build_event("replacement", "pathBehavior", 20);
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec!["pathBehavior".to_string()],
            events: vec![replacement_event.clone()],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: vec![UsageTurn {
                category: ActivityCategory::Coding,
                has_edits: true,
                one_shot: true,
                ..UsageTurn::from_event(&replacement_event, ActivityCategory::Coding)
            }],
            tool_calls: vec![UsageToolCall {
                tool_kind: ToolKind::Edit,
                ..build_tool_call(&replacement_event, "Edit")
            }],
            opencode_cursor: None,
            zcode_cursor: None,
        })?;

        let conn = store.open_connection()?;
        let (turn_count, category): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(category) FROM usage_turn WHERE source_path_hash = 'pathBehavior'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let (tool_count, tool_kind): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(tool_kind) FROM usage_tool_call WHERE source_path_hash = 'pathBehavior'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let bucket_total: i64 = conn.query_row(
            "SELECT total_tokens FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(turn_count, 1);
        assert_eq!(category, "coding");
        assert_eq!(tool_count, 1);
        assert_eq!(tool_kind, "edit");
        assert_eq!(
            bucket_total, 40,
            "behavior reset must not break usage_event bucket replacement"
        );

        Ok(())
    }

    #[test]
    fn shard_behavior_facts_are_deduped_before_sql() {
        let event = build_event("dedupe", "pathDedupe", 10);
        let turn = UsageTurn::from_event(&event, ActivityCategory::General);
        let tool = build_tool_call(&event, "Read");
        let mut shard = SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: vec![turn.clone(), turn],
            tool_calls: vec![tool.clone(), tool],
            opencode_cursor: None,
            zcode_cursor: None,
        };

        dedupe_behavior_facts(&mut shard);

        assert_eq!(shard.turns.len(), 1);
        assert_eq!(shard.tool_calls.len(), 1);
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
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: seed_events,
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        assert_eq!(seed.events_inserted, 2);

        let conn = store.open_connection()?;
        let (event_cost, event_status, event_source): (f64, String, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, pricing_status, COALESCE(pricing_source, '')
            FROM usage_event
            WHERE event_key = 'local:codex:pathA:seed-priced'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!(event_cost > 0.0);
        assert_eq!(event_status, "static");
        assert_eq!(event_source, "static-v3");

        let bucket_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bucket_count, 2, "different models keep separate buckets");

        drop(conn);
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec!["pathA".to_string()],
            events: vec![build_event("replacement", "pathA", 30)],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
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
        assert_eq!(bucket_source, "static-v3");
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
    fn reset_refreshes_shared_bucket_pricing_without_per_bucket_event_scans() -> anyhow::Result<()>
    {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let mut short = build_event("short", "pathShort", 10);
        short.model = "gpt-5.6-sol".to_string();
        let mut long = build_event("long", "pathLong", 300_000);
        long.model = "gpt-5.6-sol".to_string();

        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![short, long],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;

        let conn = store.open_connection()?;
        let initial_rate: String = conn.query_row(
            "SELECT pricing_rate FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(initial_rate, PRICING_MIXED);
        drop(conn);

        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec!["pathLong".to_string()],
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        drop(writer);

        let conn = store.open_connection()?;
        let event_rate: String = conn.query_row(
            "SELECT pricing_rate FROM usage_event WHERE source_path_hash = 'pathShort'",
            [],
            |row| row.get(0),
        )?;
        let bucket_rate: String = conn.query_row(
            "SELECT pricing_rate FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_ne!(event_rate, PRICING_MIXED);
        assert_eq!(bucket_rate, event_rate);

        conn.execute_batch(
            r#"
            CREATE TEMP TABLE llmusage_reset_bucket(
                provider_label TEXT NOT NULL,
                model TEXT NOT NULL,
                hour_start TEXT NOT NULL,
                project_hash TEXT NOT NULL,
                PRIMARY KEY(provider_label, model, hour_start, project_hash)
            ) WITHOUT ROWID;
            "#,
        )?;
        let explain_sql = format!("EXPLAIN QUERY PLAN {RESET_BUCKET_PRICING_SELECT_SQL}");
        let mut stmt = conn.prepare(&explain_sql)?;
        let plan = stmt
            .query_map(
                rusqlite::params![SourceKind::Codex.as_str(), "local"],
                |row| row.get::<_, String>(3),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(
            plan.iter()
                .filter(|detail| detail.contains("usage_event") || detail.contains(" e "))
                .count(),
            1,
            "pricing refresh must scan the source event range once: {plan:?}"
        );
        assert!(
            plan.iter().any(|detail| {
                detail.contains("idx_usage_event_source_path_hash") && detail.contains("source=?")
            }),
            "pricing refresh must constrain the single event scan by source: {plan:?}"
        );
        assert!(
            plan.iter()
                .any(|detail| detail.contains("PRIMARY KEY") && detail.contains("b")),
            "pricing refresh must probe touched buckets by temp-table primary key: {plan:?}"
        );

        Ok(())
    }

    #[test]
    fn commit_shard_splits_buckets_by_provider_label() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let map_path = temp.path().join("provider_activation.jsonl");
        std::fs::write(
            &map_path,
            r#"
{"platform":"codex","provider":"anyrouter","activated_at":"2026-05-01T10:00:00Z","event":"activate"}
{"platform":"codex","provider":"methink","activated_at":"2026-05-01T10:15:00Z","event":"activate"}
"#,
        )?;
        let provider_index = ProviderIndex::load(&map_path)?;
        let project = ProjectInfo {
            project_hash: "provider-project".to_string(),
            project_label: "Provider Project".to_string(),
            project_ref: None,
            repo_root_hash: "provider-root".to_string(),
            path_hash: "provider-path".to_string(),
        };
        let mut first = build_event("provider-a", "provider-path", 10);
        first.event_at = "2026-05-01T10:05:00Z".to_string();
        first.hour_start = "2026-05-01T10:00:00Z".to_string();
        first.project = Some(project.clone());
        let mut second = build_event("provider-b", "provider-path", 20);
        second.event_at = "2026-05-01T10:20:00Z".to_string();
        second.hour_start = "2026-05-01T10:00:00Z".to_string();
        second.project = Some(project);

        let mut writer = store.begin_sync_run_with_provider_index(Some(provider_index))?;
        let stats = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![first, second],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        assert_eq!(stats.events_inserted, 2);

        let conn = store.open_connection()?;
        let labels = {
            let mut stmt = conn.prepare(
                r#"
                SELECT provider_label
                FROM usage_event
                ORDER BY event_key
                "#,
            )?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(labels, vec!["anyrouter".to_string(), "methink".to_string()]);

        let buckets = {
            let mut stmt = conn.prepare(
                r#"
                SELECT provider_label, total_tokens, event_count
                FROM usage_bucket_30m
                WHERE source='codex'
                  AND model='gpt-5'
                  AND hour_start='2026-05-01T10:00:00Z'
                  AND project_hash='provider-project'
                ORDER BY provider_label
                "#,
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(
            buckets,
            vec![
                ("anyrouter".to_string(), 20, 1),
                ("methink".to_string(), 40, 1),
            ]
        );

        Ok(())
    }

    #[test]
    fn commit_shard_fills_empty_provider_label_without_overwriting() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;

        let map_path = temp.path().join("provider_activation.jsonl");
        std::fs::write(
            &map_path,
            r#"{"platform":"codex","provider":"anyrouter","activated_at":"2026-05-01T10:00:00Z","event":"activate"}"#,
        )?;
        let provider_index = ProviderIndex::load(&map_path)?;

        let mut stamped_codex = build_event("stamped-codex", "provider-path", 10);
        stamped_codex.provider_label = "keep-me".to_string();
        stamped_codex.event_at = "2026-05-01T10:05:00Z".to_string();
        stamped_codex.hour_start = "2026-05-01T10:00:00Z".to_string();

        let mut empty_codex = build_event("empty-codex", "provider-path", 20);
        empty_codex.event_at = "2026-05-01T10:05:00Z".to_string();
        empty_codex.hour_start = "2026-05-01T10:00:00Z".to_string();

        let mut stamped_dsh = build_event("stamped-dsh", "provider-path", 30);
        stamped_dsh.source = SourceKind::DeepseekHarness;
        stamped_dsh.event_key = "deepseek_harness:stamped-dsh".to_string();
        stamped_dsh.provider_label = "deepseek-official".to_string();
        stamped_dsh.event_at = "2026-05-01T10:05:00Z".to_string();
        stamped_dsh.hour_start = "2026-05-01T10:00:00Z".to_string();

        let mut writer = store.begin_sync_run_with_provider_index(Some(provider_index))?;
        let stats = writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![stamped_codex, empty_codex, stamped_dsh],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;
        assert_eq!(stats.events_inserted, 3);

        let conn = store.open_connection()?;
        let labels = {
            let mut stmt = conn.prepare(
                r#"
                SELECT event_key, provider_label
                FROM usage_event
                ORDER BY event_key
                "#,
            )?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(
            labels,
            vec![
                (
                    "local:codex:provider-path:empty-codex".to_string(),
                    "anyrouter".to_string()
                ),
                (
                    "local:codex:provider-path:stamped-codex".to_string(),
                    "keep-me".to_string()
                ),
                (
                    "local:deepseek_harness:stamped-dsh".to_string(),
                    "deepseek-official".to_string()
                ),
            ]
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
            host_id: "local".to_string(),
            host_prefix_applied: false,
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
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;

        let conn = store.open_connection()?;
        let (status, source, cost): (String, String, f64) = conn.query_row(
            r#"
            SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
            FROM usage_event
            WHERE event_key = 'local:codex:pathSnapshot:snapshot'
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
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: Vec::new(),
            events: vec![UsageEvent {
                event_key: "claude:pathCache:1".to_string(),
                source: SourceKind::Claude,
                provider_label: String::new(),
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
                source_cost: None,
            }],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;

        let conn = store.open_connection()?;
        let (event_cost, without_cache, status, source): (f64, f64, String, String) = conn
            .query_row(
                r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd,
                   pricing_status, COALESCE(pricing_source, '')
            FROM usage_event
            WHERE event_key = 'local:claude:pathCache:1'
            "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        assert!((event_cost - 72.6).abs() < 1e-9);
        assert!((without_cache - 78.0).abs() < 1e-9);
        assert_eq!(status, "static");
        assert_eq!(source, "static-v3");

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

    #[test]
    fn apply_host_prefix_rewrites_keys_once_and_keeps_null_turn_key() {
        let event = build_event("seed", "pathA", 10);
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard.events.push(event.clone());
        shard
            .turns
            .push(build_behavior_turn(&event, ActivityCategory::General));
        shard.tool_calls.push(UsageToolCall {
            turn_key: None,
            ..build_tool_call(&event, "Read")
        });
        shard.raw_records.push(super::super::RawRecord {
            event_key: event.event_key.clone(),
            raw_json: "{}".to_string(),
        });

        apply_host_prefix(&mut shard);
        assert!(shard.host_prefix_applied);
        assert_eq!(shard.events[0].event_key, "local:codex:pathA:seed");
        assert_eq!(shard.turns[0].turn_key, "turn:local:codex:pathA:seed");
        assert_eq!(
            shard.tool_calls[0].tool_call_key,
            "tool:codex:local:pathA:seed:Read"
        );
        assert_eq!(
            shard.tool_calls[0].event_key.as_deref(),
            Some("local:codex:pathA:seed")
        );
        assert_eq!(shard.tool_calls[0].turn_key, None);
        assert_eq!(shard.raw_records[0].event_key, "local:codex:pathA:seed");

        apply_host_prefix(&mut shard);
        assert_eq!(shard.events[0].event_key, "local:codex:pathA:seed");
        assert_eq!(shard.tool_calls[0].turn_key, None);
    }

    #[test]
    fn apply_host_prefix_skips_when_already_applied() {
        let mut shard = SyncShard::new_for_host(SourceKind::Codex, "devbox");
        shard.host_prefix_applied = true;
        shard.events.push(build_event("seed", "pathA", 10));
        apply_host_prefix(&mut shard);
        assert_eq!(shard.events[0].event_key, "codex:pathA:seed");
    }

    #[test]
    fn collect_only_commit_does_not_write_sqlite() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let mut persist = store.begin_sync_run()?;
        persist.commit_shard(SyncShard {
            events: vec![build_event("seed", "pathA", 10)],
            ..SyncShard::new(SourceKind::Codex)
        })?;
        persist.finish_sync_run()?;
        let before: i64 =
            store
                .open_connection()?
                .query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let collected_clone = std::sync::Arc::clone(&collected);
        let mut writer = store.begin_collect_run(move |shard| {
            collected_clone
                .lock()
                .expect("collect lock")
                .push(shard.events.len());
            Ok(())
        })?;
        writer.commit_shard(SyncShard {
            events: vec![build_event("new", "pathB", 20)],
            ..SyncShard::new(SourceKind::Codex)
        })?;
        writer.finish_sync_run()?;
        let after: i64 =
            store
                .open_connection()?
                .query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        assert_eq!(before, after);
        assert_eq!(*collected.lock().expect("collect lock"), vec![1]);
        Ok(())
    }

    #[test]
    fn sync_shard_serde_skips_raw_records() -> anyhow::Result<()> {
        let secret = "private prompt must never appear in diagnostics";
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard.raw_records.push(crate::store::RawRecord {
            event_key: "codex:path:1".to_string(),
            raw_json: format!(r#"{{"prompt":"{secret}"}}"#),
        });
        let json = serde_json::to_string(&shard)?;
        assert!(!json.contains("raw_records"), "{json}");
        assert!(!json.contains(secret), "{json}");
        let decoded: SyncShard = serde_json::from_str(&json)?;
        assert!(decoded.raw_records.is_empty());
        Ok(())
    }

    fn empty_shard(source: SourceKind, events: Vec<UsageEvent>) -> SyncShard {
        SyncShard {
            events,
            ..SyncShard::new(source)
        }
    }

    fn omp_event(
        suffix: &str,
        path_hash: &str,
        model: &str,
        source_cost: Option<SourceCost>,
        input: i64,
        output: i64,
    ) -> UsageEvent {
        UsageEvent {
            event_key: format!("omp:{path_hash}:{suffix}"),
            source: SourceKind::Omp,
            provider_label: String::new(),
            model: model.to_string(),
            event_at: "2026-05-01T10:00:00Z".to_string(),
            hour_start: "2026-05-01T10:00:00Z".to_string(),
            tokens: UsageTokens {
                input_tokens: input,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: output,
                reasoning_output_tokens: 0,
                total_tokens: input + output,
            },
            project: None,
            session: Some(SessionInfo {
                session_id: format!("session:{path_hash}"),
                session_label: None,
                source_path_hash: Some(path_hash.to_string()),
            }),
            source_cost,
        }
    }

    fn query_event_pricing(
        store: &Store,
        event_key: &str,
    ) -> anyhow::Result<(f64, f64, String, String)> {
        let conn = store.open_connection()?;
        Ok(conn.query_row(
            r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd,
                   pricing_status, COALESCE(pricing_source, '')
            FROM usage_event
            WHERE event_key = ?1
            "#,
            [event_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?)
    }

    #[test]
    fn source_reported_cost_wins_when_total_positive() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(empty_shard(
            SourceKind::Omp,
            vec![omp_event(
                "paid",
                "pathOmp",
                "deepseek-v4-flash",
                Some(SourceCost {
                    total: 0.032,
                    input: Some(0.01),
                    output: Some(0.02),
                    cache_read: Some(0.002),
                    cache_write: Some(0.0),
                }),
                1_000,
                200,
            )],
        ))?;

        let (with_cache, without_cache, status, source) =
            query_event_pricing(&store, "local:omp:pathOmp:paid")?;
        assert_eq!(status, "source_reported");
        assert_eq!(source, "source-reported");
        assert!((with_cache - 0.032).abs() < 1e-12);
        assert!((without_cache - 0.03).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn missing_zero_and_non_object_source_cost_use_catalog_and_keep_event() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(empty_shard(
            SourceKind::Omp,
            vec![
                omp_event("missing", "pathOmp", "deepseek-v4-flash", None, 10, 5),
                omp_event(
                    "zero",
                    "pathOmp",
                    "grok-4.6",
                    Some(SourceCost {
                        total: 0.0,
                        input: Some(0.0),
                        output: Some(0.0),
                        cache_read: Some(0.0),
                        cache_write: Some(0.0),
                    }),
                    10,
                    5,
                ),
            ],
        ))?;

        let count: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'omp'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 2, "catalog fallthrough must not drop events");

        let (_, _, missing_status, _) = query_event_pricing(&store, "local:omp:pathOmp:missing")?;
        let (_, _, zero_status, _) = query_event_pricing(&store, "local:omp:pathOmp:zero")?;
        assert_eq!(missing_status, "unpriced");
        assert_eq!(zero_status, "unpriced");
        Ok(())
    }

    #[test]
    fn overlay_omp_row_prices_zero_total_as_snapshot() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let overlay_path = temp.path().join("omp-overlay.json");
        std::fs::write(
            &overlay_path,
            r#"{
                "schema_version": 2,
                "kind": "overlay",
                "version": "omp-overlay-test",
                "models": [{
                    "id": "omp-flash",
                    "sources": ["omp"],
                    "matches": [{ "value": "deepseek-v4-flash", "mode": "exact" }],
                    "rates": {
                        "default": {
                            "input_per_mtok": 1.0,
                            "cached_per_mtok": 0.1,
                            "output_per_mtok": 2.0
                        }
                    }
                }]
            }"#,
        )?;
        store.apply_pricing_overlay(&overlay_path)?;

        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(empty_shard(
            SourceKind::Omp,
            vec![omp_event(
                "zero-overlay",
                "pathOmp",
                "deepseek-v4-flash",
                Some(SourceCost {
                    total: 0.0,
                    input: Some(0.0),
                    output: Some(0.0),
                    cache_read: None,
                    cache_write: None,
                }),
                1_000_000,
                1_000_000,
            )],
        ))?;

        let (with_cache, _, status, _) =
            query_event_pricing(&store, "local:omp:pathOmp:zero-overlay")?;
        assert_eq!(status, "snapshot");
        assert!((with_cache - 3.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn path_reset_keeps_source_reported_bucket_status() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = build_paths(temp.path());
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let cost = SourceCost {
            total: 0.125,
            input: Some(0.05),
            output: Some(0.075),
            cache_read: None,
            cache_write: None,
        };
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(empty_shard(
            SourceKind::Omp,
            vec![omp_event(
                "seed",
                "pathOmp",
                "deepseek-v4-flash",
                Some(cost.clone()),
                1_000,
                200,
            )],
        ))?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Omp,
            host_id: "local".to_string(),
            host_prefix_applied: false,
            reset_path_hashes: vec!["pathOmp".to_string()],
            events: vec![omp_event(
                "replay",
                "pathOmp",
                "deepseek-v4-flash",
                Some(cost),
                1_000,
                200,
            )],
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            opencode_cursor: None,
            zcode_cursor: None,
        })?;

        let conn = store.open_connection()?;
        let bucket_status: String = conn.query_row(
            r#"
            SELECT pricing_status FROM usage_bucket_30m
            WHERE source = 'omp' AND model = 'deepseek-v4-flash'
            "#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bucket_status, "source_reported");
        Ok(())
    }
}
