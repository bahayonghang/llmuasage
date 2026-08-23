use super::*;

fn compare_model_candidates(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<CompareModelCandidate>> {
    let bucket_filter = filter.bucket_filter(Some("b"));
    let sql = format!(
        r#"
        SELECT
            b.model,
            COALESCE(SUM(b.event_count), 0) AS calls,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cost_with_cache_usd), 0.0) AS estimated_cost_usd
        FROM usage_bucket_30m b
        {}
        GROUP BY b.model
        ORDER BY estimated_cost_usd DESC, total_tokens DESC, calls DESC, b.model ASC
        LIMIT 25
        "#,
        bucket_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            row.get::<_, Option<f64>>(3)?.unwrap_or_default(),
        ))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let (model, calls, total_tokens, estimated_cost_usd) = row?;
        candidates.push(CompareModelCandidate {
            model,
            calls,
            turns: 0,
            edit_turns: 0,
            total_tokens,
            estimated_cost_usd,
            low_sample: true,
        });
    }

    // One grouped turn query covers every candidate instead of one query per
    // model. A candidate without matching turns keeps the (0, 0) defaults,
    // which matches the legacy per-model `COUNT(*)` result for empty sets.
    if !candidates.is_empty() {
        let turn_filter = filter.turn_filter(Some("t"));
        let turn_sql = format!(
            "SELECT t.primary_model, COUNT(*), COALESCE(SUM(t.has_edits), 0) FROM usage_turn t{} GROUP BY t.primary_model",
            turn_filter.where_sql()
        );
        let mut turn_stmt = conn.prepare(&turn_sql)?;
        let turn_rows =
            turn_stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
        let mut turn_stats: std::collections::HashMap<String, (i64, i64)> =
            std::collections::HashMap::new();
        for row in turn_rows {
            let (model, turns, edit_turns) = row?;
            turn_stats.insert(model, (turns, edit_turns));
        }
        for candidate in &mut candidates {
            let (turns, edit_turns) = turn_stats.get(&candidate.model).copied().unwrap_or((0, 0));
            candidate.turns = turns;
            candidate.edit_turns = edit_turns;
            candidate.low_sample = candidate.calls < 20 || candidate.edit_turns < 10;
        }
    }
    Ok(candidates)
}

fn ratio_f64(numerator: f64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        numerator / denominator as f64
    }
}

fn build_model_compare_stats(
    model: &str,
    buckets: &HashMap<String, CompareBucketStats>,
    turns: &HashMap<String, CompareTurnStats>,
    tool_calls: &HashMap<String, i64>,
) -> Option<ModelCompareStats> {
    let bucket = buckets.get(model)?;
    if bucket.calls == 0 {
        return None;
    }
    let turn = turns.get(model).copied().unwrap_or_default();
    let tool_calls = tool_calls.get(model).copied().unwrap_or_default();
    let cache_efficiency = ratio(
        bucket.cache_read_tokens,
        bucket.input_tokens + bucket.cache_creation_tokens + bucket.cache_read_tokens,
    );
    Some(ModelCompareStats {
        model: model.to_string(),
        calls: bucket.calls,
        turns: turn.turns,
        edit_turns: turn.edit_turns,
        one_shot_turns: turn.one_shot_turns,
        retries: turn.retries,
        total_tokens: bucket.total_tokens,
        estimated_cost_usd: bucket.estimated_cost_usd,
        cache_efficiency,
        cost_per_call: ratio_f64(bucket.estimated_cost_usd, bucket.calls),
        cost_per_edit_turn: ratio_f64(bucket.estimated_cost_usd, turn.edit_turns),
        one_shot_rate: ratio(turn.one_shot_turns, turn.edit_turns),
        retry_rate: ratio(turn.retries, turn.turns),
        avg_tools_per_turn: ratio(tool_calls, turn.turns),
        delegation_rate: ratio(turn.delegation_turns, turn.turns),
        planning_rate: ratio(turn.planning_turns, turn.turns),
        low_sample: bucket.calls < 20 || turn.edit_turns < 10,
    })
}

