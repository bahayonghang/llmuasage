import json
import sqlite3
from pathlib import Path

db = Path.home() / ".llmusage" / "llmusage.db"
con = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True)
con.row_factory = sqlite3.Row
cur = con.cursor()

print("=== tool previews containing agent/sessions ===")
rows = cur.execute(
    """
    SELECT tool_name, tool_kind, LENGTH(safe_preview) AS n, safe_preview
    FROM usage_tool_call
    WHERE source = 'omp' AND COALESCE(safe_preview, '') LIKE '%agent/sessions%'
    """
).fetchall()
for row in rows:
    print(f"{row['tool_name']}\t{row['tool_kind']}\t{row['n']}\t{row['safe_preview']}")

print("\n=== omp project labels ===")
for row in cur.execute(
    """
    SELECT project_label, COUNT(*) AS n, COUNT(DISTINCT project_hash) AS hashes
    FROM usage_event
    WHERE source = 'omp'
    GROUP BY 1
    ORDER BY n DESC
    """
):
    print(dict(row))

print("\n=== retry turns ===")
for row in cur.execute(
    """
    SELECT turn_key, retries, one_shot, model, occurred_at
    FROM usage_turn
    WHERE source = 'omp' AND retries >= 1
    """
):
    print(dict(row))

print("\n=== provider counts ===")
for row in cur.execute(
    """
    SELECT provider_label, COUNT(*) AS n, ROUND(SUM(cost_with_cache_usd), 6) AS cost
    FROM usage_event
    WHERE source = 'omp'
    GROUP BY 1
    ORDER BY n DESC
    """
):
    print(dict(row))
con.close()
