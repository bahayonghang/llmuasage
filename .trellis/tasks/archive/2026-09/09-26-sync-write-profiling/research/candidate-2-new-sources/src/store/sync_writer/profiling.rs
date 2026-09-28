//! Private measurement support. No user data or product switches enter this module.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

use serde::Serialize;

use super::profiling;
use super::{
    HashSet, Result, SourceKind, SyncShard, Transaction, UsageTokens,
    audit_now as writer_audit_now, refresh_bucket_pricing_after_reset_tx,
};
use tracing::info;

thread_local! {
    static COLLECTOR: RefCell<Option<Collector>> = const { RefCell::new(None) };
    static AUDIT_NOW: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Default, Debug, Serialize)]
pub(crate) struct Record {
    pub kind: &'static str,
    pub write_ns: u128,
    pub unclassified_ns: u128,
    pub stages_ns: BTreeMap<&'static str, u128>,
    pub counts: BTreeMap<&'static str, usize>,
    pub source_apply_ns: Vec<u128>,
    pub reset_algorithms: BTreeMap<&'static str, usize>,
}

struct Frame {
    name: &'static str,
    started: Instant,
    children: Duration,
}

struct Collector {
    detailed: bool,
    active: Option<Record>,
    stack: Vec<Frame>,
    records: Vec<Record>,
}

pub(crate) struct Capture;

impl Capture {
    pub(crate) fn start(detailed: bool) -> Self {
        COLLECTOR.with(|slot| {
            assert!(slot.borrow().is_none(), "nested writer capture");
            *slot.borrow_mut() = Some(Collector {
                detailed,
                active: None,
                stack: Vec::new(),
                records: Vec::new(),
            });
        });
        Self
    }

    pub(crate) fn take(&self) -> Vec<Record> {
        COLLECTOR.with(|slot| std::mem::take(&mut slot.borrow_mut().as_mut().unwrap().records))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        COLLECTOR.with(|slot| *slot.borrow_mut() = None);
    }
}

pub(crate) struct AuditClock(Option<String>);

impl AuditClock {
    pub(crate) fn fixed() -> Self {
        Self(AUDIT_NOW.with(|slot| slot.replace(Some("2026-09-01T12:00:00.000Z".into()))))
    }
}

impl Drop for AuditClock {
    fn drop(&mut self) {
        AUDIT_NOW.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(super) fn audit_now() -> Option<String> {
    AUDIT_NOW.with(|slot| slot.borrow().clone())
}

pub(super) struct Stage(bool);

impl Stage {
    pub(super) fn enter(name: &'static str) -> Self {
        Self(COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.detailed && c.active.is_some()) else {
                return false;
            };
            c.stack.push(Frame {
                name,
                started: Instant::now(),
                children: Duration::ZERO,
            });
            true
        }))
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            let c = slot.as_mut().unwrap();
            let frame = c.stack.pop().unwrap();
            let elapsed = frame.started.elapsed();
            let exclusive = elapsed
                .checked_sub(frame.children)
                .expect("overlapping writer stages");
            if let Some(parent) = c.stack.last_mut() {
                parent.children += elapsed;
            }
            *c.active
                .as_mut()
                .unwrap()
                .stages_ns
                .entry(frame.name)
                .or_default() += exclusive.as_nanos();
        });
    }
}

pub(super) struct Scope(bool);

