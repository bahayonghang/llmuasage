use std::{
    cell::Cell,
    fs::{self, File, OpenOptions, Permissions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use thiserror::Error;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const RECOVERY_PRESENT: &[u8] = b"present\n";
const RECOVERY_ABSENT: &[u8] = b"absent\n";

#[derive(Debug, Error)]
pub enum AtomicConfigError {
    #[error("atomic config {stage} failed for {path}: {source}")]
    Io {
        stage: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("action recording failed for {target}; the external file was restored: {source}")]
    ActionRecordRolledBack {
        target: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    #[error(
        "atomic config operation failed for {target} and recovery failed; pending recovery is at {recovery_path}: operation={operation_error:#}; recovery={recovery_error:#}"
    )]
    RecoveryFailed {
        target: PathBuf,
        recovery_path: PathBuf,
        operation_error: anyhow::Error,
        recovery_error: anyhow::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailPoint {
    Write,
    Flush,
    Replace,
}

/// Crash-safe sibling-file writer used for third-party integration configs.
///
/// A deterministic pending marker and recovery file make an interrupted
/// operation discoverable on the next attempt. Windows replaces an existing
/// target with `ReplaceFileW`; it never removes the target first.
struct AtomicConfigWriter<'a> {
    target: &'a Path,
    fail_at: Cell<Option<FailPoint>>,
}

impl<'a> AtomicConfigWriter<'a> {
    fn new(target: &'a Path) -> Self {
        Self {
            target,
            fail_at: Cell::new(None),
        }
    }

    #[cfg(test)]
    fn with_failpoint(target: &'a Path, fail_at: FailPoint) -> Self {
        Self {
            target,
            fail_at: Cell::new(Some(fail_at)),
        }
    }

    fn write(&self, contents: &[u8]) -> Result<()> {
        self.prepare_recovery()?;
        match self.replace_inner(contents, self.target_permissions()) {
            Ok(()) => self.clear_recovery(),
            Err(error) => self.restore_after_failure(error),
        }
    }

    fn write_and_record<F>(&self, contents: &[u8], record: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.prepare_recovery()?;
        if let Err(error) = self.replace_inner(contents, self.target_permissions()) {
            return self.restore_after_failure(error);
        }

        match record() {
            Ok(()) => self.clear_recovery(),
            Err(record_error) => match self.recover_pending() {
                Ok(()) => Err(AtomicConfigError::ActionRecordRolledBack {
                    target: self.target.to_path_buf(),
                    source: record_error,
                }
                .into()),
                Err(recovery_error) => Err(AtomicConfigError::RecoveryFailed {
                    target: self.target.to_path_buf(),
                    recovery_path: self.recovery_path(),
                    operation_error: record_error,
                    recovery_error,
                }
                .into()),
            },
        }
    }

    fn remove_and_record<F>(&self, record: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.prepare_recovery()?;
        let remove_result = if self.target.exists() {
            self.hit(FailPoint::Replace, "remove")
                .and_then(|()| {
                    fs::remove_file(self.target)
                        .map_err(|error| anyhow::Error::from(self.io_error("remove", error)))
                })
                .and_then(|()| self.sync_parent())
        } else {
            Ok(())
        };
        if let Err(error) = remove_result {
            return self.restore_after_failure(error);
        }

        match record() {
            Ok(()) => self.clear_recovery(),
            Err(record_error) => match self.recover_pending() {
                Ok(()) => Err(AtomicConfigError::ActionRecordRolledBack {
                    target: self.target.to_path_buf(),
                    source: record_error,
                }
                .into()),
                Err(recovery_error) => Err(AtomicConfigError::RecoveryFailed {
                    target: self.target.to_path_buf(),
                    recovery_path: self.recovery_path(),
                    operation_error: record_error,
                    recovery_error,
                }
                .into()),
            },
        }
    }

    fn prepare_recovery(&self) -> Result<()> {
        if let Some(parent) = self.target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| self.io_error_at("create parent", parent, error))?;
        }
        self.recover_pending()?;
        let recovery_path = self.recovery_path();
        if recovery_path.exists() {
            fs::remove_file(&recovery_path).map_err(|error| {
                self.io_error_at("remove orphan recovery", &recovery_path, error)
            })?;
        }

        let target_exists = self.target.is_file();
        if target_exists {
            let result = (|| -> Result<()> {
                let mut source = File::open(self.target)
                    .map_err(|error| self.io_error("open recovery source", error))?;
                let mut recovery = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&recovery_path)
                    .map_err(|error| self.io_error_at("create recovery", &recovery_path, error))?;
                std::io::copy(&mut source, &mut recovery)
                    .map_err(|error| self.io_error_at("copy recovery", &recovery_path, error))?;
                if let Some(permissions) = self.target_permissions() {
                    fs::set_permissions(&recovery_path, permissions).map_err(|error| {
                        self.io_error_at("set recovery permissions", &recovery_path, error)
                    })?;
                }
                recovery
                    .sync_all()
                    .map_err(|error| self.io_error_at("flush recovery", &recovery_path, error))?;
                Ok(())
            })();
            if let Err(error) = result {
                let _ = fs::remove_file(&recovery_path);
                return Err(error);
            }
        }

        let marker_path = self.marker_path();
        let marker_result = (|| -> Result<()> {
            let mut marker = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker_path)
                .map_err(|error| self.io_error_at("create pending marker", &marker_path, error))?;
            marker
                .write_all(if target_exists {
                    RECOVERY_PRESENT
                } else {
                    RECOVERY_ABSENT
                })
                .map_err(|error| self.io_error_at("write pending marker", &marker_path, error))?;
            marker
                .sync_all()
                .map_err(|error| self.io_error_at("flush pending marker", &marker_path, error))?;
            self.sync_parent()
        })();
        if let Err(error) = marker_result {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&recovery_path);
            return Err(error);
        }
        Ok(())
    }

    fn recover_pending(&self) -> Result<()> {
        let marker_path = self.marker_path();
        let recovery_path = self.recovery_path();
        if !marker_path.exists() {
            return Ok(());
        }

        let marker = fs::read(&marker_path)
            .map_err(|error| self.io_error_at("read pending marker", &marker_path, error))?;
        if marker == RECOVERY_PRESENT {
            let mut recovery = File::open(&recovery_path)
                .map_err(|error| self.io_error_at("open recovery", &recovery_path, error))?;
            let permissions = recovery
                .metadata()
                .ok()
                .map(|metadata| metadata.permissions());
            let mut contents = Vec::new();
            recovery
                .read_to_end(&mut contents)
                .map_err(|error| self.io_error_at("read recovery", &recovery_path, error))?;
            self.replace_inner(&contents, permissions)?;
        } else if marker == RECOVERY_ABSENT {
            if self.target.exists() {
                fs::remove_file(self.target)
                    .map_err(|error| self.io_error("remove incomplete target", error))?;
                self.sync_parent()?;
            }
        } else {
            return Err(self
                .io_error_at(
                    "parse pending marker",
                    &marker_path,
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "unknown atomic recovery marker",
                    ),
                )
                .into());
        }
        self.clear_recovery()
    }

    fn replace_inner(&self, contents: &[u8], permissions: Option<Permissions>) -> Result<()> {
        let parent = self.target.parent().ok_or_else(|| {
            self.io_error("resolve parent", std::io::ErrorKind::InvalidInput.into())
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| self.io_error_at("create parent", parent, error))?;
        let (temp_path, mut temp) = self.create_temp_file()?;

        let result = (|| -> Result<()> {
            self.hit(FailPoint::Write, "write")?;
            temp.write_all(contents)
                .map_err(|error| self.io_error_at("write", &temp_path, error))?;
            if let Some(permissions) = permissions {
                fs::set_permissions(&temp_path, permissions)
                    .map_err(|error| self.io_error_at("preserve permissions", &temp_path, error))?;
            }
            self.hit(FailPoint::Flush, "flush")?;
            temp.sync_all()
                .map_err(|error| self.io_error_at("flush", &temp_path, error))?;
            drop(temp);
            self.hit(FailPoint::Replace, "replace")?;
            replace_file(&temp_path, self.target)
                .map_err(|error| self.io_error("replace", error))?;
            self.sync_parent()
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    fn create_temp_file(&self) -> Result<(PathBuf, File)> {
        let file_name = self
            .target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "llmusage-config".to_string());
        let parent = self.target.parent().unwrap_or_else(|| Path::new("."));
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        for _ in 0..1_000 {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".{file_name}.llmusage-tmp.{}.{nanos}.{counter}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(self.io_error_at("create temp", &path, error).into());
                }
            }
        }
        Err(self
            .io_error(
                "create temp",
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "exhausted 1000 sibling temp names",
                ),
            )
            .into())
    }

    fn restore_after_failure(&self, operation_error: anyhow::Error) -> Result<()> {
        match self.recover_pending() {
            Ok(()) => Err(operation_error),
            Err(recovery_error) => Err(AtomicConfigError::RecoveryFailed {
                target: self.target.to_path_buf(),
                recovery_path: self.recovery_path(),
                operation_error,
                recovery_error,
            }
            .into()),
        }
    }

    fn clear_recovery(&self) -> Result<()> {
        let marker_path = self.marker_path();
        if marker_path.exists() {
            fs::remove_file(&marker_path)
                .map_err(|error| self.io_error_at("remove pending marker", &marker_path, error))?;
        }
        let recovery_path = self.recovery_path();
        if recovery_path.exists() {
            fs::remove_file(&recovery_path)
                .map_err(|error| self.io_error_at("remove recovery", &recovery_path, error))?;
        }
        self.sync_parent()
    }

    fn marker_path(&self) -> PathBuf {
        sibling_control_path(self.target, "pending")
    }

    fn recovery_path(&self) -> PathBuf {
        sibling_control_path(self.target, "recovery")
    }

    fn target_permissions(&self) -> Option<Permissions> {
        fs::metadata(self.target)
            .ok()
            .map(|metadata| metadata.permissions())
    }

    fn sync_parent(&self) -> Result<()> {
        sync_parent(self.target).map_err(|error| self.io_error("sync parent", error).into())
    }

    fn hit(&self, point: FailPoint, stage: &'static str) -> Result<()> {
        if self.fail_at.get() == Some(point) {
            self.fail_at.set(None);
            return Err(self
                .io_error(
                    stage,
                    std::io::Error::other(format!("injected {stage} failure")),
                )
                .into());
        }
        Ok(())
    }

    fn io_error(&self, stage: &'static str, source: std::io::Error) -> AtomicConfigError {
        self.io_error_at(stage, self.target, source)
    }

    fn io_error_at(
        &self,
        stage: &'static str,
        path: &Path,
        source: std::io::Error,
    ) -> AtomicConfigError {
        AtomicConfigError::Io {
            stage,
            path: path.to_path_buf(),
            source,
        }
    }
}

