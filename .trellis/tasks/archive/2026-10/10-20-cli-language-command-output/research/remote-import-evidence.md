# C remote import presentation — source frozen, execution pending linker recovery

Date: 2026-10-21. This is the owning implementer's handoff, not an independent
whole-C PASS. The coordinator requested source freeze while it runs the escalated
complete Rust gate. No actual SSH or real user artifacts were used.

## Source and private API

- `src/remote/diagnostic.rs` adds a crate-private closed `DiagnosticFacts` enum
  and `ImportFailure { error: LlmusageError, facts: Option<Box<DiagnosticFacts>> }`.
  Error Display/source remain the original error; facts contain only neutral
  host/source/version/count/exit/stderr values, never language or templates.
- `RemoteImporter::import_with_facts` is the same existing import body with
  private failure propagation and `ImportWithFacts { outcome, warnings }`.
  Public `import` unwraps the original error/outcome, keeping its exact signature
  and public `ImportOutcome` fields/canonical warnings unchanged.
- `ShardDecoder::next_record_with_facts` preserves the decoder body and supplies
  facts at the protocol/first-record decisions. Public `next_record` unwraps the
  original error. The public mismatch helper, ShardRecord, wire format and decoder
  state transitions stay unchanged.
- `common/i18n/commands/remote_import.rs` projects the 12 typed facts to paired
  copy. Shared registration was performed by the operations owner: RemoteImport
  command facts, code/render dispatch, From conversion and known ConfigInvalid
  human-cause handling. That owner also removed the two macro arms orphaned by
  these new renderers (RemoteWarning/RemoteError).
- `commands/remote.rs::sync_remotes_with_source` consumes the private result.
  It still saves `failure.error.to_string()` to host contact state at the same
  point, then attaches a marked presentation context for immediate/final/log
  display. Existing `canonical_detail` omits only that new wrapper when saving
  run details. Warning facts are created beside the unchanged canonical warning.

No lower layer reads a language or depends on commands. No duplicate scan,
validation, read, parse, request or global side channel was added. Original
validation/commit/deferred-Pi/OMP-reset/certification/watermark/session-finish and
lock/heartbeat order remains in place. No transport.rs change, public required
field, dependency/lockfile/config/symlink change, commit/install/push or release.

## Meaningful new coverage (compiled; execution not yet run)

`commands::remote::tests::remote_import_rejections_keep_canonical_errors_contact_data_and_partial_commits`
uses MemoryShardSource in separate temporary profiles for both languages. Its
eight cases cover no header, wrong first record, protocol mismatch, missing
versions, unknown source, accounting mismatch, unlisted shard and missing
trailer. Assertions retain exact canonical Display/typed error/run-detail
projection/contact data, both human sinks, log replay, event counts, absent
certification and unchanged watermark. The missing-trailer fixture expects one
already committed event, rather than weakening partial-import semantics.

`remote_import_historical_refusal_preserves_rows_watermark_and_original_cause`
seeds existing uncertified history and asserts paired refusal, exact canonical
contact/error data, unchanged row count and the original nonempty watermark.

`remote_import_warning_facts_translate_without_changing_public_outcome` exercises
the real command with a memory MOTD/header/shard/trailer stream, then checks
unchanged public canonical warning/counts, translated warning facts, idempotent
rows and the original event-derived watermark.

`remote::protocol::tests::private_decoder_facts_preserve_public_errors_and_validation_state`
compares exact public/private canonical errors for protocol/first-record cases
and retains skipped-line/header-state assertions.

`common::i18n::commands::remote_import::tests::remote_import_catalog_has_paired_strict_facts_and_visible_unknown_metadata`
covers every fact's paired copy, unique codes, stable serialization and both
normal/detailed future-field fallback retaining raw error and complete metadata.

BeforeHeader and ExitWithoutTrailer conditions are already unreachable under the
current decoder/earlier trailer check. They were deliberately left unchanged;
their new copy is projection-tested only, not claimed as an executed runtime
path. Existing remote tests remain the public protection/stream baseline.

## Actual verification

Environment: real configured toolchain bin
`C:/Users/lyh/.rustup/toolchains/1.97.0-x86_64-pc-windows-msvc/bin` before the
existing cargo bin; alternate target `output/s-language-rust-target`;
`LLMUSAGE_PRICING_REFRESH=off`; locked/offline/all-features. The cargo-bin shim
initially hit OS448; using the real configured toolchain avoided it without
repairing any link/config.

| Check | Actual result |
| --- | --- |
| `cargo check --locked --offline --all-features` | PASS after core/CLI wiring; log `remote-import-check.log`. At that snapshot an unrelated in-flight schema bootstrap dead-code warning and filesystem hardlink-copy notices were emitted; schema warning sent to owning coordinator. |
| `cargo test --locked --offline --all-features --lib remote_import -- --test-threads=1` | FAIL before test execution at MSVC linking, LNK1104 on `C:/Users/lyh/lnk{...}.tmp`; `remote-import-paired.log` retained unchanged. Rust test compilation reached linking; no test PASS is claimed. |
| `rustfmt --check --edition 2024` on all six owned source files | PASS, zero formatting diff. |
| `git diff --check` on owned tracked files | PASS; only LF/CRLF notices. |
| Full remote/architecture/strict Clippy/full Rust gate | Coordinator/independent checker owns these after source freeze; NOT_RUN by this worker. |

The checker independently observed the same sandbox linker-temp failure and the
coordinator is handling the escalated gate centrally. No duplicate retry or
product/test workaround was made. Native terminal smoke remains separate and
NOT_RUN here.

## Remaining integration boundaries

- Public importer callers such as the sync engine still intentionally get the
  canonical public error/outcome. S's human remote-skip callbacks may need to
  consume the private entry separately; this worker does not silently claim
  their dynamic reasons are now translated.
- Transport-owned failures were outside this approved file slice and remain
  original. Known importer/decoder messages are translated; arbitrary external
  ShardSource/SSH/OS details retain localized context plus original detail.
- Source hashes at freeze are in `remote-import-source-snapshot.json`; original
  four-file hashes are in `remote-import-baseline.json`.
