use super::*;

/// Tool/action aggregate powered by `usage_tool_call` plus attributed event cost.
#[derive(Debug, Clone, Serialize)]
pub struct ToolBreakdown {
    /// Coarse tool/action family.
    pub tool_kind: String,
    /// Source tool name or MCP tool name.
    pub tool_name: String,
    /// MCP server name when applicable.
    pub mcp_server: Option<String>,
    /// Number of normalized calls.
    pub calls: i64,
    /// Distinct turns touched by this tool when turn keys are available.
    pub turn_count: i64,
    /// Distinct sessions touched by this tool.
    pub session_count: i64,
    /// Estimated cost attributed through parent events after shared-event split.
    pub estimated_cost_usd: f64,
    /// Share of all calls in the current filter.
    pub call_share: f64,
    /// First observed call timestamp.
    pub first_seen_at: Option<String>,
    /// Last observed call timestamp.
    pub last_seen_at: Option<String>,
}

/// Top-level tool analytics payload.
#[derive(Debug, Clone, Serialize)]
pub struct ToolsPayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Tool aggregates ordered by calls/cost/name.
    pub breakdown: Vec<ToolBreakdown>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub(super) struct AttributedToolRow {
    tool_kind: String,
    tool_name: String,
    mcp_server: Option<String>,
    calls: i64,
    turn_count: i64,
    session_count: i64,
    estimated_cost_usd: f64,
    input_tokens: f64,
    cache_read_tokens: f64,
    cache_creation_tokens: f64,
    output_tokens: f64,
    reasoning_output_tokens: f64,
    first_seen_at: Option<String>,
    last_seen_at: Option<String>,
}

struct ToolEventAttribution {
    event_key: String,
    event_at: String,
    session_id: Option<String>,
    estimated_cost_usd: f64,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
}

struct FilteredToolFact {
    event_key: Option<String>,
    turn_key: Option<String>,
    session_id: Option<String>,
    occurred_at: String,
    tool_kind: String,
    tool_name: String,
    mcp_server: Option<String>,
}

fn tool_event_attribution_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ToolEventAttribution> {
    Ok(ToolEventAttribution {
        event_key: row.get(0)?,
        event_at: row.get(1)?,
        session_id: row.get(2)?,
        estimated_cost_usd: row.get(3)?,
        input_tokens: row.get(4)?,
        cache_read_tokens: row.get(5)?,
        cache_creation_tokens: row.get(6)?,
        output_tokens: row.get(7)?,
        reasoning_output_tokens: row.get(8)?,
    })
}

#[derive(Default)]
struct ToolAggregate {
    calls: i64,
    turn_keys: HashSet<String>,
    session_ids: HashSet<String>,
    estimated_cost_usd: f64,
    input_tokens: f64,
    cache_read_tokens: f64,
    cache_creation_tokens: f64,
    output_tokens: f64,
    reasoning_output_tokens: f64,
    first_seen_at: Option<String>,
    last_seen_at: Option<String>,
}

type ToolAggregateKey = (String, String, Option<String>);

impl ToolAggregate {
    fn observe_at(&mut self, occurred_at: &str) {
        if self
            .first_seen_at
            .as_deref()
            .is_none_or(|current| occurred_at < current)
        {
            self.first_seen_at = Some(occurred_at.to_string());
        }
        if self
            .last_seen_at
            .as_deref()
            .is_none_or(|current| occurred_at > current)
        {
            self.last_seen_at = Some(occurred_at.to_string());
        }
    }
}

fn attribute_non_tool_event(
    aggregates: &mut BTreeMap<ToolAggregateKey, ToolAggregate>,
    event_tool_counts: &HashMap<String, i64>,
    event: &ToolEventAttribution,
) {
    if event_tool_counts.contains_key(&event.event_key) {
        return;
    }
    let aggregate = aggregates
        .entry(("(non-tool)".to_string(), "(non-tool)".to_string(), None))
        .or_default();
    aggregate
        .turn_keys
        .insert(format!("turn:{}", event.event_key));
    if let Some(session_id) = &event.session_id {
        aggregate.session_ids.insert(session_id.clone());
    }
    aggregate.estimated_cost_usd += event.estimated_cost_usd;
    aggregate.input_tokens += event.input_tokens as f64;
    aggregate.cache_read_tokens += event.cache_read_tokens as f64;
    aggregate.cache_creation_tokens += event.cache_creation_tokens as f64;
    aggregate.output_tokens += event.output_tokens as f64;
    aggregate.reasoning_output_tokens += event.reasoning_output_tokens as f64;
    aggregate.observe_at(&event.event_at);
}

