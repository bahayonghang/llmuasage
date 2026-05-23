---
layout: home

hero:
  name: "llmusage"
  text: "Local-first analytics for AI coding CLIs"
  tagline: "Track Codex, Claude, OpenCode, and Google Antigravity/Gemini with hooks, SQLite, and zero upload."
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
    details: Hooks and plugins trigger local parsing. No login, no sync service, no remote usage API.
  - title: SQLite as the source of truth
    details: Cursors, usage events, 30-minute buckets, behavior facts, source-file diagnostics, and run logs live in one local database.
  - title: One query model, four surfaces
    details: The same query layer powers reports, llmusage dash, llmusage serve, and export html.
---

## Choose your task

| Task | Start here |
| --- | --- |
| Install and initialize local hooks | [Install and initialize](./guide/install-and-init) |
| Import local usage | [First sync](./guide/first-sync) |
| Read token and cost reports | [First report](./guide/first-report) |
| Use the browser dashboard | [Dashboard](./dashboard/) |
| Export a static report | [Export HTML](./guide/export-html) |
| Check destructive boundaries | [Safety](./safety/) |
| Look up exact flags | [CLI reference](./reference/cli) |

## Dashboard preview

`llmusage serve` starts a local dashboard on `127.0.0.1`.

![llmusage web dashboard overview](/screenshots/web-dashboard-overview.png)

<small>Sanitized local fixture served by `llmusage serve`; not real user data.</small>

## Current product surface

- Version `0.6.5`.
- Sources: Codex, Claude Code, OpenCode, Google Antigravity/Gemini (`gemini` stable id).
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
