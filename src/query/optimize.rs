use super::*;

fn severity_rank(severity: &str) -> i64 {
    match severity {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

fn health_grade(score: i64) -> &'static str {
    match score {
        90..=100 => "A",
        80..=89 => "B",
        70..=79 => "C",
        60..=69 => "D",
        _ => "F",
    }
}

fn safe_short(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// One read-only optimization finding derived from normalized local facts.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizeFinding {
    /// Stable detector id.
    pub id: String,
    /// Human-readable finding title.
    pub title: String,
    /// `high`, `medium`, or `low`.
    pub severity: String,
    /// Evidence summary with bounded, display-safe values.
    pub evidence: String,
    /// Read-only recommendation. llmusage never executes it automatically.
    pub recommendation: String,
    /// Rough token-savings estimate; use as a prioritization hint only.
    pub estimated_savings_tokens: i64,
    /// Rough USD-savings estimate using already persisted local costs.
    pub estimated_savings_usd: f64,
}

/// Read-only behavior optimization payload.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizePayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Simple health score after detector penalties.
    pub score: i64,
    /// Letter grade derived from [`Self::score`].
    pub grade: String,
    /// Sum of detector token-savings estimates.
    pub estimated_savings_tokens: i64,
    /// Sum of detector USD-savings estimates.
    pub estimated_savings_usd: f64,
    /// Findings ordered by severity and estimated savings.
    pub findings: Vec<OptimizeFinding>,
}

/// Read-only zero-call ("zombie") inventory report: locally installed skills and
/// MCP servers that have no recorded call in `usage_tool_call`.
#[derive(Debug, Clone, Serialize)]
pub struct ZombieReport {
    /// Total installed items scanned (skills + MCP across detected sources).
    pub installed_total: usize,
    /// Installed-but-never-called items, sorted by source/kind/name.
    pub zombies: Vec<ZombieItem>,
}

/// One installed-but-never-called skill or MCP server.
#[derive(Debug, Clone, Serialize)]
pub struct ZombieItem {
    /// Owning CLI (`claude` / `codex` / `opencode`).
    pub source: String,
    /// `skill` or `mcp`.
    pub kind: String,
    /// Skill name or MCP server name.
    pub name: String,
}

