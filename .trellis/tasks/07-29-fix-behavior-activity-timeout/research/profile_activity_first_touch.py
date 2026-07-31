#!/usr/bin/env python3
"""Reboot-gated Activity first-touch profiler for Windows.

``prepare`` creates five isolated SQLite snapshots without modifying the live
database. ``run`` consumes exactly those manifest-listed snapshots after a
different Windows boot and persists only sanitized timing evidence.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import sqlite3
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence, TextIO


FORMAT = "llmusage.activity-first-touch.v19.v1"
FORMAT_VERSION = 2
SOURCE_SCHEMA_VERSION = 18
EXPECTED_SCHEMA_VERSION = 19
SAMPLE_IDS = tuple(f"sample-{number:02d}" for number in range(1, 6))
BOOT_EPOCH_TOLERANCE_MS = 5_000
ACTIVITY_TIMEOUT_MS = 3_000.0
SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[4]
WORK_DIR = REPO_ROOT / "target" / "tmp" / "activity-first-touch-v19"
MANIFEST_PATH = WORK_DIR / "manifest.json"
RESULTS_PATH = SCRIPT_PATH.parent / "v19-first-touch-results.json"
VALIDATION_PATH = SCRIPT_PATH.parent / "v19-first-touch-validation.md"
CONSUMED_MARKER = "v19-first-touch-consumed.json"
ACTIVITY_COST_INDEX = "idx_usage_event_activity_cost"

ACTIVITY_METRIC = re.compile(
    r'section="?activity"?.*?semaphore_wait_ms=(\d+).*?query_ms=(\d+)'
    r".*?cancelled=(true|false)"
)
QUERY_ID_FIELD = re.compile(r"\bquery_id=(\d+)\b")
ACTIVITY_SECTION_FIELD = re.compile(r'\bsection="?activity"?(?=\s|$)')
ORPHAN_SETTLED_MESSAGE = "Dashboard query orphan settled"
ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")
SQLITE_BUSY_OR_LOCKED = re.compile(
    r"(?:database(?:\s+(?:table|schema))?\s+is\s+(?:busy|locked)"
    r"|sqlite[_ ]?(?:busy|locked))",
    re.IGNORECASE,
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
FORBIDDEN_RESULT_KEY_PARTS = (
    "breakdown",
    "model",
    "project",
    "path",
    "prompt",
    "response",
    "session",
    "source",
)
WINDOWS_ABSOLUTE_PATH = re.compile(r"(?:^|\s)[A-Za-z]:[\\/]")
UNC_PATH = re.compile(r"(?:^|\s)\\\\[^\\\s]+\\")
POSIX_ABSOLUTE_PATH = re.compile(r"(?:^|\s)/(?:[^/\s]+/)*[^/\s]+")


class HarnessError(RuntimeError):
    """Raised when an evidence-safety precondition is not satisfied."""


def is_non_negative_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def utc_now() -> str:
    return (
        datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha256(payload: Mapping[str, Any]) -> str:
    encoded = json.dumps(
        payload,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def atomic_write_json(path: Path, payload: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.{uuid.uuid4().hex}.tmp")
    try:
        temporary.write_text(
            json.dumps(payload, ensure_ascii=True, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def current_windows_boot_identity() -> dict[str, Any]:
    if os.name != "nt":
        raise HarnessError("the first-touch boot guard requires Windows")
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    get_tick_count = kernel32.GetTickCount64
    get_tick_count.argtypes = []
    get_tick_count.restype = ctypes.c_ulonglong

    wall_before_ms = time.time_ns() // 1_000_000
    uptime_ms = int(get_tick_count())
    wall_after_ms = time.time_ns() // 1_000_000
    captured_at_ms = (wall_before_ms + wall_after_ms) // 2
    return {
        "captured_at_utc": utc_now(),
        "captured_at_unix_ms": captured_at_ms,
        "uptime_ms": uptime_ms,
        "boot_epoch_ms": captured_at_ms - uptime_ms,
        "uncertainty_ms": max(1, wall_after_ms - wall_before_ms),
    }


def validate_boot_identity(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise HarnessError(f"{label} boot identity is missing or invalid")
    required = {
        "captured_at_utc",
        "captured_at_unix_ms",
        "uptime_ms",
        "boot_epoch_ms",
        "uncertainty_ms",
    }
    if set(value) != required:
        raise HarnessError(f"{label} boot identity fields are invalid")
    for field in required - {"captured_at_utc"}:
        if not is_non_negative_int(value[field]):
            raise HarnessError(f"{label} boot identity field {field} is invalid")
    if not isinstance(value["captured_at_utc"], str):
        raise HarnessError(f"{label} boot capture time is invalid")
    return value


def same_boot(
    prepared: Mapping[str, Any],
    current: Mapping[str, Any],
    tolerance_ms: int = BOOT_EPOCH_TOLERANCE_MS,
) -> bool:
    uncertainty = max(
        tolerance_ms,
        int(prepared["uncertainty_ms"]) + int(current["uncertainty_ms"]),
    )
    return (
        abs(int(prepared["boot_epoch_ms"]) - int(current["boot_epoch_ms"]))
        <= uncertainty
    )


def sqlite_uri(path: Path) -> str:
    return f"{path.resolve().as_uri()}?mode=ro"


def online_backup(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=False)
    source_conn = sqlite3.connect(sqlite_uri(source), uri=True)
    destination_conn: sqlite3.Connection | None = None
    try:
        source_conn.execute("PRAGMA query_only = ON")
        destination_conn = sqlite3.connect(destination)
        source_conn.backup(destination_conn)
    finally:
        if destination_conn is not None:
            destination_conn.close()
        source_conn.close()


def fsync_snapshot(path: Path) -> None:
    with path.open("r+b") as handle:
        handle.flush()
        os.fsync(handle.fileno())


def database_content_metadata(path: Path) -> dict[str, Any]:
    conn = sqlite3.connect(sqlite_uri(path), uri=True)
    try:
        conn.execute("PRAGMA query_only = ON")
        row = conn.execute(
            "SELECT value FROM meta WHERE key = 'schema_version'"
        ).fetchone()
        if row is None:
            raise HarnessError("snapshot is missing meta.schema_version")
        quick_check_rows = [str(item[0]) for item in conn.execute("PRAGMA quick_check")]
        counts = {
            table: int(conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0])
            for table in ("usage_event", "usage_turn")
        }
        index_columns = [
            str(item[2])
            for item in conn.execute(f"PRAGMA index_info({ACTIVITY_COST_INDEX})")
        ]
        activity_plan = [
            str(item[3])
            for item in conn.execute(
                "EXPLAIN QUERY PLAN SELECT event_key, "
                "COALESCE(cost_with_cache_usd, 0.0) FROM usage_event"
            )
        ]
    finally:
        conn.close()
    return {
        "schema_version": int(row[0]),
        "counts": counts,
        "quick_check": quick_check_rows,
        "activity_cost_index_columns": index_columns,
        "activity_event_cost_plan": activity_plan,
    }


def file_stat(path: Path) -> dict[str, int]:
    stat = path.stat()
    return {"size_bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns}


def checkpoint_snapshot(path: Path) -> None:
    conn = sqlite3.connect(path)
    try:
        row = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()
        if row is None or int(row[0]) != 0:
            raise HarnessError("snapshot WAL checkpoint did not complete")
    finally:
        conn.close()
    for suffix in ("-wal", "-shm"):
        path.with_name(path.name + suffix).unlink(missing_ok=True)


def migrate_snapshot(binary: Path, path: Path, log_dir: Path) -> None:
    before = database_content_metadata(path)
    if before["schema_version"] != SOURCE_SCHEMA_VERSION:
        raise HarnessError(
            f"snapshot source has schema v{before['schema_version']}; "
            f"expected v{SOURCE_SCHEMA_VERSION}"
        )
    session = start_server(binary, path.parent, log_dir, "prepare-v19-migration")
    port = session.port
    try:
        pass
    finally:
        session.stop()
        wait_for_port_release(port)
    checkpoint_snapshot(path)
    after = database_content_metadata(path)
    if after["schema_version"] != EXPECTED_SCHEMA_VERSION:
        raise HarnessError(
            f"binary bootstrap produced schema v{after['schema_version']}; "
            f"expected v{EXPECTED_SCHEMA_VERSION}"
        )


def create_snapshots(
    source: Path, work_dir: Path, binary: Path
) -> list[dict[str, Any]]:
    first = work_dir / SAMPLE_IDS[0] / "llmusage.db"
    online_backup(source, first)
    fsync_snapshot(first)
    migrate_snapshot(binary, first, work_dir / "prepare-logs")
    fsync_snapshot(first)
    for sample_id in SAMPLE_IDS[1:]:
        destination = work_dir / sample_id / "llmusage.db"
        destination.parent.mkdir(parents=True, exist_ok=False)
        shutil.copyfile(first, destination)
        fsync_snapshot(destination)

    records = []
    expected_content: dict[str, Any] | None = None
    expected_hash: str | None = None
    for sample_id in SAMPLE_IDS:
        path = work_dir / sample_id / "llmusage.db"
        content = database_content_metadata(path)
        digest = sha256_file(path)
        if content["schema_version"] != EXPECTED_SCHEMA_VERSION:
            raise HarnessError(
                f"{sample_id} has schema v{content['schema_version']}; "
                f"expected v{EXPECTED_SCHEMA_VERSION}"
            )
        if content["quick_check"] != ["ok"]:
            raise HarnessError(f"{sample_id} failed SQLite quick_check")
        if content["activity_cost_index_columns"] != [
            "event_key",
            "cost_with_cache_usd",
        ]:
            raise HarnessError(f"{sample_id} has an incompatible Activity cost index")
        if not any(
            f"USING COVERING INDEX {ACTIVITY_COST_INDEX}" in detail
            for detail in content["activity_event_cost_plan"]
        ):
            raise HarnessError(f"{sample_id} does not use the Activity covering index")
        if expected_content is None:
            expected_content = content
            expected_hash = digest
        elif content != expected_content or digest != expected_hash:
            raise HarnessError(f"{sample_id} does not match sample-01")
        records.append(
            {
                "sample_id": sample_id,
                "relative_file": f"{sample_id}/llmusage.db",
                **file_stat(path),
                "sha256": digest,
                "database": content,
            }
        )
    return records


def binary_version(binary: Path) -> str:
    completed = subprocess.run(
        [str(binary), "--version"],
        check=True,
        capture_output=True,
        text=True,
        timeout=15,
    )
    output = completed.stdout.strip() or completed.stderr.strip()
    first_line = output.splitlines()[0] if output else ""
    if not first_line or len(first_line) > 200:
        raise HarnessError("binary returned an invalid --version response")
    return first_line


def git_head(repo_root: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
        timeout=15,
    ).stdout.strip()


def relative_to_repo(path: Path, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError as error:
        raise HarnessError("the debug binary must be inside the repository") from error


def seal_manifest(payload: Mapping[str, Any]) -> dict[str, Any]:
    sealed = dict(payload)
    sealed["integrity"] = {
        "algorithm": "sha256",
        "canonical_json_sha256": canonical_sha256(payload),
    }
    return sealed


def prepare(source: Path, binary: Path, repo_root: Path = REPO_ROOT) -> Path:
    source = source.resolve()
    binary = binary.resolve()
    work_dir = repo_root.resolve() / "target" / "tmp" / "activity-first-touch-v19"
    manifest_path = work_dir / "manifest.json"
    if not source.is_file():
        raise HarnessError("source database does not exist")
    if not binary.is_file():
        raise HarnessError("debug binary does not exist")
    binary_relative = relative_to_repo(binary, repo_root)
    if work_dir.exists():
        raise HarnessError(f"work directory already exists: {work_dir}")

    prepared_boot_before = current_windows_boot_identity()
    source_before = file_stat(source)
    work_dir.mkdir(parents=True, exist_ok=False)
    samples = create_snapshots(source, work_dir, binary)
    source_after = file_stat(source)
    prepared_boot_after = current_windows_boot_identity()
    if source_before != source_after:
        raise HarnessError("source database size or mtime changed during prepare")
    if not same_boot(prepared_boot_before, prepared_boot_after):
        raise HarnessError("Windows boot identity changed during prepare")

    command = [
        "python",
        "-B",
        SCRIPT_PATH.relative_to(repo_root.resolve()).as_posix(),
        "run",
        "--manifest",
        MANIFEST_PATH.relative_to(repo_root.resolve()).as_posix(),
    ]
    core = {
        "format": FORMAT,
        "format_version": FORMAT_VERSION,
        "prepared_at_utc": utc_now(),
        "prepared_boot": prepared_boot_after,
        "git_head": git_head(repo_root),
        "binary": {
            "relative_file": binary_relative,
            **file_stat(binary),
            "sha256": sha256_file(binary),
            "version": binary_version(binary),
        },
        "source_database": {"before": source_before, "after": source_after},
        "samples": samples,
        "post_reboot_command": command,
    }
    manifest = seal_manifest(core)
    atomic_write_json(manifest_path, manifest)
    print(f"manifest: {manifest_path}")
    print("post-reboot command:")
    print(subprocess.list2cmdline(command))
    return manifest_path


def require_exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        raise HarnessError(f"{label} fields are missing or invalid")
    return value


def validate_file_metadata(value: Any, label: str) -> dict[str, int]:
    value = require_exact_keys(value, {"size_bytes", "mtime_ns"}, label)
    for field in ("size_bytes", "mtime_ns"):
        if not is_non_negative_int(value[field]):
            raise HarnessError(f"{label}.{field} is invalid")
    return value


def validate_database_metadata(value: Any, label: str) -> None:
    value = require_exact_keys(
        value,
        {
            "schema_version",
            "counts",
            "quick_check",
            "activity_cost_index_columns",
            "activity_event_cost_plan",
        },
        label,
    )
    if value["schema_version"] != EXPECTED_SCHEMA_VERSION:
        raise HarnessError(f"{label} schema version is incompatible")
    if value["quick_check"] != ["ok"]:
        raise HarnessError(f"{label} quick_check metadata is invalid")
    counts = require_exact_keys(value["counts"], {"usage_event", "usage_turn"}, label)
    if any(not is_non_negative_int(count) for count in counts.values()):
        raise HarnessError(f"{label} row counts are invalid")
    if value["activity_cost_index_columns"] != [
        "event_key",
        "cost_with_cache_usd",
    ]:
        raise HarnessError(f"{label} Activity cost index metadata is invalid")
    plan = value["activity_event_cost_plan"]
    if not isinstance(plan, list) or not any(
        isinstance(detail, str)
        and f"USING COVERING INDEX {ACTIVITY_COST_INDEX}" in detail
        for detail in plan
    ):
        raise HarnessError(f"{label} Activity query plan metadata is invalid")


def load_manifest(manifest_path: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise HarnessError("manifest is missing or is not valid JSON") from error
    manifest = require_exact_keys(
        manifest,
        {
            "format",
            "format_version",
            "prepared_at_utc",
            "prepared_boot",
            "git_head",
            "binary",
            "source_database",
            "samples",
            "post_reboot_command",
            "integrity",
        },
        "manifest",
    )
    integrity = require_exact_keys(
        manifest["integrity"],
        {"algorithm", "canonical_json_sha256"},
        "manifest integrity",
    )
    if integrity["algorithm"] != "sha256" or not isinstance(
        integrity["canonical_json_sha256"], str
    ):
        raise HarnessError("manifest integrity metadata is invalid")
    core = {key: value for key, value in manifest.items() if key != "integrity"}
    if canonical_sha256(core) != integrity["canonical_json_sha256"]:
        raise HarnessError("manifest integrity check failed")
    if manifest["format"] != FORMAT or manifest["format_version"] != FORMAT_VERSION:
        raise HarnessError("manifest format is incompatible")
    validate_boot_identity(manifest["prepared_boot"], "prepared")
    if not isinstance(manifest["prepared_at_utc"], str):
        raise HarnessError("manifest preparation time is invalid")
    if not isinstance(manifest["git_head"], str) or not re.fullmatch(
        r"[0-9a-f]{40}", manifest["git_head"]
    ):
        raise HarnessError("manifest Git HEAD is invalid")

    binary = require_exact_keys(
        manifest["binary"],
        {"relative_file", "size_bytes", "mtime_ns", "sha256", "version"},
        "binary",
    )
    validate_file_metadata(
        {"size_bytes": binary["size_bytes"], "mtime_ns": binary["mtime_ns"]},
        "binary",
    )
    if not isinstance(binary["relative_file"], str) or not isinstance(
        binary["version"], str
    ):
        raise HarnessError("binary metadata is invalid")
    if not isinstance(binary["sha256"], str) or not SHA256_RE.fullmatch(
        binary["sha256"]
    ):
        raise HarnessError("binary SHA-256 is invalid")

    source = require_exact_keys(
        manifest["source_database"], {"before", "after"}, "source database"
    )
    before = validate_file_metadata(source["before"], "source before")
    after = validate_file_metadata(source["after"], "source after")
    if before != after:
        raise HarnessError("manifest records a source database metadata change")

    samples = manifest["samples"]
    if not isinstance(samples, list) or len(samples) != len(SAMPLE_IDS):
        raise HarnessError("manifest must list exactly five samples")
    expected_sample_hash: str | None = None
    expected_database: dict[str, Any] | None = None
    for expected_id, sample in zip(SAMPLE_IDS, samples, strict=True):
        sample = require_exact_keys(
            sample,
            {
                "sample_id",
                "relative_file",
                "size_bytes",
                "mtime_ns",
                "sha256",
                "database",
            },
            expected_id,
        )
        if sample["sample_id"] != expected_id:
            raise HarnessError("manifest sample order or identity is invalid")
        if sample["relative_file"] != f"{expected_id}/llmusage.db":
            raise HarnessError(f"{expected_id} file location is invalid")
        validate_file_metadata(
            {"size_bytes": sample["size_bytes"], "mtime_ns": sample["mtime_ns"]},
            expected_id,
        )
        if not isinstance(sample["sha256"], str) or not SHA256_RE.fullmatch(
            sample["sha256"]
        ):
            raise HarnessError(f"{expected_id} SHA-256 is invalid")
        validate_database_metadata(sample["database"], expected_id)
        if expected_sample_hash is None:
            expected_sample_hash = sample["sha256"]
            expected_database = sample["database"]
        elif (
            sample["sha256"] != expected_sample_hash
            or sample["database"] != expected_database
        ):
            raise HarnessError("manifest samples do not describe identical snapshots")
    return manifest


@dataclass(frozen=True)
class RunInputs:
    manifest: dict[str, Any]
    binary: Path
    sample_files: tuple[Path, ...]
    current_boot: dict[str, Any]


def resolve_repo_file(relative_file: str, repo_root: Path, label: str) -> Path:
    candidate = repo_root.resolve() / Path(relative_file)
    resolved = candidate.resolve()
    try:
        resolved.relative_to(repo_root.resolve())
    except ValueError as error:
        raise HarnessError(f"{label} escapes the repository") from error
    return resolved


def preflight_run(
    manifest_path: Path,
    repo_root: Path = REPO_ROOT,
    current_boot: Mapping[str, Any] | None = None,
) -> RunInputs:
    repo_root = repo_root.resolve()
    manifest_path = manifest_path.resolve()
    expected_manifest = (
        repo_root / "target" / "tmp" / "activity-first-touch-v19" / "manifest.json"
    ).resolve()
    if manifest_path != expected_manifest:
        raise HarnessError(
            "run accepts only the fixed activity-first-touch-v19 manifest"
        )
    manifest = load_manifest(manifest_path)
    expected_command = [
        "python",
        "-B",
        ".trellis/tasks/07-29-fix-behavior-activity-timeout/research/"
        "profile_activity_first_touch.py",
        "run",
        "--manifest",
        "target/tmp/activity-first-touch-v19/manifest.json",
    ]
    if manifest["post_reboot_command"] != expected_command:
        raise HarnessError("manifest post-reboot command is incompatible")
    boot = validate_boot_identity(
        dict(current_boot)
        if current_boot is not None
        else current_windows_boot_identity(),
        "current",
    )
    if same_boot(manifest["prepared_boot"], boot):
        raise HarnessError("refusing first-touch run on the prepare boot")

    binary_meta = manifest["binary"]
    binary = resolve_repo_file(binary_meta["relative_file"], repo_root, "binary")
    if not binary.is_file():
        raise HarnessError("manifest binary is missing")
    actual_binary_stat = file_stat(binary)
    if actual_binary_stat["size_bytes"] != binary_meta["size_bytes"]:
        raise HarnessError("binary size differs from the prepared binary")
    if sha256_file(binary) != binary_meta["sha256"]:
        raise HarnessError("binary SHA-256 differs from the prepared binary")

    sample_files = []
    snapshot_root = manifest_path.parent.resolve()
    for sample_meta in manifest["samples"]:
        sample_file = (snapshot_root / sample_meta["relative_file"]).resolve()
        try:
            sample_file.relative_to(snapshot_root)
        except ValueError as error:
            raise HarnessError(
                "manifest sample escapes the snapshot directory"
            ) from error
        if not sample_file.is_file():
            raise HarnessError(f"{sample_meta['sample_id']} database is missing")
        actual = file_stat(sample_file)
        expected = {
            "size_bytes": sample_meta["size_bytes"],
            "mtime_ns": sample_meta["mtime_ns"],
        }
        if actual != expected:
            raise HarnessError(
                f"{sample_meta['sample_id']} size or mtime differs from the manifest"
            )
        if (sample_file.parent / CONSUMED_MARKER).exists():
            raise HarnessError(f"{sample_meta['sample_id']} is already consumed")
        sample_files.append(sample_file)
    return RunInputs(manifest, binary, tuple(sample_files), boot)


def free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


def wait_for_server(port: int, process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise HarnessError(
                f"server exited during startup with code {process.returncode}"
            )
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise HarnessError("server did not listen before the startup deadline")


def wait_for_port_release(port: int, timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
                if hasattr(socket, "SO_EXCLUSIVEADDRUSE"):
                    probe.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
                probe.bind(("127.0.0.1", port))
                return
        except OSError:
            time.sleep(0.05)
    raise HarnessError(f"server port {port} was not released")


@dataclass
class ServerSession:
    process: subprocess.Popen[str]
    port: int
    stdout_handle: TextIO
    stderr_handle: TextIO
    stderr_file: Path
    metric_cursor: int = 0

    def next_activity_metric(self) -> dict[str, Any]:
        deadline = time.monotonic() + 2.0
        while True:
            self.stderr_handle.flush()
            lines = self.stderr_file.read_text(
                encoding="utf-8", errors="replace"
            ).splitlines()
            found = []
            for line in lines:
                clean = ANSI_ESCAPE.sub("", line)
                match = ACTIVITY_METRIC.search(clean)
                query_id = QUERY_ID_FIELD.search(clean)
                if match and query_id:
                    found.append(
                        {
                            "query_id": int(query_id.group(1)),
                            "semaphore_wait_ms": int(match.group(1)),
                            "query_ms": int(match.group(2)),
                            "cancelled": match.group(3) == "true",
                        }
                    )
            if len(found) > self.metric_cursor:
                metric = found[self.metric_cursor]
                self.metric_cursor += 1
                return metric
            if time.monotonic() >= deadline:
                raise HarnessError("Activity timing log was not emitted")
            time.sleep(0.05)

    def wait_for_orphan_settled(self, query_id: int, timeout: float = 30.0) -> None:
        if not is_non_negative_int(query_id):
            raise HarnessError("Activity query ID is invalid")
        deadline = time.monotonic() + timeout
        while True:
            self.stderr_handle.flush()
            lines = self.stderr_file.read_text(
                encoding="utf-8", errors="replace"
            ).splitlines()
            for line in lines:
                clean = ANSI_ESCAPE.sub("", line)
                logged_query_id = QUERY_ID_FIELD.search(clean)
                if (
                    ORPHAN_SETTLED_MESSAGE in clean
                    and ACTIVITY_SECTION_FIELD.search(clean)
                    and logged_query_id
                    and int(logged_query_id.group(1)) == query_id
                ):
                    return
            if time.monotonic() >= deadline:
                raise HarnessError(
                    f"Activity orphan did not settle for query ID {query_id}"
                )
            time.sleep(0.05)

    def has_sqlite_busy_or_locked(self) -> bool:
        self.stderr_handle.flush()
        text = self.stderr_file.read_text(encoding="utf-8", errors="replace")
        return SQLITE_BUSY_OR_LOCKED.search(ANSI_ESCAPE.sub("", text)) is not None

    def stop(self) -> None:
        try:
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
            if self.process.poll() is None:
                raise HarnessError("server process did not exit")
        finally:
            self.stdout_handle.close()
            self.stderr_handle.close()


def start_server(
    binary: Path, sample_dir: Path, log_dir: Path, sample_id: str
) -> ServerSession:
    port = free_loopback_port()
    log_dir.mkdir(parents=True, exist_ok=True)
    stdout_file = log_dir / f"{sample_id}.stdout.log"
    stderr_file = log_dir / f"{sample_id}.stderr.log"
    stdout_handle = stdout_file.open("w", encoding="utf-8")
    stderr_handle = stderr_file.open("w", encoding="utf-8")
    env = os.environ.copy()
    env["RUST_LOG"] = "llmusage=debug"
    env["LLMUSAGE_LOG"] = "off"
    creation_flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    try:
        process = subprocess.Popen(
            [
                str(binary),
                "--home",
                str(sample_dir),
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
    except Exception:
        stdout_handle.close()
        stderr_handle.close()
        raise
    session = ServerSession(
        process=process,
        port=port,
        stdout_handle=stdout_handle,
        stderr_handle=stderr_handle,
        stderr_file=stderr_file,
    )
    try:
        wait_for_server(port, process, timeout=30.0)
    except Exception:
        try:
            session.stop()
        finally:
            wait_for_port_release(port)
        raise
    return session


def activity_request(port: int) -> tuple[dict[str, Any], bool]:
    url = f"http://127.0.0.1:{port}/api/activity?range=all"
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(url, timeout=15) as response:
            body = response.read()
            status = int(response.status)
    except urllib.error.HTTPError as error:
        try:
            body = error.read()
            status = int(error.code)
        finally:
            error.close()
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        raise HarnessError("Activity request failed before an HTTP response") from error
    wall_ms = (time.perf_counter() - started) * 1000
    try:
        payload = json.loads(body)
    except (UnicodeDecodeError, json.JSONDecodeError):
        payload = {}
    support = payload.get("support") if isinstance(payload, dict) else None
    support = support if isinstance(support, dict) else {}
    level = support.get("level")
    level = level if isinstance(level, str) and level else "protocol_error"
    reason = support.get("reason")
    reason = reason if isinstance(reason, str) else ""
    supported = support.get("supported") is True
    busy_or_locked = SQLITE_BUSY_OR_LOCKED.search(reason) is not None
    return (
        {
            "wall_ms": round(wall_ms, 2),
            "http_status": status,
            "support_level": level,
            "degraded": status != 200 or level == "degraded" or not supported,
            "timeout": "timeout" in reason.lower(),
        },
        busy_or_locked,
    )


def mark_consumed(sample_dir: Path, sample_id: str, boot: Mapping[str, Any]) -> Path:
    marker = sample_dir / CONSUMED_MARKER
    payload = {
        "format": FORMAT,
        "sample_id": sample_id,
        "consumed_at_utc": utc_now(),
        "run_boot_epoch_ms": int(boot["boot_epoch_ms"]),
    }
    try:
        with marker.open("x", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=True, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
    except FileExistsError as error:
        raise HarnessError(f"{sample_id} is already consumed") from error
    return marker


def collect_sample(
    binary: Path,
    sample_file: Path,
    sample_id: str,
    boot: Mapping[str, Any],
    log_dir: Path,
) -> dict[str, Any]:
    mark_consumed(sample_file.parent, sample_id, boot)
    session: ServerSession | None = None
    cleanup_error: BaseException | None = None
    try:
        session = start_server(binary, sample_file.parent, log_dir, sample_id)
        first_touch, first_busy = activity_request(session.port)
        first_metric = session.next_activity_metric()
        first_query_id = first_metric.pop("query_id")
        first_touch["server"] = first_metric
        if first_touch["timeout"] or first_metric["cancelled"]:
            session.wait_for_orphan_settled(first_query_id)
        warm, warm_busy = activity_request(session.port)
        warm_metric = session.next_activity_metric()
        warm_query_id = warm_metric.pop("query_id")
        warm["server"] = warm_metric
        if warm["timeout"] or warm_metric["cancelled"]:
            session.wait_for_orphan_settled(warm_query_id)
        busy_or_locked = first_busy or warm_busy or session.has_sqlite_busy_or_locked()
        return {
            "sample_id": sample_id,
            "first_touch": first_touch,
            "warm": warm,
            "sqlite_busy_or_locked": busy_or_locked,
            "server_cleanup": {"process_exited": True, "port_released": True},
        }
    finally:
        if session is not None:
            try:
                session.stop()
            except BaseException as error:
                cleanup_error = error
            try:
                wait_for_port_release(session.port)
            except BaseException as error:
                cleanup_error = cleanup_error or error
            if cleanup_error is not None:
                raise cleanup_error


def observation_is_complete(observation: Any) -> bool:
    if not isinstance(observation, dict):
        return False
    if set(observation) != {
        "wall_ms",
        "http_status",
        "support_level",
        "degraded",
        "timeout",
        "server",
    }:
        return False
    server = observation["server"]
    return (
        isinstance(observation["wall_ms"], (int, float))
        and observation["wall_ms"] >= 0
        and is_non_negative_int(observation["http_status"])
        and isinstance(observation["support_level"], str)
        and isinstance(observation["degraded"], bool)
        and isinstance(observation["timeout"], bool)
        and isinstance(server, dict)
        and set(server) == {"semaphore_wait_ms", "query_ms", "cancelled"}
        and is_non_negative_int(server["semaphore_wait_ms"])
        and is_non_negative_int(server["query_ms"])
        and isinstance(server["cancelled"], bool)
    )


def calculate_gate(samples: Sequence[Mapping[str, Any]]) -> dict[str, Any]:
    expected_ids = list(SAMPLE_IDS)
    actual_ids = [sample.get("sample_id") for sample in samples]
    complete = (
        len(samples) == len(SAMPLE_IDS)
        and actual_ids == expected_ids
        and all(
            observation_is_complete(sample.get(temperature))
            for sample in samples
            for temperature in ("first_touch", "warm")
        )
        and all(
            isinstance(sample.get("sqlite_busy_or_locked"), bool) for sample in samples
        )
        and all(
            sample.get("server_cleanup")
            == {"process_exited": True, "port_released": True}
            for sample in samples
        )
        and all(
            sample[temperature]["http_status"] == 200
            for sample in samples
            for temperature in ("first_touch", "warm")
        )
        and all(not sample["warm"]["degraded"] for sample in samples)
    )
    if len(samples) != len(SAMPLE_IDS) or actual_ids != expected_ids:
        raise HarnessError("gate requires exactly the five ordered manifest samples")
    if not all(
        observation_is_complete(sample.get(temperature))
        for sample in samples
        for temperature in ("first_touch", "warm")
    ):
        raise HarnessError("gate observations are incomplete")

    cold_values = [float(sample["first_touch"]["wall_ms"]) for sample in samples]
    warm_values = [float(sample["warm"]["wall_ms"]) for sample in samples]
    cold_median = statistics.median(cold_values)
    timeout_samples = [
        sample
        for sample in samples
        if sample["first_touch"]["timeout"]
        or float(sample["first_touch"]["wall_ms"]) >= ACTIVITY_TIMEOUT_MS
    ]
    wait_not_primary = all(
        sample["first_touch"]["server"]["semaphore_wait_ms"]
        < sample["first_touch"]["server"]["query_ms"]
        for sample in timeout_samples
    )
    no_busy_or_locked = all(
        sample.get("sqlite_busy_or_locked") is False for sample in samples
    )
    cold_median_over_timeout = cold_median > ACTIVITY_TIMEOUT_MS
    warm_all_under_timeout = all(value < ACTIVITY_TIMEOUT_MS for value in warm_values)
    wait_and_lock_not_primary = wait_not_primary and no_busy_or_locked
    go = (
        complete
        and cold_median_over_timeout
        and warm_all_under_timeout
        and wait_and_lock_not_primary
    )
    return {
        "evidence_complete": complete,
        "first_touch_median_ms": round(cold_median, 2),
        "first_touch_median_over_3000_ms": cold_median_over_timeout,
        "warm_all_under_3000_ms": warm_all_under_timeout,
        "timeout_sample_count": len(timeout_samples),
        "permit_wait_not_primary": wait_not_primary,
        "sqlite_busy_or_locked_absent": no_busy_or_locked,
        "wait_and_lock_not_primary": wait_and_lock_not_primary,
        "decision": "GO D1" if go else "NO-GO D1/D2",
    }


def assert_privacy_safe(value: Any, location: str = "result") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            normalized = str(key).lower().replace("-", "_")
            if any(part in normalized for part in FORBIDDEN_RESULT_KEY_PARTS):
                raise HarnessError(
                    f"privacy-forbidden result field at {location}.{key}"
                )
            assert_privacy_safe(child, f"{location}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            assert_privacy_safe(child, f"{location}[{index}]")
    elif isinstance(value, str):
        if (
            WINDOWS_ABSOLUTE_PATH.search(value)
            or UNC_PATH.search(value)
            or POSIX_ABSOLUTE_PATH.search(value)
        ):
            raise HarnessError(
                f"absolute path leaked into sanitized result at {location}"
            )


def render_validation(results: Mapping[str, Any]) -> str:
    samples = results["samples"]
    gate = results["gate"]
    rows = []
    for sample in samples:
        cold = sample["first_touch"]
        warm = sample["warm"]
        rows.append(
            "| {sample} | {cold_wall:.2f} | {cold_state} | {wait}/{query}/{cancelled} | "
            "{warm_wall:.2f} | {warm_state} | {busy} | {cleanup} |".format(
                sample=sample["sample_id"],
                cold_wall=float(cold["wall_ms"]),
                cold_state=(
                    f"{cold['http_status']}/{cold['support_level']}/"
                    f"{str(cold['degraded']).lower()}/{str(cold['timeout']).lower()}"
                ),
                warm_wall=float(warm["wall_ms"]),
                warm_state=(
                    f"{warm['http_status']}/{warm['support_level']}/"
                    f"{str(warm['degraded']).lower()}/{str(warm['timeout']).lower()}"
                ),
                wait=cold["server"]["semaphore_wait_ms"],
                query=cold["server"]["query_ms"],
                cancelled=str(cold["server"]["cancelled"]).lower(),
                busy=str(sample["sqlite_busy_or_locked"]).lower(),
                cleanup=(
                    "confirmed"
                    if sample["server_cleanup"]
                    == {"process_exited": True, "port_released": True}
                    else "failed"
                ),
            )
        )
    checks = [
        (
            "Five first-touch `all` samples have median > 3000 ms",
            gate["first_touch_median_over_3000_ms"],
        ),
        (
            "All five paired warm `all` samples are < 3000 ms",
            gate["warm_all_under_3000_ms"],
        ),
        (
            "Permit waiting is not primary and no SQLite busy/lock was observed",
            gate["wait_and_lock_not_primary"],
        ),
        ("Evidence is complete", gate["evidence_complete"]),
    ]
    check_lines = [f"- [{'x' if passed else ' '}] {label}" for label, passed in checks]
    return "\n".join(
        [
            "# Activity first-touch validation",
            "",
            f"Captured: `{results['captured_at_utc']}`",
            "",
            "Each sample used a distinct manifest-listed SQLite snapshot and a fresh debug "
            "server. Server bootstrap was part of the product lifecycle before the first "
            "Activity request. The five files share one reboot-cleared cache event; user-mode "
            "code cannot prove that third-party software did not read them first.",
            "",
            "State columns are `HTTP/support/degraded/timeout`; first-touch server "
            "timing is `wait/query/cancelled`. Permit wait is non-primary only when "
            "`wait < query` for every first-touch timeout sample.",
            "",
            "| Sample | First-touch wall ms | First state | First server ms/state | Warm wall ms | Warm state | SQLite busy/locked | Cleanup |",
            "| --- | ---: | --- | --- | ---: | --- | --- | --- |",
            *rows,
            "",
            "## Mechanical gate",
            "",
            f"First-touch median: `{gate['first_touch_median_ms']:.2f} ms`",
            "",
            *check_lines,
            "",
            f"Conclusion: `{gate['decision']}`",
            "",
        ]
    )


def run(
    manifest_path: Path,
    repo_root: Path = REPO_ROOT,
    results_path: Path = RESULTS_PATH,
    validation_path: Path = VALIDATION_PATH,
) -> tuple[Path, Path]:
    inputs = preflight_run(manifest_path, repo_root)
    if results_path.exists() or validation_path.exists():
        raise HarnessError("refusing to overwrite existing first-touch evidence")
    log_dir = manifest_path.resolve().parent / "server-logs"
    samples = []
    for sample_id, sample_file in zip(SAMPLE_IDS, inputs.sample_files, strict=True):
        samples.append(
            collect_sample(
                inputs.binary,
                sample_file,
                sample_id,
                inputs.current_boot,
                log_dir,
            )
        )
    gate = calculate_gate(samples)
    results = {
        "format": FORMAT,
        "captured_at_utc": utc_now(),
        "method": {
            "range": "all",
            "sample_count": len(samples),
            "first_touch": "fresh server after reboot; server bootstrap precedes request",
            "warm": "paired request in the same server process",
        },
        "binary": {
            "version": inputs.manifest["binary"]["version"],
            "sha256": inputs.manifest["binary"]["sha256"],
        },
        "samples": samples,
        "gate": gate,
    }
    assert_privacy_safe(results)
    atomic_write_json(results_path, results)
    validation_path.write_text(render_validation(results), encoding="utf-8")
    print(f"results: {results_path}")
    print(f"validation: {validation_path}")
    print(f"decision: {gate['decision']}")
    return results_path, validation_path


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare or run the reboot-gated Activity first-touch profile."
    )
    commands = parser.add_subparsers(dest="command", required=True)

    prepare_parser = commands.add_parser(
        "prepare", help="create and verify five snapshots before a manual reboot"
    )
    prepare_parser.add_argument("--source-db", required=True, type=Path)
    prepare_parser.add_argument(
        "--binary",
        type=Path,
        default=REPO_ROOT / "target" / "debug" / "llmusage.exe",
    )

    run_parser = commands.add_parser(
        "run", help="consume the manifest after a different Windows boot"
    )
    run_parser.add_argument("--manifest", type=Path, default=MANIFEST_PATH)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> None:
    args = parse_args(argv)
    if args.command == "prepare":
        prepare(args.source_db, args.binary)
    elif args.command == "run":
        run(args.manifest)
    else:
        raise AssertionError(f"unexpected command: {args.command}")


if __name__ == "__main__":
    try:
        main()
    except HarnessError as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(2)
    except KeyboardInterrupt:
        sys.exit(130)
