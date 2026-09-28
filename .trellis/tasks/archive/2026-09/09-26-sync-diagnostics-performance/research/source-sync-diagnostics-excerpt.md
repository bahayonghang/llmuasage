# Source Sync contract excerpt

Authoritative source: `.trellis/spec/llmusage/backend/source-sync-contracts.md`.
SHA-256 at implementation refresh: `80f4d0a2245c000044ed5773cb2993dae946ad6acda12fc8cf7acd24aeb62eff`.
Quoted source lines follow. This is a research snapshot; current source takes precedence if the source changes. Read omitted sections when the implementation scope needs them.

## Source lines 394-476

L394: ### 6. Tests Required
L395: 
L396: - Parser stats tests for changed and unchanged sync runs, including
L397:   `skipped_files`.
L398: - Backward-compatible serde/default tests when adding optional stats fields.
L399: - Query/TUI payload tests when a stats field becomes visible in dashboard or
L400:   terminal panels.
L401: - Registry/monitor tests proving monitored platforms do not accidentally become
L402:   parser-backed sources.
L403: - Descriptor/status tests proving CLI and IDE have distinct passive parser
L404:   capabilities, filters and coverage; CLI readiness never implies IDE readiness.
L405: - Behavior-level rebuild coverage proving explicit Antigravity repair replaces
L406:   attributed parser history atomically and retains unattributed hook facts,
L407:   including failed reads, rollback and reopened retained-history warnings.
L408: - Report and dashboard projection coverage proving historical Antigravity rows
L409:   remain aggregated and selectable.
L410: - Kimi, Pi, Oh My Pi, and Grok fixture tests covering normalized fields, raw/future model ids,
L411:   malformed/non-usage rows, second-sync idempotency, append, rewrite/truncate,
L412:   deleted history/rebuild protection, missing roots, and status projections.
L413:   Pi/Oh My Pi listing tests cover disjoint roots and the three overlap shapes.
L414: - Sync-summary unit/subprocess tests covering the `TOTAL` row, absent and empty
L415:   sources, ANSI-free redirected output, stderr/stdout separation, removed
L416:   completion sentences, and narrow/wide column budgets.
L417: - Claude multi-project tests proving unchanged projects remain skipped while
L418:   cross-file streaming/sidechain dedupe inside the changed project is stable.
L419: - Codex append tests asserting only the changed file and appended byte range are
L420:   scanned.
L421: - OpenCode growth/replacement and part high-water tests covering hot zero-row,
L422:   one-row append, closed upper bounds, and idempotent replacement replay.
L423:   OpenCode source DB open is read-only with a non-zero busy timeout; malformed
L424:   tool-part JSON increments `parse_issues.malformed_lines`; OpenCode/ZCode
L425:   SQLite cursors commit in the same `commit_shard` transaction as their events.
L426: - Table-driven CLI, Web, and public `JobRegistry::try_start` tests asserting
L427:   the same validation codes and no job creation for invalid input.
L428: - Architecture tests scan `sync`/`remote -> commands`, every non-command
L429:   layer's dependency on `commands::sync`, and sync-owned trait/type impls
L430:   placed in `commands`; each policy has a negative fixture.
L431: - Remote accounting tests drive the shipped importer, protocol
L432:   encode/decode, and `source-status`. Missing/mismatched Header versions
L433:   refuse before the first related shard commit. Local current plus remote
L434:   unknown/old fixtures print accurate labels. Existing remote rows without
L435:   a trusted marker refuse current-version incremental mix-in. Empty source
L436:   plus no `since` plus success may establish a host/source marker that
L437:   survives Store reopen; mid-failure does not. Matching-version replay,
L438:   cross-host isolation, OMP/Pi migration, and missing-trailer watermark
L439:   rules stay green. No live SSH. No history restore.
L440: - Recent-window regressions asserting event-time filtering, an old file with a
L441:   recent append, no bounded cursor/reset writes, later full-history recovery,
L442:   OpenCode SQL lower-bound pruning, and `RecentReady` ordering.
L443: - Migration/query-plan tests proving behavior reset indexes exist; writer tests
L444:   proving shard-local behavior dedupe and shared-bucket pricing recovery.
L445: - Writer performance comparisons restore the same immutable seed before every
L446:   measured run. Build the seed with the original algorithm. Keep fixture
L447:   generation, input cloning, database copying, and checkpointing outside the
L448:   timed operation; report writer open/finish separately from shard WRITE.
L449:   Keep detailed stage collection test-only and disable it for acceptance A/B.
L450: - Freeze fixtures, pair counts, alternating order, and acceptance metrics
L451:   before measuring a candidate. Keep all samples and failed controls. A ratio
L452:   of medians and a median of paired ratios are different statistics; do not
L453:   change the acceptance metric after a failure. Profile a failed control
L454:   separately before assigning a cause. Aggregate timing alone cannot assign
L455:   a regression to a query, cache behavior, or background load.
L456: - Compare all persistent tables and complete SQLite schema before accepting a
L457:   writer candidate. Only cost columns allow a documented absolute tolerance;
L458:   other values are exact. Include untouched source/host/status sentinels,
L459:   duplicate replay, shared buckets, failure rollback, and cancellation.
L460:   Path-index changes need a skewed multi-host control because source path
L461:   hashes are shared across hosts and the existing path index omits host_id.
L462: - Single-path reset tests assert no selectivity stage and only default-plan
L463:   selections, including repeated entries of the same path. Review the lazy
L464:   statement preparation to confirm unused path-index statements are skipped.
L465:   Multi-path tests assert the adaptive route, path/default-plan decisions,
L466:   and unchanged persistent state.
L467: - Human and subprocess tests covering pricing phase text, ordered additive
L468:   NDJSON variants, stdout purity, and structured log phase fields.
L469: - A multi-thread `#[tokio::test]` covering the TUI sync action, duplicate-start
L470:   cancellation, progress text projection, and bounded shutdown behavior.
L471: - Command-center regressions covering recovered abort plus later successful
L472:   sync with rebuild risk (ready/good headline, success last_run, preserved
L473:   risk facts), usage-import last-run surviving serve-row noise, and failed
L474:   last-run pairing the failed headline with `lastRunFailed` even when rebuild
L475:   risk is also present.
L476: 

