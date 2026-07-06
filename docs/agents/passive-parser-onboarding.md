# Passive parser onboarding gate

Use this checklist before adding any passive reader source. A passive reader does not install hooks or write third-party config; it only reads existing local artifacts.

## Required evidence

- Real local samples for normal, empty, and interrupted/error sessions.
- Sanitization note explaining whether prompts/responses are present and what was removed.
- Artifact discovery rule: path pattern, DB path, or log source.
- Cursor strategy: file fingerprint/offset, DB row id/timestamp, or cumulative snapshot delta.
- Token semantics: input, cache read, cache creation, output, reasoning, and total.
- Quality label: `precise`, `total_only`, or `estimated`.
- Privacy boundary: fields read, fields persisted, and whether raw archive is allowed.

## Required tests

- Fixture parse test from sample artifact to normalized `UsageEvent`.
- Sync-twice integration test: second sync inserts no duplicate events.
- Cursor regression for truncation/rotation/deleted file or DB rebuild.
- Rebuild/source-file guard test when the source uses file artifacts.
- Status/probe test for `passive_ready`, `passive_no_data`, `degraded_*`, or `estimated`.

## Stop rule

If samples or token semantics are missing, do not write a parser. Add only a descriptor/candidate note and mark the source blocked.
