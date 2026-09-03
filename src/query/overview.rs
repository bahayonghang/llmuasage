use super::*;

fn period_turn_counts(
    conn: &Connection,
    filter: &QueryFilter,
    period_expr: &str,
) -> Result<HashMap<String, i64>> {
    let sql_filter = filter.turn_filter(Some("t"));
    let sql = format!(
        "SELECT {period_expr} AS period_key, COUNT(*) FROM usage_turn t {} GROUP BY period_key",
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (key, count) = row?;
        counts.insert(key, count);
    }
    Ok(counts)
}

struct LifetimeBucketOverview {
    tokens: TokenSummary,
    events: i64,
    cost_usd: f64,
    source_count: i64,
    bucket_count: i64,
}

fn query_lifetime_bucket_overview(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<LifetimeBucketOverview> {
    let sql_filter = filter.bucket_filter(None);
    let sql = format!(
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cache_creation_tokens), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0),
            COALESCE(SUM(event_count), 0),
            COALESCE(SUM(cost_with_cache_usd), 0.0),
            COUNT(DISTINCT source),
            COUNT(*)
        FROM usage_bucket_30m
        {}
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    Ok(
        stmt.query_row(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(LifetimeBucketOverview {
                tokens: map_token_summary(row)?,
                events: row.get(6)?,
                cost_usd: row.get(7)?,
                source_count: row.get(8)?,
                bucket_count: row.get(9)?,
            })
        })?,
    )
}

fn query_recent_bucket_overview(
    conn: &Connection,
    filter: &QueryFilter,
    cutoff: &str,
) -> Result<(TokenSummary, i64)> {
    let mut sql_filter = filter.bucket_filter(None);
    sql_filter.push("hour_start >= ?", cutoff);
    let sql = format!(
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cache_creation_tokens), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0),
            COALESCE(SUM(event_count), 0)
        FROM usage_bucket_30m
        {}
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    Ok(
        stmt.query_row(params_from_iter(sql_filter.params().iter()), |row| {
            Ok((map_token_summary(row)?, row.get(6)?))
        })?,
    )
}

fn map_token_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenSummary> {
    Ok(TokenSummary {
        input_tokens: row.get(0)?,
        cache_creation_tokens: row.get(1)?,
        cache_read_tokens: row.get(2)?,
        output_tokens: row.get(3)?,
        reasoning_output_tokens: row.get(4)?,
        total_tokens: row.get(5)?,
    })
}

/// Aggregated token counters returned by overview and trend queries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    /// Sum of non-cache read tokens.
    pub input_tokens: i64,
    /// Sum of cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Sum of cached/reused input tokens.
    pub cache_read_tokens: i64,
    /// Sum of non-reasoning output tokens.
    pub output_tokens: i64,
    /// Sum of separately reported reasoning tokens.
    pub reasoning_output_tokens: i64,
    /// Total normalized tokens across all categories.
    pub total_tokens: i64,
}

impl TokenSummary {
    /// Combined output tokens ccr-ui should display at API boundaries.
    pub fn output_tokens_with_reasoning(&self) -> i64 {
        self.output_tokens + self.reasoning_output_tokens
    }

    /// Cross-source cache reuse ratio, returning `0.0` when no input was used.
    pub fn cache_efficiency(&self) -> f64 {
        let denominator = self.input_tokens + self.cache_creation_tokens + self.cache_read_tokens;
        if denominator == 0 {
            0.0
        } else {
            self.cache_read_tokens as f64 / denominator as f64
        }
    }
}

/// Top-level dashboard numbers shown in status, web, and export views.
#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    /// Snapshot generation time in RFC 3339 format.
    pub generated_at: String,
    /// Lifetime totals across the entire dataset.
    pub total: TokenSummary,
    /// Totals restricted to the last 24 hours.
    pub last_24h: TokenSummary,
    /// Distinct source count present in aggregated buckets.
    pub source_count: i64,
    /// Number of persisted 30-minute buckets.
    pub bucket_count: i64,
    /// Lifetime usage event count, summed from `usage_bucket_30m.event_count`.
    pub total_events: i64,
    /// Usage event count restricted to the last 24 hours.
    pub last_24h_events: i64,
    /// Estimated lifetime cost using persisted `cost_with_cache_usd` buckets.
    pub total_cost_usd: f64,
    /// Cross-source cache read ratio for the filtered lifetime total.
    pub cache_efficiency: f64,
    /// Last successful usage-import finish time, including historical hook runs.
    pub last_sync_at: Option<String>,
    /// Last successful HTML export finish time.
    pub last_export_at: Option<String>,
}

