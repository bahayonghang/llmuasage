# Dashboard

`llmusage serve` starts the local browser dashboard and JSON API.

```powershell
llmusage serve
```

By default it probes local ports starting at `37421`, binds to `127.0.0.1`, prints the URL, and tries to open the default browser.

Before binding a port, `serve` checks parser-backed sources for legacy token
accounting. Safe sources are rebuilt one at a time in registry order. Sources
with missing-file rebuild risk are left unchanged and reported as warnings, so
their historical reports remain available and the dashboard can still start.
Unexpected parser, SQLite, or commit failures stop startup. This automatic path
never enables `--allow-lossy-rebuild` and never rebuilds parserless Antigravity.

Use a fixed port when you need a stable URL:

```powershell
llmusage serve --port 37421
```

## Remote or SSH access

For a remote server, opt in explicitly and suppress browser launching:

```powershell
llmusage serve --public --no-open --port 37421
```

`--public` binds `0.0.0.0`; open `http://<server-host-or-ip>:37421` from a machine that can reach the server. The dashboard and JSON API have no authentication or TLS, so use a firewall, SSH tunnel, or reverse proxy before exposing the port.

For a private SSH session, leave out `--public`, then forward the local listener from your client:

```powershell
ssh -L 37421:127.0.0.1:37421 <user>@<server>
```

SSH sessions automatically skip browser launching.

![llmusage web dashboard overview](/screenshots/web-dashboard-overview.png)

<small>Sanitized local fixture served by `llmusage serve`; not real user data.</small>

## First-screen workflow

The first screen is task-oriented:

1. Confirm the active time/source/model filter.
2. Read the KPI strip for total tokens, cost, cache, and bucket count.
3. Check the trend chart for day/week/month/all movement.
4. Compare project, model, source, and cost rankings.
5. Review behavior panels for activity, tool usage, optimization hints, and model comparison.
6. Use Cost Explorer for ad hoc local slice-and-dice questions.
7. Use sync/export actions or diagnostics when data looks stale.

## Filters

Dashboard filters map to the shared `QueryFilter` used by the Rust query layer.

| Filter | Meaning |
| --- | --- |
| `source` | `codex`, `claude`, `opencode`, or `antigravity` |
| `model` | Exact model string from normalized events |
| `since` / `until` | Date range for dashboard queries |
| `window` | Quick window such as day/week/month/all |
| `timezone` | `UTC`, `local`, or a fixed offset such as `+08:00`; `local` means the machine's current fixed local offset, not an IANA/DST-aware timezone |

The URL preserves filters so a refreshed page or shared local URL keeps the same view.

Cost Explorer adds its own query controls on top of the shared filters:

| Control | Accepted values |
| --- | --- |
| `granularity` | `total`, `day`, `week`, or `month` |
| `metric` | `attributed_cost_usd`, `calls`, `turns`, `sessions`, or `total_tokens` |
| `group_by` | `source`, `model`, `project`, `session`, `tool`, `tool_kind`, `is_tool`, or `token_type` |
| `limit` / `include_other` | Top N rows, optionally merging the rest into `Other` |
| `session_id`, `tool_name`, `tool_kind`, `is_tool`, `token_type` | Explorer-specific filters |

## Sections

### KPI and trend

The KPI strip and trend chart come from `Dashboard::snapshot(&QueryFilter)`. The live dashboard prefers `/api/dashboard`, which builds overview, trends, rankings, health, and diagnostics from one local database snapshot.

### Rankings

The model, source, project, and cost tables answer different questions:

- Models: which model names dominate usage and cost.
- Sources: which local CLI produced the data.
- Projects: which local repositories or folders are active.
- Costs: where estimated cost is concentrated.

### Behavior analytics

Behavior panels read normalized `usage_turn` and `usage_tool_call` rows produced during sync. They do not parse raw transcripts in the browser.

| Panel | Purpose |
| --- | --- |
| Activity | Turn categories such as coding, debugging, exploration, testing, and planning |
| Tools | Tool/action mix such as read, edit, search, bash, MCP, and agent actions |
| Optimize | Read-only findings such as repeated reads or low Read/Edit ratio |
| Compare | Directional comparison between two models with sample-size warnings |

Optimize is advisory only. It never deletes, moves, archives, rewrites, or cleans files.

### Cost Explorer

The Cost Explorer workbench is an additive panel, not a replacement for the fixed dashboard sections. It asks questions such as:

- "How much did tool calls cost by session today?"
- "Which tool kinds dominate attributed cost?"
- "How do input/cache/output token components split by source?"

The browser calls `/api/explorer` with the selected controls. The response already contains aggregated `totals`, ranked `rows`, and time `series`; the frontend only renders that payload and does not fetch or pivot raw transcript rows. Tool-scoped views use query-time attribution: cost-bearing turns with multiple tools are split across sibling tool calls, and cost-bearing assistant turns with no tools appear as `(non-tool)` when included.

## Degraded states

The dashboard must show capability gaps explicitly instead of pretending missing data is zero.

Common states:

- `no_data`: the filter has no matching local facts.
- `degraded`: a behavior query timed out or failed, while core dashboard data still loaded.
- `insufficient_models`: model comparison needs at least two model candidates.
- `low_sample`: comparison exists but the sample is too small for a strong claim.
- `unsupported`: the selected Explorer metric/dimension/filter combination is not meaningful.
- source-limited facts: Antigravity and OpenCode can degrade to conservative turn facts when source logs do not expose tool-level evidence.

Core `/api/dashboard` data should remain responsive even when Activity, Tools, Optimize, Explorer, or Compare is degraded.

## JSON export and static export

The live dashboard can export the current JSON snapshot, including the currently loaded Explorer result. For an offline HTML bundle, use:

```powershell
llmusage export html --out .\llmusage-report
```

The static bundle includes `snapshot.json` with the default Explorer payload and the same Explorer renderer assets. Snapshot mode disables live Explorer controls because it reads from the captured JSON instead of `/api/explorer`.

## Sync jobs

Live mode can start, poll, and cancel in-process sync jobs. Jobs share the same local worker lock as CLI sync, so CLI, hook, and dashboard workers do not write concurrently.

## Screenshot fixture for docs maintainers

Use the dev-only example when refreshing docs screenshots without real user data:

```powershell
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

Then capture `http://127.0.0.1:37421` at `1440×1100` and write the result to `docs/public/screenshots/web-dashboard-overview.png`.
