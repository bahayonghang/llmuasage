# ADR 0011 - Passive-only synchronization

- Status: Accepted
- Date: 2026-07-27
- Supersedes: ADR 0001 section 5; ADR 0008 activation modes, integration registry, and hook-run consequences; ADR 0009's Antigravity integration-only transport
- Related code: `src/commands/init.rs`, `src/commands/uninstall.rs`, `src/domain/source_descriptor.rs`, `src/integrations/`, `src/registry.rs`
- Related terms: Source, SourceParser, SourceDescriptor, Platform Monitor, Store

## Context

llmusage previously imported usage through two paths: passive parsing during
`sync`, and tool-installed hooks/plugins that invoked a hidden `hook-run`
command. The active path modified third-party configuration, required wrapper
scripts and trigger coordination, and exposed installation state throughout
doctor, diagnostics, TUI, and dashboard payloads.

Codex, Claude, and OpenCode already have passive parsers. Kimi Code, Pi, and
Grok Build are passive readers. Antigravity is the only persisted source
without a verified token-bearing passive artifact, so removing its hook
transport stops new Antigravity events while leaving historical rows useful.

## Decision

Use passive parsing as the only usage-import mechanism.

- `sync` walks registered parsers for Codex, Claude, OpenCode, Kimi Code, Pi,
  and Grok Build. `init` only prepares the runtime root and bootstraps SQLite.
- Remove hook/plugin installation, probing, wrapper generation, `hook-run`,
  trigger-state writes, integration capability fields, and integration health
  presentation.
- Keep Antigravity's stable source id and descriptor so historical database
  rows remain queryable. Its source status is derived as `historical_only`; its
  separate platform monitor remains monitor-only and `blocked_no_samples`.
- Keep `uninstall` as legacy cleanup. It removes only llmusage-owned Claude,
  Codex, Antigravity/legacy Gemini, and OpenCode artifacts plus exact atomic
  write residue. It preserves sibling user entries and historical backups.
  Cleanup writes an `integration_install` audit row only after an actual change
  or on failure; a no-op writes none. `--purge` retains its explicit runtime-root
  deletion behavior.
- Do not rewrite migrations or delete historical tables/rows. The
  `trigger_state` and `integration_install` tables, `HolderKind::Hook`, and
  historical `hook-run` run-log query labels remain readable for old databases.

## Security and recovery note

The removed `src/integrations/hook_target.rs` contained the SEC-002 hardened
platform command quoting used when installing executable commands into
third-party configuration. If command installation is ever reintroduced,
recover and review that implementation from Git history rather than rebuilding
quoting ad hoc. Legacy cleanup itself matches stable llmusage ownership markers
and does not construct new executable command strings.

## Consequences

- Third-party tools no longer invoke llmusage automatically. Users run `sync`
  directly or through the in-process dashboard job surface.
- Machines upgraded from hook-enabled releases should run `llmusage uninstall`
  once to remove legacy entries and wrappers.
- Antigravity history remains visible in reports and dashboards but does not
  grow until a separately evidenced passive parser is approved.
- New databases still contain historical compatibility tables because existing
  migrations are immutable. Their negligible empty-table cost avoids migration
  history risk.

## Rejected alternatives

- Keep hooks as an optional fast path: rejected because it preserves the
  third-party mutation, trigger coordination, and health surface this decision
  removes.
- Guess an Antigravity passive parser: rejected because no accepted
  token-bearing fixture or cursor contract exists.
- Drop historical tables or remove `hook-run` query labels: rejected because it
  would change old-database behavior and recent-sync reporting.
- Remove `uninstall`: rejected because upgraded machines need a safe path to
  detach artifacts installed by older releases.

## Verification

- `init` tests assert that third-party configuration is unchanged.
- Legacy-cleanup tests cover historical quoting variants, user-entry
  preservation, Codex marker consumption, OpenCode ownership markers, atomic
  residue, backup preservation, audit rules, and second-run idempotency.
- Old-database tests retain `trigger_state`, `integration_install`, historical
  `hook-run` run-log rows, and `holder_kind='hook'` compatibility.
- Source-status tests assert Antigravity `historical_only` and monitor-only
  `blocked_no_samples` behavior.
- Grok tests assert parser-backed `total_only` status, fixed-depth sidecar
  discovery, session replay, and `unpriced` cost.
- Dashboard/TUI payload tests assert integration health is absent.