/// One plotted point in a trend series.
#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    /// Display label for the time window bucket.
    pub label: String,
    /// Total tokens in the bucket.
    pub total_tokens: i64,
}

/// Context-window utilization summary produced by [`Dashboard::context_pressure`].
///
/// Percentages are prompt-side occupancy (`input + cache_read + cache_creation`)
/// over the model's known maximum context window. Events whose model has no
/// known window are excluded from the ratios and counted in `unpriced_events`.
#[derive(Debug, Clone, Serialize)]
pub struct ContextPressurePayload {
    /// Highest single-event context occupancy ratio in [0, 1], if any priced.
    pub peak_percent: f64,
    /// Mean per-event context occupancy ratio in [0, 1] across priced events.
    pub avg_percent: f64,
    /// `source:model` label behind `peak_percent`, when known.
    pub peak_model: Option<String>,
    /// Events counted toward the ratios (model window known).
    pub priced_events: i64,
    /// Events skipped because the model window is unknown.
    pub unpriced_events: i64,
}

/// One daily trend row produced by [`Dashboard::trends_daily`].
///
/// Persisted `total_tokens` is authoritative. Reasoning remains a separate
/// diagnostic channel and is not added to output or total by this projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTrendPoint {
    /// Local calendar date in `YYYY-MM-DD`, computed in [`QueryFilter::timezone`].
    pub date: String,
    /// Summed non-cache prompt tokens.
    pub input_tokens: i64,
    /// Summed cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Summed cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Summed persisted output tokens, without adding diagnostic reasoning.
    pub output_tokens: i64,
    /// Total normalized tokens for the day.
    pub total_tokens: i64,
    /// Number of underlying usage events for the day.
    pub event_count: i64,
    /// Estimated cost for the day using cache-aware pricing.
    pub cost_with_cache_usd: f64,
    /// Distinct `usage_turn` rows starting on this local date. TUI-only.
    #[serde(default, skip_serializing)]
    pub turn_count: i64,
}

/// One local clock-hour row for the TUI Hourly panel.
#[derive(Debug, Clone)]
pub struct HourlyTrendPoint {
    /// Local clock hour as `YYYY-MM-DD HH:00`.
    pub hour_start: String,
    /// Summed non-cache prompt tokens.
    pub input_tokens: i64,
    /// Summed cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Summed cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Output tokens with reasoning already folded in.
    pub output_tokens: i64,
    /// Total normalized tokens for the hour.
    pub total_tokens: i64,
    /// Number of underlying usage events for the hour.
    pub event_count: i64,
    /// Distinct turns that started in this local hour.
    pub turn_count: i64,
    /// Estimated cost using cache-aware pricing.
    pub cost_with_cache_usd: f64,
    /// Distinct source ids, sorted.
    pub sources: Vec<String>,
}

/// One local calendar-month row for the TUI Monthly panel.
#[derive(Debug, Clone)]
pub struct MonthlyTrendPoint {
    /// Local month as `YYYY-MM`.
    pub month: String,
    /// Summed non-cache prompt tokens.
    pub input_tokens: i64,
    /// Summed cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Summed cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Output tokens with reasoning already folded in.
    pub output_tokens: i64,
    /// Total normalized tokens for the month.
    pub total_tokens: i64,
    /// Number of underlying usage events for the month.
    pub event_count: i64,
    /// Distinct turns that started in this local month.
    pub turn_count: i64,
    /// Estimated cost using cache-aware pricing.
    pub cost_with_cache_usd: f64,
}

