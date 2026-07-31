from __future__ import annotations

import importlib.util
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("profile_activity_confirmation.py")
SPEC = importlib.util.spec_from_file_location("profile_activity_confirmation", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


def attempt(number: int, wall_ms: float, timeout: bool) -> dict:
    return {
        "attempt": number,
        "wall_ms": wall_ms,
        "http_status": 200,
        "support_level": "degraded" if timeout else "normalized",
        "degraded": timeout,
        "timeout": timeout,
        "server": {
            "semaphore_wait_ms": 0,
            "query_ms": int(wall_ms),
            "cancelled": timeout,
        },
    }


def copy_result(copy_id: str) -> dict:
    return {
        "copy_id": copy_id,
        "attempts": [
            attempt(1, 3_010, True),
            attempt(2, 1_200, False),
            attempt(3, 1_100, False),
        ],
        "sqlite_busy_or_locked": False,
        "cleanup": {
            "process_exited": True,
            "port_released": True,
            "database_removed": True,
            "sidecars_removed": True,
            "raw_logs_removed": True,
        },
    }


class ConfirmationGateTests(unittest.TestCase):
    def test_gate_passes_only_when_all_three_copies_pass(self) -> None:
        copies = [copy_result(copy_id) for copy_id in HARNESS.COPY_IDS]
        self.assertEqual(
            HARNESS.calculate_confirmation_gate(copies)["decision"], "GO D1"
        )

    def test_gate_rejects_no_initial_timeout_or_nonconsecutive_success(self) -> None:
        copies = [copy_result(copy_id) for copy_id in HARNESS.COPY_IDS]
        copies[0]["attempts"] = [attempt(1, 1_000, False)] * 3
        self.assertEqual(
            HARNESS.calculate_confirmation_gate(copies)["decision"],
            "NO-GO D1/D2",
        )
        copies[0] = copy_result(HARNESS.COPY_IDS[0])
        copies[0]["attempts"] = [
            attempt(1, 3_010, True),
            attempt(2, 1_000, False),
            attempt(3, 3_010, True),
            attempt(4, 1_000, False),
            attempt(5, 3_010, True),
        ]
        self.assertEqual(
            HARNESS.calculate_confirmation_gate(copies)["decision"],
            "NO-GO D1/D2",
        )

    def test_gate_rejects_permit_lock_and_cleanup_failures(self) -> None:
        for mutation in ("permit", "lock", "cleanup"):
            copies = [copy_result(copy_id) for copy_id in HARNESS.COPY_IDS]
            if mutation == "permit":
                copies[0]["attempts"][0]["server"]["semaphore_wait_ms"] = 1
            elif mutation == "lock":
                copies[1]["sqlite_busy_or_locked"] = True
            else:
                copies[2]["cleanup"]["database_removed"] = False
            with self.subTest(mutation=mutation):
                self.assertEqual(
                    HARNESS.calculate_confirmation_gate(copies)["decision"],
                    "NO-GO D1/D2",
                )


class AttemptLifecycleTests(unittest.TestCase):
    def test_collect_waits_for_settled_and_stops_after_two_successes(self) -> None:
        actions: list[str] = []
        session = mock.Mock(port=37000)
        observations = iter(
            [
                (attempt(1, 3_010, True) | {"attempt": None}, False),
                (attempt(2, 1_200, False) | {"attempt": None}, False),
                (attempt(3, 1_100, False) | {"attempt": None}, False),
            ]
        )
        metrics = iter(
            [
                {"query_id": 9, **attempt(1, 3_010, True)["server"]},
                {"query_id": 10, **attempt(2, 1_200, False)["server"]},
                {"query_id": 11, **attempt(3, 1_100, False)["server"]},
            ]
        )

        def request(_port: int):
            actions.append("request")
            observation, busy = next(observations)
            observation.pop("server")
            observation.pop("attempt")
            return observation, busy

        def metric():
            actions.append("metric")
            return next(metrics)

        def settled(query_id: int):
            actions.append(f"settled:{query_id}")

        session.next_activity_metric.side_effect = metric
        session.wait_for_orphan_settled.side_effect = settled
        session.has_sqlite_busy_or_locked.return_value = False
        with mock.patch.object(HARNESS.FIRST_TOUCH, "activity_request", side_effect=request):
            attempts, busy = HARNESS.collect_attempts(
                session, stop_after_confirmation=True
            )
        self.assertFalse(busy)
        self.assertEqual(len(attempts), 3)
        self.assertEqual(
            actions,
            ["request", "metric", "settled:9", "request", "metric", "request", "metric"],
        )


class SafetyTests(unittest.TestCase):
    def test_cleanup_deletes_only_exact_database_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory) / "work"
            runtime = work / "copy-01"
            runtime.mkdir(parents=True)
            for name in ("llmusage.db", "llmusage.db-wal", "llmusage.db-shm"):
                (runtime / name).write_bytes(b"x")
            retained = runtime / "retained.txt"
            retained.write_text("keep", encoding="utf-8")
            result = HARNESS.remove_database_files(runtime, work)
            self.assertTrue(all(result.values()))
            self.assertTrue(retained.exists())

    def test_cleanup_refuses_path_outside_work_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(HARNESS.HarnessError):
                HARNESS.remove_database_files(root / "outside", root / "work")

    def test_privacy_scan_rejects_rows_dimensions_and_paths(self) -> None:
        for payload in (
            {"response_rows": []},
            {"model": "secret"},
            {"detail": r"C:\Users\person\llmusage.db"},
        ):
            with self.subTest(payload=payload):
                with self.assertRaises(HARNESS.HarnessError):
                    HARNESS.FIRST_TOUCH.assert_privacy_safe(payload)

    def test_confirmation_metadata_is_privacy_safe(self) -> None:
        payload = {
            "method": {
                "copy_mode": "robocopy unbuffered mode",
                "request": "Activity GET with range=all",
            },
            "copies": [copy_result("copy-01")],
        }
        HARNESS.FIRST_TOUCH.assert_privacy_safe(payload)

    def test_d1_requires_passing_confirmation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "llmusage.exe"
            binary.write_bytes(b"test-binary")
            result = Path(directory) / "confirmation.json"
            copies = [copy_result(copy_id) for copy_id in HARNESS.COPY_IDS]
            copies[0]["attempts"] = [attempt(1, 1_000, False)] * 3
            gate = HARNESS.calculate_confirmation_gate(copies)
            result.write_text(
                json.dumps(
                    {
                        "format": "llmusage.activity-confirmation.v1",
                        "binary": {"sha256": HARNESS.sha256(binary)},
                        "copies": copies,
                        "gate": gate,
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(HARNESS.HarnessError, "passing"):
                HARNESS.load_confirmation_gate(result, binary)

    def test_d1_recalculates_gate_and_matches_binary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "llmusage.exe"
            binary.write_bytes(b"test-binary")
            copies = [copy_result(copy_id) for copy_id in HARNESS.COPY_IDS]
            payload = {
                "format": "llmusage.activity-confirmation.v1",
                "binary": {"sha256": HARNESS.sha256(binary)},
                "copies": copies,
                "gate": HARNESS.calculate_confirmation_gate(copies),
            }
            result = Path(directory) / "confirmation.json"
            result.write_text(json.dumps(payload), encoding="utf-8")
            self.assertEqual(
                HARNESS.load_confirmation_gate(result, binary)["decision"], "GO D1"
            )

            payload["copies"][0]["cleanup"]["database_removed"] = False
            result.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(HARNESS.HarnessError, "does not match"):
                HARNESS.load_confirmation_gate(result, binary)

            payload["gate"] = HARNESS.calculate_confirmation_gate(payload["copies"])
            payload["binary"]["sha256"] = "0" * 64
            result.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(HARNESS.HarnessError, "binary"):
                HARNESS.load_confirmation_gate(result, binary)

    def test_d1_render_marks_zero_wal_baseline_unverified(self) -> None:
        payload = json.loads(HARNESS.D1_RESULTS.read_text(encoding="utf-8"))
        payload["write_probe_before"]["wal_bytes_after_insert_and_update"] = 0
        payload["write_wal_ratio"] = None
        rendered = HARNESS.render_d1(payload)
        self.assertIn("UNVERIFIED ratio", rendered)
        self.assertIn("write amplification remains UNVERIFIED", rendered)

    def test_evidence_markdown_matches_sanitized_json(self) -> None:
        confirmation = json.loads(
            HARNESS.CONFIRM_RESULTS.read_text(encoding="utf-8")
        )
        d1 = json.loads(HARNESS.D1_RESULTS.read_text(encoding="utf-8"))
        self.assertEqual(
            HARNESS.render_confirmation(confirmation),
            HARNESS.CONFIRM_VALIDATION.read_text(encoding="utf-8"),
        )
        self.assertEqual(
            HARNESS.render_d1(d1),
            HARNESS.D1_VALIDATION.read_text(encoding="utf-8"),
        )

    def test_d1_write_probe_rolls_back_and_index_changes_plan(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "llmusage.db"
            connection = sqlite3.connect(database)
            connection.executescript(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);"
                "INSERT INTO meta VALUES('schema_version', '18');"
                "CREATE TABLE usage_event("
                "event_key TEXT PRIMARY KEY, cost_with_cache_usd REAL, payload TEXT);"
            )
            connection.executemany(
                "INSERT INTO usage_event VALUES(?, 1.0, ?)",
                [(f"event-{index}", "x" * 1_000) for index in range(200)],
            )
            connection.commit()
            connection.close()
            connection = sqlite3.connect(database)
            try:
                before_count = connection.execute(
                    "SELECT COUNT(*) FROM usage_event"
                ).fetchone()[0]
            finally:
                connection.close()
            probe = HARNESS.representative_write_observation(database, "baseline")
            HARNESS.build_index(database)
            connection = sqlite3.connect(database)
            try:
                after_count = connection.execute(
                    "SELECT COUNT(*) FROM usage_event"
                ).fetchone()[0]
                indexes = {
                    str(row[1])
                    for row in connection.execute("PRAGMA index_list(usage_event)")
                }
            finally:
                connection.close()
            self.assertTrue(probe["rolled_back"])
            self.assertEqual(probe["inserted_rows"], 100)
            self.assertEqual(probe["updated_rows"], 100)
            self.assertGreater(probe["wal_bytes_after_insert_and_update"], 0)
            self.assertEqual(before_count, after_count)
            self.assertIn(HARNESS.INDEX_NAME, indexes)


if __name__ == "__main__":
    unittest.main()
