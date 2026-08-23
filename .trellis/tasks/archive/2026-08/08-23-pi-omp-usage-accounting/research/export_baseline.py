import csv
import json
from pathlib import Path
import sqlite3

base = Path.home() / ".llmusage" / "baselines" / "08-23-pi-omp-usage-accounting"
base.mkdir(parents=True, exist_ok=True)
db = Path.home() / ".llmusage" / "llmusage.db"
con = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True)
con.row_factory = sqlite3.Row
cur = con.cursor()
agg = cur.execute(
    """
    SELECT COUNT(*) AS n,
           COALESCE(SUM(total_tokens), 0) AS tokens,
           COALESCE(SUM(cost_with_cache_usd), 0) AS cost
    FROM usage_event
    WHERE source = 'pi'
    """
).fetchone()
rows = cur.execute(
    """
    SELECT source_path_hash, event_at, model, total_tokens
    FROM usage_event
    WHERE source = 'pi'
    ORDER BY 1, 2, 3, 4
    """
).fetchall()
csv_path = base / "baseline_pi_identity.csv"
with csv_path.open("w", newline="", encoding="utf-8") as handle:
    writer = csv.writer(handle)
    writer.writerow(["source_path_hash", "event_at", "model", "total_tokens"])
    for row in rows:
        writer.writerow(
            [row["source_path_hash"], row["event_at"], row["model"], row["total_tokens"]]
        )

extra = {}
queries = {
    "pi_events": "SELECT COUNT(*) FROM usage_event WHERE source = 'pi'",
    "omp_events": "SELECT COUNT(*) FROM usage_event WHERE source = 'omp'",
    "pi_turns": "SELECT COUNT(*) FROM usage_turn WHERE source = 'pi'",
    "pi_tool_calls": "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'pi'",
    "pi_source_files": "SELECT COUNT(*) FROM source_file WHERE source = 'pi'",
    "omp_source_files": "SELECT COUNT(*) FROM source_file WHERE source = 'omp'",
}
for key, sql in queries.items():
    extra[key] = cur.execute(sql).fetchone()[0]
prov = cur.execute(
    """
    SELECT
      SUM(CASE WHEN COALESCE(provider_label, '') = '' THEN 1 ELSE 0 END) AS empty_provider,
      SUM(CASE WHEN COALESCE(project_hash, '') = '' THEN 1 ELSE 0 END) AS empty_project,
      COUNT(DISTINCT provider_label) AS distinct_providers
    FROM usage_event
    WHERE source = 'pi'
    """
).fetchone()
out = {
    "pi_event_count": agg["n"],
    "pi_total_tokens": agg["tokens"],
    "pi_cost_with_cache_usd": agg["cost"],
    "identity_rows": len(rows),
    "empty_provider": prov["empty_provider"],
    "empty_project": prov["empty_project"],
    "distinct_providers": prov["distinct_providers"],
    **extra,
}
(base / "aggregate_baseline.json").write_text(
    json.dumps(out, indent=2) + "\n", encoding="utf-8"
)
print(json.dumps(out, indent=2))
print(f"csv {csv_path} bytes {csv_path.stat().st_size}")
con.close()
