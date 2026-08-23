import glob
import json
import os
import sqlite3
from pathlib import Path

root = os.path.expanduser("~/.omp/agent/sessions")
print("=== retryRecovery records ===")
for path in glob.glob(os.path.join(root, "**", "*.jsonl"), recursive=True):
    for line in open(path, encoding="utf-8", errors="replace"):
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue
        message = record.get("message")
        if not isinstance(message, dict) or not message.get("retryRecovery"):
            continue
        usage = message.get("usage") if isinstance(message.get("usage"), dict) else {}
        tokens = sum(
            usage.get(key, 0) or 0
            for key in ("input", "output", "cacheRead", "cacheWrite", "totalTokens")
        )
        print(
            json.dumps(
                {
                    "file": os.path.relpath(path, root),
                    "type": record.get("type"),
                    "role": message.get("role"),
                    "tokens": tokens,
                    "retryRecovery": message.get("retryRecovery"),
                    "has_toolCall": any(
                        isinstance(block, dict) and block.get("type") == "toolCall"
                        for block in (message.get("content") or [])
                        if isinstance(message.get("content"), list)
                    ),
                },
                ensure_ascii=False,
            )
        )

con = sqlite3.connect(
    f"file:{(Path.home() / '.llmusage' / 'llmusage.db').as_posix()}?mode=ro",
    uri=True,
)
print("\n=== db retry/preview counts ===")
print(
    "retries>=1",
    con.execute(
        "SELECT COUNT(*) FROM usage_turn WHERE source='omp' AND retries >= 1"
    ).fetchone()[0],
)
print(
    "retries any",
    con.execute(
        "SELECT retries, COUNT(*) FROM usage_turn WHERE source='omp' GROUP BY 1"
    ).fetchall(),
)
print(
    "preview agent/sessions",
    con.execute(
        """
        SELECT COUNT(*) FROM usage_tool_call
        WHERE source='omp' AND COALESCE(safe_preview,'') LIKE '%agent/sessions%'
        """
    ).fetchone()[0],
)
print(
    "preview sessions",
    con.execute(
        """
        SELECT COUNT(*) FROM usage_tool_call
        WHERE source='omp' AND COALESCE(safe_preview,'') LIKE '%sessions%'
        """
    ).fetchone()[0],
)
print(
    "turn columns",
    [row[1] for row in con.execute("PRAGMA table_info(usage_turn)")],
)
