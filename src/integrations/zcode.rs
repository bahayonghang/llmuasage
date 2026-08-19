//! ZCode (Z.ai CLI) local SQLite discovery.
//!
//! The current ZCode CLI persists per-model-call usage in
//! `~/.zcode/cli/db/db.sqlite::model_usage` (the older
//! `~/.zcode/projects/**/*.jsonl` layout is no longer written). This module
//! only resolves that path; it never creates or writes any file.

use std::path::PathBuf;

/// Resolves the ZCode usage database path.
///
/// `ZCODE_HOME` is an llmusage-owned override (ZCode itself has no public
/// storage-root variable) used for tests and redirection; it points at the
/// `~/.zcode` equivalent, so the database lives at `<root>/cli/db/db.sqlite`.
/// The default `~/.zcode` path is the contract; the env override is not.
pub(crate) fn resolve_db_path() -> PathBuf {
    let root = std::env::var_os("ZCODE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::util::resolve_home_dir().join(".zcode"));
    root.join("cli").join("db").join("db.sqlite")
}

#[cfg(test)]
mod tests {
    use super::resolve_db_path;
    use std::path::Path;

    struct EnvGuard {
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(value: Option<&str>) -> Self {
            let previous = std::env::var("ZCODE_HOME").ok();
            unsafe {
                match value {
                    Some(value) => std::env::set_var("ZCODE_HOME", value),
                    None => std::env::remove_var("ZCODE_HOME"),
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var("ZCODE_HOME", previous);
                } else {
                    std::env::remove_var("ZCODE_HOME");
                }
            }
        }
    }

    #[test]
    fn default_path_points_at_zcode_cli_db() {
        let _guard = EnvGuard::set(None);
        let path = resolve_db_path();
        assert!(path.ends_with(Path::new(".zcode").join("cli").join("db").join("db.sqlite")));
    }

    #[test]
    fn zcode_home_override_replaces_the_root() {
        let _guard = EnvGuard::set(Some("C:/custom/zcode-root"));
        let path = resolve_db_path();
        assert!(path.starts_with("C:/custom/zcode-root"));
        assert!(path.ends_with(Path::new("cli").join("db").join("db.sqlite")));
    }

    #[test]
    fn empty_override_falls_back_to_default_root() {
        let _guard = EnvGuard::set(Some(""));
        let path = resolve_db_path();
        assert!(path.ends_with(Path::new(".zcode").join("cli").join("db").join("db.sqlite")));
    }
}