fn compare_metrics(left: &ModelCompareStats, right: &ModelCompareStats) -> Vec<CompareMetric> {
    vec![
        CompareMetric {
            id: "one_shot_rate".to_string(),
            label: "One-shot rate".to_string(),
            model_a_value: left.one_shot_rate,
            model_b_value: right.one_shot_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "retry_rate".to_string(),
            label: "Retry rate".to_string(),
            model_a_value: left.retry_rate,
            model_b_value: right.retry_rate,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cost_per_call".to_string(),
            label: "Cost / call".to_string(),
            model_a_value: left.cost_per_call,
            model_b_value: right.cost_per_call,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cost_per_edit_turn".to_string(),
            label: "Cost / edit".to_string(),
            model_a_value: left.cost_per_edit_turn,
            model_b_value: right.cost_per_edit_turn,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cache_efficiency".to_string(),
            label: "Cache efficiency".to_string(),
            model_a_value: left.cache_efficiency,
            model_b_value: right.cache_efficiency,
            higher_is_better: true,
        },
    ]
}

fn working_style_metrics(
    left: &ModelCompareStats,
    right: &ModelCompareStats,
) -> Vec<CompareMetric> {
    vec![
        CompareMetric {
            id: "delegation_rate".to_string(),
            label: "Delegation".to_string(),
            model_a_value: left.delegation_rate,
            model_b_value: right.delegation_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "planning_rate".to_string(),
            label: "Planning".to_string(),
            model_a_value: left.planning_rate,
            model_b_value: right.planning_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "tools_per_turn".to_string(),
            label: "Tools / turn".to_string(),
            model_a_value: left.avg_tools_per_turn,
            model_b_value: right.avg_tools_per_turn,
            higher_is_better: true,
        },
    ]
}

/// Candidate model row for model comparison.
#[derive(Debug, Clone, Serialize)]
pub struct CompareModelCandidate {
    /// Normalized model name.
    pub model: String,
    /// Number of usage events/calls observed for the model.
    pub calls: i64,
    /// Normalized behavior turns observed for the model.
    pub turns: i64,
    /// Edit/write turns observed for the model.
    pub edit_turns: i64,
    /// Summed tokens from usage buckets.
    pub total_tokens: i64,
    /// Summed persisted cache-aware cost.
    pub estimated_cost_usd: f64,
    /// True when the sample is too small for confident behavioral comparison.
    pub low_sample: bool,
}

/// Per-model comparison statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ModelCompareStats {
    /// Normalized model name.
    pub model: String,
    /// Number of usage events/calls.
    pub calls: i64,
    /// Number of normalized turns.
    pub turns: i64,
    /// Number of edit/write turns.
    pub edit_turns: i64,
    /// Number of one-shot edit/write turns.
    pub one_shot_turns: i64,
    /// Sum of deterministic retry estimates.
    pub retries: i64,
    /// Summed tokens.
    pub total_tokens: i64,
    /// Summed cache-aware cost.
    pub estimated_cost_usd: f64,
    /// Cache read ratio across persisted bucket tokens.
    pub cache_efficiency: f64,
    /// Cost per usage event/call.
    pub cost_per_call: f64,
    /// Cost per edit/write turn.
    pub cost_per_edit_turn: f64,
    /// One-shot edit/write rate.
    pub one_shot_rate: f64,
    /// Retry estimate per turn.
    pub retry_rate: f64,
    /// Average normalized tool calls per turn.
    pub avg_tools_per_turn: f64,
    /// Delegation-category turn share.
    pub delegation_rate: f64,
    /// Planning-category turn share.
    pub planning_rate: f64,
    /// True when calls or edit turns are below the comparison threshold.
    pub low_sample: bool,
}

#[derive(Clone, Copy)]
struct CompareBucketStats {
    calls: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
}

#[derive(Clone, Copy, Default)]
struct CompareTurnStats {
    turns: i64,
    edit_turns: i64,
    one_shot_turns: i64,
    retries: i64,
    delegation_turns: i64,
    planning_turns: i64,
}

/// Side-by-side scalar comparison metric.
#[derive(Debug, Clone, Serialize)]
pub struct CompareMetric {
    /// Stable metric id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Value for model A.
    pub model_a_value: f64,
    /// Value for model B.
    pub model_b_value: f64,
    /// Whether a higher value is generally better for this metric.
    pub higher_is_better: bool,
}