impl Dashboard {
    /// Loads read-only behavior optimization findings.
    ///
    /// The detectors are intentionally conservative and only use normalized
    /// `usage_turn` / `usage_tool_call` rows plus persisted event costs. They
    /// never inspect raw transcripts and never execute cleanup actions.
    pub fn optimize(&self, filter: &QueryFilter) -> Result<OptimizePayload> {
        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        if !support.supported {
            return Ok(OptimizePayload {
                support,
                score: 100,
                grade: "A".to_string(),
                estimated_savings_tokens: 0,
                estimated_savings_usd: 0.0,
                findings: Vec::new(),
            });
        }

        let mut findings = Vec::new();
        if let Some(finding) = self.detect_low_read_edit_ratio(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_duplicate_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_junk_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_session_outlier(filter)? {
            findings.push(finding);
        }

        findings.sort_by(|left, right| {
            severity_rank(&right.severity)
                .cmp(&severity_rank(&left.severity))
                .then_with(|| {
                    right
                        .estimated_savings_tokens
                        .cmp(&left.estimated_savings_tokens)
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        let estimated_savings_tokens = findings
            .iter()
            .map(|finding| finding.estimated_savings_tokens)
            .sum();
        let estimated_savings_usd = findings
            .iter()
            .map(|finding| finding.estimated_savings_usd)
            .sum();
        let penalty = findings
            .iter()
            .map(|finding| match finding.severity.as_str() {
                "high" => 25,
                "medium" => 15,
                _ => 7,
            })
            .sum::<i64>();
        let score = (100 - penalty).clamp(0, 100);

        Ok(OptimizePayload {
            support,
            score,
            grade: health_grade(score).to_string(),
            estimated_savings_tokens,
            estimated_savings_usd,
            findings,
        })
    }

    #[cfg(test)]
    pub(super) fn legacy_optimize(&self, filter: &QueryFilter) -> Result<OptimizePayload> {
        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        if !support.supported {
            return Ok(OptimizePayload {
                support,
                score: 100,
                grade: "A".to_string(),
                estimated_savings_tokens: 0,
                estimated_savings_usd: 0.0,
                findings: Vec::new(),
            });
        }

        let mut findings = Vec::new();
        if let Some(finding) = self.legacy_detect_low_read_edit_ratio(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_duplicate_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_junk_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.legacy_detect_session_outlier(filter)? {
            findings.push(finding);
        }
        findings.sort_by(|left, right| {
            severity_rank(&right.severity)
                .cmp(&severity_rank(&left.severity))
                .then_with(|| {
                    right
                        .estimated_savings_tokens
                        .cmp(&left.estimated_savings_tokens)
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        let estimated_savings_tokens = findings
            .iter()
            .map(|finding| finding.estimated_savings_tokens)
            .sum();
        let estimated_savings_usd = findings
            .iter()
            .map(|finding| finding.estimated_savings_usd)
            .sum();
        let penalty = findings
            .iter()
            .map(|finding| match finding.severity.as_str() {
                "high" => 25,
                "medium" => 15,
                _ => 7,
            })
            .sum::<i64>();
        let score = (100 - penalty).clamp(0, 100);
        Ok(OptimizePayload {
            support,
            score,
            grade: health_grade(score).to_string(),
            estimated_savings_tokens,
            estimated_savings_usd,
            findings,
        })
    }

    /// Diffs locally-installed skills / MCP servers against the actually-called
    /// set in `usage_tool_call`, returning never-called ("zombie") candidates.
    ///
    /// Read-only: this only reports candidates; llmusage never deletes or modifies
    /// anything. Matching is by `(source, name)` — skills resolve to concrete names
    /// only for Claude and OpenCode (Codex skills are not scanned), MCP matches by
    /// `(source, server)` across all three CLIs.
    pub fn zombie_report(&self, roots: &inventory::InventoryRoots) -> Result<ZombieReport> {
        let installed = roots.scan();
        let used_skills = self.used_tool_pairs("skill", "tool_name")?;
        let used_mcp = self.used_tool_pairs("mcp", "mcp_server")?;

        let mut zombies = Vec::new();
        for item in &installed {
            let used = match item.kind {
                inventory::InventoryKind::Skill => &used_skills,
                inventory::InventoryKind::Mcp => &used_mcp,
            };
            let key = (item.source.as_str().to_string(), item.name.clone());
            if !used.contains(&key) {
                zombies.push(ZombieItem {
                    source: item.source.as_str().to_string(),
                    kind: item.kind.as_str().to_string(),
                    name: item.name.clone(),
                });
            }
        }
        Ok(ZombieReport {
            installed_total: installed.len(),
            zombies,
        })
    }

    /// Distinct `(source, value)` pairs actually observed for a `tool_kind`.
    /// `column` is a fixed identifier (`tool_name` / `mcp_server`), never user input.
    fn used_tool_pairs(&self, tool_kind: &str, column: &str) -> Result<BTreeSet<(String, String)>> {
        let sql = format!(
            "SELECT DISTINCT source, {column} FROM usage_tool_call \
             WHERE tool_kind = ?1 AND {column} IS NOT NULL AND {column} != ''"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([tool_kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut set = BTreeSet::new();
        for row in rows {
            set.insert(row?);
        }
        Ok(set)
    }

    fn detect_low_read_edit_ratio(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let tool_filter = filter.tool_filter(Some("tc"));
        let count_sql = format!(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN tc.tool_kind IN ('read', 'search') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN 1 ELSE 0 END), 0)
            FROM usage_tool_call tc
            {}
            "#,
            tool_filter.where_sql()
        );
        let (read_calls, edit_calls): (i64, i64) = self.conn.query_row(
            &count_sql,
            params_from_iter(tool_filter.params().iter()),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if edit_calls < 3 {
            return Ok(None);
        }
        let read_edit_ratio = read_calls as f64 / edit_calls as f64;
        if read_edit_ratio >= 0.5 {
            return Ok(None);
        }

        let mut edit_filter = filter.tool_filter(Some("tc"));
        edit_filter.push_raw("tc.tool_kind = 'edit'");
        let edit_sql = format!(
            r#"
            SELECT
                COALESCE(SUM(e.total_tokens), 0),
                COALESCE(SUM(e.cost_with_cache_usd), 0.0)
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            "#,
            edit_filter.where_sql()
        );
        let (edit_tokens, edit_cost): (i64, f64) = self.conn.query_row(
            &edit_sql,
            params_from_iter(edit_filter.params().iter()),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(Some(OptimizeFinding {
            id: "low_read_edit_ratio".to_string(),
            title: "Low Read/Edit ratio".to_string(),
            severity: if read_edit_ratio < 0.25 {
                "high"
            } else {
                "medium"
            }
            .to_string(),
            evidence: format!(
                "{read_calls} read/search calls for {edit_calls} edit calls in this filter."
            ),
            recommendation:
                "Review files before larger edit runs; this is a read-only signal, not an automatic rewrite."
                    .to_string(),
            estimated_savings_tokens: (edit_tokens / 5).max(0),
            estimated_savings_usd: (edit_cost * 0.20).max(0.0),
        }))
    }

    #[cfg(test)]
    fn legacy_detect_low_read_edit_ratio(
        &self,
        filter: &QueryFilter,
    ) -> Result<Option<OptimizeFinding>> {
        let tool_filter = filter.tool_filter(Some("tc"));
        let sql = format!(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN tc.tool_kind IN ('read', 'search') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.cost_with_cache_usd ELSE 0.0 END), 0.0)
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            "#,
            tool_filter.where_sql()
        );
        let (read_calls, edit_calls, edit_tokens, edit_cost): (i64, i64, i64, f64) = self
            .conn
            .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
        if edit_calls < 3 {
            return Ok(None);
        }
        let read_edit_ratio = read_calls as f64 / edit_calls as f64;
        if read_edit_ratio >= 0.5 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "low_read_edit_ratio".to_string(),
            title: "Low Read/Edit ratio".to_string(),
            severity: if read_edit_ratio < 0.25 {
                "high"
            } else {
                "medium"
            }
            .to_string(),
            evidence: format!(
                "{read_calls} read/search calls for {edit_calls} edit calls in this filter."
            ),
            recommendation:
                "Review files before larger edit runs; this is a read-only signal, not an automatic rewrite."
                    .to_string(),
            estimated_savings_tokens: (edit_tokens / 5).max(0),
            estimated_savings_usd: (edit_cost * 0.20).max(0.0),
        }))
    }

    fn detect_duplicate_reads(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let mut tool_filter = filter.tool_filter(Some("tc"));
        tool_filter.push_raw("tc.tool_kind IN ('read', 'search')");
        tool_filter.push_raw("tc.session_id IS NOT NULL");
        let sql = format!(
            r#"
            SELECT
                tc.session_id,
                COALESCE(tc.input_fingerprint, tc.safe_preview, tc.tool_name) AS target,
                COUNT(*) AS calls,
                COALESCE(SUM(e.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            GROUP BY tc.session_id, target
            HAVING calls > 1
            ORDER BY calls DESC, tokens DESC
            LIMIT 1
            "#,
            tool_filter.where_sql()
        );
        let row = self
            .conn
            .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .optional()?;
        let Some((session_id, target, calls, tokens, cost)) = row else {
            return Ok(None);
        };
        Ok(Some(OptimizeFinding {
            id: "duplicate_reads".to_string(),
            title: "Repeated reads in one session".to_string(),
            severity: if calls >= 5 { "high" } else { "medium" }.to_string(),
            evidence: format!(
                "Session {session_id} read/search target `{}` {calls} times.",
                safe_short(&target, 72)
            ),
            recommendation:
                "Cache the relevant facts in notes or inspect a narrower range before rereading the same target."
                    .to_string(),
            estimated_savings_tokens: (tokens * (calls - 1) / calls).max(0),
            estimated_savings_usd: (cost * (calls - 1) as f64 / calls as f64).max(0.0),
        }))
    }

    fn detect_junk_reads(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let mut tool_filter = filter.tool_filter(Some("tc"));
        tool_filter.push_raw(
            r#"
            tc.tool_kind IN ('read', 'search')
            AND (
                LOWER(COALESCE(tc.safe_preview, '')) LIKE '%node_modules%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/target/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\target\%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/dist/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\dist\%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/build/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\build\%'
            )
            "#,
        );
        let sql = format!(
            r#"
            SELECT
                COUNT(*) AS calls,
                COALESCE(SUM(e.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost,
                MAX(COALESCE(tc.safe_preview, tc.tool_name)) AS example
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            "#,
            tool_filter.where_sql()
        );
        let (calls, tokens, cost, example): (i64, i64, f64, Option<String>) =
            self.conn
                .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?;
        if calls == 0 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "junk_reads".to_string(),
            title: "Generated or dependency reads".to_string(),
            severity: if calls >= 5 { "high" } else { "low" }.to_string(),
            evidence: format!(
                "{calls} read/search calls touched generated or dependency-looking paths; example `{}`.",
                safe_short(example.as_deref().unwrap_or("--"), 72)
            ),
            recommendation:
                "Prefer source directories and ignore generated/dependency folders in manual investigation."
                    .to_string(),
            estimated_savings_tokens: (tokens / 2).max(0),
            estimated_savings_usd: (cost * 0.50).max(0.0),
        }))
    }

    fn detect_session_outlier(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let mut turn_filter = filter.turn_filter(Some("t"));
        turn_filter.push_raw("t.session_id IS NOT NULL");
        let sql = format!(
            r#"
            WITH session_totals AS (
                SELECT
                    t.session_id,
                    COUNT(*) AS turns,
                    COALESCE(SUM(t.total_tokens), 0) AS tokens
                FROM usage_turn t
                {}
                GROUP BY t.session_id
            )
            SELECT
                session_id,
                turns,
                tokens,
                COALESCE(SUM(tokens) OVER (), 0) AS total_tokens
            FROM session_totals
            ORDER BY tokens DESC
            LIMIT 1
            "#,
            turn_filter.where_sql()
        );
        let top = self
            .conn
            .query_row(&sql, params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?;
        let Some((session_id, turns, tokens, total_tokens)) = top else {
            return Ok(None);
        };
        if total_tokens <= 0 || tokens * 100 / total_tokens < 40 || turns < 3 {
            return Ok(None);
        }

        let mut session_filter = filter.turn_filter(Some("t"));
        session_filter.push("t.session_id = ?", session_id.clone());
        let cost = self.conn.query_row(
            &format!(
                r#"
                SELECT COALESCE(SUM(e.cost_with_cache_usd), 0.0)
                FROM usage_turn t
                LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
                {}
                "#,
                session_filter.where_sql()
            ),
            params_from_iter(session_filter.params().iter()),
            |row| row.get::<_, f64>(0),
        )?;
        Ok(Some(OptimizeFinding {
            id: "session_outlier".to_string(),
            title: "One session dominates behavior cost".to_string(),
            severity: "medium".to_string(),
            evidence: format!(
                "Session {session_id} accounts for {:.1}% of turn tokens in this filter.",
                tokens as f64 * 100.0 / total_tokens as f64
            ),
            recommendation:
                "Inspect this session before optimizing globally; long context or repeated retries may be local to it."
                    .to_string(),
            estimated_savings_tokens: (tokens / 4).max(0),
            estimated_savings_usd: (cost * 0.25).max(0.0),
        }))
    }

    #[cfg(test)]
    fn legacy_detect_session_outlier(
        &self,
        filter: &QueryFilter,
    ) -> Result<Option<OptimizeFinding>> {
        let turn_filter = filter.turn_filter(Some("t"));
        let sql = format!(
            r#"
            SELECT
                t.session_id,
                COUNT(*) AS turns,
                COALESCE(SUM(t.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
            FROM usage_turn t
            LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
            {}
            GROUP BY t.session_id
            HAVING t.session_id IS NOT NULL
            ORDER BY tokens DESC
            LIMIT 1
            "#,
            turn_filter.where_sql()
        );
        let top = self
            .conn
            .query_row(&sql, params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })
            .optional()?;
        let Some((session_id, turns, tokens, cost)) = top else {
            return Ok(None);
        };
        let total_tokens = scalar_i64(
            &self.conn,
            &format!(
                "SELECT COALESCE(SUM(t.total_tokens), 0) FROM usage_turn t{}",
                turn_filter.where_sql()
            ),
            params_from_iter(turn_filter.params().iter()),
        )?;
        if total_tokens <= 0 || tokens * 100 / total_tokens < 40 || turns < 3 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "session_outlier".to_string(),
            title: "One session dominates behavior cost".to_string(),
            severity: "medium".to_string(),
            evidence: format!(
                "Session {session_id} accounts for {:.1}% of turn tokens in this filter.",
                tokens as f64 * 100.0 / total_tokens as f64
            ),
            recommendation:
                "Inspect this session before optimizing globally; long context or repeated retries may be local to it."
                    .to_string(),
            estimated_savings_tokens: (tokens / 4).max(0),
            estimated_savings_usd: (cost * 0.25).max(0.0),
        }))
    }
}
