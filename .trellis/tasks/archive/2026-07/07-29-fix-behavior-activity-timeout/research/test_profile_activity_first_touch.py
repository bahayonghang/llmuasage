from __future__ import annotations

import importlib.util
import io
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("profile_activity_first_touch.py")
SPEC = importlib.util.spec_from_file_location(
    "profile_activity_first_touch", MODULE_PATH
)
assert SPEC is not None and SPEC.loader is not None
HARNESS = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = HARNESS
SPEC.loader.exec_module(HARNESS)


def boot(epoch_ms: int) -> dict[str, object]:
    return {
        "captured_at_utc": "2026-07-29T00:00:00Z",
        "captured_at_unix_ms": epoch_ms + 10_000,
        "uptime_ms": 10_000,
        "boot_epoch_ms": epoch_ms,
        "uncertainty_ms": 1,
    }


def observation(wall_ms: float, *, wait_ms: int = 0, query_ms: int = 1_000) -> dict:
    return {
        "wall_ms": wall_ms,
        "http_status": 200,
        "support_level": "normalized",
        "degraded": False,
        "timeout": wall_ms >= 3_000,
        "server": {
            "semaphore_wait_ms": wait_ms,
            "query_ms": query_ms,
            "cancelled": wall_ms >= 3_000,
        },
    }


def sample(sample_id: str, cold_ms: float, warm_ms: float) -> dict:
    return {
        "sample_id": sample_id,
        "first_touch": observation(cold_ms, query_ms=int(cold_ms)),
        "warm": observation(warm_ms, query_ms=int(warm_ms)),
        "sqlite_busy_or_locked": False,
        "server_cleanup": {"process_exited": True, "port_released": True},
    }


class ManifestFixture:
    def __init__(self, root: Path):
        self.root = root
        self.work_dir = root / "target" / "tmp" / "activity-first-touch-v19"
        self.work_dir.mkdir(parents=True)
        self.manifest_path = self.work_dir / "manifest.json"
        self.binary = root / "target" / "debug" / "llmusage.exe"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b"test binary")
        samples = []
        for sample_id in HARNESS.SAMPLE_IDS:
            database = self.work_dir / sample_id / "llmusage.db"
            database.parent.mkdir()
            database.write_bytes(f"snapshot {sample_id}".encode())
            samples.append(
                {
                    "sample_id": sample_id,
                    "relative_file": f"{sample_id}/llmusage.db",
                    **HARNESS.file_stat(database),
                    "sha256": "0" * 64,
                    "database": {
                        "schema_version": HARNESS.EXPECTED_SCHEMA_VERSION,
                        "counts": {"usage_event": 1, "usage_turn": 1},
                        "quick_check": ["ok"],
                        "activity_cost_index_columns": [
                            "event_key",
                            "cost_with_cache_usd",
                        ],
                        "activity_event_cost_plan": [
                            "SCAN usage_event USING COVERING INDEX "
                            "idx_usage_event_activity_cost"
                        ],
                    },
                }
            )
        source_stat = {"size_bytes": 10, "mtime_ns": 20}
        self.core = {
            "format": HARNESS.FORMAT,
            "format_version": HARNESS.FORMAT_VERSION,
            "prepared_at_utc": "2026-07-29T00:00:00Z",
            "prepared_boot": boot(1_000_000),
            "git_head": "a" * 40,
            "binary": {
                "relative_file": "target/debug/llmusage.exe",
                **HARNESS.file_stat(self.binary),
                "sha256": HARNESS.sha256_file(self.binary),
                "version": "llmusage test",
            },
            "source_database": {"before": source_stat, "after": source_stat},
            "samples": samples,
            "post_reboot_command": [
                "python",
                "-B",
                ".trellis/tasks/07-29-fix-behavior-activity-timeout/research/"
                "profile_activity_first_touch.py",
                "run",
                "--manifest",
                "target/tmp/activity-first-touch-v19/manifest.json",
            ],
        }
        self.write()

    def write(self) -> None:
        self.manifest_path.write_text(
            json.dumps(HARNESS.seal_manifest(self.core)), encoding="utf-8"
        )


