"""Break scan usage/retry counts into parser-eligible vs skipped."""

import glob
import json
import os
from collections import Counter

root = os.path.expanduser("~/.omp/agent/sessions")
usage_roles = Counter()
usage_types = Counter()
retry_with_usage_assistant = 0
retry_other = 0
zero_token_assistant = 0
parser_eligible = 0

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
        if not isinstance(message, dict):
            continue
        usage = message.get("usage")
        has_usage = isinstance(usage, dict)
        role = message.get("role")
        rec_type = record.get("type")
        if message.get("retryRecovery"):
            if has_usage and role == "assistant" and rec_type == "message":
                retry_with_usage_assistant += 1
            else:
                retry_other += 1
        if not has_usage:
            continue
        usage_roles[str(role)] += 1
        usage_types[str(rec_type)] += 1
        if role == "assistant" and rec_type == "message":
            parser_eligible += 1
            tokens = sum(
                usage.get(key, 0) or 0
                for key in ("input", "output", "cacheRead", "cacheWrite", "totalTokens")
            )
            if tokens == 0:
                zero_token_assistant += 1

print("usage_roles", dict(usage_roles))
print("usage_types", dict(usage_types))
print("parser_eligible", parser_eligible)
print("zero_token_assistant", zero_token_assistant)
print("retry_with_usage_assistant", retry_with_usage_assistant)
print("retry_other", retry_other)
