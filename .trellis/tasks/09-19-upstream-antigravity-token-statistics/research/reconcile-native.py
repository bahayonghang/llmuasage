"""Reconcile an isolated llmusage import with the independently collected oracle.

Reads only local metadata/statistics. Prints aggregate counts and hashed mismatch
identifiers, never paths, request IDs, prompts, responses, or native blobs.
Run after importing CLI and IDE into a task-owned --home under target/.
"""
import argparse
import hashlib
import json
import sqlite3
from collections import Counter
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", type=Path, required=True)
    args = parser.parse_args()
    oracle = json.loads(Path(__file__).with_name("sanitized-native-usage.json").read_text(encoding="utf-8"))["full_native_oracle"]
    expected = {(row["family"], row["cascade_id_hash"]): row for row in oracle["databases"]}
    conn = sqlite3.connect((args.home / "llmusage.db").resolve().as_uri() + "?mode=ro", uri=True)
    conn.execute("PRAGMA query_only=ON")
    counters = Counter()
    mismatches = []
    checked = Counter()
    query = """SELECT COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0),
        COALESCE(SUM(reasoning_output_tokens),0), COALESCE(SUM(cache_read_tokens),0),
        COALESCE(SUM(cache_creation_tokens),0), COALESCE(SUM(total_tokens),0)
        FROM usage_event WHERE host_id='local' AND source=? AND session_id=?"""
    for source, raw_path in conn.execute("SELECT source,file_path FROM source_cursor WHERE host_id='local' AND source IN ('antigravity','antigravity_ide')"):
        path = Path(raw_path)
        family = "antigravity-cli" if source == "antigravity" else "antigravity-ide"
        identity = hashlib.sha256(path.stem.encode()).hexdigest()
        baseline = expected.get((family, identity))
        counters[family + ":imported_databases"] += 1
        if baseline is None:
            counters[family + ":new_since_oracle"] += 1
            continue
        wal = Path(str(path) + "-wal")
        wal_mtime = wal.stat().st_mtime_ns if wal.exists() else None
        if path.stat().st_mtime_ns != baseline["db_mtime_ns"] or wal_mtime != baseline["wal_mtime_ns"]:
            counters[family + ":changed_since_oracle"] += 1
            continue
        session = hashlib.sha256(raw_path.encode()).hexdigest()
        actual = tuple(conn.execute(query, (source, session)).fetchone())
        totals = baseline["totals_unambiguous_events"]
        wanted = (baseline["counts"].get("unique_events", 0), *(totals.get(key, 0) for key in
            ("input", "output_visible", "reasoning", "cache_read", "cache_write", "total_tokens")))
        if actual != wanted:
            mismatches.append({"source": source, "database_hash": identity, "expected": wanted, "actual": actual})
        counters[family + ":checked_databases"] += 1
        checked[family + ":checked_events"] += actual[0]
        checked[family + ":checked_tokens"] += actual[-1]
    totals = [dict(zip(("source", "events", "input", "visible", "reasoning", "cache_read", "cache_write", "total_tokens"), row))
        for row in conn.execute("SELECT source,COUNT(*),SUM(input_tokens),SUM(output_tokens),SUM(reasoning_output_tokens),SUM(cache_read_tokens),SUM(cache_creation_tokens),SUM(total_tokens) FROM usage_event WHERE host_id='local' GROUP BY source")]
    print(json.dumps({"oracle_started_utc": oracle["started_utc"], "coverage": dict(counters),
        "checked": dict(checked), "current_import": totals, "mismatches": mismatches}, indent=2))
    if not counters or mismatches:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
