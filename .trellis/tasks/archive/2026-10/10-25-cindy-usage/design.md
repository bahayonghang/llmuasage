# Design: Cindy passive source

## Boundary

Add `SourceKind::Cindy` (`cindy`, display name `Cindy`) beside Pi and Oh My Pi. Register a third `PiFormatParser` instance. Do not add a parser, token contract, or cost mode. Do not add a `llmusage cindy` subcommand; Pi and Oh My Pi also do not have one. Unified reports and `--source cindy` come from the existing `SourceKind` value enum and descriptor list.

`PI_AGENT_DIR` stays Pi-only. Cindy must not read it.

## Discovery

Official roots, both always considered, missing directories empty:

- `{config_dir}/Cindy/pi-agent-home/sessions`
- `{config_dir}/CindyGlobal/pi-agent-home/sessions`

`config_dir` is `dirs::config_dir()`: Windows `%APPDATA%`, macOS `~/Library/Application Support`, Linux `$XDG_CONFIG_HOME` or `~/.config`. Match those two directory names exactly. Do not glob `Cindy*`, so `CindyDev` and `CindyGlobal-dev2-*` stay out.

`LLMUSAGE_CINDY_SESSION_DIRS`, when set and non-empty, replaces those two roots with a comma-separated list. It does not change Pi or Oh My Pi roots. Unset means the official profiles. This is the test seam and the only supported nonstandard location.

Enumerate `*.jsonl` recursively, as Pi does. Drop files whose path is inside a directory named `shared` directly under a Cindy sessions root (`sessions/shared/**`). Those are imported share transcripts.

## Ownership

After canonicalization, skip a Cindy candidate when its path overlaps any Pi root or the Oh My Pi root: equal, ancestor, or descendant. Pi and Oh My Pi keep their current rule. Cindy never steals a file. Record a listing note (`skipped N cindy session file(s) already owned by pi or omp`) and do not set `SourceSyncStats.last_error`, matching the Oh My Pi skip note.

If a user points `PI_AGENT_DIR` at a Cindy sessions directory, those files remain `pi`. That is the existing ownership rule, not a second import.

## Parse and identity

`PiFormatParser::cindy()` uses `SourceKind::Cindy` and the Cindy lister. `parse_session_file` must receive Cindy discovery roots so project resolution uses the same roots as listing. `event_key` stays `{source}:{hash}`.

Token and cost mapping stay in `parse_pi_tokens` / `parse_pi_source_cost`. Quality is `Precise`. Privacy is `LocalArtifacts`. Accounting version is `TOKEN_ACCOUNTING_VERSION` (`2`), same as Oh My Pi. There is no legacy Cindy ledger, so do not bump Pi's version `3` and do not add a schema migration. `usage_event.source` is already text.

Provider label remains `message.provider`. Session header `cwd` remains the project input.

## Registration surface

Compiler-exhaustive:

- `SourceKind` and `as_str`
- `expected_token_accounting_version`
- `source_color`
- `discovery_roots` inside the Pi parser

Hand-maintained, must include `cindy` or the source is invisible there:

- `SOURCE_DESCRIPTORS`
- `registered_parsers`, after `omp` and before `grok`
- `PlatformMonitorDescriptor` with two `ConfigDir` roots: `Cindy/pi-agent-home/sessions` and `CindyGlobal/pi-agent-home/sessions`
- `commands/remote.rs` host usage reset list
- registry parse test
- dashboard `--source-cindy` color token, light and dark, plus a project-authored `cindy.svg` because the badge catalog requires one file per registered source. Do not copy the Cindy trademark.
- docs listed in `implement.md`

`home_overview` ignores Pi and Oh My Pi in its fixed series match. Leave that match unchanged. Cindy still appears in generic source filters.

Monitor descriptors cannot express the env override. The parser lister owns `LLMUSAGE_CINDY_SESSION_DIRS`. Status may show the official config-dir roots even when the override is set; do not invent a second monitor root type for this task. Document that status roots are the official profiles.

## Compatibility

No rebuild of `pi` or `omp`. No remote migration flag. A newer sender can emit `cindy` shards; an older receiver that does not know the id should keep its current unknown-source behavior. Do not bump the crate version in this task.

Adding a public enum variant can break downstream exhaustive matches. Follow the same public-enum pattern used for `deepseek_harness`. Do not mark `SourceKind` `non_exhaustive` only for Cindy.

## Rollback

Revert the source registration. Existing `cindy` rows remain until an explicit source reset. They do not affect Pi or Oh My Pi identity.
