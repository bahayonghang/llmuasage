use super::*;

/// Support/degradation metadata for behavior analytics.
#[derive(Debug, Clone, Serialize)]
pub struct BehaviorSupport {
    /// Whether at least one normalized behavior row is available for the filter.
    pub supported: bool,
    /// Machine-readable source support level.
    pub level: String,
    /// Human-readable explanation suitable for empty or degraded states.
    pub reason: Option<String>,
}

/// Activity category aggregate powered by `usage_turn`.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityBreakdown {
    /// Deterministic category id, e.g. `coding` or `exploration`.
    pub category: String,
    /// Number of normalized turns in this category.
    pub turns: i64,
    /// Turns with at least one edit/write action.
    pub edit_turns: i64,
    /// Edit turns without a detected retry.
    pub one_shot_turns: i64,
    /// Sum of deterministic retry estimates.
    pub retries: i64,
    /// Number of API calls/events represented by the turns.
    pub call_count: i64,
    /// Summed tokens attributed to the turns.
    pub total_tokens: i64,
    /// Estimated cost attributed through the conservative event key embedded in
    /// each normalized turn key.
    pub estimated_cost_usd: f64,
    /// `one_shot_turns / edit_turns`, or 0 when there are no edit turns.
    pub one_shot_rate: f64,
    /// `retries / turns`, or 0 when there are no turns.
    pub retry_rate: f64,
}

/// Top-level activity analytics payload.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityPayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Category aggregates ordered by attributed cost/tokens/turns.
    pub breakdown: Vec<ActivityBreakdown>,
}

impl Dashboard {
    /// Loads activity category aggregates from normalized `usage_turn` facts.
    ///
    /// This intentionally does not read raw JSONL or frontend-owned data. Cost
    /// is attribution-only: persisted event costs are joined by the
    /// conservative event key embedded in each turn key.
    pub fn activity_breakdown(&self, filter: &QueryFilter) -> Result<ActivityPayload> {
        let turn_filter = filter.turn_filter(Some("t"));
        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        if !support.supported {
            return Ok(ActivityPayload {
                support,
                breakdown: Vec::new(),
            });
        }
        let sql = format!(
            r#"
            /* activity_breakdown */
            SELECT
                t.category,
                COUNT(*) AS turns,
                COALESCE(SUM(t.has_edits), 0) AS edit_turns,
                COALESCE(SUM(t.one_shot), 0) AS one_shot_turns,
                COALESCE(SUM(t.retries), 0) AS retries,
                COALESCE(SUM(t.call_count), 0) AS call_count,
                COALESCE(SUM(t.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS estimated_cost_usd
            FROM usage_turn t
            LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
            {}
            GROUP BY t.category
            ORDER BY estimated_cost_usd DESC, total_tokens DESC, turns DESC, t.category ASC
            "#,
            turn_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
            let turns = row.get::<_, Option<i64>>(1)?.unwrap_or_default();
            let edit_turns = row.get::<_, Option<i64>>(2)?.unwrap_or_default();
            let one_shot_turns = row.get::<_, Option<i64>>(3)?.unwrap_or_default();
            let retries = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
            Ok(ActivityBreakdown {
                category: row.get(0)?,
                turns,
                edit_turns,
                one_shot_turns,
                retries,
                call_count: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                estimated_cost_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                one_shot_rate: ratio(one_shot_turns, edit_turns),
                retry_rate: ratio(retries, turns),
            })
        })?;
        Ok(ActivityPayload {
            support,
            breakdown: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        })
    }

    #[cfg(test)]
    pub(super) fn legacy_activity_breakdown(
        &self,
        filter: &QueryFilter,
    ) -> Result<ActivityPayload> {
        #[derive(Default)]
        struct ActivityAggregate {
            turns: i64,
            edit_turns: i64,
            one_shot_turns: i64,
            retries: i64,
            call_count: i64,
            total_tokens: i64,
            estimated_cost_usd: f64,
        }

        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        if !support.supported {
            return Ok(ActivityPayload {
                support,
                breakdown: Vec::new(),
            });
        }

        let mut event_costs = HashMap::new();
        let mut event_stmt = self
            .conn
            .prepare("SELECT event_key, COALESCE(cost_with_cache_usd, 0.0) FROM usage_event")?;
        let event_rows = event_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        for row in event_rows {
            let (event_key, cost) = row?;
            event_costs.insert(event_key, cost);
        }

        let turn_filter = filter.turn_filter(Some("t"));
        let sql = format!(
            r#"
            SELECT
                substr(t.turn_key, 6),
                t.category,
                t.has_edits,
                t.one_shot,
                t.retries,
                t.call_count,
                t.total_tokens
            FROM usage_turn t
            {}
            "#,
            turn_filter.where_sql()
        );
        let mut aggregates: BTreeMap<String, ActivityAggregate> = BTreeMap::new();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        for row in rows {
            let (event_key, category, has_edits, one_shot, retries, call_count, total_tokens) =
                row?;
            let aggregate = aggregates.entry(category).or_default();
            aggregate.turns += 1;
            aggregate.edit_turns += has_edits;
            aggregate.one_shot_turns += one_shot;
            aggregate.retries += retries;
            aggregate.call_count += call_count;
            aggregate.total_tokens += total_tokens;
            aggregate.estimated_cost_usd +=
                event_costs.get(&event_key).copied().unwrap_or_default();
        }
        let mut breakdown = aggregates
            .into_iter()
            .map(|(category, aggregate)| ActivityBreakdown {
                category,
                turns: aggregate.turns,
                edit_turns: aggregate.edit_turns,
                one_shot_turns: aggregate.one_shot_turns,
                retries: aggregate.retries,
                call_count: aggregate.call_count,
                total_tokens: aggregate.total_tokens,
                estimated_cost_usd: aggregate.estimated_cost_usd,
                one_shot_rate: ratio(aggregate.one_shot_turns, aggregate.edit_turns),
                retry_rate: ratio(aggregate.retries, aggregate.turns),
            })
            .collect::<Vec<_>>();
        breakdown.sort_by(|left, right| {
            right
                .estimated_cost_usd
                .total_cmp(&left.estimated_cost_usd)
                .then_with(|| right.total_tokens.cmp(&left.total_tokens))
                .then_with(|| right.turns.cmp(&left.turns))
                .then_with(|| left.category.cmp(&right.category))
        });
        Ok(ActivityPayload { support, breakdown })
    }
}
