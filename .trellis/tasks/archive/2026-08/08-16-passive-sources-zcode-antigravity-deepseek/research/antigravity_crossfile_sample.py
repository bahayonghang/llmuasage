"""Cross-file field occurrence sampling for antigravity-cli conversations/*.db.

R1 evidence gate (2026-08-16): the 109-row deep-dive covered one file; this
script sweeps every conversation DB and reports per-field occurrence rates for
the wire layout documented in antigravity-artifacts.md section 2.1.

Read-only; no blob bytes are printed, only numeric/string scalars.
"""

import json
import sqlite3
import sys
from pathlib import Path

CONV_ROOT = Path.home() / ".gemini" / "antigravity-cli" / "conversations"


def read_varint(buf: bytes, pos: int):
    result = 0
    shift = 0
    while True:
        if pos >= len(buf):
            raise ValueError("varint truncated")
        byte = buf[pos]
        pos += 1
        result |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return result, pos
        shift += 7


def iter_fields(buf: bytes):
    """Yield (field_no, wire_type, value) for each wire field; skip unknown."""
    pos = 0
    while pos < len(buf):
        key, pos = read_varint(buf, pos)
        field_no, wire_type = key >> 3, key & 7
        if wire_type == 0:
            value, pos = read_varint(buf, pos)
            yield field_no, wire_type, value
        elif wire_type == 2:
            length, pos = read_varint(buf, pos)
            if pos + length > len(buf):
                raise ValueError("len-delim truncated")
            yield field_no, wire_type, buf[pos:pos + length]
            pos += length
        elif wire_type == 1:
            if pos + 8 > len(buf):
                raise ValueError("64-bit truncated")
            yield field_no, wire_type, buf[pos:pos + 8]
            pos += 8
        elif wire_type == 5:
            if pos + 4 > len(buf):
                raise ValueError("32-bit truncated")
            yield field_no, wire_type, buf[pos:pos + 4]
            pos += 4
        else:
            raise ValueError(f"unsupported wire type {wire_type}")


def message_fields(buf: bytes):
    return {field_no: value for field_no, _wt, value in iter_fields(buf)}


def decode_gen_metadata(blob: bytes):
    """Decode the documented shape: top #1 chatModel -> {#4 usage, #19, #21, #9.#4}."""
    top = message_fields(blob)
    chat_raw = top.get(1)
    if not isinstance(chat_raw, bytes):
        return None
    chat = message_fields(chat_raw)
    usage_raw = chat.get(4)
    usage = message_fields(usage_raw) if isinstance(usage_raw, bytes) else {}
    return {
        "top4_len": len(top[4]) if isinstance(top.get(4), bytes) else None,
        "usage_present": isinstance(usage_raw, bytes),
        "usage_fields": sorted(k for k in usage if k in (1, 2, 3, 5, 6, 8, 9, 10, 11)),
        "system_prompt": usage.get(1),
        "input": usage.get(2),
        "checksum": usage.get(3),
        "cache_read": usage.get(5),
        "output": usage.get(9),
        "thinking": usage.get(10),
        "response_id": usage.get(11),
        "model": chat.get(19),
        "label": chat.get(21),
    }


def main():
    dbs = sorted(CONV_ROOT.glob("*.db"))
    print(f"databases: {len(dbs)}")
    totals = {"rows": 0, "decoded": 0, "usage": 0, "checksum_ok": 0, "checksum_bad": 0,
              "cache_read": 0, "model19": 0, "label21": 0, "usage8": 0, "response_id": 0,
              "all_zero": 0, "top4": 0}
    per_file = []
    for db in dbs:
        uri = f"file:{db.as_posix()}?mode=ro"
        try:
            conn = sqlite3.connect(uri, uri=True)
            rows = conn.execute("SELECT data FROM gen_metadata ORDER BY idx").fetchall()
        except sqlite3.Error as exc:
            print(f"  !! {db.name}: {exc}")
            continue
        finally:
            try:
                conn.close()
            except Exception:
                pass
        file_stats = {"db": db.name, "rows": len(rows), "decoded": 0, "usage": 0,
                      "checksum_bad": 0, "all_zero": 0, "models": set(), "labels": set()}
        for (blob,) in rows:
            if not blob:
                continue
            try:
                decoded = decode_gen_metadata(bytes(blob))
            except ValueError:
                file_stats["checksum_bad"] += 1  # treat malformed blob as decode failure
                continue
            if decoded is None or not decoded["usage_present"]:
                continue
            file_stats["decoded"] += 1
            file_stats["usage"] += 1
            u = decoded
            if u["top4_len"] is not None:
                totals["top4"] += 1
            if u["checksum"] is not None and u["output"] is not None and u["thinking"] is not None:
                if u["checksum"] == u["output"] + u["thinking"]:
                    totals["checksum_ok"] += 1
                else:
                    totals["checksum_bad"] += 1
                    file_stats["checksum_bad"] += 1
            if u["cache_read"] is not None:
                totals["cache_read"] += 1
            if u["model"] is not None:
                totals["model19"] += 1
                file_stats["models"].add(u["model"].decode("utf-8", "replace"))
            if u["label"] is not None:
                totals["label21"] += 1
                file_stats["labels"].add(u["label"].decode("utf-8", "replace"))
            if 8 in u["usage_fields"]:
                totals["usage8"] += 1
            if u["response_id"] is not None:
                totals["response_id"] += 1
            channels = [u["system_prompt"], u["input"], u["cache_read"], u["output"], u["thinking"]]
            if all(not c for c in channels):
                totals["all_zero"] += 1
                file_stats["all_zero"] += 1
        totals["rows"] += file_stats["rows"]
        totals["decoded"] += file_stats["decoded"]
        totals["usage"] += file_stats["usage"]
        file_stats["models"] = sorted(file_stats["models"])
        file_stats["labels"] = sorted(file_stats["labels"])
        per_file.append(file_stats)

    rows = totals["rows"]
    usage = totals["usage"]
    print(json.dumps({
        "rows_total": rows,
        "usage_rows": usage,
        "top_level_4_present": totals["top4"],
        "checksum_ok": totals["checksum_ok"],
        "checksum_bad": totals["checksum_bad"],
        "usage_5_cache_read": totals["cache_read"],
        "chat_19_model": totals["model19"],
        "chat_21_label": totals["label21"],
        "usage_8": totals["usage8"],
        "usage_11_response_id": totals["response_id"],
        "all_zero_usage_rows": totals["all_zero"],
    }, indent=2))

    print("\nempty / minimal conversations (fewest rows):")
    for stats in sorted(per_file, key=lambda s: s["rows"])[:8]:
        print(f"  {stats['db']}: rows={stats['rows']} usage={stats['usage']} all_zero={stats['all_zero']} models={stats['models']}")

    print("\nlabel -> model pairs (across files):")
    pairs = {}
    for stats in per_file:
        for model in stats["models"]:
            pairs.setdefault(model, 0)
    # label/model mapping needs row-level pairing; report distinct models and labels
    print(f"  distinct models: {sorted({m for s in per_file for m in s['models']})}")
    print(f"  distinct labels: {sorted({l for s in per_file for l in s['labels']})[:20]}")

    print(f"\nfiles with zero usage rows: {sum(1 for s in per_file if s['usage'] == 0)}")
    print(f"files with checksum_bad>0: {sum(1 for s in per_file if s['checksum_bad'] > 0)}")


if __name__ == "__main__":
    sys.exit(main())
