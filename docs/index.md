---
layout: home

hero:
  name: "llmusage"
  text: "Local-first analytics for AI coding CLIs"
  tagline: "Track Codex, Claude, and OpenCode with hooks, SQLite, and zero upload."
  actions:
    - theme: brand
      text: Getting Started
      link: /guide/getting-started
    - theme: alt
      text: 中文文档
      link: /zh/

features:
  - title: Local-only data path
    details: Hooks and plugins trigger local parsing. No login, no sync service, no remote API.
  - title: SQLite as the single source of truth
    details: Cursors, usage events, 30-minute buckets, integration state, and run logs all live in one local database.
  - title: One query model, three surfaces
    details: The same query layer powers the browser dashboard, TUI view, and static HTML export.
---

## What ships in v1

- Codex `notify` integration
- Claude `Stop` and `SessionEnd` hooks
- OpenCode `session.updated` plugin
- Local Web UI on `127.0.0.1`
- TUI status surface
- Offline HTML export

## Development workflow

Use the root `justfile`:

```powershell
just install
just build
just docs
just ci
```
