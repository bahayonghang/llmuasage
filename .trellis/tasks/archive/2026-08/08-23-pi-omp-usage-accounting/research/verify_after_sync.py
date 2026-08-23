"""Read-only checks against the live llmusage DB after the omp split sync.

Compares usage_event/omp rows to the identity baseline and the latest
scan_pi_source.py-style expectations. Does not mutate the database.
"""

from __future__ import annotations

import csv
import json
import sqlite3
from pathlib import Path

BASE = Path.home() / ".llmusage" / "baselines" / "08-23-pi-omp-usage-accounting"
DB = Path.home() / ".llmusage" / "llmusage.db"
IDENTITY = BASE / "baseline_pi_identity.csv"


def main() -> None:
    con = sqlite3.connect(f"file:{DB.as_posix()}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    cur = con.cursor()

    baseline = set()
    with IDENTITY.open(encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        for row in reader:
            baseline.add(
                (
                    row["source_path_hash"],
                    row["event_at"],
                    row["model"],
                    str(row["total_tokens"]),
                )
            )

    omp_rows = cur.execute(
        """
        SELECT source_path_hash, event_at, model, total_tokens,
               provider_label, project_hash, cost_with_cache_usd,
               pricing_status, event_key, session_label
        FROM usage_event
        WHERE source = 'omp'
        """
    ).fetchall()
    omp_ids = {
        (
            row["source_path_hash"],
            row["event_at"],
            row["model"],
            str(row["total_tokens"]),
        )
        for row in omp_rows
    }
    missing = sorted(baseline - omp_ids)
    extra = len(omp_ids - baseline)

    pi_count = cur.execute(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'pi'"
    ).fetchone()[0]
    paired = cur.execute(
        """
        SELECT COUNT(*)
        FROM usage_event a
        JOIN usage_event b
          ON a.source_path_hash = b.source_path_hash
         AND a.event_at = b.event_at
        WHERE a.source = 'pi' AND b.source = 'omp'
        """
    ).fetchone()[0]

    empty_provider = sum(1 for row in omp_rows if not (row["provider_label"] or ""))
    empty_project = sum(1 for row in omp_rows if not (row["project_hash"] or ""))
    providers = sorted(
        {
            row["provider_label"]
            for row in omp_rows
            if row["provider_label"]
        }
    )
    cost = cur.execute(
        """
        SELECT COALESCE(SUM(cost_with_cache_usd), 0)
        FROM usage_event
        WHERE source = 'omp'
        """
    ).fetchone()[0]
    bucket_cost = cur.execute(
        """
        SELECT COALESCE(SUM(cost_with_cache_usd), 0)
        FROM usage_bucket_30m
        WHERE source = 'omp'
        """
    ).fetchone()[0]
    status_rows = cur.execute(
        """
        SELECT pricing_status, COUNT(*) AS n
        FROM usage_event
        WHERE source = 'omp'
        GROUP BY 1
        """
    ).fetchall()
    tool_calls = cur.execute(
        "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'omp'"
    ).fetchone()[0]
    turns = cur.execute(
        "SELECT COUNT(*) FROM usage_turn WHERE source = 'omp'"
    ).fetchone()[0]
    retries = cur.execute(
        """
        SELECT COUNT(*) FROM usage_turn
        WHERE source = 'omp' AND retries >= 1
        """
    ).fetchone()[0]
    empty_turn_project = cur.execute(
        """
        SELECT COUNT(*) FROM usage_turn
        WHERE source = 'omp'
          AND (project_hash IS NULL OR project_hash = '')
        """
    ).fetchone()[0]
    long_preview = cur.execute(
        """
        SELECT COUNT(*) FROM usage_tool_call
        WHERE source = 'omp' AND LENGTH(COALESCE(safe_preview, '')) > 120
        """
    ).fetchone()[0]
    path_leak_events = cur.execute(
        """
        SELECT COUNT(*) FROM usage_event
        WHERE COALESCE(session_label, '') LIKE '%agent/sessions%'
        """
    ).fetchone()[0]
    path_leak_tools = cur.execute(
        """
        SELECT COUNT(*) FROM usage_tool_call
        WHERE COALESCE(safe_preview, '') LIKE '%agent/sessions%'
        """
    ).fetchone()[0]

    out = {
        "omp_events": len(omp_rows),
        "pi_events": pi_count,
        "baseline_rows": len(baseline),
        "baseline_missing": len(missing),
        "omp_extra_vs_baseline": extra,
        "paired_pi_omp": paired,
        "empty_provider": empty_provider,
        "empty_project": empty_project,
        "providers": providers,
        "event_cost": cost,
        "bucket_cost": bucket_cost,
        "cost_delta": abs(float(cost) - float(bucket_cost)),
        "pricing_status": {row["pricing_status"]: row["n"] for row in status_rows},
        "tool_calls": tool_calls,
        "turns": turns,
        "retry_turns": retries,
        "empty_turn_project": empty_turn_project,
        "long_preview": long_preview,
        "path_leak_events": path_leak_events,
        "path_leak_tools": path_leak_tools,
        "missing_sample": missing[:5],
    }
    print(json.dumps(out, indent=2))
    con.close()


if __name__ == "__main__":
    main()
