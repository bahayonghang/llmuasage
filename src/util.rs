use std::{
    collections::hash_map::DefaultHasher,
    fs::{File, Metadata},
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use sha2::{Digest, Sha256};

pub fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Higher-resolution UTC timestamp for internal sync ordering.
///
/// Sync runs may fire back-to-back inside the same wall-clock second (e.g.
/// during integration tests or rapid `--rebuild` cycles). The `source_file`
/// state machine compares `last_seen_at` against the run's `run_started_at`
/// to flip stale `live` rows to `missing`; second-resolution loses fidelity
/// when two runs share the same second. Use millisecond resolution for
/// internal timestamps that participate in such comparisons.
pub fn now_utc_millis() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn resolve_home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn bucket_start(timestamp: DateTime<Utc>) -> String {
    let minute = if timestamp.minute() >= 30 { 30 } else { 0 };
    timestamp
        .with_minute(minute)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(timestamp)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn bucket_start_from_rfc3339(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| bucket_start(value.with_timezone(&Utc)))
}

pub fn hash_string(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    digest_to_hex(hasher.finalize())
}

pub fn normalize_model(raw: Option<&str>) -> String {
    raw.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
    .unwrap_or_else(|| "unknown".to_string())
}

pub fn metadata_modified_ns(metadata: &Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

pub fn read_head_signature(path: &Path, window_size: usize) -> std::io::Result<String> {
    let file_size = std::fs::metadata(path)?.len();
    let length = file_size.min(window_size as u64) as usize;
    read_window_signature(path, 0, length)
}

pub fn read_tail_signature(path: &Path, window_size: usize) -> std::io::Result<String> {
    let file_size = std::fs::metadata(path)?.len();
    read_window_signature_at(path, file_size, window_size)
}

pub fn read_window_signature_at(
    path: &Path,
    end_offset: u64,
    window_size: usize,
) -> std::io::Result<String> {
    let start_offset = end_offset.saturating_sub(window_size as u64);
    let length = end_offset.saturating_sub(start_offset) as usize;
    read_window_signature(path, start_offset, length)
}

fn read_window_signature(
    path: &Path,
    start_offset: u64,
    window_size: usize,
) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start_offset))?;

    let mut buffer = vec![0u8; window_size];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);

    let mut hasher = Sha256::new();
    hasher.update((start_offset as usize).to_le_bytes());
    hasher.update(buffer);
    Ok(digest_to_hex(hasher.finalize()))
}

fn digest_to_hex(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn metadata_inode(metadata: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return metadata.ino();
    }

    #[cfg(windows)]
    {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs())
            .unwrap_or_default();
        return metadata.len() ^ modified;
    }

    #[allow(unreachable_code)]
    {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs())
            .unwrap_or_default();
        metadata.len() ^ modified
    }
}

pub fn file_identity(path: &Path) -> std::io::Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let mut hasher = DefaultHasher::new();
    metadata_inode(&metadata).hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata_modified_ns(&metadata).hash(&mut hasher);
    read_head_signature(path, 256)?.hash(&mut hasher);
    Ok(hasher.finish() & i64::MAX as u64)
}
