"""Read only the saved sync samples and whitelisted record metadata.

Never writes source data or SQLite. Never emits prompt, tool, or response text.
The caller may save stdout under this task's research directory.
"""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3

DB = Path("C:/Users/lyh/.llmusage/llmusage.db")
EXPECTED = "2026-09-26T08:33:53Z"
LIMIT = 64 * 1024 * 1024
SAFE_TYPES = {
    "event_msg", "response_item", "session_meta", "turn_context",
    "message", "token_count", "function_call", "function_call_output",
    "custom_tool_call", "custom_tool_call_output", "agent_message",
    "user_message", "reasoning", "turn_completed",
}


def digest(raw):
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def metadata(path):
    st = path.stat()
    return {"bytes": st.st_size, "mtime_ns": st.st_mtime_ns}


def safe_type(value):
    if value is None:
        return None
    return value if isinstance(value, str) and value in SAFE_TYPES else "other"


def object_or_empty(value):
    return value if isinstance(value, dict) else {}


def inspect_record(path, offset):
    result = {"offset": offset}
    with path.open("rb") as stream:
        if offset:
            stream.seek(offset - 1)
            result["starts_after_newline"] = stream.read(1) == b"\n"
        else:
            result["starts_after_newline"] = True
        stream.seek(offset)
        raw = stream.readline(LIMIT + 1)
    result["record_bytes_excluding_lf"] = len(raw.removesuffix(b"\n"))
    result["newline_terminated"] = raw.endswith(b"\n")
    result["over_4_mib"] = result["record_bytes_excluding_lf"] > 4 * 1024 * 1024
    if len(raw) > LIMIT:
        result["probe_limit_reached"] = True
        return result
    try:
        value = json.loads(raw)
    except (ValueError, UnicodeError) as error:
        result["json_valid"] = False
        result["error_class"] = type(error).__name__
        for field in ("pos", "lineno", "colno"):
            if hasattr(error, field):
                result[field] = getattr(error, field)
        return result
    result["json_valid"] = True
    value = object_or_empty(value)
    payload = object_or_empty(value.get("payload"))
    msg = object_or_empty(payload.get("msg"))
    result["outer_type"] = safe_type(value.get("type"))
    result["payload_type"] = safe_type(payload.get("type"))
    result["payload_msg_type"] = safe_type(msg.get("type"))
    update = object_or_empty(object_or_empty(value.get("params")).get("update"))
    if update:
        result["session_update"] = safe_type(update.get("sessionUpdate"))
        usage = object_or_empty(update.get("usage"))
        result["usage_object_present"] = isinstance(update.get("usage"), dict)
        result["usage_is_incomplete"] = usage.get("usageIsIncomplete") is True
        result["usage_numbers"] = {
            key: usage[key]
            for key in ("inputTokens", "outputTokens", "totalTokens",
                        "cachedReadTokens", "cacheCreationTokens", "reasoningTokens")
            if isinstance(usage.get(key), (int, float)) and not isinstance(usage.get(key), bool)
        }
    return result


def session_usage_counts(path):
    counts = {"turn_usage_records": 0, "usage_incomplete_records": 0,
              "input_below_cache_records": 0, "json_errors": 0}
    with path.open("rb") as stream:
        for raw in stream:
            try:
                value = object_or_empty(json.loads(raw))
            except (ValueError, UnicodeError):
                counts["json_errors"] += 1
                continue
            update = object_or_empty(object_or_empty(value.get("params")).get("update"))
            usage = object_or_empty(update.get("usage"))
            if update.get("sessionUpdate") != "turn_completed" or not usage:
                continue
            counts["turn_usage_records"] += 1
            counts["usage_incomplete_records"] += usage.get("usageIsIncomplete") is True
            channels = [usage.get(k, 0) for k in ("inputTokens", "cachedReadTokens", "cacheCreationTokens")]
            if all(isinstance(x, (int, float)) and not isinstance(x, bool) for x in channels):
                counts["input_below_cache_records"] += channels[0] < max(channels[1], 0) + max(channels[2], 0)
    return counts


wal = Path(str(DB) + "-wal")
if wal.exists():
    raise SystemExit("Refusing immutable probe because a WAL exists.")
before_db = metadata(DB)
connection = sqlite3.connect(DB.as_uri() + "?mode=ro&immutable=1", uri=True)
connection.execute("PRAGMA query_only=ON")
report = {"observed_at_utc": datetime.now(timezone.utc).isoformat(),
          "expected_sync_status_updated_at": EXPECTED, "sources": []}
for source in ("codex", "grok"):
    row = connection.execute("SELECT updated_at,parse_issues_json FROM source_sync_status WHERE host_id='local' AND source=?", (source,)).fetchone()
    if row is None or row[0] != EXPECTED:
        raise SystemExit("Saved diagnostics no longer match the reported run.")
    issues = json.loads(row[1])
    files = connection.execute("SELECT file_path FROM source_file WHERE host_id='local' AND source=?", (source,)).fetchall()
    cursors = {r[0]: {"bytes": r[1], "mtime_ns": r[2]} for r in connection.execute("SELECT file_path,file_size,file_mtime_ns FROM source_cursor WHERE host_id='local' AND source=?", (source,))}
    candidates = {}
    for (raw,) in files:
        if source == "codex":
            candidates[digest(raw)] = raw
        else:
            split = max(raw.rfind("/"), raw.rfind("\\"))
            parent = raw[:split]
            if raw[split + 1:] == "updates.jsonl":
                candidates[digest(parent)] = raw
    source_report = {"source": source, "status_updated_at": row[0],
                     "counts": {k: v for k, v in issues.items() if k != "samples"},
                     "sample_count": len(issues.get("samples", [])), "samples": []}
    grok_scanned = set()
    for sample in issues.get("samples", []):
        raw_path = candidates.get(sample["path_hash"])
        entry = {"kind": sample["kind"], "reason": sample.get("reason", ""),
                 "offset": sample["offset"], "path_mapping_found": raw_path is not None}
        if raw_path is not None:
            path = Path(raw_path)
            entry["basename"] = path.name
            entry["file_exists"] = path.is_file()
            if path.is_file():
                before = metadata(path)
                entry["current_file_matches_saved_cursor"] = before == cursors.get(raw_path)
                entry["record"] = inspect_record(path, sample["offset"])
                if source == "grok" and raw_path not in grok_scanned:
                    entry["current_session_counts"] = session_usage_counts(path)
                    grok_scanned.add(raw_path)
                entry["file_metadata_unchanged_during_probe"] = before == metadata(path)
        source_report["samples"].append(entry)
    report["sources"].append(source_report)
connection.close()
report["db_metadata_unchanged"] = before_db == metadata(DB)
report["wal_absent_after_probe"] = not wal.exists()
print(json.dumps(report, ensure_ascii=False, indent=2))