class BootGuardTests(unittest.TestCase):
    def test_preflight_rejects_prepare_boot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            with self.assertRaisesRegex(HARNESS.HarnessError, "prepare boot"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(1_002_000)
                )


class ManifestTests(unittest.TestCase):
    def test_tampered_manifest_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            payload = json.loads(fixture.manifest_path.read_text(encoding="utf-8"))
            payload["git_head"] = "b" * 40
            fixture.manifest_path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(HARNESS.HarnessError, "integrity check failed"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_missing_manifest_sample_is_rejected_without_content_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            missing = fixture.work_dir / "sample-03" / "llmusage.db"
            missing.unlink()
            with self.assertRaisesRegex(
                HARNESS.HarnessError, "sample-03 database is missing"
            ):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_size_or_mtime_change_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            changed = fixture.work_dir / "sample-04" / "llmusage.db"
            changed.write_bytes(b"changed size")
            with self.assertRaisesRegex(HARNESS.HarnessError, "size or mtime differs"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_resealed_sample_path_tamper_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            fixture.core["samples"][1]["relative_file"] = "sample-01/llmusage.db"
            fixture.write()
            with self.assertRaisesRegex(HARNESS.HarnessError, "file location"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_boolean_file_metadata_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            fixture.core["samples"][0]["size_bytes"] = True
            fixture.write()
            with self.assertRaisesRegex(HARNESS.HarnessError, "size_bytes is invalid"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_already_consumed_sample_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            marker = fixture.work_dir / "sample-02" / HARNESS.CONSUMED_MARKER
            marker.write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(HARNESS.HarnessError, "already consumed"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_incompatible_binary_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            fixture.binary.write_bytes(b"evil binary")
            with self.assertRaisesRegex(HARNESS.HarnessError, "binary SHA-256 differs"):
                HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )

    def test_preflight_does_not_open_snapshot_content(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = ManifestFixture(Path(directory))
            original_open = Path.open

            def guarded_open(path: Path, *args, **kwargs):
                if path.name == "llmusage.db":
                    raise AssertionError("preflight opened snapshot content")
                return original_open(path, *args, **kwargs)

            with mock.patch.object(Path, "open", guarded_open):
                inputs = HARNESS.preflight_run(
                    fixture.manifest_path, fixture.root, current_boot=boot(2_000_000)
                )
            self.assertEqual(len(inputs.sample_files), 5)


class SnapshotPreparationTests(unittest.TestCase):
    def test_tiny_online_backup_creates_five_identical_v19_snapshots(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.db"
            conn = sqlite3.connect(source)
            try:
                conn.executescript(
                    """
                    CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                    INSERT INTO meta VALUES ('schema_version', '18');
                    CREATE TABLE usage_event (
                        event_key TEXT PRIMARY KEY,
                        cost_with_cache_usd REAL NOT NULL DEFAULT 0.0,
                        padding TEXT NOT NULL
                    );
                    WITH RECURSIVE seq(value) AS (
                        SELECT 1 UNION ALL SELECT value + 1 FROM seq WHERE value < 1000
                    )
                    INSERT INTO usage_event
                    SELECT printf('event-%04d', value), 1.0, printf('%0500d', value)
                    FROM seq;
                    CREATE TABLE usage_turn (turn_key TEXT PRIMARY KEY);
                    INSERT INTO usage_turn VALUES ('turn');
                    """
                )
                conn.commit()
            finally:
                conn.close()
            before = HARNESS.file_stat(source)
            work_dir = root / "activity-first-touch-v19"
            work_dir.mkdir()

            def migrate(_binary: Path, path: Path, _log_dir: Path) -> None:
                migration = sqlite3.connect(path)
                try:
                    migration.executescript(
                        """
                        CREATE INDEX idx_usage_event_activity_cost
                            ON usage_event(event_key, cost_with_cache_usd);
                        UPDATE meta SET value = '19' WHERE key = 'schema_version';
                        """
                    )
                    migration.commit()
                finally:
                    migration.close()

            with mock.patch.object(HARNESS, "migrate_snapshot", side_effect=migrate):
                records = HARNESS.create_snapshots(
                    source, work_dir, root / "llmusage.exe"
                )
            self.assertEqual(HARNESS.file_stat(source), before)
            self.assertEqual(
                [record["sample_id"] for record in records], list(HARNESS.SAMPLE_IDS)
            )
            self.assertEqual(len({record["sha256"] for record in records}), 1)
            self.assertTrue(
                all(record["database"]["quick_check"] == ["ok"] for record in records)
            )
            self.assertTrue(
                all(
                    record["database"]["schema_version"] == 19
                    and record["database"]["activity_cost_index_columns"]
                    == ["event_key", "cost_with_cache_usd"]
                    for record in records
                )
            )


class GateTests(unittest.TestCase):
    def test_gate_allows_d1_only_when_all_three_prd_conditions_hold(self) -> None:
        samples = [
            sample(sample_id, cold, 1_000)
            for sample_id, cold in zip(
                HARNESS.SAMPLE_IDS, [3_100, 3_200, 3_300, 3_400, 3_500], strict=True
            )
        ]
        gate = HARNESS.calculate_gate(samples)
        self.assertEqual(gate["decision"], "GO D1")
        self.assertEqual(gate["first_touch_median_ms"], 3_300)

    def test_gate_is_no_go_when_one_warm_sample_reaches_timeout(self) -> None:
        samples = [
            sample(sample_id, 3_500, 3_000 if index == 4 else 1_000)
            for index, sample_id in enumerate(HARNESS.SAMPLE_IDS)
        ]
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")

    def test_gate_is_no_go_when_cold_median_equals_timeout(self) -> None:
        samples = [
            sample(sample_id, cold, 1_000)
            for sample_id, cold in zip(
                HARNESS.SAMPLE_IDS, [2_900, 2_950, 3_000, 3_100, 3_200], strict=True
            )
        ]
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")

    def test_gate_rejects_unordered_samples(self) -> None:
        samples = [sample(sample_id, 3_500, 1_000) for sample_id in HARNESS.SAMPLE_IDS]
        samples[0], samples[1] = samples[1], samples[0]
        with self.assertRaisesRegex(HARNESS.HarnessError, "five ordered"):
            HARNESS.calculate_gate(samples)

    def test_gate_rejects_boolean_timing_as_incomplete(self) -> None:
        samples = [sample(sample_id, 3_500, 1_000) for sample_id in HARNESS.SAMPLE_IDS]
        samples[0]["first_touch"]["server"]["query_ms"] = True
        with self.assertRaisesRegex(
            HARNESS.HarnessError, "observations are incomplete"
        ):
            HARNESS.calculate_gate(samples)

    def test_gate_is_no_go_when_permit_wait_is_primary(self) -> None:
        samples = [sample(sample_id, 3_500, 1_000) for sample_id in HARNESS.SAMPLE_IDS]
        samples[0]["first_touch"]["server"].update(
            semaphore_wait_ms=3_100, query_ms=300
        )
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")

    def test_gate_is_no_go_when_sqlite_lock_is_observed(self) -> None:
        samples = [sample(sample_id, 3_500, 1_000) for sample_id in HARNESS.SAMPLE_IDS]
        samples[2]["sqlite_busy_or_locked"] = True
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")

    def test_gate_is_no_go_when_cleanup_or_warm_support_is_degraded(self) -> None:
        samples = [sample(sample_id, 3_500, 1_000) for sample_id in HARNESS.SAMPLE_IDS]
        samples[1]["server_cleanup"]["port_released"] = False
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")
        samples[1]["server_cleanup"]["port_released"] = True
        samples[3]["warm"]["degraded"] = True
        self.assertEqual(HARNESS.calculate_gate(samples)["decision"], "NO-GO D1/D2")


class PrivacyTests(unittest.TestCase):
    def test_sanitized_result_shape_passes(self) -> None:
        payload = {
            "samples": [sample("sample-01", 3_500, 1_000)],
            "binary": {"sha256": "0" * 64, "version": "llmusage test"},
        }
        HARNESS.assert_privacy_safe(payload)

    def test_forbidden_fields_and_absolute_paths_are_rejected(self) -> None:
        for payload in (
            {"project_hash": "secret"},
            {"nested": {"response_breakdown": []}},
            {"detail": r"C:\Users\person\.llmusage\llmusage.db"},
            {"detail": "/home/person/.llmusage/llmusage.db"},
        ):
            with self.subTest(payload=payload):
                with self.assertRaises(HARNESS.HarnessError):
                    HARNESS.assert_privacy_safe(payload)


class ServerCleanupTests(unittest.TestCase):
    def test_collect_sample_waits_for_matching_orphan_before_warm(self) -> None:
        actions: list[str] = []
        fake_session = mock.Mock()
        fake_session.port = 37000
        requests = iter(
            [
                (observation(3_500), False),
                (observation(1_000), False),
            ]
        )
        metrics = iter(
            [
                {
                    "query_id": 11,
                    "semaphore_wait_ms": 0,
                    "query_ms": 3_000,
                    "cancelled": True,
                },
                {
                    "query_id": 12,
                    "semaphore_wait_ms": 0,
                    "query_ms": 900,
                    "cancelled": False,
                },
            ]
        )

        def request(_port: int) -> tuple[dict, bool]:
            actions.append("request")
            return next(requests)

        def metric() -> dict:
            actions.append("metric")
            return next(metrics)

        def settled(query_id: int, timeout: float = 30.0) -> None:
            self.assertEqual(timeout, 30.0)
            actions.append(f"settled:{query_id}")

        fake_session.next_activity_metric.side_effect = metric
        fake_session.wait_for_orphan_settled.side_effect = settled
        fake_session.has_sqlite_busy_or_locked.return_value = False

        with tempfile.TemporaryDirectory() as directory:
            sample_file = Path(directory) / "sample-01" / "llmusage.db"
            sample_file.parent.mkdir()
            sample_file.touch()
            with (
                mock.patch.object(HARNESS, "mark_consumed"),
                mock.patch.object(HARNESS, "start_server", return_value=fake_session),
                mock.patch.object(HARNESS, "activity_request", side_effect=request),
                mock.patch.object(HARNESS, "wait_for_port_release"),
            ):
                result = HARNESS.collect_sample(
                    Path("binary"),
                    sample_file,
                    "sample-01",
                    boot(2_000_000),
                    Path(directory) / "logs",
                )

        self.assertEqual(
            actions,
            ["request", "metric", "settled:11", "request", "metric"],
        )
        self.assertEqual(
            set(result["first_touch"]["server"]),
            {"semaphore_wait_ms", "query_ms", "cancelled"},
        )

    def test_session_matches_query_id_for_metric_and_settled_event(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            stderr_file = Path(directory) / "server.log"
            stderr_file.write_text(
                'Dashboard query timed out query_id=41 section="activity" '
                "semaphore_wait_ms=0 query_ms=3001 cancelled=true\n"
                'Dashboard query orphan settled query_id=40 section="activity" '
                "orphan_duration_ms=4 join_outcome=cancelled\n"
                'Dashboard query orphan settled query_id=41 section="activity" '
                "orphan_duration_ms=5 join_outcome=cancelled\n",
                encoding="utf-8",
            )
            with stderr_file.open("a", encoding="utf-8") as stderr:
                session = HARNESS.ServerSession(
                    process=mock.Mock(),
                    port=37000,
                    stdout_handle=io.StringIO(),
                    stderr_handle=stderr,
                    stderr_file=stderr_file,
                )
                metric = session.next_activity_metric()
                session.wait_for_orphan_settled(metric["query_id"], timeout=0.1)

        self.assertEqual(
            metric,
            {
                "query_id": 41,
                "semaphore_wait_ms": 0,
                "query_ms": 3001,
                "cancelled": True,
            },
        )

    def test_session_rejects_missing_matching_settled_event(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            stderr_file = Path(directory) / "server.log"
            stderr_file.write_text(
                "Dashboard query orphan settled query_id=7 section=activity "
                "orphan_duration_ms=5 join_outcome=cancelled\n",
                encoding="utf-8",
            )
            with stderr_file.open("a", encoding="utf-8") as stderr:
                session = HARNESS.ServerSession(
                    process=mock.Mock(),
                    port=37000,
                    stdout_handle=io.StringIO(),
                    stderr_handle=stderr,
                    stderr_file=stderr_file,
                )
                with self.assertRaisesRegex(
                    HARNESS.HarnessError, "orphan did not settle"
                ):
                    session.wait_for_orphan_settled(8, timeout=0.01)

    def test_collect_sample_stops_server_and_checks_port_on_request_failure(
        self,
    ) -> None:
        fake_session = mock.Mock()
        fake_session.port = 37001
        with tempfile.TemporaryDirectory() as directory:
            sample_file = Path(directory) / "sample-01" / "llmusage.db"
            sample_file.parent.mkdir()
            sample_file.touch()
            with (
                mock.patch.object(HARNESS, "mark_consumed"),
                mock.patch.object(HARNESS, "start_server", return_value=fake_session),
                mock.patch.object(
                    HARNESS,
                    "activity_request",
                    side_effect=RuntimeError("request failed"),
                ),
                mock.patch.object(HARNESS, "wait_for_port_release") as release,
            ):
                with self.assertRaisesRegex(RuntimeError, "request failed"):
                    HARNESS.collect_sample(
                        Path("binary"),
                        sample_file,
                        "sample-01",
                        boot(2_000_000),
                        Path(directory) / "logs",
                    )
            fake_session.stop.assert_called_once_with()
            release.assert_called_once_with(fake_session.port)

    def test_collect_sample_stops_server_and_checks_port_on_timing_failure(
        self,
    ) -> None:
        fake_session = mock.Mock()
        fake_session.port = 37002
        fake_session.next_activity_metric.side_effect = HARNESS.HarnessError(
            "timing missing"
        )
        with tempfile.TemporaryDirectory() as directory:
            sample_file = Path(directory) / "sample-01" / "llmusage.db"
            sample_file.parent.mkdir()
            sample_file.touch()
            with (
                mock.patch.object(HARNESS, "mark_consumed"),
                mock.patch.object(HARNESS, "start_server", return_value=fake_session),
                mock.patch.object(
                    HARNESS,
                    "activity_request",
                    return_value=(observation(1_000), False),
                ),
                mock.patch.object(HARNESS, "wait_for_port_release") as release,
            ):
                with self.assertRaisesRegex(HARNESS.HarnessError, "timing missing"):
                    HARNESS.collect_sample(
                        Path("binary"),
                        sample_file,
                        "sample-01",
                        boot(2_000_000),
                        Path(directory) / "logs",
                    )
            fake_session.stop.assert_called_once_with()
            release.assert_called_once_with(fake_session.port)

    def test_sqlite_busy_or_locked_recognizes_table_and_schema_messages(self) -> None:
        for message in (
            "database is busy",
            "database is locked",
            "database table is locked",
            "database schema is locked: main",
            "SQLITE_BUSY",
            "SQLITE_LOCKED",
        ):
            with self.subTest(message=message):
                self.assertIsNotNone(HARNESS.SQLITE_BUSY_OR_LOCKED.search(message))

    def test_server_session_stop_closes_handles_after_process_exit(self) -> None:
        process = mock.Mock()
        process.poll.side_effect = [None, 0]
        stdout = io.StringIO()
        stderr = io.StringIO()
        session = HARNESS.ServerSession(
            process=process,
            port=37001,
            stdout_handle=stdout,
            stderr_handle=stderr,
            stderr_file=Path("unused"),
        )
        session.stop()
        process.send_signal.assert_called_once()
        process.wait.assert_called_once_with(timeout=10)
        self.assertTrue(stdout.closed)
        self.assertTrue(stderr.closed)


if __name__ == "__main__":
    unittest.main()