/// One model × source row for Daily Enter detail.
#[derive(Debug, Clone)]
pub struct PeriodDetailRow {
    /// Normalized model name.
    pub model: String,
    /// Source identifier.
    pub source: String,
    /// Number of underlying usage events.
    pub event_count: i64,
    /// Summed non-cache prompt tokens.
    pub input_tokens: i64,
    /// Summed cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Summed cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Output tokens with reasoning already folded in.
    pub output_tokens: i64,
    /// Total normalized tokens.
    pub total_tokens: i64,
    /// Estimated cost using cache-aware pricing.
    pub cost_with_cache_usd: f64,
}

/// Inclusive local-date bounds for a `YYYY-MM` month key.
pub fn month_date_bounds(month: &str) -> Option<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok()?;
    let end = if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)?.pred_opt()?
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)?.pred_opt()?
    };
    Some((start, end))
}

/// One local-date × model total used by the TUI Overview stacked chart.
#[derive(Debug, Clone, Serialize)]
pub struct DailyModelPoint {
    /// Local calendar date in `YYYY-MM-DD`, computed in [`QueryFilter::timezone`].
    pub date: String,
    /// Normalized model name.
    pub model: String,
    /// Total normalized tokens for that model on that date.
    pub total_tokens: i64,
}

impl Dashboard {
    /// Loads top-level lifetime/24h overview metrics plus recent sync/export timestamps.
    pub fn overview(&self, filter: &QueryFilter) -> Result<OverviewPayload> {
        let lifetime = query_lifetime_bucket_overview(&self.conn, filter)?;
        let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let (last_24h, last_24h_events) =
            query_recent_bucket_overview(&self.conn, filter, &cutoff)?;
        let cache_efficiency = lifetime.tokens.cache_efficiency();
        // `hook-run` is retained as a historical run_log label for old databases.
        let last_sync_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
            [],
        )?;
        let last_export_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command = 'export html' AND status = 'success'",
            [],
        )?;

        Ok(OverviewPayload {
            generated_at: now_utc(),
            total: lifetime.tokens,
            last_24h,
            source_count: lifetime.source_count,
            bucket_count: lifetime.bucket_count,
            total_events: lifetime.events,
            last_24h_events,
            total_cost_usd: lifetime.cost_usd,
            cache_efficiency,
            last_sync_at,
            last_export_at,
        })
    }

    /// Loads aggregated trend points for the requested window (`day`, `week`, `month`, or `all`).
    ///
    /// Retained for the legacy `/api/trends?window=` HTTP route.
    /// New surfaces should prefer [`Dashboard::trends_daily`] for full token
    /// breakdown and event counts.
    pub fn trends(&self, window: &str, filter: &QueryFilter) -> Result<Vec<TrendPoint>> {
        let mut sql_filter = filter.bucket_filter(None);
        let cutoff = match window {
            "day" => Some(Utc::now() - Duration::hours(24)),
            "week" => Some(Utc::now() - Duration::days(7)),
            "month" => Some(Utc::now() - Duration::days(30)),
            _ => None,
        };
        if let Some(cutoff) = cutoff {
            sql_filter.push(
                "hour_start >= ?",
                cutoff.to_rfc3339_opts(SecondsFormat::Secs, true),
            );
        }
        let label_expr = match window {
            "day" | "hourly" => "hour_start".to_string(),
            "week" | "month" => filter.local_date_expr("hour_start"),
            _ => filter.local_month_expr("hour_start"),
        };
        let sql = format!(
            r#"
            SELECT {label_expr} AS label,
                   COALESCE(SUM(total_tokens), 0) AS total_tokens
            FROM usage_bucket_30m
            {}
            GROUP BY label
            ORDER BY label ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(TrendPoint {
                label: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads a per-day trend series with full token breakdown and event count
    /// (D9/F4.2). Reasoning remains a separate diagnostic channel.
    ///
    /// Days are grouped by the local calendar date in
    /// [`QueryFilter::timezone`]; UTC days are reconstructed from the
    /// underlying `hour_start` column when the filter requests UTC.
    pub fn trends_daily(&self, filter: &QueryFilter) -> Result<Vec<DailyTrendPoint>> {
        let sql_filter = filter.bucket_filter(None);
        let local_date = filter.local_date_expr("hour_start");
        let sql = format!(
            r#"
            SELECT
                {local_date} AS local_date,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(event_count), 0),
                COALESCE(SUM(cost_with_cache_usd), 0.0)
            FROM usage_bucket_30m
            {}
            GROUP BY local_date
            ORDER BY local_date ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(DailyTrendPoint {
                date: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                turn_count: 0,
            })
        })?;
        let mut points = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let turns = period_turn_counts(&self.conn, filter, &filter.local_date_expr("t.started_at"))
            .unwrap_or_default();
        for point in &mut points {
            point.turn_count = turns.get(&point.date).copied().unwrap_or(0);
        }
        Ok(points)
    }

    /// Loads daily token totals grouped by local date and model.
    pub fn trends_daily_by_model(&self, filter: &QueryFilter) -> Result<Vec<DailyModelPoint>> {
        let sql_filter = filter.bucket_filter(None);
        let local_date = filter.local_date_expr("hour_start");
        let sql = format!(
            r#"
            SELECT
                {local_date} AS local_date,
                model,
                COALESCE(SUM(total_tokens), 0)
            FROM usage_bucket_30m
            {}
            GROUP BY local_date, model
            ORDER BY local_date ASC, model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(DailyModelPoint {
                date: row.get(0)?,
                model: row.get(1)?,
                total_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads a per-clock-hour series with token channels, cost, and sources.
    pub fn trends_hourly(&self, filter: &QueryFilter) -> Result<Vec<HourlyTrendPoint>> {
        let sql_filter = filter.bucket_filter(None);
        let local_hour = filter.local_hour_expr("hour_start");
        let sql = format!(
            r#"
            SELECT
                {local_hour} AS local_hour,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(event_count), 0),
                COALESCE(SUM(cost_with_cache_usd), 0.0),
                GROUP_CONCAT(DISTINCT source)
            FROM usage_bucket_30m
            {}
            GROUP BY local_hour
            ORDER BY local_hour ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(HourlyTrendPoint {
                hour_start: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                sources: sorted_unique_sources(row.get(8)?),
                turn_count: 0,
            })
        })?;
        let mut points = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let turns = period_turn_counts(&self.conn, filter, &filter.local_hour_expr("t.started_at"))
            .unwrap_or_default();
        for point in &mut points {
            point.turn_count = turns.get(&point.hour_start).copied().unwrap_or(0);
        }
        Ok(points)
    }

    /// Loads a per-local-month series with token channels and cost.
    pub fn trends_monthly(&self, filter: &QueryFilter) -> Result<Vec<MonthlyTrendPoint>> {
        let sql_filter = filter.bucket_filter(None);
        let local_month = filter.local_month_expr("hour_start");
        let sql = format!(
            r#"
            SELECT
                {local_month} AS local_month,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(event_count), 0),
                COALESCE(SUM(cost_with_cache_usd), 0.0)
            FROM usage_bucket_30m
            {}
            GROUP BY local_month
            ORDER BY local_month ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(MonthlyTrendPoint {
                month: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                turn_count: 0,
            })
        })?;
        let mut points = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let turns =
            period_turn_counts(&self.conn, filter, &filter.local_month_expr("t.started_at"))
                .unwrap_or_default();
        for point in &mut points {
            point.turn_count = turns.get(&point.month).copied().unwrap_or(0);
        }
        Ok(points)
    }

    /// Loads model × source totals for the current filter (Daily Enter detail).
    pub fn period_model_breakdown(&self, filter: &QueryFilter) -> Result<Vec<PeriodDetailRow>> {
        let sql_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                model,
                source,
                COALESCE(SUM(event_count), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_with_cache_usd), 0.0)
            FROM usage_bucket_30m
            {}
            GROUP BY model, source
            ORDER BY SUM(cost_with_cache_usd) DESC, model ASC, source ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(PeriodDetailRow {
                model: row.get(0)?,
                source: row.get(1)?,
                event_count: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(8)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
