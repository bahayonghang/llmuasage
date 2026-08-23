---
layout: home

hero:
  name: "llmusage"
  text: "Local-first analytics for AI coding CLIs"
  tagline: "Track Codex, Claude, OpenCode, Antigravity, Kimi Code, Pi, and Grok Build with local artifacts, SQLite, and zero upload."
  actions:
    - theme: brand
      text: Start the guide
      link: /guide/getting-started
    - theme: alt
      text: Open the dashboard docs
      link: /dashboard/
    - theme: alt
      text: 中文文档
      link: /zh/

features:
  - title: Local-only data path
    details: Sync passively reads local artifacts. No hooks, plugins, login, sync service, or remote usage API. SSH remote import is a user-owned pull of normalized fields between machines you already control.
  - title: SQLite as the source of truth
    details: Cursors, usage events, 30-minute buckets, behavior facts, source-file diagnostics, and run logs live in one local database.
  - title: One query model, four surfaces
    details: The same query layer powers reports, llmusage dash, llmusage serve, and export html.
---

## Choose your task

| Task | Start here |
| --- | --- |
| Install and initialize the local database | [Install and initialize](./guide/install-and-init) |
| Import local usage | [First sync](./guide/first-sync) |
| Read token and cost reports | [First report](./guide/first-report) |
| Inspect Codex-only call details | [Codex Tracer](./guide/codex-tracer) |
| Use the browser dashboard | [Dashboard](./dashboard/) |
| Export a static report | [Export HTML](./guide/export-html) |
| Check destructive boundaries | [Safety](./safety/) |
| Look up exact flags | [CLI reference](./reference/cli) |

## Dashboard preview

`llmusage serve` starts a dashboard on `127.0.0.1` by default; `--public` explicitly enables remote listening.

![llmusage web dashboard overview](/screenshots/web-dashboard-overview.png)

<small>Sanitized local fixture served by `llmusage serve`; not real user data.</small>

## Current product surface

- Version `1.3.0`.
- Sources: passive Codex, Claude Code, OpenCode, Kimi Code (`kimi_code`), Pi (`pi`), Oh My Pi (`omp`), Grok Build (`grok`), ZCode (`zcode`), Antigravity CLI (`antigravity`), and DeepSeek Harness (`deepseek_harness`).
- Report commands: `daily`, `monthly`, `session`, `blocks`, `statusline`.
- Local UI commands: `dash`, `serve`, `export html`.
- Safety commands: `status`, `diagnostics`, `doctor`, `uninstall`.

## Development workflow

```powershell
just install
just build
just docs
just ci
```