/// Category-level one-shot comparison.
#[derive(Debug, Clone, Serialize)]
pub struct CategoryCompareRow {
    /// Activity category.
    pub category: String,
    /// Edit/write turns for model A in this category.
    pub model_a_edit_turns: i64,
    /// One-shot rate for model A in this category.
    pub model_a_one_shot_rate: f64,
    /// Edit/write turns for model B in this category.
    pub model_b_edit_turns: i64,
    /// One-shot rate for model B in this category.
    pub model_b_one_shot_rate: f64,
}

/// Model-pair comparison payload.
#[derive(Debug, Clone, Serialize)]
pub struct ModelComparePayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Available model candidates for the current filter.
    pub candidates: Vec<CompareModelCandidate>,
    /// Chosen left-hand model stats.
    pub model_a: Option<ModelCompareStats>,
    /// Chosen right-hand model stats.
    pub model_b: Option<ModelCompareStats>,
    /// Performance/efficiency metrics.
    pub metrics: Vec<CompareMetric>,
    /// Category head-to-head rows.
    pub category_head_to_head: Vec<CategoryCompareRow>,
    /// Working style metrics.
    pub working_style: Vec<CompareMetric>,
    /// Warning shown for no-data, insufficient models, or low-sample comparisons.
    pub warning: Option<String>,
}

impl Dashboard {
    /// Loads model candidates for behavior comparison.
    pub fn compare_models(&self, filter: &QueryFilter) -> Result<Vec<CompareModelCandidate>> {
        compare_model_candidates(&self.conn, filter)
    }