fn sibling_control_path(target: &Path, suffix: &str) -> PathBuf {
    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "llmusage-config".to_string());
    target.with_file_name(format!(".{file_name}.llmusage-{suffix}"))
}

#[cfg(unix)]
fn sync_parent(target: &Path) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(windows)]
fn sync_parent(_target: &Path) -> std::io::Result<()> {
    // ReplaceFileW uses WRITE_THROUGH. Rust cannot open a directory for sync on
    // Windows without another handle-level API, so there is no separate parent
    // directory fsync here.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent(_target: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};

    if !target.exists() {
        return fs::rename(temp, target);
    }

    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let temp_wide = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are valid, NUL-terminated UTF-16 buffers that remain
    // alive for the call. Optional pointers are null as required by Win32.
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            temp_wide.as_ptr(),
            ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            ptr::null(),
            ptr::null(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(temp, target)
}

pub fn write_file_atomic(target: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    AtomicConfigWriter::new(target).write(contents.as_ref())
}

pub fn write_file_atomic_and_record<F>(
    target: &Path,
    contents: impl AsRef<[u8]>,
    record: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    AtomicConfigWriter::new(target).write_and_record(contents.as_ref(), record)
}

pub fn remove_file_atomic_and_record<F>(target: &Path, record: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    AtomicConfigWriter::new(target).remove_and_record(record)
}

/// Restores an interrupted atomic operation, then removes exact sibling
/// residue left by older llmusage integration writes.
pub fn recover_and_cleanup_residue(target: &Path) -> Result<bool> {
    let writer = AtomicConfigWriter::new(target);
    let marker_path = writer.marker_path();
    let recovery_path = writer.recovery_path();
    let mut changed = marker_path.exists() || recovery_path.exists();

    writer.recover_pending()?;

    for control_path in [&marker_path, &recovery_path] {
        if control_path.exists() {
            fs::remove_file(control_path)?;
            changed = true;
        }
    }

    let Some(parent) = target.parent() else {
        return Ok(changed);
    };
    let Some(file_name) = target.file_name().and_then(|name| name.to_str()) else {
        return Ok(changed);
    };
    let temp_prefix = format!(".{file_name}.llmusage-tmp.");
    if parent.is_dir() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&temp_prefix))
            {
                fs::remove_file(entry.path())?;
                changed = true;
            }
        }
    }
    if changed {
        writer.sync_parent()?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn assert_no_atomic_residue(directory: &Path) {
        let leftovers = fs::read_dir(directory)
            .expect("read temp directory")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("llmusage-"))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "atomic residue: {leftovers:?}");
    }

    #[test]
    fn failpoints_leave_existing_target_complete() {
        for point in [FailPoint::Write, FailPoint::Flush, FailPoint::Replace] {
            let temp = TempDir::new().expect("temp dir");
            let target = temp.path().join("config.json");
            fs::write(&target, b"old").expect("seed target");

            let result = AtomicConfigWriter::with_failpoint(&target, point).write(b"new");

            assert!(result.is_err(), "{point:?} should fail");
            assert_eq!(fs::read(&target).expect("read target"), b"old");
            assert_no_atomic_residue(temp.path());
        }
    }

    #[test]
    fn failpoints_leave_missing_target_absent() {
        for point in [FailPoint::Write, FailPoint::Flush, FailPoint::Replace] {
            let temp = TempDir::new().expect("temp dir");
            let target = temp.path().join("config.json");

            let result = AtomicConfigWriter::with_failpoint(&target, point).write(b"new");

            assert!(result.is_err(), "{point:?} should fail");
            assert!(
                !target.exists(),
                "{point:?} must not leave a partial target"
            );
            assert_no_atomic_residue(temp.path());
        }
    }

    #[test]
    fn record_failure_restores_existing_target() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("config.json");
        fs::write(&target, b"old").expect("seed target");

        let result = AtomicConfigWriter::new(&target)
            .write_and_record(b"new", || anyhow::bail!("injected record failure"));

        assert!(matches!(
            result
                .expect_err("record should fail")
                .downcast_ref::<AtomicConfigError>(),
            Some(AtomicConfigError::ActionRecordRolledBack { .. })
        ));
        assert_eq!(fs::read(&target).expect("read target"), b"old");
        assert_no_atomic_residue(temp.path());
    }

    #[test]
    fn record_failure_restores_missing_target() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("config.json");

        let result = AtomicConfigWriter::new(&target)
            .write_and_record(b"new", || anyhow::bail!("injected record failure"));

        assert!(result.is_err());
        assert!(!target.exists(), "new target must be removed on rollback");
        assert_no_atomic_residue(temp.path());
    }

    #[test]
    fn next_attempt_recovers_interrupted_operation() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("config.json");
        fs::write(&target, b"old").expect("seed target");
        let writer = AtomicConfigWriter::new(&target);
        writer.prepare_recovery().expect("prepare recovery");
        writer
            .replace_inner(b"new", writer.target_permissions())
            .expect("replace target");
        assert!(writer.marker_path().exists(), "pending marker must survive");

        AtomicConfigWriter::new(&target)
            .recover_pending()
            .expect("recover on next attempt");

        assert_eq!(fs::read(&target).expect("read target"), b"old");
        assert_no_atomic_residue(temp.path());
    }

    #[test]
    fn next_attempt_recovers_interrupted_creation() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("config.json");
        let writer = AtomicConfigWriter::new(&target);
        writer.prepare_recovery().expect("prepare recovery");
        writer
            .replace_inner(b"new", writer.target_permissions())
            .expect("create target");
        assert!(writer.marker_path().exists(), "pending marker must survive");

        AtomicConfigWriter::new(&target)
            .recover_pending()
            .expect("recover on next attempt");

        assert!(!target.exists(), "interrupted creation must be rolled back");
        assert_no_atomic_residue(temp.path());
    }

    #[test]
    fn remove_record_failure_restores_target() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("plugin.js");
        fs::write(&target, b"plugin").expect("seed target");

        let result = AtomicConfigWriter::new(&target)
            .remove_and_record(|| anyhow::bail!("injected record failure"));

        assert!(result.is_err());
        assert_eq!(fs::read(&target).expect("read target"), b"plugin");
        assert_no_atomic_residue(temp.path());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("script.sh");
        fs::write(&target, b"old").expect("seed target");
        fs::set_permissions(&target, Permissions::from_mode(0o751)).expect("set mode");

        write_file_atomic(&target, b"new").expect("replace target");

        assert_eq!(
            fs::metadata(&target)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_covers_existing_and_missing_targets() {
        let temp = TempDir::new().expect("temp dir");
        let existing = temp.path().join("existing.json");
        let missing = temp.path().join("missing.json");
        fs::write(&existing, b"old").expect("seed target");

        write_file_atomic(&existing, b"new").expect("replace existing");
        write_file_atomic(&missing, b"created").expect("create missing");

        assert_eq!(fs::read(existing).expect("read existing"), b"new");
        assert_eq!(fs::read(missing).expect("read missing"), b"created");
        assert_no_atomic_residue(temp.path());
    }
}
