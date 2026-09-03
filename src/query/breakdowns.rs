use super::*;

pub(super) fn context_pressure_event_filter(filter: &QueryFilter) -> filter::SqlFilter {
    let mut event_filter = filter.event_filter(None);
    if filter.source.is_none() && (filter.since.is_some() || filter.until.is_some()) {
        let sources = registered_source_descriptors();
        event_filter.push_raw(format!(
            "source IN ({})",
            std::iter::repeat_n("?", sources.len())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for descriptor in sources {
            event_filter.push_value(rusqlite::types::Value::Text(
                descriptor.stable_id.to_string(),
            ));
        }
    }
    event_filter
}

/// Per-model aggregate shown in dashboard breakdowns.
#[derive(Debug, Clone, Serialize)]
pub struct ModelBreakdown {
    /// Normalized model name.
    pub model: String,
    /// Summed non-cache read tokens.
    pub input_tokens: i64,
    /// Summed cache creation tokens.
    pub cache_creation_tokens: i64,
    /// Summed cache read tokens.
    pub cache_read_tokens: i64,
    /// Summed output tokens.
    pub output_tokens: i64,
    /// Summed reasoning-only output tokens.
    pub reasoning_output_tokens: i64,
    /// Summed total tokens.
    pub total_tokens: i64,
    /// Number of underlying usage events contributing to this row.
    pub event_count: i64,
    /// Estimated cost using cache-aware pricing.
    pub cost_with_cache_usd: f64,
    /// Estimated cost if cache reads were billed as regular input.
    pub cost_without_cache_usd: f64,
    /// Estimated cache savings compared with no-cache pricing.
    pub cache_savings_usd: f64,
    /// Aggregated pricing status for this model (`static`, `snapshot`, `source_reported`, `unpriced`, or `mixed`).
    pub pricing_status: String,
    /// Aggregated pricing catalog/source label, or `mixed` when multiple values contributed.
    pub pricing_source: Option<String>,
    /// Aggregated pricing rate JSON, or `mixed` when multiple rates contributed.
    pub pricing_rate: Option<String>,
    /// Distinct source ids for this model, sorted. TUI-only; omitted from JSON.
    #[serde(skip_serializing)]
    pub sources: Vec<String>,
}

/// Per-source aggregate plus freshest observed event time.
#[derive(Debug, Clone, Serialize)]
pub struct SourceBreakdown {
    /// Source identifier.
    pub source: String,
    /// Summed total tokens for the source.
    pub total_tokens: i64,
    /// Latest raw event timestamp observed for the source.
    pub last_event_at: Option<String>,
    /// Number of underlying usage events for the source.
    pub event_count: i64,
}

/// Per-host aggregate plus freshest observed event time.
#[derive(Debug, Clone, Serialize)]
pub struct HostBreakdown {
    /// Internal host identifier.
    pub host_id: String,
    /// User-visible host label.
    pub label: String,
    /// Summed total tokens for the host.
    pub total_tokens: i64,
    /// Latest raw event timestamp observed for the host.
    pub last_event_at: Option<String>,
    /// Number of underlying usage events for the host.
    pub event_count: i64,
}

/// Per-project aggregate shown in rankings.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectBreakdown {
    /// Stable hashed project key.
    pub project_hash: String,
    /// Human-readable project label.
    pub project_label: String,
    /// Optional repo/project reference.
    pub project_ref: Option<String>,
    /// Summed total tokens for the project.
    pub total_tokens: i64,
    /// Number of underlying usage events for the project.
    pub event_count: i64,
    /// Estimated project cost using cache-aware pricing.
    pub total_cost_usd: f64,
    /// Display-safe project path surrogate. Raw filesystem paths are not
    /// persisted by llmusage; adapters that need a path-like display can use
    /// this stable project reference/label.
    pub project_path: Option<String>,
}

/// Cost estimate line for one `(source, model)` pair.
#[derive(Debug, Clone, Serialize)]
pub struct CostLine {
    /// Source identifier.
    pub source: String,
    /// Normalized model name.
    pub model: String,
    /// Summed total tokens for the pair.
    pub total_tokens: i64,
    /// Estimated USD cost using the persisted cache-aware cost column.
    pub estimated_cost_usd: f64,
    /// Number of underlying usage events for the pair.
    pub event_count: i64,
}

impl Dashboard {
    /// Loads total token usage grouped by normalized model.
    pub fn model_breakdown(&self, filter: &QueryFilter) -> Result<Vec<ModelBreakdown>> {
        let sql_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                model,
                SUM(input_tokens),
                SUM(cache_creation_tokens),
                SUM(cache_read_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                SUM(event_count),
                SUM(cost_with_cache_usd),
                SUM(cost_without_cache_usd),
                CASE
                    WHEN COUNT(DISTINCT pricing_status) = 1 THEN MAX(pricing_status)
                    ELSE '{PRICING_MIXED}'
                END,
                CASE
                    WHEN COUNT(DISTINCT COALESCE(pricing_source, '__llmusage_null__')) = 1 THEN MAX(pricing_source)
                    ELSE '{PRICING_MIXED}'
                END,
                CASE
                    WHEN COUNT(DISTINCT COALESCE(pricing_rate, '__llmusage_null__')) = 1 THEN MAX(pricing_rate)
                    ELSE '{PRICING_MIXED}'
                END,
                GROUP_CONCAT(DISTINCT source)
            FROM usage_bucket_30m
            {}
            GROUP BY model
            ORDER BY
                SUM(total_tokens) DESC,
                model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let cost_with_cache_usd = row.get::<_, Option<f64>>(8)?.unwrap_or_default();
            let cost_without_cache_usd = row.get::<_, Option<f64>>(9)?.unwrap_or_default();
            Ok(ModelBreakdown {
                model: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                reasoning_output_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                cost_with_cache_usd,
                cost_without_cache_usd,
                cache_savings_usd: (cost_without_cache_usd - cost_with_cache_usd).max(0.0),
                pricing_status: row
                    .get::<_, Option<String>>(10)?
                    .unwrap_or_else(|| PRICING_UNPRICED.to_string()),
                pricing_source: row.get(11)?,
                pricing_rate: row.get(12)?,
                sources: sorted_unique_sources(row.get(13)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Summarizes context-window utilization across the filtered event set.
    ///
    /// Grouped by `(source, model)` to avoid per-event scans: each group's peak
    /// prompt tokens and summed prompt tokens are divided by the model's known
    /// context window (from the static catalog). Groups whose model window is
    /// unknown are excluded from the ratios and reported as `unpriced_events`.
    pub fn context_pressure(&self, filter: &QueryFilter) -> Result<ContextPressurePayload> {
        let event_filter = context_pressure_event_filter(filter);
        let sql = format!(
            r#"
            SELECT
                source,
                model,
                MAX(input_tokens + cache_read_tokens + cache_creation_tokens) AS peak_prompt,
                SUM(input_tokens + cache_read_tokens + cache_creation_tokens) AS sum_prompt,
                COUNT(*) AS event_count
            FROM usage_event
            {}
            GROUP BY source, model
            "#,
            event_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(event_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let catalog = self.store.active_pricing_catalog()?;
        let mut peak_percent = 0.0_f64;
        let mut peak_model: Option<String> = None;
        let mut ratio_sum = 0.0_f64;
        let mut priced_events = 0_i64;
        let mut unpriced_events = 0_i64;
        for (source, model, peak_prompt, sum_prompt, event_count) in rows {
            match catalog.context_window(&source, &model) {
                Some(window) => {
                    let window = window as f64;
                    let group_peak = peak_prompt.max(0) as f64 / window;
                    if group_peak > peak_percent {
                        peak_percent = group_peak;
                        peak_model = Some(format!("{source}:{model}"));
                    }
                    ratio_sum += sum_prompt.max(0) as f64 / window;
                    priced_events += event_count;
                }
                None => unpriced_events += event_count,
            }
        }
        let avg_percent = if priced_events > 0 {
            ratio_sum / priced_events as f64
        } else {
            0.0
        };
        Ok(ContextPressurePayload {
            peak_percent,
            avg_percent,
            peak_model,
            priced_events,
            unpriced_events,
        })
    }

    /// Loads recent 5-hour rolling blocks (burn rate / projection) for the
    /// interactive dashboard, reusing the CLI `blocks` report engine with
    /// dashboard-friendly defaults (recent blocks, local time, 5h windows).
    pub fn blocks_report(&self) -> anyhow::Result<Vec<reports::BlockReportRow>> {
        let filter = reports::ReportFilter {
            since: None,
            until: None,
            order: reports::SortOrder::Desc,
            timezone: reports::ReportTimezone::Local,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: false,
            host_id: None,
        };
        let options = reports::BlockReportOptions {
            active_only: false,
            recent_only: true,
            token_limit: None,
            session_length_hours: 5.0,
        };
        Ok(reports::load_blocks_report(&self.store, &filter, &options)?.blocks)
    }

    /// Loads total token usage grouped by source plus each source's freshest event time.
    pub fn source_breakdown(&self, filter: &QueryFilter) -> Result<Vec<SourceBreakdown>> {
        let bucket_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                source,
                SUM(total_tokens) AS total_tokens,
                SUM(event_count) AS event_count
            FROM usage_bucket_30m
            {}
            GROUP BY source
            ORDER BY total_tokens DESC, source ASC
            "#,
            bucket_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
            Ok(SourceBreakdown {
                source: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                last_event_at: None,
                event_count: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            })
        })?;
        let mut sources = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let last_event_at = last_event_at_by_group(&self.conn, filter, "source")?;
        for source in &mut sources {
            source.last_event_at = last_event_at.get(&source.source).cloned().flatten();
        }

        Ok(sources)
    }

    /// Loads total token usage grouped by host plus each host's freshest event time.
    pub fn host_breakdown(&self, filter: &QueryFilter) -> Result<Vec<HostBreakdown>> {
        let bucket_filter = filter.bucket_filter(Some("b"));
        let sql = format!(
            r#"
            SELECT
                b.host_id,
                COALESCE(NULLIF(h.label, ''), b.host_id) AS label,
                SUM(b.total_tokens) AS total_tokens,
                SUM(b.event_count) AS event_count
            FROM usage_bucket_30m b
            LEFT JOIN host h ON h.host_id = b.host_id
            {}
            GROUP BY b.host_id
            ORDER BY total_tokens DESC, label ASC, b.host_id ASC
            "#,
            bucket_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
            Ok(HostBreakdown {
                host_id: row.get(0)?,
                label: row.get(1)?,
                total_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                last_event_at: None,
                event_count: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            })
        })?;
        let mut hosts = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let last_event_at = last_event_at_by_group(&self.conn, filter, "host_id")?;
        for host in &mut hosts {
            host.last_event_at = last_event_at.get(&host.host_id).cloned().flatten();
        }

        Ok(hosts)
    }

    /// Loads ranked project totals derived from aggregated buckets.
    pub fn project_breakdown(&self, filter: &QueryFilter) -> Result<Vec<ProjectBreakdown>> {
        let mut sql_filter = filter.bucket_filter(None);
        sql_filter.push_raw("project_hash <> ''");
        let sql = format!(
            r#"
            SELECT
                project_hash,
                MAX(project_label),
                MAX(project_ref),
                SUM(total_tokens),
                SUM(event_count),
                SUM(cost_with_cache_usd)
            FROM usage_bucket_30m
            {}
            GROUP BY project_hash
            ORDER BY
                SUM(total_tokens) DESC,
                MAX(project_label) ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let project_label = row
                .get::<_, Option<String>>(1)?
                .unwrap_or_else(|| "unknown-project".to_string());
            let project_ref = row.get::<_, Option<String>>(2)?;
            let project_path = project_ref.clone().or_else(|| Some(project_label.clone()));
            Ok(ProjectBreakdown {
                project_hash: row.get(0)?,
                project_label,
                project_ref,
                total_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_cost_usd: row.get::<_, Option<f64>>(5)?.unwrap_or_default(),
                project_path,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads estimated cost totals for each `(source, model)` pair.
    pub fn cost_breakdown(&self, filter: &QueryFilter) -> Result<Vec<CostLine>> {
        let sql_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                source,
                model,
                SUM(input_tokens),
                SUM(cache_creation_tokens),
                SUM(cache_read_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                SUM(cost_with_cache_usd),
                SUM(event_count)
            FROM usage_bucket_30m
            {}
            GROUP BY source, model
            ORDER BY
                SUM(total_tokens) DESC,
                source ASC,
                model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let source: String = row.get(0)?;
            let model: String = row.get(1)?;
            let total_tokens = row.get::<_, Option<i64>>(7)?.unwrap_or_default();
            let estimated_cost_usd = row.get::<_, Option<f64>>(8)?.unwrap_or_default();
            let event_count = row.get::<_, Option<i64>>(9)?.unwrap_or_default();

            Ok(CostLine {
                source,
                model,
                total_tokens,
                estimated_cost_usd,
                event_count,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn last_event_at_by_group(
    conn: &Connection,
    filter: &QueryFilter,
    group_column: &str,
) -> Result<HashMap<String, Option<String>>> {
    let event_filter = filter.event_filter(None);
    let sql = format!(
        r#"
        /* last_event_at_grouped */
        SELECT {group_column}, MAX(event_at)
        FROM usage_event
        {}
        GROUP BY {group_column}
        "#,
        event_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(event_filter.params().iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut last_event_at = HashMap::new();
    for row in rows {
        let (group, event_at) = row?;
        last_event_at.insert(group, event_at);
    }
    Ok(last_event_at)
}
