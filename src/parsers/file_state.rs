use std::{
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    store::FileCursor,
    util::{
        metadata_modified_ns, read_head_signature, read_tail_signature, read_window_signature_at,
    },
};

const SIGNATURE_WINDOW: usize = 4096;

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

/// Wraps a `BufReader` and tracks two byte positions: the current offset
/// (advanced by every `read_line` call, including partial lines at EOF) and
/// the *complete* offset (advanced only for lines that end with `'\n'`).
///
/// JSONL source files can end with a partial line whose `'\n'` has not yet
/// been flushed by the source tool. A plain `BufReader` + manual byte
/// counter would advance past that partial line on the first sync, permanently
/// skipping it on every subsequent run. `BoundedJsonlReader` avoids this by
/// letting callers use `complete_offset()` — not the total bytes consumed —
/// as the durable cursor; the partial tail is re-read next sync and picked up
/// once the file has been written completely.
pub struct BoundedJsonlReader<R: Read> {
    inner: BufReader<R>,
    complete_offset: u64,
    current_offset: u64,
}

impl<R: Read + Seek> BoundedJsonlReader<R> {
    /// Creates a new reader, seeking to `start_offset` before the first read.
    pub fn new(reader: R, start_offset: u64) -> Result<Self> {
        let mut inner = BufReader::new(reader);
        inner.seek(SeekFrom::Start(start_offset))?;
        Ok(Self {
            inner,
            complete_offset: start_offset,
            current_offset: start_offset,
        })
    }

    /// The byte offset of the start of the next line to be read.
    ///
    /// Capture this value *before* calling `read_line` when you need a stable
    /// `record_offset` (e.g. as a component of an event key).
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

    /// Reads the next line into `buf` (does *not* clear `buf` before reading,
    /// matching the `BufRead::read_line` contract). Returns the number of
    /// bytes appended, or `0` at EOF.
    ///
    /// Advances `complete_offset` only when the line ends with `'\n'`.
    pub fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let bytes_read = self.inner.read_line(buf)?;
        self.current_offset += bytes_read as u64;
        if bytes_read > 0 && buf.ends_with('\n') {
            self.complete_offset = self.current_offset;
        }
        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn reader(s: &str) -> BoundedJsonlReader<Cursor<Vec<u8>>> {
        BoundedJsonlReader::new(Cursor::new(s.as_bytes().to_vec()), 0).expect("new reader")
    }

    #[test]
    fn complete_lines_advance_complete_offset() {
        let mut r = reader("line1\nline2\n");
        let mut buf = String::new();

        r.read_line(&mut buf).unwrap();
        assert_eq!(r.complete_offset(), 6); // "line1\n"

        buf.clear();
        r.read_line(&mut buf).unwrap();
        assert_eq!(r.complete_offset(), 12); // "line1\nline2\n"
    }

    #[test]
    fn partial_tail_does_not_advance_complete_offset() {
        let mut r = reader("complete\npartial");
        let mut buf = String::new();

        r.read_line(&mut buf).unwrap(); // "complete\n"
        assert_eq!(r.complete_offset(), 9);

        buf.clear();
        r.read_line(&mut buf).unwrap(); // "partial" — no newline
                                        // complete_offset must NOT advance past the partial line
        assert_eq!(r.complete_offset(), 9);
        assert_eq!(r.current_offset(), 16); // current consumed all bytes
    }

    #[test]
    fn eof_returns_zero_and_offsets_are_stable() {
        let mut r = reader("done\n");
        let mut buf = String::new();
        r.read_line(&mut buf).unwrap();
        let before = r.complete_offset();
        buf.clear();
        assert_eq!(r.read_line(&mut buf).unwrap(), 0);
        assert_eq!(r.complete_offset(), before); // stable at EOF
    }

    #[test]
    fn start_offset_is_reflected_in_both_positions() {
        let content = "skip\nkeep\n";
        let start = 5u64; // position of "keep\n"
        let mut r = BoundedJsonlReader::new(Cursor::new(content.as_bytes().to_vec()), start)
            .expect("new reader");
        assert_eq!(r.current_offset(), 5);
        assert_eq!(r.complete_offset(), 5);
        let mut buf = String::new();
        r.read_line(&mut buf).unwrap();
        assert_eq!(buf, "keep\n");
        assert_eq!(r.complete_offset(), 10);
    }
}
