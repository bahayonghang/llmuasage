#!/usr/bin/env python3
"""Verify the stable GitHub required-check contract for this repo.

Branch protection on `main` must require the `CI gate` check name. That name is
the display name of the `ci-gate` job. The job must depend on every other job
in `.github/workflows/ci.yml` and must use `if: always()` so the check still
reports when a dependency fails.

GitHub matches required checks to the job `name:` field, not the job id.
Renaming leaf jobs is safe. Renaming `CI gate` without updating protection
recreates a check that never reports.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional


WORKFLOW_PATH = Path(".github/workflows/ci.yml")
GATE_JOB_ID = "ci-gate"
GATE_CHECK_NAME = "CI gate"
PROTECTED_BRANCH = "main"


class Job:
    def __init__(self, job_id: str) -> None:
        self.id = job_id
        self.name: Optional[str] = None
        self.needs: List[str] = []
        self.if_expr: Optional[str] = None


def _unquote(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        return value[1:-1]
    return value


def parse_jobs(text: str) -> Dict[str, Job]:
    jobs: Dict[str, Job] = {}
    in_jobs = False
    current: Optional[Job] = None
    in_needs = False

    for raw in text.splitlines():
        if not in_jobs:
            if raw.rstrip() == "jobs:":
                in_jobs = True
            continue

        if raw.strip() == "" or raw.lstrip().startswith("#"):
            continue

        indent = len(raw) - len(raw.lstrip(" "))
        stripped = raw.strip()

        if indent == 0:
            break

        if indent == 2 and stripped.endswith(":") and not stripped.startswith("-"):
            job_id = stripped[:-1]
            current = Job(job_id)
            jobs[job_id] = current
            in_needs = False
            continue

        if current is None or indent < 4:
            in_needs = False
            continue

        if in_needs:
            if indent == 6 and stripped.startswith("- "):
                current.needs.append(_unquote(stripped[2:]))
                continue
            in_needs = False

        if indent == 4 and stripped.startswith("name:"):
            current.name = _unquote(stripped[len("name:") :])
        elif indent == 4 and stripped.startswith("if:"):
            current.if_expr = _unquote(stripped[len("if:") :])
        elif indent == 4 and stripped.startswith("needs:"):
            rest = stripped[len("needs:") :].strip()
            if rest.startswith("[") and rest.endswith("]"):
                inner = rest[1:-1].strip()
                if inner:
                    current.needs = [_unquote(part) for part in inner.split(",")]
            else:
                in_needs = True

    return jobs


def _always_if(expr: Optional[str]) -> bool:
    if expr is None:
        return False
    compact = expr.replace(" ", "")
    return compact in {"always()", "${{always()}}"}


def verify_workflow(text: str) -> List[str]:
    jobs = parse_jobs(text)
    errors: List[str] = []
    gate = jobs.get(GATE_JOB_ID)
    if gate is None:
        errors.append(f"missing job `{GATE_JOB_ID}` in {WORKFLOW_PATH.as_posix()}")
        return errors

    if gate.name != GATE_CHECK_NAME:
        errors.append(
            f"job `{GATE_JOB_ID}` name is {gate.name!r}; required check name is {GATE_CHECK_NAME!r}"
        )
    if not _always_if(gate.if_expr):
        errors.append(
            f"job `{GATE_JOB_ID}` must set `if: always()` so a failed dependency still reports {GATE_CHECK_NAME!r}"
        )

    expected = sorted(job_id for job_id in jobs if job_id != GATE_JOB_ID)
    actual = sorted(gate.needs)
    if actual != expected:
        errors.append(
            f"job `{GATE_JOB_ID}` needs {actual} but must list every other job {expected}"
        )
    return errors


def verify_needs_json(payload: str) -> List[str]:
    data = json.loads(payload)
    errors: List[str] = []
    if not isinstance(data, dict):
        return ["--needs-json must be a JSON object of GitHub `needs` results"]
    for job_id, entry in data.items():
        result = entry.get("result") if isinstance(entry, dict) else None
        if result != "success":
            errors.append(f"required job `{job_id}` result is {result!r}")
    return errors


def github_required_check_names() -> List[str]:
    repo = subprocess.check_output(
        ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
        text=True,
    ).strip()
    raw = subprocess.check_output(
        [
            "gh",
            "api",
            f"repos/{repo}/branches/{PROTECTED_BRANCH}/protection/required_status_checks",
        ],
        text=True,
    )
    data = json.loads(raw)
    names: List[str] = []
    for item in data.get("checks") or []:
        context = item.get("context")
        if isinstance(context, str) and context not in names:
            names.append(context)
    for context in data.get("contexts") or []:
        if isinstance(context, str) and context not in names:
            names.append(context)
    return names


def verify_github_protection(workflow_text: str) -> List[str]:
    errors: List[str] = []
    job_names = {job.name for job in parse_jobs(workflow_text).values() if job.name}
    required = github_required_check_names()
    if required != [GATE_CHECK_NAME]:
        errors.append(
            f"{PROTECTED_BRANCH} required checks are {required}; expected [{GATE_CHECK_NAME!r}]"
        )
    missing = [name for name in required if name not in job_names]
    if missing:
        errors.append(
            f"required checks that no workflow job reports: {missing}"
        )
    return errors


def _self_test() -> None:
    good = """
jobs:
  rust:
    name: Rust (ubuntu-latest)
  ci-gate:
    name: CI gate
    if: always()
    needs:
      - rust
"""
    assert verify_workflow(good) == []

    missing_gate = """
jobs:
  rust:
    name: Rust (ubuntu-latest)
"""
    assert any("missing job" in err for err in verify_workflow(missing_gate))

    stale_name = """
jobs:
  rust:
    name: Rust (ubuntu-latest)
  ci-gate:
    name: Rust and docs
    if: always()
    needs:
      - rust
"""
    assert any("required check name" in err for err in verify_workflow(stale_name))

    incomplete_needs = """
jobs:
  rust:
    name: Rust (ubuntu-latest)
  docs-and-js:
    name: Docs and dashboard JS
  ci-gate:
    name: CI gate
    if: always()
    needs:
      - rust
"""
    assert any("must list every other job" in err for err in verify_workflow(incomplete_needs))

    missing_always = """
jobs:
  rust:
    name: Rust (ubuntu-latest)
  ci-gate:
    name: CI gate
    needs: [rust]
"""
    assert any("if: always()" in err for err in verify_workflow(missing_always))

    needs_errors = verify_needs_json(
        '{"rust":{"result":"success"},"docs-and-js":{"result":"failure"}}'
    )
    assert needs_errors == ["required job `docs-and-js` result is 'failure'"]
    print("self-test passed", flush=True)


def _report(errors: List[str]) -> int:
    if errors:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 1
    print("CI gate contract ok", flush=True)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run parser fixtures and exit",
    )
    parser.add_argument(
        "--needs-json",
        help="GitHub Actions `toJson(needs)` payload from the gate job",
    )
    parser.add_argument(
        "--github-protection",
        action="store_true",
        help=f"compare {PROTECTED_BRANCH} required checks to {GATE_CHECK_NAME!r}",
    )
    args = parser.parse_args()

    if args.self_test:
        _self_test()
        return 0

    text = WORKFLOW_PATH.read_text(encoding="utf-8")
    errors = verify_workflow(text)
    if args.needs_json:
        errors.extend(verify_needs_json(args.needs_json))
    if args.github_protection:
        errors.extend(verify_github_protection(text))
    return _report(errors)


if __name__ == "__main__":
    sys.exit(main())