impl Scope {
    pub(super) fn start(kind: &'static str, shards: &[&SyncShard]) -> Self {
        Self(COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut() else {
                return false;
            };
            assert!(c.active.is_none());
            let mut record = Record {
                kind,
                ..Record::default()
            };
            for shard in shards {
                for (name, count) in [
                    ("events", shard.events.len()),
                    ("reset_paths", shard.reset_path_hashes.len()),
                    ("turns_raw", shard.turns.len()),
                    ("tools_raw", shard.tool_calls.len()),
                    ("cursors", shard.cursors.len()),
                    ("seen_paths", shard.seen_file_paths.len()),
                    ("raw", shard.raw_records.len()),
                    (
                        "event_batches",
                        shard.events.len().div_ceil(super::EVENT_WRITE_BATCH_SIZE),
                    ),
                ] {
                    *record.counts.entry(name).or_default() += count;
                }
            }
            c.active = Some(record);
            true
        }))
    }

    pub(super) fn finish(self, write: Duration) {
        if !self.0 {
            return;
        }
        COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            let c = slot.as_mut().unwrap();
            assert!(c.stack.is_empty());
            let mut record = c.active.take().unwrap();
            record.write_ns = write.as_nanos();
            let classified: u128 = record
                .stages_ns
                .iter()
                .filter(|(name, _)| !name.starts_with("pre_"))
                .map(|(_, value)| value)
                .sum();
            record.unclassified_ns = record
                .write_ns
                .checked_sub(classified)
                .expect("stages exceed WRITE boundary");
            c.records.push(record);
        });
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.0 {
            COLLECTOR.with(|slot| {
                if let Some(c) = slot.borrow_mut().as_mut() {
                    c.active = None;
                    c.stack.clear();
                }
            });
        }
    }
}

pub(super) fn count(name: &'static str, value: usize) {
    COLLECTOR.with(|slot| {
        if let Some(record) = slot.borrow_mut().as_mut().and_then(|c| c.active.as_mut()) {
            *record.counts.entry(name).or_default() += value;
        }
    });
}

pub(super) fn record_reset(algorithm: &'static str) {
    COLLECTOR.with(|slot| {
        if let Some(record) = slot.borrow_mut().as_mut().and_then(|c| c.active.as_mut()) {
            *record.reset_algorithms.entry(algorithm).or_default() += 1;
        }
    });
}

pub(super) fn source_apply(elapsed: Duration) {
    COLLECTOR.with(|slot| {
        if let Some(record) = slot.borrow_mut().as_mut().and_then(|c| c.active.as_mut()) {
            record.source_apply_ns.push(elapsed.as_nanos());
        }
    });
}

#[cfg(test)]
pub(crate) mod tests;

// Frozen SQL from the measured original implementation; only tests select it.
pub(super) const BASELINE_RESET_EVENT_AGGREGATE_SQL: &str = r#"
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
pub(super) const BASELINE_RESET_EVENT_DELETE_SQL: &str =
    "DELETE FROM usage_event WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Variant {
    Baseline,
    Candidate,
}

thread_local! {
    static VARIANT: std::cell::Cell<Variant> = const { std::cell::Cell::new(Variant::Candidate) };
}

pub(crate) struct VariantGuard(Variant);

impl VariantGuard {
    pub(crate) fn set(variant: Variant) -> Self {
        Self(VARIANT.with(|slot| slot.replace(variant)))
    }
}

impl Drop for VariantGuard {
    fn drop(&mut self) {
        VARIANT.with(|slot| slot.set(self.0));
    }
}

pub(super) fn is_baseline() -> bool {
    VARIANT.with(|slot| slot.get() == Variant::Baseline)
}

// Frozen original reset protocol used only as the A/B oracle.
pub(super) fn reset_file_events_baseline_tx(
    tx: &Transaction<'_>,
    source: SourceKind,
    host_id: &str,
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

    // 4.1 在 shard 事务里扣减 bucket 并删除旧 event
    record_reset("baseline");
    let mut unique = HashSet::new();
    {
        let mut aggregate_stmt = tx.prepare_cached(BASELINE_RESET_EVENT_AGGREGATE_SQL)?;
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
        let mut delete_event_stmt = tx.prepare_cached(BASELINE_RESET_EVENT_DELETE_SQL)?;
        let updated_at = writer_audit_now();
        let mut touched_buckets = Vec::new();

        for path_hash in path_hashes {
            if !unique.insert(path_hash.clone()) {
                continue;
            }

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
                            cache_read_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
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
                delete_event_stmt.execute(rusqlite::params![source.as_str(), host_id, path_hash])
            )?;
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