impl Dashboard {
    /// Loads attributed tool/action aggregates from normalized behavior facts.
    ///
    /// Shared-event cost is split across sibling tool calls, and cost-bearing
    /// turns without any tool calls are surfaced as a `(non-tool)` bucket.
    pub fn tool_breakdown(&self, filter: &QueryFilter) -> Result<ToolsPayload> {
        let support = behavior_support(&self.conn, "usage_event", filter.event_filter(None))?;
        if !support.supported {
            return Ok(ToolsPayload {
                support,
                breakdown: Vec::new(),
            });
        }

        let rows = self.tool_attribution_rows(filter)?;
        let total_calls: i64 = rows.iter().map(|row| row.calls).sum();
        let breakdown = rows
            .into_iter()
            .map(|row| ToolBreakdown {
                tool_kind: row.tool_kind,
                tool_name: row.tool_name,
                mcp_server: row.mcp_server,
                calls: row.calls,
                turn_count: row.turn_count,
                session_count: row.session_count,
                estimated_cost_usd: row.estimated_cost_usd,
                call_share: ratio(row.calls, total_calls),
                first_seen_at: row.first_seen_at,
                last_seen_at: row.last_seen_at,
            })
            .collect();
        Ok(ToolsPayload { support, breakdown })
    }

    pub(super) fn tool_attribution_rows(
        &self,
        filter: &QueryFilter,
    ) -> Result<Vec<AttributedToolRow>> {
        let tool_filter = filter.tool_filter(Some("tc"));
        let sql = format!(
            r#"
            SELECT
                tc.event_key,
                tc.turn_key,
                tc.session_id,
                tc.occurred_at,
                tc.tool_kind,
                tc.tool_name,
                tc.mcp_server
            FROM usage_tool_call tc
            {}
            "#,
            tool_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(tool_filter.params().iter()), |row| {
            Ok(FilteredToolFact {
                event_key: row.get(0)?,
                turn_key: row.get(1)?,
                session_id: row.get(2)?,
                occurred_at: row.get(3)?,
                tool_kind: row.get(4)?,
                tool_name: row.get(5)?,
                mcp_server: row.get(6)?,
            })
        })?;
        let mut tool_facts = Vec::new();
        let mut event_tool_counts: HashMap<String, i64> = HashMap::new();
        for row in rows {
            let fact = row?;
            if let Some(event_key) = &fact.event_key {
                *event_tool_counts.entry(event_key.clone()).or_default() += 1;
            }
            tool_facts.push(fact);
        }

        let event_filter = filter.event_filter(Some("e"));
        let has_event_filter = !event_filter.where_sql().is_empty();
        let event_sql = format!(
            r#"
            SELECT
                e.event_key,
                e.event_at,
                e.session_id,
                COALESCE(e.cost_with_cache_usd, 0.0),
                COALESCE(e.input_tokens, 0),
                COALESCE(e.cache_read_tokens, 0),
                COALESCE(e.cache_creation_tokens, 0),
                COALESCE(e.output_tokens, 0),
                COALESCE(e.reasoning_output_tokens, 0)
            FROM usage_event e
            {}
            "#,
            event_filter.where_sql()
        );
        let mut events = Vec::new();
        let mut event_indexes = HashMap::new();
        let mut filtered_event_keys = has_event_filter.then(HashSet::new);
        let mut event_stmt = self.conn.prepare(&event_sql)?;
        let event_rows = event_stmt.query_map(
            params_from_iter(event_filter.params().iter()),
            tool_event_attribution_from_row,
        )?;
        for row in event_rows {
            let event = row?;
            if let Some(keys) = &mut filtered_event_keys {
                keys.insert(event.event_key.clone());
            }
            event_indexes.insert(event.event_key.clone(), events.len());
            events.push(event);
        }

        if has_event_filter {
            let missing_tool_event_keys = event_tool_counts
                .keys()
                .filter(|event_key| !event_indexes.contains_key(*event_key))
                .cloned()
                .collect::<Vec<_>>();
            for event_keys in missing_tool_event_keys.chunks(500) {
                let placeholders = std::iter::repeat_n("?", event_keys.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    r#"
                    SELECT
                        event_key,
                        event_at,
                        session_id,
                        COALESCE(cost_with_cache_usd, 0.0),
                        COALESCE(input_tokens, 0),
                        COALESCE(cache_read_tokens, 0),
                        COALESCE(cache_creation_tokens, 0),
                        COALESCE(output_tokens, 0),
                        COALESCE(reasoning_output_tokens, 0)
                    FROM usage_event
                    WHERE event_key IN ({placeholders})
                    "#
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params_from_iter(event_keys.iter()),
                    tool_event_attribution_from_row,
                )?;
                for row in rows {
                    let event = row?;
                    event_indexes.insert(event.event_key.clone(), events.len());
                    events.push(event);
                }
            }
        }

        let mut aggregates: BTreeMap<ToolAggregateKey, ToolAggregate> = BTreeMap::new();
        for fact in tool_facts {
            let Some(event_key) = fact.event_key.as_deref() else {
                continue;
            };
            let Some(event) = event_indexes
                .get(event_key)
                .and_then(|index| events.get(*index))
            else {
                continue;
            };
            let sibling_count = event_tool_counts
                .get(event_key)
                .copied()
                .unwrap_or_default();
            if sibling_count <= 0 {
                continue;
            }
            let aggregate = aggregates
                .entry((fact.tool_kind, fact.tool_name, fact.mcp_server))
                .or_default();
            aggregate.calls += 1;
            aggregate
                .turn_keys
                .insert(fact.turn_key.unwrap_or_else(|| format!("turn:{event_key}")));
            if let Some(session_id) = fact.session_id.or_else(|| event.session_id.clone()) {
                aggregate.session_ids.insert(session_id);
            }
            aggregate.estimated_cost_usd += event.estimated_cost_usd / sibling_count as f64;
            let fraction = 1.0 / sibling_count as f64;
            aggregate.input_tokens += event.input_tokens as f64 * fraction;
            aggregate.cache_read_tokens += event.cache_read_tokens as f64 * fraction;
            aggregate.cache_creation_tokens += event.cache_creation_tokens as f64 * fraction;
            aggregate.output_tokens += event.output_tokens as f64 * fraction;
            aggregate.reasoning_output_tokens += event.reasoning_output_tokens as f64 * fraction;
            aggregate.observe_at(&fact.occurred_at);
        }

        if let Some(filtered_event_keys) = filtered_event_keys {
            for event_key in filtered_event_keys {
                if let Some(event) = event_indexes
                    .get(&event_key)
                    .and_then(|index| events.get(*index))
                {
                    attribute_non_tool_event(&mut aggregates, &event_tool_counts, event);
                }
            }
        } else {
            for event in &events {
                attribute_non_tool_event(&mut aggregates, &event_tool_counts, event);
            }
        }

        let mut rows = aggregates
            .into_iter()
            .map(
                |((tool_kind, tool_name, mcp_server), aggregate)| AttributedToolRow {
                    tool_kind,
                    tool_name,
                    mcp_server,
                    calls: aggregate.calls,
                    turn_count: aggregate.turn_keys.len() as i64,
                    session_count: aggregate.session_ids.len() as i64,
                    estimated_cost_usd: aggregate.estimated_cost_usd,
                    input_tokens: aggregate.input_tokens,
                    cache_read_tokens: aggregate.cache_read_tokens,
                    cache_creation_tokens: aggregate.cache_creation_tokens,
                    output_tokens: aggregate.output_tokens,
                    reasoning_output_tokens: aggregate.reasoning_output_tokens,
                    first_seen_at: aggregate.first_seen_at,
                    last_seen_at: aggregate.last_seen_at,
                },
            )
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .calls
                .cmp(&left.calls)
                .then_with(|| right.estimated_cost_usd.total_cmp(&left.estimated_cost_usd))
                .then_with(|| left.tool_kind.cmp(&right.tool_kind))
                .then_with(|| left.tool_name.cmp(&right.tool_name))
                .then_with(|| left.mcp_server.cmp(&right.mcp_server))
        });
        rows.truncate(50);
        Ok(rows)
    }

    #[cfg(test)]
    pub(super) fn legacy_tool_attribution_rows(
        &self,
        filter: &QueryFilter,
    ) -> Result<Vec<AttributedToolRow>> {
        let event_filter = filter.event_filter(Some("e"));
        let tool_filter = filter.tool_filter(Some("tc"));
        let sql = format!(
            r#"
            WITH filtered_events AS (
                SELECT
                    e.event_key,
                    e.event_at,
                    e.session_id,
                    COALESCE(e.cost_with_cache_usd, 0.0) AS cost_with_cache_usd,
                    COALESCE(e.input_tokens, 0) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
                FROM usage_event e
                {event_where}
            ),
            filtered_tools AS (
                SELECT
                    tc.tool_call_key,
                    tc.event_key,
                    tc.turn_key,
                    tc.session_id,
                    tc.occurred_at,
                    tc.tool_kind,
                    tc.tool_name,
                    tc.mcp_server
                FROM usage_tool_call tc
                {tool_where}
            ),
            event_tool_counts AS (
                SELECT
                    tc.event_key,
                    COUNT(*) AS tool_count
                FROM filtered_tools tc
                WHERE tc.event_key IS NOT NULL
                GROUP BY tc.event_key
            ),
            attributed_rows AS (
                SELECT
                    tc.tool_kind AS tool_kind,
                    tc.tool_name AS tool_name,
                    tc.mcp_server AS mcp_server,
                    COALESCE(tc.turn_key, 'turn:' || tc.event_key) AS turn_key,
                    COALESCE(tc.session_id, e.session_id) AS session_id,
                    tc.occurred_at AS occurred_at,
                    1 AS call_count,
                    COALESCE(e.cost_with_cache_usd, 0.0) / ec.tool_count AS estimated_cost_usd,
                    COALESCE(e.input_tokens, 0) * (1.0 / ec.tool_count) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) * (1.0 / ec.tool_count) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) * (1.0 / ec.tool_count) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) * (1.0 / ec.tool_count) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) * (1.0 / ec.tool_count) AS reasoning_output_tokens
                FROM filtered_tools tc
                JOIN usage_event e ON e.event_key = tc.event_key
                JOIN event_tool_counts ec ON ec.event_key = tc.event_key

                UNION ALL

                SELECT
                    '(non-tool)' AS tool_kind,
                    '(non-tool)' AS tool_name,
                    NULL AS mcp_server,
                    'turn:' || e.event_key AS turn_key,
                    e.session_id AS session_id,
                    e.event_at AS occurred_at,
                    0 AS call_count,
                    COALESCE(e.cost_with_cache_usd, 0.0) AS estimated_cost_usd,
                    COALESCE(e.input_tokens, 0) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
                FROM filtered_events e
                LEFT JOIN filtered_tools tc ON tc.event_key = e.event_key
                WHERE tc.tool_call_key IS NULL
            )
            SELECT
                tool_kind,
                tool_name,
                mcp_server,
                COALESCE(SUM(call_count), 0) AS calls,
                COUNT(DISTINCT turn_key) AS turn_count,
                COUNT(DISTINCT session_id) AS session_count,
                COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
                COALESCE(SUM(input_tokens), 0.0) AS input_tokens,
                COALESCE(SUM(cache_read_tokens), 0.0) AS cache_read_tokens,
                COALESCE(SUM(cache_creation_tokens), 0.0) AS cache_creation_tokens,
                COALESCE(SUM(output_tokens), 0.0) AS output_tokens,
                COALESCE(SUM(reasoning_output_tokens), 0.0) AS reasoning_output_tokens,
                MIN(occurred_at) AS first_seen_at,
                MAX(occurred_at) AS last_seen_at
            FROM attributed_rows
            GROUP BY tool_kind, tool_name, mcp_server
            ORDER BY calls DESC, estimated_cost_usd DESC, tool_kind ASC, tool_name ASC
            LIMIT 50
            "#,
            event_where = event_filter.where_sql(),
            tool_where = tool_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter(
                event_filter
                    .params()
                    .iter()
                    .chain(tool_filter.params().iter()),
            ),
            |row| {
                Ok(AttributedToolRow {
                    tool_kind: row.get(0)?,
                    tool_name: row.get(1)?,
                    mcp_server: row.get(2)?,
                    calls: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                    turn_count: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                    session_count: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                    estimated_cost_usd: row.get::<_, Option<f64>>(6)?.unwrap_or_default(),
                    input_tokens: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                    cache_read_tokens: row.get::<_, Option<f64>>(8)?.unwrap_or_default(),
                    cache_creation_tokens: row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
                    output_tokens: row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
                    reasoning_output_tokens: row.get::<_, Option<f64>>(11)?.unwrap_or_default(),
                    first_seen_at: row.get(12)?,
                    last_seen_at: row.get(13)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
