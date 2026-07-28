#!/usr/bin/env python3
"""Read-only timing and query-plan probe for Behavior dashboard SQL."""

from __future__ import annotations

import argparse
import json
import sqlite3
import time
from datetime import datetime, time as wall_time, timedelta, timezone
from pathlib import Path


def utc_text(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def one_day_bounds() -> tuple[str, str]:
    local_now = datetime.now().astimezone()
    local_zone = local_now.tzinfo
    assert local_zone is not None
    start = datetime.combine(local_now.date() - timedelta(days=1), wall_time.min, local_zone)
    end = datetime.combine(local_now.date() + timedelta(days=1), wall_time.min, local_zone)
    return utc_text(start), utc_text(end)


def where(alias: str, column: str, bounds: tuple[str, str] | None) -> tuple[str, tuple[str, ...]]:
    if bounds is None:
        return "", ()
    return f" WHERE {alias}.{column} >= ? AND {alias}.{column} < ?", bounds


def where_with(
    alias: str,
    column: str,
    bounds: tuple[str, str] | None,
    clauses: tuple[str, ...],
) -> tuple[str, tuple[str, ...]]:
    parts = list(clauses)
    params: tuple[str, ...] = ()
    if bounds is not None:
        parts[:0] = [f"{alias}.{column} >= ?", f"{alias}.{column} < ?"]
        params = bounds
    return (f" WHERE {' AND '.join(parts)}" if parts else ""), params


def profile(
    conn: sqlite3.Connection,
    scope: str,
    name: str,
    sql: str,
    params: tuple[str, ...] = (),
) -> list[sqlite3.Row]:
    plan = [row[3] for row in conn.execute(f"EXPLAIN QUERY PLAN {sql}", params)]
    started = time.perf_counter()
    rows = conn.execute(sql, params).fetchall()
    elapsed_ms = (time.perf_counter() - started) * 1000
    print(
        json.dumps(
            {
                "scope": scope,
                "query": name,
                "elapsed_ms": round(elapsed_ms, 1),
                "result_rows": len(rows),
                "plan": plan,
            },
            ensure_ascii=True,
        )
    )
    return rows


def profile_scope(
    conn: sqlite3.Connection,
    scope: str,
    bounds: tuple[str, str] | None,
) -> None:
    turn_where, turn_params = where("t", "started_at", bounds)
    event_where, event_params = where("e", "event_at", bounds)
    tool_where, tool_params = where("tc", "occurred_at", bounds)

    profile(
        conn,
        scope,
        "activity",
        f"""
        SELECT t.category, COUNT(*), COALESCE(SUM(t.has_edits), 0),
               COALESCE(SUM(t.one_shot), 0), COALESCE(SUM(t.retries), 0),
               COALESCE(SUM(t.call_count), 0), COALESCE(SUM(t.total_tokens), 0),
               COALESCE(SUM(e.cost_with_cache_usd), 0.0)
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        {turn_where}
        GROUP BY t.category
        ORDER BY 8 DESC, 7 DESC, 2 DESC, t.category ASC
        """,
        turn_params,
    )

    profile(
        conn,
        scope,
        "tools",
        f"""
        WITH filtered_events AS (
            SELECT e.event_key, e.event_at, e.session_id,
                   COALESCE(e.cost_with_cache_usd, 0.0) AS cost_with_cache_usd,
                   COALESCE(e.input_tokens, 0) AS input_tokens,
                   COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                   COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                   COALESCE(e.output_tokens, 0) AS output_tokens,
                   COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
            FROM usage_event e {event_where}
        ),
        filtered_tools AS (
            SELECT tc.tool_call_key, tc.event_key, tc.turn_key, tc.session_id,
                   tc.occurred_at, tc.tool_kind, tc.tool_name, tc.mcp_server
            FROM usage_tool_call tc {tool_where}
        ),
        event_tool_counts AS (
            SELECT tc.event_key, COUNT(*) AS tool_count
            FROM filtered_tools tc
            WHERE tc.event_key IS NOT NULL
            GROUP BY tc.event_key
        ),
        attributed_rows AS (
            SELECT tc.tool_kind, tc.tool_name, tc.mcp_server,
                   COALESCE(tc.turn_key, 'turn:' || tc.event_key) AS turn_key,
                   COALESCE(tc.session_id, e.session_id) AS session_id,
                   tc.occurred_at, 1 AS call_count,
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
            SELECT '(non-tool)', '(non-tool)', NULL, 'turn:' || e.event_key,
                   e.session_id, e.event_at, 0,
                   COALESCE(e.cost_with_cache_usd, 0.0), COALESCE(e.input_tokens, 0),
                   COALESCE(e.cache_read_tokens, 0), COALESCE(e.cache_creation_tokens, 0),
                   COALESCE(e.output_tokens, 0), COALESCE(e.reasoning_output_tokens, 0)
            FROM filtered_events e
            LEFT JOIN filtered_tools tc ON tc.event_key = e.event_key
            WHERE tc.tool_call_key IS NULL
        )
        SELECT tool_kind, tool_name, mcp_server, COALESCE(SUM(call_count), 0),
               COUNT(DISTINCT turn_key), COUNT(DISTINCT session_id),
               COALESCE(SUM(estimated_cost_usd), 0.0), COALESCE(SUM(input_tokens), 0.0),
               COALESCE(SUM(cache_read_tokens), 0.0), COALESCE(SUM(cache_creation_tokens), 0.0),
               COALESCE(SUM(output_tokens), 0.0), COALESCE(SUM(reasoning_output_tokens), 0.0),
               MIN(occurred_at), MAX(occurred_at)
        FROM attributed_rows
        GROUP BY tool_kind, tool_name, mcp_server
        ORDER BY 4 DESC, 7 DESC, tool_kind ASC, tool_name ASC
        LIMIT 50
        """,
        event_params + tool_params,
    )

    profile(
        conn,
        scope,
        "tools_raw_join_candidate",
        f"""
        WITH filtered_events AS (
            SELECT e.event_key, e.event_at, e.session_id,
                   COALESCE(e.cost_with_cache_usd, 0.0) AS cost_with_cache_usd,
                   COALESCE(e.input_tokens, 0) AS input_tokens,
                   COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                   COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                   COALESCE(e.output_tokens, 0) AS output_tokens,
                   COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
            FROM usage_event e {event_where}
        ),
        filtered_tools AS (
            SELECT tc.tool_call_key, tc.event_key, tc.turn_key, tc.session_id,
                   tc.occurred_at, tc.tool_kind, tc.tool_name, tc.mcp_server
            FROM usage_tool_call tc {tool_where}
        )
        SELECT e.event_key, e.event_at, e.session_id, e.cost_with_cache_usd,
               e.input_tokens, e.cache_read_tokens, e.cache_creation_tokens,
               e.output_tokens, e.reasoning_output_tokens,
               tc.tool_call_key, tc.turn_key, tc.session_id, tc.occurred_at,
               tc.tool_kind, tc.tool_name, tc.mcp_server
        FROM filtered_events e
        LEFT JOIN filtered_tools tc ON tc.event_key = e.event_key
        ORDER BY e.event_key
        """,
        event_params + tool_params,
    )

    low_where, low_params = where("tc", "occurred_at", bounds)
    profile(
        conn,
        scope,
        "optimize_low_read_edit",
        f"""
        SELECT COALESCE(SUM(CASE WHEN tc.tool_kind IN ('read', 'search') THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.total_tokens ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.cost_with_cache_usd ELSE 0.0 END), 0.0)
        FROM usage_tool_call tc
        LEFT JOIN usage_event e ON e.event_key = tc.event_key
        {low_where}
        """,
        low_params,
    )

    edit_where, edit_params = where_with(
        "tc", "occurred_at", bounds, ("tc.tool_kind = 'edit'",)
    )
    profile(
        conn,
        scope,
        "optimize_low_read_edit_candidate",
        f"""
        WITH call_counts AS (
            SELECT COALESCE(SUM(CASE WHEN tc.tool_kind IN ('read', 'search') THEN 1 ELSE 0 END), 0) AS read_calls,
                   COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN 1 ELSE 0 END), 0) AS edit_calls
            FROM usage_tool_call tc {low_where}
        ),
        edit_cost AS (
            SELECT COALESCE(SUM(e.total_tokens), 0) AS edit_tokens,
                   COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS edit_cost
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {edit_where}
        )
        SELECT read_calls, edit_calls, edit_tokens, edit_cost
        FROM call_counts CROSS JOIN edit_cost
        """,
        low_params + edit_params,
    )

    duplicate_where, duplicate_params = where_with(
        "tc",
        "occurred_at",
        bounds,
        ("tc.tool_kind IN ('read', 'search')", "tc.session_id IS NOT NULL"),
    )
    profile(
        conn,
        scope,
        "optimize_duplicate_reads",
        f"""
        SELECT tc.session_id,
               COALESCE(tc.input_fingerprint, tc.safe_preview, tc.tool_name) AS target,
               COUNT(*) AS calls, COALESCE(SUM(e.total_tokens), 0) AS tokens,
               COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
        FROM usage_tool_call tc
        LEFT JOIN usage_event e ON e.event_key = tc.event_key
        {duplicate_where}
        GROUP BY tc.session_id, target
        HAVING calls > 1
        ORDER BY calls DESC, tokens DESC
        LIMIT 1
        """,
        duplicate_params,
    )

    junk_where, junk_params = where_with(
        "tc",
        "occurred_at",
        bounds,
        (
            "tc.tool_kind IN ('read', 'search')",
            "(LOWER(COALESCE(tc.safe_preview, '')) LIKE '%node_modules%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/target/%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\\target\\%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/dist/%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\\dist\\%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/build/%' "
            "OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\\build\\%')",
        ),
    )
    profile(
        conn,
        scope,
        "optimize_junk_reads",
        f"""
        SELECT COUNT(*), COALESCE(SUM(e.total_tokens), 0),
               COALESCE(SUM(e.cost_with_cache_usd), 0.0),
               MAX(COALESCE(tc.safe_preview, tc.tool_name))
        FROM usage_tool_call tc
        LEFT JOIN usage_event e ON e.event_key = tc.event_key
        {junk_where}
        """,
        junk_params,
    )

    profile(
        conn,
        scope,
        "optimize_session_outlier",
        f"""
        SELECT t.session_id, COUNT(*), COALESCE(SUM(t.total_tokens), 0) AS tokens,
               COALESCE(SUM(e.cost_with_cache_usd), 0.0)
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        {turn_where}
        GROUP BY t.session_id
        HAVING t.session_id IS NOT NULL
        ORDER BY tokens DESC
        LIMIT 1
        """,
        turn_params,
    )

    profile(
        conn,
        scope,
        "optimize_session_outlier_candidate",
        f"""
        WITH top_session AS MATERIALIZED (
            SELECT t.session_id, COUNT(*) AS turns,
                   COALESCE(SUM(t.total_tokens), 0) AS tokens
            FROM usage_turn t
            {turn_where}{' AND' if turn_where else ' WHERE'} t.session_id IS NOT NULL
            GROUP BY t.session_id
            ORDER BY tokens DESC
            LIMIT 1
        )
        SELECT top.session_id, top.turns, top.tokens,
               COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
        FROM top_session top
        LEFT JOIN usage_turn t ON t.session_id = top.session_id
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        GROUP BY top.session_id, top.turns, top.tokens
        """,
        turn_params,
    )

    profile(
        conn,
        scope,
        "compare_candidates",
        f"""
        SELECT b.model, COALESCE(SUM(b.event_count), 0),
               COALESCE(SUM(b.total_tokens), 0),
               COALESCE(SUM(b.cost_with_cache_usd), 0.0)
        FROM usage_bucket_30m b
        {where('b', 'hour_start', bounds)[0]}
        GROUP BY b.model
        ORDER BY 4 DESC, 3 DESC, 2 DESC, b.model ASC
        LIMIT 25
        """,
        where("b", "hour_start", bounds)[1],
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True, type=Path)
    parser.add_argument("--scope", choices=("all", "1d", "both"), default="both")
    args = parser.parse_args()

    uri = f"{args.db.resolve().as_uri()}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    conn.execute("PRAGMA query_only = ON")
    try:
        counts = {
            table: conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            for table in ("usage_event", "usage_bucket_30m", "usage_turn", "usage_tool_call")
        }
        indexes = {
            table: [row[1] for row in conn.execute(f"PRAGMA index_list({table})")]
            for table in ("usage_event", "usage_bucket_30m", "usage_turn", "usage_tool_call")
        }
        print(json.dumps({"counts": counts, "indexes": indexes}, sort_keys=True))
        if args.scope in ("all", "both"):
            profile_scope(conn, "all", None)
        if args.scope in ("1d", "both"):
            profile_scope(conn, "1d", one_day_bounds())
    finally:
        conn.close()


if __name__ == "__main__":
    main()
