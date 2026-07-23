# Source Admission Matrix

| Candidate | Stable id | Artifact and roots | Token quality | Evidence state | Decision for this task | Required gate |
| --- | --- | --- | --- | --- | --- | --- |
| Kimi Code | `kimi_code` | `~/.kimi-code/sessions/**/wire.jsonl` | `precise` | 22 files / 1099 turn records; K3 observed | Parser-backed | Fixture plus cursor/dedupe/rewrite tests |
| Pi | `pi` | `~/.pi/agent/sessions` | `precise` | Upstream adapter and format proven; no local Pi files | Parser-backed | Synthetic fixture plus missing-root test |
| Oh My Pi | `pi` (same source) | `~/.omp/agent/sessions` | `precise` | 3 files / 8 assistant usage records; Pi-compatible | Parser-backed path variant | Merge roots without duplicate source rows |
| Reasonix | none in this slice | `%APPDATA%/reasonix/projects/**/sessions/*.jsonl`; old telemetry sidecars | unsupported for current sessions | Current transcript has no usage; old sidecars aggregate | Monitor-only / out of scope | Current per-turn usage contract and privacy review |

## Rationale

1. Kimi and Pi have structured, source-owned usage fields and can reuse `FileCursor`, `SyncShard`, SQLite dedupe, and query/report paths.
2. Reasonix's telemetry summary is not a replacement for current transcript event usage. It remains visible only as a candidate status, if a future monitor descriptor is added.

## Dependency Order

- Sync table output is independent and can ship with the existing four sources.
- Kimi and Pi parser work can proceed independently once the parent task is approved.
- Reasonix is explicitly not a dependency of any deliverable.
