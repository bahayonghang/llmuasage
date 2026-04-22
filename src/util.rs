use std::{fs::Metadata, path::PathBuf};

use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use sha2::{Digest, Sha256};

pub fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
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
    format!("{:x}", hasher.finalize())
}

pub fn normalize_model(raw: Option<&str>) -> String {
    raw.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
    .unwrap_or_else(|| "unknown".to_string())
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
