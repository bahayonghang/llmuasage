#!/usr/bin/env python3
"""Privacy-safe Activity cold/warm HTTP and SQLite baseline profiler.

The script creates one SQLite online backup from the source database, then uses
only copies of that backup for server measurements. It persists timings,
support state, row counts, and query-plan operators; response rows and user
dimensions are never written.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.request
from dataclasses import dataclass
from datetime import datetime, time as wall_time, timedelta, timezone
from pathlib import Path
from typing import Any


ACTIVITY_METRIC = re.compile(
    r'section="?activity"?.*?semaphore_wait_ms=(\d+).*?query_ms=(\d+)'
    r'.*?cancelled=(true|false)'
)
ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")


def utc_text(value: datetime) -> str:
    return (
        value.astimezone(timezone.utc)
        .isoformat(timespec="seconds")
        .replace("+00:00", "Z")
    )


def one_day_bounds() -> tuple[str, str]:
    local_now = datetime.now().astimezone()
    local_zone = local_now.tzinfo
    assert local_zone is not None
    start = datetime.combine(
        local_now.date() - timedelta(days=1), wall_time.min, local_zone
    )
    end = datetime.combine(
        local_now.date() + timedelta(days=1), wall_time.min, local_zone
    )
    return utc_text(start), utc_text(end)


def sqlite_uri(path: Path) -> str:
    return f"{path.resolve().as_uri()}?mode=ro"


def online_backup(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    source_conn = sqlite3.connect(sqlite_uri(source), uri=True)
    source_conn.execute("PRAGMA query_only = ON")
    destination_conn = sqlite3.connect(destination)
    try:
        source_conn.backup(destination_conn)
    finally:
        destination_conn.close()
        source_conn.close()


def database_metadata(path: Path) -> dict[str, Any]:
    conn = sqlite3.connect(sqlite_uri(path), uri=True)
    conn.execute("PRAGMA query_only = ON")
    try:
        schema_version = int(
            conn.execute(
                "SELECT value FROM meta WHERE key = 'schema_version'"
            ).fetchone()[0]
        )
        counts = {
            table: int(conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0])
            for table in ("usage_event", "usage_turn")
        }
        quick_check = str(conn.execute("PRAGMA quick_check").fetchone()[0])
        indexes = {
            table: sorted(
                str(row[1]) for row in conn.execute(f"PRAGMA index_list({table})")
            )
            for table in ("usage_event", "usage_turn")
        }
    finally:
        conn.close()
    return {
        "schema_version": schema_version,
        "size_bytes": path.stat().st_size,
        "counts": counts,
        "quick_check": quick_check,
        "indexes": indexes,
    }


def timed_projection(
    conn: sqlite3.Connection,
    scope: str,
    query: str,
    sql: str,
    params: tuple[str, ...],
) -> dict[str, Any]:
    plan = [
        str(row[3]) for row in conn.execute(f"EXPLAIN QUERY PLAN {sql}", params)
    ]
    started = time.perf_counter()
    row_count = sum(1 for _ in conn.execute(sql, params))
    elapsed_ms = (time.perf_counter() - started) * 1000
    return {
        "scope": scope,
        "query": query,
        "elapsed_ms": round(elapsed_ms, 2),
        "result_rows": row_count,
        "plan": plan,
    }


def profile_projections(path: Path, iterations: int) -> list[dict[str, Any]]:
    """Profile the two statements used by the current production reducer."""
    profiles: list[dict[str, Any]] = []
    bounds = one_day_bounds()
    for scope in ("1d", "all"):
        for iteration in range(1, iterations + 1):
            conn = sqlite3.connect(sqlite_uri(path), uri=True)
            conn.execute("PRAGMA query_only = ON")
            try:
                event = timed_projection(
                    conn,
                    scope,
                    "activity_event_cost_projection",
                    "SELECT event_key, COALESCE(cost_with_cache_usd, 0.0) "
                    "FROM usage_event",
                    (),
                )
                if scope == "1d":
                    turn_where = "WHERE t.started_at >= ? AND t.started_at < ?"
                    turn_params = bounds
                else:
                    turn_where = ""
                    turn_params = ()
                turns = timed_projection(
                    conn,
                    scope,
                    "activity_turn_projection",
                    "SELECT substr(t.turn_key, 6), t.category, t.has_edits, "
                    "t.one_shot, t.retries, t.call_count, t.total_tokens "
                    f"FROM usage_turn t {turn_where}",
                    turn_params,
                )
            finally:
                conn.close()
            event["iteration"] = iteration
            turns["iteration"] = iteration
            profiles.extend((event, turns))
    return profiles


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


def wait_for_server(port: int, process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"server exited during startup with code {process.returncode}")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError(f"server did not listen on loopback port {port}")


def activity_request(port: int, scope: str) -> dict[str, Any]:
    url = f"http://127.0.0.1:{port}/api/activity?range={scope}"
    started = time.perf_counter()
    with urllib.request.urlopen(url, timeout=10) as response:
        payload = json.loads(response.read())
        status = int(response.status)
    wall_ms = (time.perf_counter() - started) * 1000
    support = payload.get("support") or {}
    level = str(support.get("level") or "unknown")
    reason = str(support.get("reason") or "")
    return {
        "wall_ms": round(wall_ms, 2),
        "http_status": status,
        "support_level": level,
        "degraded": level == "degraded" or not bool(support.get("supported", False)),
        "timeout_reason": "timeout" in reason.lower(),
    }


def request_round(port: int, scope: str, concurrency: int) -> list[dict[str, Any]]:
    if concurrency == 1:
        return [activity_request(port, scope)]
    barrier = threading.Barrier(concurrency + 1)

    def worker() -> dict[str, Any]:
        barrier.wait()
        return activity_request(port, scope)

    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(worker) for _ in range(concurrency)]
        barrier.wait()
        return [future.result() for future in futures]


@dataclass
class ServerSession:
    process: subprocess.Popen[str]
    port: int
    stdout_handle: Any
    stderr_handle: Any
    stderr_path: Path
    metric_cursor: int = 0

    def metrics(self, expected: int) -> list[dict[str, Any]]:
        deadline = time.monotonic() + 2
        while True:
            self.stderr_handle.flush()
            lines = self.stderr_path.read_text(encoding="utf-8", errors="replace").splitlines()
            found = []
            for line in lines:
                match = ACTIVITY_METRIC.search(ANSI_ESCAPE.sub("", line))
                if match:
                    found.append(
                        {
                            "semaphore_wait_ms": int(match.group(1)),
                            "query_ms": int(match.group(2)),
                            "cancelled": match.group(3) == "true",
                        }
                    )
            available = found[self.metric_cursor :]
            if len(available) >= expected:
                selected = available[:expected]
                self.metric_cursor += expected
                return selected
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    f"expected {expected} Activity timing logs, found {len(available)}"
                )
            time.sleep(0.05)

    def stop(self) -> None:
        if self.process.poll() is None:
            try:
                if os.name == "nt":
                    self.process.send_signal(signal.CTRL_BREAK_EVENT)
                else:
                    self.process.send_signal(signal.SIGINT)
                self.process.wait(timeout=10)
            except (OSError, subprocess.TimeoutExpired):
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
        self.stdout_handle.close()
        self.stderr_handle.close()


def start_server(binary: Path, runtime: Path, label: str) -> ServerSession:
    port = free_loopback_port()
    logs = runtime / "profile-logs"
    logs.mkdir(parents=True, exist_ok=True)
    stdout_path = logs / f"{label}.stdout.log"
    stderr_path = logs / f"{label}.stderr.log"
    stdout_handle = stdout_path.open("w", encoding="utf-8")
    stderr_handle = stderr_path.open("w", encoding="utf-8")
    env = os.environ.copy()
    env["RUST_LOG"] = "llmusage=debug"
    env["LLMUSAGE_LOG"] = "off"
    creation_flags = (
        subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    )
    process = subprocess.Popen(
        [
            str(binary),
            "--home",
            str(runtime),
            "serve",
            "--port",
            str(port),
            "--no-open",
        ],
        stdin=subprocess.DEVNULL,
        stdout=stdout_handle,
        stderr=stderr_handle,
        text=True,
        env=env,
        creationflags=creation_flags,
    )
    session = ServerSession(
        process=process,
        port=port,
        stdout_handle=stdout_handle,
        stderr_handle=stderr_handle,
        stderr_path=stderr_path,
    )
    try:
        wait_for_server(port, process, timeout=30)
    except Exception:
        session.stop()
        raise
    return session


def copy_runtime(snapshot: Path, root: Path) -> None:
    root.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(snapshot, root / "llmusage.db")


def cleanup_database_files(root: Path, work_dir: Path, keep: bool) -> None:
    if keep:
        return
    resolved_root = root.resolve()
    resolved_work = work_dir.resolve()
    if resolved_work not in resolved_root.parents:
        raise RuntimeError(f"refusing cleanup outside work directory: {resolved_root}")
    for name in ("llmusage.db", "llmusage.db-wal", "llmusage.db-shm"):
        (root / name).unlink(missing_ok=True)


def measure_http_matrix(
    binary: Path,
    snapshot: Path,
    work_dir: Path,
    iterations: int,
    keep_databases: bool,
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for scope in ("1d", "all"):
        for concurrency in (1, 2):
            load = "solo" if concurrency == 1 else "concurrency-2"
            for round_number in range(1, iterations + 1):
                label = f"cold-{scope}-{load}-{round_number}"
                runtime = work_dir / "runtimes" / label
                copy_runtime(snapshot, runtime)
                server = start_server(binary, runtime, label)
                try:
                    requests = request_round(server.port, scope, concurrency)
                    metrics = server.metrics(concurrency)
                finally:
                    server.stop()
                results.append(
                    {
                        "temperature": "copy-backed-cold",
                        "scope": scope,
                        "load": load,
                        "round": round_number,
                        "requests": requests,
                        "server_metrics": metrics,
                    }
                )
                cleanup_database_files(runtime, work_dir, keep_databases)

            label = f"warm-{scope}-{load}"
            runtime = work_dir / "runtimes" / label
            copy_runtime(snapshot, runtime)
            server = start_server(binary, runtime, label)
            try:
                request_round(server.port, scope, concurrency)
                server.metrics(concurrency)
                for round_number in range(1, iterations + 1):
                    requests = request_round(server.port, scope, concurrency)
                    metrics = server.metrics(concurrency)
                    results.append(
                        {
                            "temperature": "warm-after-one-unmeasured-round",
                            "scope": scope,
                            "load": load,
                            "round": round_number,
                            "requests": requests,
                            "server_metrics": metrics,
                        }
                    )
            finally:
                server.stop()
            cleanup_database_files(runtime, work_dir, keep_databases)
    return results


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-db", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--work-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--keep-databases", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    source = args.source_db.resolve()
    binary = args.binary.resolve()
    work_dir = args.work_dir.resolve()
    output = args.output.resolve()
    if args.iterations < 5:
        raise SystemExit("--iterations must be at least 5 for the Step 1 gate")
    if not source.is_file() or not binary.is_file():
        raise SystemExit("source database and debug binary must exist")
    if work_dir.exists():
        raise SystemExit(f"work directory already exists: {work_dir}")
    work_dir.mkdir(parents=True)
    snapshot = work_dir / "snapshot" / "llmusage.db"

    online_backup(source, snapshot)
    metadata = database_metadata(snapshot)
    profiles = profile_projections(snapshot, args.iterations)
    results = measure_http_matrix(
        binary,
        snapshot,
        work_dir,
        args.iterations,
        args.keep_databases,
    )
    version = subprocess.run(
        [str(binary), "--version"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    payload = {
        "captured_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "methodology": {
            "source_access": "SQLite online backup from a read-only source connection",
            "cold_proxy": (
                "fresh database copy plus fresh server process per measured round; "
                "Windows OS file cache was not evicted"
            ),
            "warm": "same copied database and process after one unmeasured request round",
            "privacy": "only timings, support state, counts, and plan operators persisted",
            "iterations_per_cell": args.iterations,
        },
        "binary": {"version": version, "sha256": sha256(binary)},
        "database": metadata,
        "direct_profiles": profiles,
        "http_results": results,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    print(output)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
