use std::{
    cell::RefCell,
    ffi::OsString,
    sync::{Mutex, MutexGuard},
};

static ENV_LOCK: Mutex<()> = Mutex::new(());
type SavedEnv = Vec<(String, Option<OsString>)>;

/// Serializes process-environment fixtures within one integration-test target
/// and restores every captured variable once, including constructor failures.
pub(crate) struct ScopedEnv {
    saved: RefCell<Option<SavedEnv>>,
    _guard: MutexGuard<'static, ()>,
}

impl ScopedEnv {
    pub(crate) fn capture(keys: &[&str]) -> Self {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let saved = keys
            .iter()
            .map(|key| ((*key).to_string(), std::env::var_os(key)))
            .collect();
        Self {
            saved: RefCell::new(Some(saved)),
            _guard: guard,
        }
    }

    pub(crate) fn restore(&self) {
        let Some(saved) = self.saved.borrow_mut().take() else {
            return;
        };
        for (key, value) in saved.into_iter().rev() {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        self.restore();
    }
}
