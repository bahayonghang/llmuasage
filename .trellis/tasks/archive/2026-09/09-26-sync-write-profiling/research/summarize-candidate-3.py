import collections
import datetime
import hashlib
import json
import pathlib
import statistics


research = pathlib.Path(__file__).resolve().parent


def extract(path):
    output = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        start = line.find("{")
        if start >= 0:
            output.append(json.loads(line[start:]))
    return output


def distribution(values):
    ordered = sorted(values)
    median = statistics.median(ordered)
    return {
        "min_ns": ordered[0],
        "max_ns": ordered[-1],
        "median_ns": median,
        "iqr_ns": ordered[3 * len(ordered) // 4] - ordered[len(ordered) // 4],
        "mad_ns": statistics.median(abs(value - median) for value in ordered),
        "quartile_method": "sorted indices n/4 and 3n/4",
    }


prior_writer = extract(research / "writer-candidate-2-ab.log")
prior_parser = extract(research / "parser-candidate-2.log")
previous_manifests = {
    (row["kind"], row.get("workload", row.get("fixture", {}).get("workload"))): row
    for row in prior_writer + prior_parser
    if row.get("kind") in ["ab_manifest", "parser_manifest"]
}
comparisons = []
summaries = []
runs = []
for name in ["shared-bucket", "writer", "host", "parser"]:
    stem = "candidate-3-" + name
    path = research / (stem + ".log")
    contents = path.read_text(encoding="utf-8", errors="replace")
    exits = [line for line in contents.splitlines() if line.startswith("EXIT_CODE=")]
    assert len(exits) == 1, (name, exits)
    exit_code = int(exits[0].split("=")[1])
    records = extract(path)
    (research / (stem + ".json")).write_text(
        json.dumps(records, indent=2) + "\n", encoding="utf-8"
    )
    kinds = collections.Counter(row.get("kind") for row in records)
    run = {"name": name, "exit_code": exit_code, "kinds": dict(kinds)}
    run["log_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    runs.append(run)
    for row in records:
        if "equivalence" in row:
            comparisons.append(row["equivalence"])
    manifests = [row for row in records if row.get("kind") in ["ab_manifest", "parser_manifest"]]
    for manifest in manifests:
        workload = manifest.get("workload", manifest.get("fixture", {}).get("workload"))
        previous = previous_manifests[(manifest["kind"], workload)]
        assert manifest["fine_profile"] is False
        if manifest["kind"] == "ab_manifest":
            assert manifest["fixture"] == previous["fixture"], workload
            rounds = manifest["rounds"]
            assert rounds == previous["rounds"]
        else:
            rounds = manifest["measured_pairs"]
            for field in ["changed_files", "expected_scanned_bytes", "files", "initial_events", "measured_pairs", "parallelism", "seed_variant", "temp_volume"]:
                assert manifest[field] == previous[field], (workload, field)
        for field in ["pragmas", "sqlite", "schema_version", "profile"]:
            assert manifest["environment"][field] == previous["environment"][field]
        samples = [row for row in records if row.get("kind") in ["ab_sample", "parser_sample"] and row["workload"] == workload and not row.get("warmup", False)]
        assert len(samples) == 2 * rounds, workload
        summary = next(row for row in records if row.get("kind") in ["ab_summary", "parser_summary"] and row["workload"] == workload)
        computed = {"run": name, "workload": workload, "measured_pairs": rounds}
        for variant, prefix in [("Baseline", "baseline"), ("Candidate", "candidate")]:
            selected = [row for row in samples if row["variant"] == variant]
            assert len(selected) == rounds
            first_round = 1 if manifest["kind"] == "parser_manifest" else 0
            assert sorted(row["round"] for row in selected) == list(range(first_round, first_round + rounds))
            for row in selected:
                expected_order = ["Baseline", "Candidate"] if row["round"] % 2 == 0 else ["Candidate", "Baseline"]
                assert row["order"] == expected_order
                for record in row["records"]:
                    assert record["stages_ns"] == {}
                    assert record["unclassified_ns"] == record["write_ns"]
            for metric in ["write", "total"]:
                key = prefix + "_" + metric
                computed[key] = distribution([row[metric + "_ns"] for row in selected])
                if key in summary:
                    assert computed[key] == summary[key], (workload, key)
        baseline_write = computed["baseline_write"]["median_ns"]
        computed["write_ratio"] = computed["candidate_write"]["median_ns"] / baseline_write if baseline_write else None
        computed["total_ratio"] = computed["candidate_total"]["median_ns"] / computed["baseline_total"]["median_ns"]
        assert computed["total_ratio"] == summary["total_ratio"]
        if "write_ratio" in summary:
            assert computed["write_ratio"] == summary["write_ratio"]
        computed["primary"] = summary.get("primary", False)
        computed["passed"] = computed["write_ratio"] <= 0.80 if computed["primary"] else computed["total_ratio"] <= 1.10
        assert computed["passed"] == summary["passed"]
        summaries.append(computed)

assert len(comparisons) == 136, len(comparisons)
assert all(set(row["rows"]) == set(comparisons[0]["rows"]) for row in comparisons)
assert len(comparisons[0]["rows"]) == 16
result = {
    "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "status": "passed" if all(row["exit_code"] == 0 for row in runs) and all(row["passed"] for row in summaries) else "failed",
    "runs": runs,
    "summaries": summaries,
    "state_comparisons_including_warmups": len(comparisons),
    "all_state_digests_equal": all(row["baseline_digest"] == row["candidate_digest"] for row in comparisons),
    "maximum_cost_error": max(row["maximum_cost_error"] for row in comparisons),
    "distribution_recomputed_from_every_measured_sample": True,
    "original_fixture_rounds_environment_matched": True,
    "note": "Shared and host standalone gates are separate from their full-writer controls. All samples, including outliers, are retained. The acceptance statistic is the ratio of medians. Fine stage clocks are off; structural counts and reset provenance remain on.",
}
assert result["all_state_digests_equal"]
assert result["maximum_cost_error"] <= 1e-9
(research / "candidate-3-results.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps({key: value for key, value in result.items() if key not in ["runs", "summaries"]}))
for row in summaries:
    print(json.dumps({key: row[key] for key in ["run", "workload", "measured_pairs", "write_ratio", "total_ratio", "passed"]}))