    /// Loads a model-pair comparison. When either model is omitted, the top two
    /// candidates in the current filter are chosen automatically.
    pub fn model_compare(
        &self,
        filter: &QueryFilter,
        model_a: Option<&str>,
        model_b: Option<&str>,
    ) -> Result<ModelComparePayload> {
        let candidates = self.compare_models(filter)?;
        if candidates.len() < 2 {
            return Ok(ModelComparePayload {
                support: BehaviorSupport {
                    supported: false,
                    level: "insufficient_models".to_string(),
                    reason: Some(
                        "At least two models with local usage are required for comparison."
                            .to_string(),
                    ),
                },
                candidates,
                model_a: None,
                model_b: None,
                metrics: Vec::new(),
                category_head_to_head: Vec::new(),
                working_style: Vec::new(),
                warning: Some("Need at least two models in the current filter.".to_string()),
            });
        }

        let selected_a = model_a
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(candidates[0].model.as_str());
        let selected_b = model_b
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.model != selected_a)
                    .map(|candidate| candidate.model.as_str())
                    .unwrap_or(candidates[1].model.as_str())
            });

        let (stats_a, stats_b) = self.model_compare_stats_pair(filter, selected_a, selected_b)?;
        let (support, warning) = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                let warning = if left.low_sample || right.low_sample {
                    Some(
                        "Low sample: compare directionally until each model has more calls/edit turns."
                            .to_string(),
                    )
                } else {
                    None
                };
                (
                    BehaviorSupport {
                        supported: true,
                        level: if warning.is_some() {
                            "low_sample"
                        } else {
                            "normalized"
                        }
                        .to_string(),
                        reason: warning.clone(),
                    },
                    warning,
                )
            }
            _ => (
                BehaviorSupport {
                    supported: false,
                    level: "missing_model".to_string(),
                    reason: Some("One selected model has no data in this filter.".to_string()),
                },
                Some("One selected model has no data in this filter.".to_string()),
            ),
        };

        let metrics = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => compare_metrics(left, right),
            _ => Vec::new(),
        };
        let working_style = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => working_style_metrics(left, right),
            _ => Vec::new(),
        };
        let category_head_to_head = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                self.category_compare_pair(filter, &left.model, &right.model)?
            }
            _ => Vec::new(),
        };

        Ok(ModelComparePayload {
            support,
            candidates,
            model_a: stats_a,
            model_b: stats_b,
            metrics,
            category_head_to_head,
            working_style,
            warning,
        })
    }

    #[cfg(test)]
    pub(super) fn legacy_model_compare(
        &self,
        filter: &QueryFilter,
        model_a: Option<&str>,
        model_b: Option<&str>,
    ) -> Result<ModelComparePayload> {
        let candidates = self.compare_models(filter)?;
        if candidates.len() < 2 {
            return Ok(ModelComparePayload {
                support: BehaviorSupport {
                    supported: false,
                    level: "insufficient_models".to_string(),
                    reason: Some(
                        "At least two models with local usage are required for comparison."
                            .to_string(),
                    ),
                },
                candidates,
                model_a: None,
                model_b: None,
                metrics: Vec::new(),
                category_head_to_head: Vec::new(),
                working_style: Vec::new(),
                warning: Some("Need at least two models in the current filter.".to_string()),
            });
        }

        let selected_a = model_a
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(candidates[0].model.as_str());
        let selected_b = model_b
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.model != selected_a)
                    .map(|candidate| candidate.model.as_str())
                    .unwrap_or(candidates[1].model.as_str())
            });
        let stats_a = self.legacy_model_compare_stats(filter, selected_a)?;
        let stats_b = self.legacy_model_compare_stats(filter, selected_b)?;
        let (support, warning) = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                let warning = if left.low_sample || right.low_sample {
                    Some(
                        "Low sample: compare directionally until each model has more calls/edit turns."
                            .to_string(),
                    )
                } else {
                    None
                };
                (
                    BehaviorSupport {
                        supported: true,
                        level: if warning.is_some() {
                            "low_sample"
                        } else {
                            "normalized"
                        }
                        .to_string(),
                        reason: warning.clone(),
                    },
                    warning,
                )
            }
            _ => (
                BehaviorSupport {
                    supported: false,
                    level: "missing_model".to_string(),
                    reason: Some("One selected model has no data in this filter.".to_string()),
                },
                Some("One selected model has no data in this filter.".to_string()),
            ),
        };
        let metrics = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => compare_metrics(left, right),
            _ => Vec::new(),
        };
        let working_style = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => working_style_metrics(left, right),
            _ => Vec::new(),
        };
        let category_head_to_head = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                self.legacy_category_compare(filter, &left.model, &right.model)?
            }
            _ => Vec::new(),
        };
        Ok(ModelComparePayload {
            support,
            candidates,
            model_a: stats_a,
            model_b: stats_b,
            metrics,
            category_head_to_head,
            working_style,
            warning,
        })
    }

    fn model_compare_stats_pair(
        &self,
        filter: &QueryFilter,
        model_a: &str,
        model_b: &str,
    ) -> Result<(Option<ModelCompareStats>, Option<ModelCompareStats>)> {
        let mut pair_filter = filter.clone();
        pair_filter.model = None;

        let mut bucket_filter = pair_filter.bucket_filter(Some("b"));
        bucket_filter.push_raw("b.model IN (?, ?)");
        bucket_filter.push_value(rusqlite::types::Value::Text(model_a.to_string()));
        bucket_filter.push_value(rusqlite::types::Value::Text(model_b.to_string()));
        let bucket_sql = format!(
            r#"
            SELECT
                b.model,
                COALESCE(SUM(b.event_count), 0),
                COALESCE(SUM(b.total_tokens), 0),
                COALESCE(SUM(b.cost_with_cache_usd), 0.0),
                COALESCE(SUM(b.input_tokens), 0),
                COALESCE(SUM(b.cache_creation_tokens), 0),
                COALESCE(SUM(b.cache_read_tokens), 0)
            FROM usage_bucket_30m b
            {}
            GROUP BY b.model
            "#,
            bucket_filter.where_sql()
        );
        let mut bucket_stmt = self.conn.prepare(&bucket_sql)?;
        let bucket_rows =
            bucket_stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    CompareBucketStats {
                        calls: row.get(1)?,
                        total_tokens: row.get(2)?,
                        estimated_cost_usd: row.get(3)?,
                        input_tokens: row.get(4)?,
                        cache_creation_tokens: row.get(5)?,
                        cache_read_tokens: row.get(6)?,
                    },
                ))
            })?;
        let mut buckets = HashMap::new();
        for row in bucket_rows {
            let (model, stats) = row?;
            buckets.insert(model, stats);
        }

        let mut turn_filter = pair_filter.turn_filter(Some("t"));
        turn_filter.push_raw("t.primary_model IN (?, ?)");
        turn_filter.push_value(rusqlite::types::Value::Text(model_a.to_string()));
        turn_filter.push_value(rusqlite::types::Value::Text(model_b.to_string()));
        let turn_sql = format!(
            r#"
            SELECT
                t.primary_model,
                COUNT(*),
                COALESCE(SUM(t.has_edits), 0),
                COALESCE(SUM(t.one_shot), 0),
                COALESCE(SUM(t.retries), 0),
                COALESCE(SUM(CASE WHEN t.category = 'delegation' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.category = 'planning' THEN 1 ELSE 0 END), 0)
            FROM usage_turn t
            {}
            GROUP BY t.primary_model
            "#,
            turn_filter.where_sql()
        );
        let mut turn_stmt = self.conn.prepare(&turn_sql)?;
        let turn_rows =
            turn_stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    CompareTurnStats {
                        turns: row.get(1)?,
                        edit_turns: row.get(2)?,
                        one_shot_turns: row.get(3)?,
                        retries: row.get(4)?,
                        delegation_turns: row.get(5)?,
                        planning_turns: row.get(6)?,
                    },
                ))
            })?;
        let mut turns = HashMap::new();
        for row in turn_rows {
            let (model, stats) = row?;
            turns.insert(model, stats);
        }

        let mut tool_filter = pair_filter.tool_filter(Some("tc"));
        tool_filter.push_raw("tc.model IN (?, ?)");
        tool_filter.push_value(rusqlite::types::Value::Text(model_a.to_string()));
        tool_filter.push_value(rusqlite::types::Value::Text(model_b.to_string()));
        let tool_sql = format!(
            "SELECT tc.model, COUNT(*) FROM usage_tool_call tc{} GROUP BY tc.model",
            tool_filter.where_sql()
        );
        let mut tool_stmt = self.conn.prepare(&tool_sql)?;
        let tool_rows = tool_stmt
            .query_map(params_from_iter(tool_filter.params().iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
        let mut tool_calls = HashMap::new();
        for row in tool_rows {
            let (model, calls) = row?;
            tool_calls.insert(model, calls);
        }

        Ok((
            build_model_compare_stats(model_a, &buckets, &turns, &tool_calls),
            build_model_compare_stats(model_b, &buckets, &turns, &tool_calls),
        ))
    }

    #[cfg(test)]
    fn legacy_model_compare_stats(
        &self,
        filter: &QueryFilter,
        model: &str,
    ) -> Result<Option<ModelCompareStats>> {
        let mut model_filter = filter.clone();
        model_filter.model = Some(model.to_string());

        let bucket_filter = model_filter.bucket_filter(Some("b"));
        let token_sql = format!(
            r#"
            SELECT
                COALESCE(SUM(b.event_count), 0),
                COALESCE(SUM(b.total_tokens), 0),
                COALESCE(SUM(b.cost_with_cache_usd), 0.0),
                COALESCE(SUM(b.input_tokens), 0),
                COALESCE(SUM(b.cache_creation_tokens), 0),
                COALESCE(SUM(b.cache_read_tokens), 0)
            FROM usage_bucket_30m b
            {}
            "#,
            bucket_filter.where_sql()
        );
        let (calls, total_tokens, cost, input, cache_creation, cache_read): (
            i64,
            i64,
            f64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            &token_sql,
            params_from_iter(bucket_filter.params().iter()),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
        if calls == 0 {
            return Ok(None);
        }

        let turn_filter = model_filter.turn_filter(Some("t"));
        let turn_sql = format!(
            r#"
            SELECT
                COUNT(*),
                COALESCE(SUM(t.has_edits), 0),
                COALESCE(SUM(t.one_shot), 0),
                COALESCE(SUM(t.retries), 0),
                COALESCE(SUM(CASE WHEN t.category = 'delegation' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.category = 'planning' THEN 1 ELSE 0 END), 0)
            FROM usage_turn t
            {}
            "#,
            turn_filter.where_sql()
        );
        let (turns, edit_turns, one_shot_turns, retries, delegation_turns, planning_turns): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            &turn_sql,
            params_from_iter(turn_filter.params().iter()),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;

        let tool_filter = model_filter.tool_filter(Some("tc"));
        let tool_calls = scalar_i64(
            &self.conn,
            &format!(
                "SELECT COUNT(*) FROM usage_tool_call tc{}",
                tool_filter.where_sql()
            ),
            params_from_iter(tool_filter.params().iter()),
        )?;
        let cache_efficiency = ratio(cache_read, input + cache_creation + cache_read);
        Ok(Some(ModelCompareStats {
            model: model.to_string(),
            calls,
            turns,
            edit_turns,
            one_shot_turns,
            retries,
            total_tokens,
            estimated_cost_usd: cost,
            cache_efficiency,
            cost_per_call: ratio_f64(cost, calls),
            cost_per_edit_turn: ratio_f64(cost, edit_turns),
            one_shot_rate: ratio(one_shot_turns, edit_turns),
            retry_rate: ratio(retries, turns),
            avg_tools_per_turn: ratio(tool_calls, turns),
            delegation_rate: ratio(delegation_turns, turns),
            planning_rate: ratio(planning_turns, turns),
            low_sample: calls < 20 || edit_turns < 10,
        }))
    }

    fn category_compare_pair(
        &self,
        filter: &QueryFilter,
        model_a: &str,
        model_b: &str,
    ) -> Result<Vec<CategoryCompareRow>> {
        let mut pair_filter = filter.clone();
        pair_filter.model = None;
        let mut turn_filter = pair_filter.turn_filter(Some("t"));
        turn_filter.push_raw("t.primary_model IN (?, ?)");
        turn_filter.push_value(rusqlite::types::Value::Text(model_a.to_string()));
        turn_filter.push_value(rusqlite::types::Value::Text(model_b.to_string()));
        let sql = format!(
            r#"
            SELECT
                t.primary_model,
                t.category,
                COALESCE(SUM(t.has_edits), 0) AS edit_turns,
                COALESCE(SUM(t.one_shot), 0) AS one_shot_turns
            FROM usage_turn t
            {}
            GROUP BY t.primary_model, t.category
            HAVING edit_turns > 0
            "#,
            turn_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut rows_by_category: BTreeMap<String, (i64, i64, i64, i64)> = BTreeMap::new();
        for row in rows {
            let (model, category, edit_turns, one_shot_turns) = row?;
            let entry = rows_by_category.entry(category).or_default();
            if model == model_a {
                entry.0 = edit_turns;
                entry.1 = one_shot_turns;
            }
            if model == model_b {
                entry.2 = edit_turns;
                entry.3 = one_shot_turns;
            }
        }
        Ok(rows_by_category
            .into_iter()
            .map(
                |(category, (a_edits, a_one_shot, b_edits, b_one_shot))| CategoryCompareRow {
                    category,
                    model_a_edit_turns: a_edits,
                    model_a_one_shot_rate: ratio(a_one_shot, a_edits),
                    model_b_edit_turns: b_edits,
                    model_b_one_shot_rate: ratio(b_one_shot, b_edits),
                },
            )
            .collect())
    }

    #[cfg(test)]
    fn legacy_category_compare(
        &self,
        filter: &QueryFilter,
        model_a: &str,
        model_b: &str,
    ) -> Result<Vec<CategoryCompareRow>> {
        let mut rows_by_category: BTreeMap<String, (i64, i64, i64, i64)> = BTreeMap::new();
        for (index, model) in [model_a, model_b].into_iter().enumerate() {
            let mut model_filter = filter.clone();
            model_filter.model = Some(model.to_string());
            let turn_filter = model_filter.turn_filter(Some("t"));
            let sql = format!(
                r#"
                SELECT
                    t.category,
                    COALESCE(SUM(t.has_edits), 0) AS edit_turns,
                    COALESCE(SUM(t.one_shot), 0) AS one_shot_turns
                FROM usage_turn t
                {}
                GROUP BY t.category
                HAVING edit_turns > 0
                "#,
                turn_filter.where_sql()
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (category, edit_turns, one_shot_turns) = row?;
                let entry = rows_by_category.entry(category).or_default();
                if index == 0 {
                    entry.0 = edit_turns;
                    entry.1 = one_shot_turns;
                } else {
                    entry.2 = edit_turns;
                    entry.3 = one_shot_turns;
                }
            }
        }
        Ok(rows_by_category
            .into_iter()
            .map(
                |(category, (a_edits, a_one_shot, b_edits, b_one_shot))| CategoryCompareRow {
                    category,
                    model_a_edit_turns: a_edits,
                    model_a_one_shot_rate: ratio(a_one_shot, a_edits),
                    model_b_edit_turns: b_edits,
                    model_b_one_shot_rate: ratio(b_one_shot, b_edits),
                },
            )
            .collect())
    }
}
