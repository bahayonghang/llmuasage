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

`--public` binds `0.0.0.0`; open `http://<server-host-or-ip>:37421` from a machine that can reach the server. Its compile-time route allowlist contains only the browser shell/assets, `/api/dashboard`, and `/api/health`. The dashboard projection includes aggregate overview, trend, model, source, and cost values; project labels, raw logs, diagnostics, cursor details, job state, behavior detail, Usage analysis, and write routes remain unavailable. Public dashboard requests also ignore `project` and `project_hash` filters so project-specific totals cannot be probed indirectly.

The public aggregate surface still has no authentication or TLS and still reveals usage totals and model/source names. Use a firewall or authenticated reverse proxy even for this reduced view. To use every local dashboard feature remotely, keep the default loopback listener and use the SSH tunnel below instead of `--public`.

SSH is also a data channel for usage import. See [CLI reference](../reference/cli.md#llmusage-remote) for `llmusage remote add` / `llmusage sync`. That pull is separate from dashboard tunneling.

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
2. Read the six summary cards for sessions, requests, token usage, estimated cost, active days, and cache-read share.
3. Check Daily activity, Weekly activity, and the daily token usage mix, then use the short-window trend for 24-hour detail.
4. Compare Highest-usage sessions with project, model, source, and cost rankings.
5. Review behavior panels for interaction activity, tool usage, optimization hints, and model comparison.
6. Use Usage analysis for ad hoc multidimensional questions about local data.
7. Open Event Logs for cursor-paginated event detail, or use sync/CSV export and diagnostics when data looks stale.

On screens up to `720px` wide, Data status becomes a compact disclosure in the first screen. Its headline follows the current worker lock and latest usage-import result, while recovered `serve` interruptions and other historical command failures remain available in runtime diagnostics without turning the current data status into a warning. The full card remains visible on wider screens. Integration installation health is no longer part of the dashboard.

## Filters

Dashboard filters map to the shared `QueryFilter` used by the Rust query layer.

| Filter | Meaning |
| --- | --- |
| `source` | `codex`, `claude`, `opencode`, `antigravity`, `kimi_code`, `pi`, or `grok` |
| `model` | Exact model string from normalized events |
| `since` / `until` | Date range for dashboard queries |
| `window` | Quick window such as day/week/month/all |
| `timezone` | `UTC`, `local`, or a fixed offset such as `+08:00`; `local` means the machine's current fixed local offset, not an IANA/DST-aware timezone |

The URL preserves filters so a refreshed page or shared local URL keeps the same view.

Antigravity CLI conversations are imported by the registered parser. Hook-era Antigravity rows remain selectable in reports and dashboard filters. A rebuild is refused while those rows have no file attribution. The IDE-side `conversations/*.pb` family stays planned.

Usage analysis adds its own query controls on top of the shared filters:

| Control | Accepted values |
| --- | --- |
| `granularity` | `total`, `day`, `week`, or `month` |
| `metric` | `attributed_cost_usd`, `calls`, `turns`, `sessions`, or `total_tokens` |
| `group_by` | `source`, `model`, `project`, `session`, `tool`, `tool_kind`, `is_tool`, or `token_type` |
| `limit` / `include_other` | Maximum result count, optionally merging the rest into `Other` |
| `session_id`, `tool_name`, `tool_kind`, `is_tool`, `token_type` | Explorer-specific filters |

## Sections

### Summary, daily activity, and trends

The six summary cards use the current filter to show sessions, requests, token
usage, estimated cost, active days, and cache-read share. The highlighted Token
usage card also names the top source. Daily activity switches between token usage
and request intensity, supports keyboard focus and tooltips, and
clicks a date to drill the global filter into that day; clicking it again
restores the previous range. The last-1-day preset hides this calendar so Weekly
activity uses the full row. Ranges of about a month or less render a labeled
day strip. The all-time range keeps the week-column calendar and stretches it
to the panel width.

The daily stacked chart separates input, cache read, cache creation, and output
tokens and includes daily cost in its tooltip. It intentionally shows an empty
state for the 24-hour range, where the existing short-window chart provides the
finer view. Live data for these panels is loaded as secondary work through the
same latest-request-wins lifecycle as Activity categories, Tool usage,
Optimization hints, Usage analysis, and Model comparison, so stale responses
cannot overwrite a newer filter.

Static HTML export stores compact summary data, up to 366 heatmap days, and the
daily series in `snapshot.json`. Older snapshots without these keys load the
panels as empty states instead of failing. The live dashboard initially loads
`/api/dashboard`; range changes, automatic refresh, and post-sync refresh use
`scope=interactive` plus independent secondary requests. A slow or degraded
secondary query does not block the first screen.

Weekly activity folds 30-minute buckets into a Monday-first `7 x 24` grid using
the browser's IANA timezone. It sits beside Daily activity on wide screens,
uses the full row when Daily activity is hidden for the last-1-day preset, and
stacks below it on narrower screens. Highest-usage sessions supports server-side
token usage, active-duration, and estimated-cost ordering. Selecting a session opens Event Logs with a
server-side session filter; expanding an event fetches its retained raw JSON on
demand. Event Logs are live-only and keep the existing 50-row cursor pagination.

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
| Activity categories | Categories such as coding, debugging, exploration, testing, and planning |
| Tool usage | Tool/action mix such as read, edit, search, shell, MCP, and sub-agent actions |
| Optimization hints | Read-only findings such as repeated reads or low Read/Edit ratio |
| Model comparison | Directional comparison between two models with sample-size warnings |

Optimization hints are advisory only. They never delete, move, archive, rewrite, or clean files.

### Usage analysis

The Usage analysis workbench is an additive panel, not a replacement for the fixed dashboard sections. It asks questions such as:

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
- `unsupported`: the selected Usage analysis metric/dimension/filter combination is not meaningful.
- source-limited facts: historical Antigravity rows and OpenCode rows can degrade to conservative turn facts when source logs do not expose tool-level evidence.

Core `/api/dashboard` data should remain responsive even when Activity categories, Tool usage, Optimization hints, Usage analysis, or Model comparison is degraded.

## CSV export and static export

The live dashboard exports the currently loaded summary, daily trends, projects,
models, sources, and Highest-usage sessions as a UTF-8 BOM CSV. Untrusted labels are
formula-neutralized before RFC-style quoting. For an offline HTML bundle, use:

```powershell
llmusage export html --out .\llmusage-report
```

The static bundle includes `snapshot.json` with the summary cards, Daily activity, Weekly activity, Highest-usage sessions, the daily token series, default Usage analysis payload, and their renderer assets. Older snapshots omit the new keys safely. Snapshot mode disables live Usage analysis controls and shows Event Logs as live-only.

## Sync jobs

Live mode can start, poll, and cancel in-process sync jobs. Jobs share the same local worker lock as CLI sync, so CLI and dashboard workers do not write concurrently.

## Live refresh and HTTP transfer

- Automatic refresh (`30s` or `60s`) and a completed sync reuse the interactive dashboard path, including custom `since`/`until` filters. Unchanged panel data is not written to the DOM again.
- Filesystem diagnostics are cached for 30 seconds at the web-request boundary and shared by `/api/diagnostics` and dashboard scopes. A completed/failed/cancelled sync or an explicit diagnostics forget invalidates the entry immediately; direct library calls to `Dashboard::diagnostics()` remain cold reads.
- Embedded CSS, JavaScript, and SVG assets return `Cache-Control: no-cache` plus a content ETag, so browsers can revalidate to `304` without stale versioned URLs. Text assets and JSON responses negotiate gzip or Brotli compression. API JSON remains uncached.

## Screenshot fixture for docs maintainers

Use the dev-only example when refreshing docs screenshots without real user data:

```powershell
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

Then capture `http://127.0.0.1:37421` at `1440×1100` and write the result to `docs/public/screenshots/web-dashboard-overview.png`.
