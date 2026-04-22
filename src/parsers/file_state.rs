use std::path::{Path, PathBuf};

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
