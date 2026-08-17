use std::{
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    models::{ParseIssueKind, ParseIssues, SourceKind},
    store::FileCursor,
    util::{
        metadata_modified_ns, read_head_signature, read_tail_signature, read_window_signature_at,
    },
};

const SIGNATURE_WINDOW: usize = 4096;
pub const DEFAULT_MAX_JSONL_RECORD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CandidateFile {
    pub path: PathBuf,
    pub existing: Option<FileCursor>,
}

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub file_size: u64,
    pub file_mtime_ns: i64,
    pub file_fingerprint: String,
    pub tail_signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileReplayMode {
    Append,
    Reparse,
}

#[derive(Debug, Clone)]
pub struct FileDecision {
    pub snapshot: FileSnapshot,
    pub start_offset: u64,
    pub replay_mode: FileReplayMode,
}

pub fn should_rescan_file(path: &Path, existing: Option<&FileCursor>) -> Result<bool> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();
    let file_mtime_ns = metadata_modified_ns(&metadata);

    let Some(existing) = existing else {
        return Ok(true);
    };

    if existing.offset < existing.file_size {
        return Ok(true);
    }

    Ok(existing.file_size != file_size || existing.file_mtime_ns != file_mtime_ns)
}

pub fn decide_file_replay(candidate: CandidateFile) -> Result<FileDecision> {
    let snapshot = capture_file_snapshot(&candidate.path)?;
    let Some(existing) = candidate.existing.as_ref() else {
        return Ok(FileDecision {
            snapshot,
            start_offset: 0,
            replay_mode: FileReplayMode::Reparse,
        });
    };

    if existing.file_size == snapshot.file_size
        && existing.file_mtime_ns == snapshot.file_mtime_ns
        && existing.offset == existing.file_size
        && existing.file_fingerprint == snapshot.file_fingerprint
        && existing.tail_signature == snapshot.tail_signature
    {
        return Ok(FileDecision {
            snapshot,
            start_offset: existing.offset,
            replay_mode: FileReplayMode::Append,
        });
    }

    if snapshot.file_size >= existing.offset
        && existing.tail_signature
            == read_window_signature_at(&candidate.path, existing.offset, SIGNATURE_WINDOW)?
    {
        return Ok(FileDecision {
            snapshot,
            start_offset: existing.offset,
            replay_mode: FileReplayMode::Append,
        });
    }

    Ok(FileDecision {
        snapshot,
        start_offset: 0,
        replay_mode: FileReplayMode::Reparse,
    })
}

pub fn finalize_cursor(
    path: &Path,
    snapshot: &FileSnapshot,
    offset: u64,
    last_total: Option<crate::models::UsageTokens>,
    last_model: Option<String>,
) -> FileCursor {
    FileCursor {
        cursor_key: path.to_string_lossy().to_string(),
        file_path: path.to_string_lossy().to_string(),
        file_fingerprint: snapshot.file_fingerprint.clone(),
        file_size: snapshot.file_size,
        file_mtime_ns: snapshot.file_mtime_ns,
        tail_signature: snapshot.tail_signature.clone(),
        offset,
        last_total,
        last_model,
        updated_at: crate::util::now_utc(),
    }
}

fn capture_file_snapshot(path: &Path) -> Result<FileSnapshot> {
    let metadata = std::fs::metadata(path)?;
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        file_size: metadata.len(),
        file_mtime_ns: metadata_modified_ns(&metadata),
        file_fingerprint: read_head_signature(path, SIGNATURE_WINDOW)?,
        tail_signature: read_tail_signature(path, SIGNATURE_WINDOW)?,
    })
}