## Source lines 524-768

L524: ## Scenario: Bounded Passive JSONL Records And Cooperative Cancellation
L525: 
L526: ### 1. Scope / Trigger
L527: 
L528: - Trigger: any Codex, Claude, Kimi Code, Pi, or Grok JSONL read loop, parse issue
L529:   projection, file cursor update, or blocking parser cancellation change.
L530: - The shared reader owns byte bounds, JSON decoding, durable record boundaries,
L531:   privacy-safe issues, and cancellation polling. Source parsers own only the
L532:   decoded JSON-to-domain mapping.
L533: 
L534: ### 2. Signatures
L535: 
L536: - `BoundedJsonlReader::new(reader, start_offset)` uses a 4 MiB maximum record
L537:   size; tests may use `with_limit` for smaller boundaries.
L538: - `read_json_records(source, path_hash, cancel, issues, callback)` passes
L539:   `JsonlRecord { start_offset, end_offset, value }` to the source callback.
L540:   `read_json_records_with_oversized` additionally exposes the bounded 4 MiB
L541:   prefix so a parser can recover or reclassify an oversized record.
L542: - Domain-owned `ParseIssues { malformed_lines, oversized_lines, skipped_lines,
L543:   accounting_anomaly_lines, samples }` is embedded in `SourceSyncStats` and
L544:   `SourceSyncStatus` with serde defaults for the two new counters. Parser
L545:   modules may re-export it but storage must not depend on parser modules.
L546:   `total()` is malformed + oversized (faults). `informational_total()` is
L547:   skipped + accounting_anomaly. Record diagnostics make doctor warn only when
L548:   `total() > 0`; a separate source failure also makes doctor warn.
L549: - `ParseIssueSample` includes `reason` (serde default empty). `record`
L550:   sanitizes reason to at most 64 characters in `[A-Za-z0-9_:-]`.
L551: - `ParseIssueKind` is `malformed`, `oversized`, `skipped`, or
L552:   `accounting_anomaly`. The four classes are mutually exclusive.
L553: - Schema v17 persists the latest bounded diagnostic payload in
L554:   `source_sync_status.parse_issues_json TEXT NOT NULL`.
L555: - A crate-private persisted DTO flattens the existing `ParseIssues` fields and
L556:   adds optional `source_issues`. Each source issue has a closed-set code, count,
L557:   UTC observation time, and product-group scope. It contains no raw path,
L558:   record body, or free-text error. Keep public diagnostic and sync-status
L559:   struct literals compatible; no schema migration is required.
L560: 
L561: ### 3. Contracts
L562: 
L563: - The reader searches for newlines through `BufRead::fill_buf`; it buffers at
L564:   most 4 MiB of record content and discards the rest of an oversized record in
L565:   bounded chunks through the next newline.
L566: - `complete_offset` advances only after a newline record boundary. A
L567:   syntactically complete JSON value at EOF may still produce an event for
L568:   compatibility, but it cannot advance the durable cursor until its newline is
L569:   observed. A truncated EOF value produces neither an event nor a malformed
L570:   issue.
L571: - Cancellation is checked before each buffered read/discard chunk and between
L572:   files. Async parser loops must await every spawned blocking handle in the
L573:   current batch before returning; a cancelled batch is drained and not
L574:   committed.
L575: - Malformed, oversized, skipped, and accounting-anomaly samples contain only
L576:   source id, bounded path hash, byte offset, issue kind, and an optional
L577:   closed-set reason. Raw JSON, prompts, assistant content, full paths,
L578:   `error_message`, and raw row ids are forbidden in samples, human summaries,
L579:   and logs. CLI sample lines print kind and reason when present. `@offset` is
L580:   printed whenever `offset > 0` for JSONL, including samples with a reason.
L581:   ZCode's existing offset stores a millisecond timestamp, so print
L582:   `timestamp_ms=<offset>` instead. Do not change stored location values or label
L583:   a timestamp as a byte offset. OpenCode's offset is a private database rowid;
L584:   omit that value from human output. Optional
L585:   basename may follow when a file cursor can resolve it. They never print
L586:   `path_hash` or record text.
L587: - At most eight samples are retained per source run. Counters continue with
L588:   saturating arithmetic after the sample budget is exhausted. The human summary
L589:   prints the combined diagnostic count and the omitted sample count. Each
L590:   diagnostic class counts separately; one record can produce multiple accounting
L591:   anomalies. These counters do not count distinct physical records.
L592: - Codex classifies an oversized prefix from the first 8 KiB of payload/msg
L593:   type: other types are skipped; a complete `token_count` JSON prefix (trailing
L594:   whitespace allowed) is recovered with no issue; a `token_count` prefix that
L595:   cannot be parsed stays oversized. Peek-none (unclassified junk) stays
L596:   oversized, not skipped. The stable skip reason is `oversized_non_usage_record`.
L597: - ZCode `error`/`cancelled` rows are skipped. The unfinished reason is
L598:   `zcode_unfinished:{status}:{error_type}` where status is
L599:   `error`/`cancelled`/`other` and `error_type` comes from
L600:   `model_usage.error_type` when the column exists and matches
L601:   `[A-Za-z0-9_-]`; otherwise `unknown`. Never read `error_message`. Cache
L602:   overlap and `computed_total` mismatch are accounting anomalies; events
L603:   still store.
L604: - Antigravity discovery, fingerprint, missing tracked member, snapshot-change,
L605:   database metadata access, and incomplete snapshot failures use source issues.
L606:   Coverage codes are `tracked_member_missing` for a confirmed absent local
L607:   path, `tracked_member_out_of_scope` for an existing path outside discovered
L608:   input, and `tracked_member_unreadable` for metadata/access failure. Each
L609:   code carries an affected-path count, UTC observation time, and product-group
L610:   scope. `discovery_incomplete` also covers inaccessible or invalid roots.
L611:   A metadata access error remains a blocker even with lossy rebuild consent.
L612:   A source failure must not create a fabricated malformed record. Actual row
L613:   decode and timestamp errors retain their record-level diagnostics. Neither
L614:   failure class may reset imported events or advance the file cursor. An output
L615:   checksum mismatch or conflicting observations fails the complete product
L616:   snapshot; conflicting observations also record an accounting anomaly. Missing
L617:   all request identities uses a file+location fallback with an accounting
L618:   anomaly; a missing response ID alone can use message/provider ID.
L619: - Source issues persist through status updates, query diagnostics, and remote
L620:   transport. All clean-success and accounting certification checks must examine
L621:   source issues as well as record faults. A successful new source run clears
L622:   prior source failures. A stored inventory state describes its completed
L623:   observation; it must not be presented as a current filesystem existence check.
L624: - Grok sidecars over the size cap stay oversized; bad sidecar JSON stays
L625:   malformed. OpenCode records malformed tool-part JSON as `malformed_lines`
L626:   and continues other parts; non-usage message rows stay silent.
L627: - Sync human summary prints every non-zero class (`malformed=`, `oversized=`,
L628:   `skipped=`, `accounting=`). Warning color is only for malformed/oversized.
L629: - Interactive dashboard `SyncSourcePayload` carries the four counters and
L630:   never parse-issue samples. Source cards and the TUI Usage wide table show
L631:   non-zero counts from those fields, not by parsing human summary strings.
L632: 
L633: ### 4. Validation & Error Matrix
L634: 
L635: - Record exceeds 4 MiB and reaches newline -> the wrapper path increments
L636:   `oversized_lines`; a source-specific oversized callback may instead recover
L637:   usage (`Accepted`), count `skipped_lines`, or count `malformed_lines`, then
L638:   advance to that boundary and continue with the next record.
L639: - Complete record is invalid JSON -> increment `malformed_lines`, skip it, and
L640:   continue without failing the file.
L641: - Complete non-usage JSONL rows that a parser ignores are not parse issues.
L642: - ZCode unfinished rows that are new relative to both watermarks ->
L643:   `skipped_lines` plus a reason sample. A later unchanged sync of the same
L644:   unfinished rows -> `skipped_lines == 0`, no samples, and no new
L645:   parse-issue info event. Token-channel inconsistency with a stored event ->
L646:   `accounting_anomaly_lines`.
L647: - EOF contains invalid/incomplete JSON -> keep the prior durable offset and do
L648:   not count malformed until a record boundary exists.
L649: - Cancellation during ordinary read or oversized discard -> stop without a
L650:   cursor for the interrupted file; drain every blocking worker before terminal
L651:   cancellation.
L652: - Persisted issue JSON is invalid -> diagnostics/status loading fails as a
L653:   SQLite conversion error instead of silently inventing clean counters.
L654: - Old `parse_issues_json` without `skipped_lines` / `accounting_anomaly_lines`
L655:   deserializes those counters as `0`. Old samples without `reason`
L656:   deserialize `reason` as `""`.
L657: - Old diagnostic JSON without `source_issues` loads an empty source-failure list.
L658:   Every typed read/write path in the current binary preserves the optional
L659:   field. A source-only failure has zero record faults but still blocks clean
L660:   repair certification.
L661: - Doctor `parse.issues` is `ok` when every source `total() == 0`, even if
L662:   skipped or accounting-anomaly counts are non-zero.
L663: - Antigravity conversation DB open failure, `sqlite_master` probe failure, or
L664:   `gen_metadata` prepare failure other than a missing table -> a source issue,
L665:   no fabricated malformed record, no `reset_path_hashes`, no success cursor,
L666:   prior `usage_event` rows kept.
L667: - Antigravity missing `gen_metadata` table -> read `steps` when present. Only
L668:   a complete successful product snapshot may reset its group. Empty recognized
L669:   usage tables are valid; a DB missing both tables or containing a malformed
L670:   usage table preserves history.
L671: - Antigravity row decode, wire-type, timestamp, and output-checksum faults
L672:   remain malformed record diagnostics with stable reasons. Attach private typed
L673:   row context before propagating decode errors; do not classify by matching
L674:   free-text error messages. Product-attribution and SQL access failures remain
L675:   source issues. Both classes block the affected product-group commit and keep
L676:   events, buckets, cursors, inventory, and accounting markers unchanged.
L677:   Attribute each row fault to the native product carried by typed row context.
L678:   Affected historical owners retain separate source blockers; the row-fault
L679:   count belongs to the native product only.
L680: 
L681: ### 5. Good/Base/Bad Cases
L682: 
L683: - Good: a 10 MiB line yields one oversized issue while the in-memory record
L684:   buffer stays at or below 4 MiB, then the following valid line is parsed.
L685: - Base: valid newline-delimited records advance `complete_offset` and emit the
L686:   same source events as before.
L687: - Good: a valid EOF record is visible immediately but is retried from the prior
L688:   durable boundary; event keys/store dedupe keep the retry idempotent.
L689: - Good: a missing tracked Antigravity member records a source issue with count
L690:   and observation time, persists it across restart, and preserves prior usage.
L691: - Bad: converting a source-access error into a malformed sample or treating
L692:   zero record faults as sufficient evidence for clean accounting certification.
L693: - Bad: `BufRead::read_line`, `lines()`, or a source-local `serde_json::from_str`
L694:   loop in a passive JSONL parser.
L695: - Bad: dropping `JoinHandle`s when cancellation is observed; blocking tasks
L696:   continue consuming CPU/I/O after the job reports cancelled.
L697: 
L698: ### 6. Tests Required
L699: 
L700: - Shared Codex/Claude/Kimi/Pi/Grok contract harness: 10 MiB oversized line,
L701:   malformed line with secret content, UTF-8 record, EOF tail, identical issue
L702:   counters, safe samples, and durable offset. The 10 MiB junk-line prefix
L703:   (`x` bytes) remains oversized, not skipped.
L704: - Reader unit tests: maximum buffered bytes, discard continuation, malformed
L705:   privacy, mid-discard cancellation, start offsets, EOF stability, and
L706:   oversized-prefix reclassification (`Skipped` / recovered / oversized).
L707: - Codex tests: 10 MiB non-`token_count` -> skipped + later rows parse;
L708:   complete `token_count` prefix padded with whitespace -> event and zero
L709:   issues; unusable `token_count` prefix -> oversized + later rows parse.
L710: - Per-source partial-tail/append tests plus `tests/sync/sources/` for
L711:   rewrite, retry, idempotency, and stored totals.
L712: - Antigravity row/source classification fixtures cover generation/step/trajectory
L713:   protobuf, invalid usage wire types, timestamp nanos, missing timestamps, output
L714:   checksum mismatch, and SQL prepare failure. Start from positive history; verify
L715:   all persisted usage/cursor/inventory state is preserved, clean certification
L716:   does not occur, diagnostics survive restart, and repaired input clears them.
L717:   Include a copied database whose native product differs from its historical
L718:   owner, with a source-limited sync, to verify row attribution and protection.
L719: - Antigravity preflight tests count actual decoder invocations for CLI-only,
L720:   IDE-only, and both-selected all-blocked runs. Compare persisted events,
L721:   buckets, inventory timestamps, cursors, and markers before and after the
L722:   failure. Verify diagnostics survive restart and restored input recovers.
L723:   Cover existing JSON, changed roots, metadata permission errors, non-directory
L724:   roots, incomplete discovery, and an inaccessible unselected root. Retain
L725:   partial blocking, copied product ownership, stronger unselected identity,
L726:   WAL-only changes, bounded/cancelled runs, hook history, and explicit rebuild
L727:   with and without lossy consent.
L728: - Sync-summary, doctor, and source-status tests covering four-class counters,
L729:   warning color only for faults, CLI samples without `path_hash`/record text
L730:   or `@0`, JSONL reason and `@offset` together, ZCode `timestamp_ms=`,
L731:   OpenCode rowid suppression, 13 diagnostics / 8 samples / 5 omitted, and
L732:   doctor skipping skipped-only sources while warning on source failures.
L733: - ZCode skip-watermark tests covering first-sighting reasons, a second
L734:   unchanged sync with `skipped_lines == 0`, a newer unfinished row reported
L735:   once, `--recent-days` not advancing the skip watermark, cancel after the
L736:   first page save not persisting the skip watermark, and rebuild resetting
L737:   both watermarks.
L738: - Query/dashboard/TUI tests covering `SyncSourcePayload` four counters, no
L739:   samples in interactive JSON, source-card counts, and Usage Issues column.
L740: - JobRegistry test: status remains `cancelling` and `finished_at` stays absent
L741:   until a blocking worker confirms drain.
L742: - Migration/status tests: v17 default payload and `ParseIssues` round trip,
L743:   including missing new fields deserializing as zero. Source-only diagnostics
L744:   survive all status updates and remote transport; a successful new run clears
L745:   old source issues. Remote source failures block repair certification.
L746: - Antigravity: unreadable rewrite after a successful import keeps event count
L747:   and the previous cursor fingerprint; missing `gen_metadata` retains steps
L748:   support; prepare error on an existing table does not reset the group.
L749: 
L750: ### 7. Wrong vs Correct
L751: 
L752: #### Wrong
L753: 
L754: ```rust
L755: let mut line = String::new();
L756: while reader.read_line(&mut line)? != 0 {
L757:     let Ok(value) = serde_json::from_str(&line) else { continue };
L758:     parse_source_value(value)?;
L759: }
L760: ```
L761: 
L762: #### Correct
L763: 
L764: ```rust
L765: reader.read_json_records(source, path_hash, cancel, &mut issues, |record| {
L766:     parse_source_value(record.value)
L767: })?;
L768: ```
