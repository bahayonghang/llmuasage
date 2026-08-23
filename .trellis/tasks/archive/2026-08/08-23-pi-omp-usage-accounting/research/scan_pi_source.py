#!/usr/bin/env python
"""Read-only scan of the local Pi / Oh My Pi session roots.

Prints the counters that the task acceptance criteria reference, so every
verification uses one measurement definition instead of an ad-hoc glob.

Usage:
    python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py

Definition notes:
- Session files are enumerated RECURSIVELY (`<root>/**/*.jsonl`). Oh My Pi
  writes named sub-sessions one level deeper than the project directory, and
  `llmusage` discovery uses WalkDir, so a depth-1 glob undercounts.
- A "usage record" is a record whose `message.usage` is an object. That is the
  same predicate `src/parsers/pi.rs` uses to emit one event.
- Nothing is written and no file content is printed.
"""

import collections
import glob
import json
import os

ROOTS = {
    "pi": os.path.expanduser("~/.pi/agent/sessions"),
    "omp": os.path.expanduser("~/.omp/agent/sessions"),
}


def scan(root):
    files = sorted(glob.glob(os.path.join(root, "**", "*.jsonl"), recursive=True))
    stats = {
        "files": len(files),
        "files_nested": 0,
        "usage_records": 0,
        "cost_present": 0,
        "cost_positive": 0,
        "cost_total": 0.0,
        "reasoning_records": 0,
        "reasoning_gt_output": 0,
        "total_matches_channel_sum": 0,
        "total_mismatch": 0,
        "tool_call_blocks": 0,
        "tool_args_object": 0,
        "tool_args_string": 0,
        "tool_args_other": 0,
        "retry_records": 0,
        "session_headers": 0,
        "session_headers_with_cwd": 0,
        "child_usage_records": 0,
    }
    providers = collections.Counter()
    models = collections.Counter()
    tools = collections.Counter()

    for path in files:
        relative = os.path.relpath(path, root)
        if relative.count(os.sep) > 1:
            stats["files_nested"] += 1
        for line in open(path, encoding="utf-8", errors="replace"):
            line = line.strip()
            if not line:
                continue
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if record.get("type") == "session":
                stats["session_headers"] += 1
                if record.get("cwd"):
                    stats["session_headers_with_cwd"] += 1
            if "childUsage" in record or "aggregateUsage" in record:
                stats["child_usage_records"] += 1
            message = record.get("message")
            if not isinstance(message, dict):
                continue
            content = message.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict) or block.get("type") != "toolCall":
                        continue
                    stats["tool_call_blocks"] += 1
                    tools[block.get("name")] += 1
                    arguments = block.get("arguments")
                    if isinstance(arguments, dict):
                        stats["tool_args_object"] += 1
                    elif isinstance(arguments, str):
                        stats["tool_args_string"] += 1
                    else:
                        stats["tool_args_other"] += 1
            if message.get("retryRecovery"):
                stats["retry_records"] += 1
            usage = message.get("usage")
            if not isinstance(usage, dict):
                continue
            stats["usage_records"] += 1
            providers[message.get("provider")] += 1
            models[message.get("model")] += 1
            cost = usage.get("cost")
            if isinstance(cost, dict):
                stats["cost_present"] += 1
                total = cost.get("total") or 0
                if total > 0:
                    stats["cost_positive"] += 1
                    stats["cost_total"] += total
            channels = sum(
                usage.get(key, 0) or 0
                for key in ("input", "output", "cacheRead", "cacheWrite")
            )
            if (usage.get("totalTokens") or 0) == channels:
                stats["total_matches_channel_sum"] += 1
            else:
                stats["total_mismatch"] += 1
            reasoning = usage.get("reasoningTokens")
            if reasoning is not None:
                stats["reasoning_records"] += 1
                if reasoning > (usage.get("output") or 0):
                    stats["reasoning_gt_output"] += 1
    return stats, providers, models, tools


def main():
    for name, root in ROOTS.items():
        print(f"=== {name}: {root} ===")
        if not os.path.isdir(root):
            print("  root absent")
            continue
        stats, providers, models, tools = scan(root)
        stats["cost_total"] = round(stats["cost_total"], 6)
        for key, value in stats.items():
            print(f"  {key}: {value}")
        print(f"  providers: {dict(providers)}")
        print(f"  models: {dict(models)}")
        print(f"  tools: {dict(tools.most_common())}")


if __name__ == "__main__":
    main()