/// One decoded JSON value with physical byte offsets.
///
/// A syntactically complete value at EOF may be delivered before its newline is
/// flushed, but `complete_offset()` still remains at the preceding record boundary.
pub struct JsonlRecord {
    pub start_offset: u64,
    pub end_offset: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonlRecordDisposition {
    Accepted,
    Ignored,
    Skipped,
    Malformed,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonlReadStatus {
    Complete,
    Cancelled,
    Stopped,
}

enum RecordRead {
    Complete { start_offset: u64, end_offset: u64 },
    Oversized { start_offset: u64 },
    PartialTail { start_offset: u64, oversized: bool },
    Eof,
    Cancelled,
}

/// Bounded JSONL decoder with durable complete-record cursor semantics.
///
/// JSONL source files can end with a partial line whose `'\n'` has not yet
/// been flushed by the source tool. A plain `BufReader` + manual byte
/// counter would advance past that partial line on the first sync, permanently
/// skipping it on every subsequent run. `BoundedJsonlReader` avoids this by
/// letting callers use `complete_offset()` — not the total bytes consumed —
/// as the durable cursor; the partial tail is re-read next sync and picked up
/// once the file has been written completely. Records larger than the configured
/// limit are discarded in bounded chunks until the next newline.
pub struct BoundedJsonlReader<R: Read> {
    inner: BufReader<R>,
    complete_offset: u64,
    current_offset: u64,
    max_record_bytes: usize,
    record: Vec<u8>,
    max_buffered_bytes: usize,
}

impl<R: Read> BoundedJsonlReader<R> {
    /// Creates a reader over a non-seekable stream starting at byte 0.
    ///
    /// Use this for decoded frames (zstd) where the physical file offset is
    /// not a durable JSONL cursor. Seekable files should keep using [`Self::new`].
    pub fn from_read(reader: R) -> Self {
        Self::from_read_with_limit(reader, DEFAULT_MAX_JSONL_RECORD_BYTES)
    }

    pub fn from_read_with_limit(reader: R, max_record_bytes: usize) -> Self {
        Self {
            inner: BufReader::new(reader),
            complete_offset: 0,
            current_offset: 0,
            max_record_bytes,
            record: Vec::new(),
            max_buffered_bytes: 0,
        }
    }

    /// The byte offset of the start of the next line to be read.
    ///
    /// Capture this value before reading when you need a stable per-record byte
    /// position (for example, as a component of an event key).
    pub fn current_offset(&self) -> u64 {
        self.current_offset
    }

    /// The byte offset after the last `'\n'`-terminated line.
    ///
    /// Store this as the durable cursor. A partial tail line at EOF is not
    /// included, so it will be re-read on the next sync.
    pub fn complete_offset(&self) -> u64 {
        self.complete_offset
    }

    pub fn read_json_records<F>(
        &mut self,
        source: SourceKind,
        path_hash: &str,
        cancel: &CancellationToken,
        issues: &mut ParseIssues,
        callback: F,
    ) -> Result<JsonlReadStatus>
    where
        F: FnMut(JsonlRecord) -> Result<JsonlRecordDisposition>,
    {
        self.read_json_records_with_oversized(
            source,
            path_hash,
            cancel,
            issues,
            callback,
            |_, _| Ok(JsonlRecordDisposition::Ignored),
        )
    }

    /// Like [`Self::read_json_records`], but exposes the bounded oversized
    /// prefix so a source parser can recover or reclassify the record.
    ///
    /// Oversized dispositions:
    /// - `Accepted`: recovered usage, no issue
    /// - `Skipped`: skipped counter
    /// - `Malformed`: malformed counter
    /// - `Ignored`: oversized (the default wrapper path)
    /// - `Stop`: stop reading
    pub fn read_json_records_with_oversized<F, O>(
        &mut self,
        source: SourceKind,
        path_hash: &str,
        cancel: &CancellationToken,
        issues: &mut ParseIssues,
        mut callback: F,
        mut oversized_callback: O,
    ) -> Result<JsonlReadStatus>
    where
        F: FnMut(JsonlRecord) -> Result<JsonlRecordDisposition>,
        O: FnMut(&[u8], u64) -> Result<JsonlRecordDisposition>,
    {
        loop {
            match self.read_record(cancel)? {
                RecordRead::Complete {
                    start_offset,
                    end_offset,
                } => {
                    let value = match serde_json::from_slice::<Value>(&self.record) {
                        Ok(value) => value,
                        Err(_) => {
                            issues.record(
                                source,
                                path_hash,
                                start_offset,
                                ParseIssueKind::Malformed,
                                "",
                            );
                            continue;
                        }
                    };
                    match callback(JsonlRecord {
                        start_offset,
                        end_offset,
                        value,
                    })? {
                        JsonlRecordDisposition::Accepted | JsonlRecordDisposition::Ignored => {}
                        JsonlRecordDisposition::Skipped => issues.record(
                            source,
                            path_hash,
                            start_offset,
                            ParseIssueKind::Skipped,
                            "",
                        ),
                        JsonlRecordDisposition::Malformed => issues.record(
                            source,
                            path_hash,
                            start_offset,
                            ParseIssueKind::Malformed,
                            "",
                        ),
                        JsonlRecordDisposition::Stop => return Ok(JsonlReadStatus::Stopped),
                    }
                }
                RecordRead::Oversized { start_offset } => {
                    match oversized_callback(&self.record, start_offset)? {
                        JsonlRecordDisposition::Accepted => {}
                        JsonlRecordDisposition::Skipped => issues.record(
                            source,
                            path_hash,
                            start_offset,
                            ParseIssueKind::Skipped,
                            "",
                        ),
                        JsonlRecordDisposition::Malformed => issues.record(
                            source,
                            path_hash,
                            start_offset,
                            ParseIssueKind::Malformed,
                            "",
                        ),
                        JsonlRecordDisposition::Ignored => issues.record(
                            source,
                            path_hash,
                            start_offset,
                            ParseIssueKind::Oversized,
                            "",
                        ),
                        JsonlRecordDisposition::Stop => return Ok(JsonlReadStatus::Stopped),
                    }
                }
                RecordRead::PartialTail {
                    start_offset,
                    oversized,
                } => {
                    if !oversized && let Ok(value) = serde_json::from_slice::<Value>(&self.record) {
                        let disposition = callback(JsonlRecord {
                            start_offset,
                            end_offset: self.current_offset,
                            value,
                        })?;
                        if disposition == JsonlRecordDisposition::Stop {
                            return Ok(JsonlReadStatus::Stopped);
                        }
                    }
                    return Ok(JsonlReadStatus::Complete);
                }
                RecordRead::Eof => return Ok(JsonlReadStatus::Complete),
                RecordRead::Cancelled => return Ok(JsonlReadStatus::Cancelled),
            }
        }
    }

    fn read_record(&mut self, cancel: &CancellationToken) -> std::io::Result<RecordRead> {
        if cancel.is_cancelled() {
            return Ok(RecordRead::Cancelled);
        }

        let start_offset = self.current_offset;
        self.record.clear();
        let mut oversized = false;

        loop {
            if cancel.is_cancelled() {
                return Ok(RecordRead::Cancelled);
            }
            let available = self.inner.fill_buf()?;
            if available.is_empty() {
                return Ok(if self.current_offset == start_offset {
                    RecordRead::Eof
                } else {
                    RecordRead::PartialTail {
                        start_offset,
                        oversized,
                    }
                });
            }

            let newline = available.iter().position(|byte| *byte == b'\n');
            let content_len = newline.unwrap_or(available.len());
            if !oversized {
                let remaining = self.max_record_bytes.saturating_sub(self.record.len());
                let append_len = remaining.min(content_len);
                self.record.extend_from_slice(&available[..append_len]);
                self.max_buffered_bytes = self.max_buffered_bytes.max(self.record.len());
                oversized = content_len > remaining;
            }

            let consumed = content_len + usize::from(newline.is_some());
            self.inner.consume(consumed);
            self.current_offset = self.current_offset.saturating_add(consumed as u64);
            if newline.is_some() {
                self.complete_offset = self.current_offset;
                return Ok(if oversized {
                    RecordRead::Oversized { start_offset }
                } else {
                    RecordRead::Complete {
                        start_offset,
                        end_offset: self.current_offset,
                    }
                });
            }
        }
    }
}

impl<R: Read + Seek> BoundedJsonlReader<R> {
    /// Creates a new reader, seeking to `start_offset` before the first read.
    pub fn new(reader: R, start_offset: u64) -> Result<Self> {
        Self::with_limit(reader, start_offset, DEFAULT_MAX_JSONL_RECORD_BYTES)
    }

    pub fn with_limit(reader: R, start_offset: u64, max_record_bytes: usize) -> Result<Self> {
        let mut inner = BufReader::new(reader);
        inner.seek(SeekFrom::Start(start_offset))?;
        Ok(Self {
            inner,
            complete_offset: start_offset,
            current_offset: start_offset,
            max_record_bytes,
            record: Vec::new(),
            max_buffered_bytes: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read, Seek, SeekFrom},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use super::*;
    use crate::domain::models::{MAX_PARSE_ISSUE_SAMPLES, MAX_PATH_HASH_CHARS};

    fn reader(s: &str) -> BoundedJsonlReader<Cursor<Vec<u8>>> {
        BoundedJsonlReader::new(Cursor::new(s.as_bytes().to_vec()), 0).expect("new reader")
    }

    fn collect(reader: &mut BoundedJsonlReader<Cursor<Vec<u8>>>) -> (Vec<Value>, ParseIssues) {
        let mut values = Vec::new();
        let mut issues = ParseIssues::default();
        let status = reader
            .read_json_records(
                SourceKind::Codex,
                "safe-path-hash",
                &CancellationToken::new(),
                &mut issues,
                |record| {
                    values.push(record.value);
                    Ok(JsonlRecordDisposition::Accepted)
                },
            )
            .expect("read json records");
        assert_eq!(status, JsonlReadStatus::Complete);
        (values, issues)
    }

    #[test]
    fn complete_lines_advance_complete_offset() {
        let mut r = reader("{\"line\":1}\n{\"line\":2}\n");
        let (values, issues) = collect(&mut r);
        assert_eq!(values.len(), 2);
        assert_eq!(issues.total(), 0);
        assert_eq!(r.complete_offset(), r.current_offset());
    }

    #[test]
    fn partial_tail_does_not_advance_complete_offset() {
        let complete = "{\"complete\":true}\n";
        let mut r = reader(&format!("{complete}{{\"partial\":"));
        let (values, issues) = collect(&mut r);
        assert_eq!(values.len(), 1);
        assert_eq!(issues.total(), 0);
        assert_eq!(r.complete_offset(), complete.len() as u64);
        assert!(r.current_offset() > r.complete_offset());
    }

    #[test]
    fn partial_tail_split_inside_utf8_code_point_retries_from_record_start() {
        let complete = b"{\"complete\":true}\n";
        let multibyte = "界".as_bytes();
        let mut partial = complete.to_vec();
        partial.extend_from_slice(b"{\"text\":\"");
        partial.extend_from_slice(&multibyte[..1]);

        let mut first =
            BoundedJsonlReader::new(Cursor::new(partial.clone()), 0).expect("new partial reader");
        let (values, issues) = collect(&mut first);
        assert_eq!(values, vec![serde_json::json!({"complete": true})]);
        assert_eq!(issues.total(), 0);
        assert_eq!(first.complete_offset(), complete.len() as u64);

        partial.extend_from_slice(&multibyte[1..]);
        partial.extend_from_slice(b"\"}\n");
        let mut retry =
            BoundedJsonlReader::new(Cursor::new(partial.clone()), first.complete_offset())
                .expect("new retry reader");
        let (values, issues) = collect(&mut retry);
        assert_eq!(values, vec![serde_json::json!({"text": "界"})]);
        assert_eq!(issues.total(), 0);
        assert_eq!(retry.complete_offset(), partial.len() as u64);
    }

    #[test]
    fn exact_limit_eof_record_becomes_durable_when_newline_arrives() {
        let limit = 64usize;
        let record = format!("{{\"v\":\"{}\"}}", "x".repeat(limit - 8));
        assert_eq!(record.len(), limit);

        let mut first =
            BoundedJsonlReader::with_limit(Cursor::new(record.as_bytes().to_vec()), 0, limit)
                .expect("new partial reader");
        let (values, issues) = collect(&mut first);
        assert_eq!(values.len(), 1);
        assert_eq!(issues.total(), 0);
        assert_eq!(first.complete_offset(), 0);
        assert_eq!(first.max_buffered_bytes, limit);

        let completed = format!("{record}\n");
        let mut retry = BoundedJsonlReader::with_limit(
            Cursor::new(completed.as_bytes().to_vec()),
            first.complete_offset(),
            limit,
        )
        .expect("new retry reader");
        let (values, issues) = collect(&mut retry);
        assert_eq!(values.len(), 1);
        assert_eq!(issues.total(), 0);
        assert_eq!(retry.complete_offset(), completed.len() as u64);
        assert_eq!(retry.max_buffered_bytes, limit);
    }

    #[test]
    fn eof_returns_zero_and_offsets_are_stable() {
        let mut r = reader("{\"done\":true}\n");
        let _ = collect(&mut r);
        let before = r.complete_offset();
        let _ = collect(&mut r);
        assert_eq!(r.complete_offset(), before); // stable at EOF
    }

    #[test]
    fn start_offset_is_reflected_in_both_positions() {
        let content = "{\"skip\":true}\n{\"keep\":true}\n";
        let start = 14u64;
        let mut r = BoundedJsonlReader::new(Cursor::new(content.as_bytes().to_vec()), start)
            .expect("new reader");
        assert_eq!(r.current_offset(), start);
        assert_eq!(r.complete_offset(), start);
        let (values, _) = collect(&mut r);
        assert_eq!(values, vec![serde_json::json!({"keep": true})]);
        assert_eq!(r.complete_offset(), content.len() as u64);
    }

    #[test]
    fn oversized_record_is_discarded_without_buffering_the_full_line() {
        let limit = DEFAULT_MAX_JSONL_RECORD_BYTES;
        let mut content = vec![b'x'; 10 * 1024 * 1024];
        content.extend_from_slice(b"\n{\"after\":true}\n");
        let mut r = BoundedJsonlReader::with_limit(Cursor::new(content.clone()), 0, limit)
            .expect("new reader");
        let (values, issues) = collect(&mut r);

        assert_eq!(issues.oversized_lines, 1);
        assert_eq!(issues.malformed_lines, 0);
        assert_eq!(values, vec![serde_json::json!({"after": true})]);
        assert!(r.max_buffered_bytes <= limit);
        assert_eq!(r.complete_offset(), content.len() as u64);
    }

    #[test]
    fn oversized_prefix_callback_can_reclassify_and_keeps_the_bound() {
        let limit = 64usize;
        let mut content = b"{\"type\":\"skip-me\",".to_vec();
        content.extend(std::iter::repeat_n(b'x', limit));
        content.extend_from_slice(b"}\n{\"after\":true}\n");
        let mut r = BoundedJsonlReader::with_limit(Cursor::new(content.clone()), 0, limit)
            .expect("new reader");
        let mut values = Vec::new();
        let mut issues = ParseIssues::default();
        let mut prefixes = Vec::new();
        let status = r
            .read_json_records_with_oversized(
                SourceKind::Codex,
                "safe-path-hash",
                &CancellationToken::new(),
                &mut issues,
                |record| {
                    values.push(record.value);
                    Ok(JsonlRecordDisposition::Accepted)
                },
                |prefix, start_offset| {
                    prefixes.push((start_offset, prefix.len()));
                    Ok(JsonlRecordDisposition::Skipped)
                },
            )
            .expect("read json records");

        assert_eq!(status, JsonlReadStatus::Complete);
        assert_eq!(issues.skipped_lines, 1);
        assert_eq!(issues.oversized_lines, 0);
        assert_eq!(issues.malformed_lines, 0);
        assert_eq!(values, vec![serde_json::json!({"after": true})]);
        assert_eq!(prefixes, vec![(0, limit)]);
        assert!(r.max_buffered_bytes <= limit);
        assert_eq!(r.complete_offset(), content.len() as u64);
    }

    #[test]
    fn oversized_prefix_accepted_does_not_count_an_issue() {
        let limit = 64usize;
        let mut content = b"{\"type\":\"token_count\",".to_vec();
        content.extend(std::iter::repeat_n(b'x', limit));
        content.extend_from_slice(b"}\n{\"after\":true}\n");
        let mut r = BoundedJsonlReader::with_limit(Cursor::new(content.clone()), 0, limit)
            .expect("new reader");
        let mut issues = ParseIssues::default();
        let status = r
            .read_json_records_with_oversized(
                SourceKind::Codex,
                "safe-path-hash",
                &CancellationToken::new(),
                &mut issues,
                |_| Ok(JsonlRecordDisposition::Accepted),
                |_, _| Ok(JsonlRecordDisposition::Accepted),
            )
            .expect("read json records");

        assert_eq!(status, JsonlReadStatus::Complete);
        assert_eq!(issues.total(), 0);
        assert_eq!(issues.informational_total(), 0);
        assert!(r.max_buffered_bytes <= limit);
        assert_eq!(r.complete_offset(), content.len() as u64);
    }

    #[test]
    fn malformed_samples_do_not_include_record_contents() {
        let secret = "private prompt must not leak";
        let mut r = reader(&format!("{{\"prompt\":\"{secret}\"\n"));
        let (_, issues) = collect(&mut r);
        let encoded = serde_json::to_string(&issues).expect("serialize issues");

        assert_eq!(issues.malformed_lines, 1);
        assert_eq!(issues.samples.len(), 1);
        assert!(!encoded.contains(secret));
        assert_eq!(issues.samples[0].path_hash, "safe-path-hash");
        assert_eq!(issues.samples[0].offset, 0);
    }

    #[test]
    fn issue_sample_count_and_path_hash_are_bounded() {
        let content = "{not-json}\n".repeat(10);
        let mut r = reader(&content);
        let mut issues = ParseIssues::default();
        let long_hash = "x".repeat(MAX_PATH_HASH_CHARS + 20);
        r.read_json_records(
            SourceKind::Codex,
            &long_hash,
            &CancellationToken::new(),
            &mut issues,
            |_| Ok(JsonlRecordDisposition::Ignored),
        )
        .expect("read malformed records");

        assert_eq!(issues.malformed_lines, 10);
        assert_eq!(issues.samples.len(), MAX_PARSE_ISSUE_SAMPLES);
        assert!(
            issues
                .samples
                .iter()
                .all(|sample| sample.path_hash.chars().count() == MAX_PATH_HASH_CHARS)
        );
    }

    #[test]
    fn cancellation_stops_an_oversized_discard_before_record_boundary() {
        struct SlowReader {
            inner: Cursor<Vec<u8>>,
            bytes_read: Arc<AtomicUsize>,
        }

        impl Read for SlowReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(Duration::from_millis(1));
                let limit = buf.len().min(4096);
                let read = self.inner.read(&mut buf[..limit])?;
                self.bytes_read.fetch_add(read, Ordering::Relaxed);
                Ok(read)
            }
        }

        impl Seek for SlowReader {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                self.inner.seek(pos)
            }
        }

        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let bytes_read = Arc::new(AtomicUsize::new(0));
        let observed_bytes = Arc::clone(&bytes_read);
        let canceller = std::thread::spawn(move || {
            while observed_bytes.load(Ordering::Relaxed) < 32 * 1024 {
                std::thread::yield_now();
            }
            worker_cancel.cancel();
        });
        let content = vec![b'x'; 10 * 1024 * 1024];
        let reader = SlowReader {
            inner: Cursor::new(content.clone()),
            bytes_read,
        };
        let mut r = BoundedJsonlReader::new(reader, 0).expect("new reader");
        let mut issues = ParseIssues::default();
        let started = Instant::now();
        let status = r
            .read_json_records(
                SourceKind::Codex,
                "safe-path-hash",
                &cancel,
                &mut issues,
                |_| Ok(JsonlRecordDisposition::Ignored),
            )
            .expect("cancelled read");
        canceller.join().expect("canceller thread");

        assert_eq!(status, JsonlReadStatus::Cancelled);
        assert_eq!(r.complete_offset(), 0);
        assert_eq!(issues.total(), 0);
        assert!(r.current_offset() < content.len() as u64);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
