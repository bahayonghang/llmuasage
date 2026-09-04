use llmusage::{JobStartError, LlmusageError, SyncRequestError};
use rusqlite::ErrorCode;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DesktopError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
}

impl DesktopError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        holder: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            holder,
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message, None)
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "cancelled: dashboard query", None)
    }

    pub fn timeout(duration: std::time::Duration) -> Self {
        Self::new(
            "timeout",
            format!(
                "dashboard query exceeded {} ms timeout",
                duration.as_millis()
            ),
            None,
        )
    }

    pub fn job_active(active_job_id: &str) -> Self {
        Self::new(
            "job_active",
            format!("sync job already active: {active_job_id}"),
            None,
        )
    }

    pub fn from_anyhow(error: anyhow::Error) -> Self {
        if let Some(llmusage) = error.downcast_ref::<LlmusageError>() {
            return map_llmusage_error_ref(llmusage);
        }
        Self::invalid_request(error.to_string())
    }
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.holder {
            Some(holder) => write!(f, "{}: {} ({holder})", self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for DesktopError {}

pub fn map_llmusage_error(error: LlmusageError) -> DesktopError {
    map_llmusage_error_ref(&error)
}

pub fn map_llmusage_error_ref(error: &LlmusageError) -> DesktopError {
    match error {
        LlmusageError::NotInitialized => {
            DesktopError::new("not_initialized", error.to_string(), None)
        }
        LlmusageError::SchemaTooNew { .. } => {
            DesktopError::new("schema_too_new", error.to_string(), None)
        }
        LlmusageError::LockBusy { holder } => {
            DesktopError::new("lock_busy", error.to_string(), Some(holder.clone()))
        }
        LlmusageError::LockLost => DesktopError::new("lock_lost", error.to_string(), None),
        LlmusageError::Cancelled { .. } => DesktopError::new("cancelled", error.to_string(), None),
        LlmusageError::ConfigInvalid { detail }
            if detail.contains("exceeded") && detail.contains("timeout") =>
        {
            DesktopError::new("timeout", error.to_string(), None)
        }
        LlmusageError::Db(db) if is_interrupted(db) => DesktopError::cancelled(),
        LlmusageError::ConfigInvalid { .. } => DesktopError::invalid_request(error.to_string()),
        _ => DesktopError::invalid_request(error.to_string()),
    }
}

pub fn map_job_start_error(error: JobStartError) -> DesktopError {
    match error {
        JobStartError::Active(rejected) => DesktopError::job_active(&rejected.active_job_id),
        JobStartError::InvalidRequest(error) => map_sync_request_error(error),
    }
}

pub fn map_sync_request_error(error: SyncRequestError) -> DesktopError {
    DesktopError::invalid_request(error.to_string())
}

fn is_interrupted(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::OperationInterrupted)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_llmusage_error_not_initialized() {
        let mapped = map_llmusage_error(LlmusageError::NotInitialized);
        assert_eq!(mapped.code, "not_initialized");
        assert!(mapped.holder.is_none());
    }

    #[test]
    fn map_llmusage_error_schema_too_new() {
        let mapped = map_llmusage_error(LlmusageError::SchemaTooNew {
            db_version: 99,
            binary_version: 24,
        });
        assert_eq!(mapped.code, "schema_too_new");
    }

    #[test]
    fn map_llmusage_error_lock_busy_keeps_holder() {
        let mapped = map_llmusage_error(LlmusageError::LockBusy {
            holder: "cli:1@2026-01-01T00:00:00Z".to_string(),
        });
        assert_eq!(mapped.code, "lock_busy");
        assert_eq!(mapped.holder.as_deref(), Some("cli:1@2026-01-01T00:00:00Z"));
    }

    #[test]
    fn map_llmusage_error_lock_lost() {
        let mapped = map_llmusage_error(LlmusageError::LockLost);
        assert_eq!(mapped.code, "lock_lost");
    }

    #[test]
    fn map_llmusage_error_cancelled() {
        let mapped = map_llmusage_error(LlmusageError::Cancelled {
            operation: "dashboard query",
        });
        assert_eq!(mapped.code, "cancelled");
    }

    #[test]
    fn map_llmusage_error_timeout_config() {
        let mapped = map_llmusage_error(LlmusageError::ConfigInvalid {
            detail: "dashboard query exceeded 5000 ms timeout".to_string(),
        });
        assert_eq!(mapped.code, "timeout");
    }

    #[test]
    fn map_job_start_error_active() {
        let mapped = map_job_start_error(JobStartError::Active(llmusage::JobStartRejected {
            active_job_id: "job-1".to_string(),
        }));
        assert_eq!(mapped.code, "job_active");
        assert!(mapped.message.contains("job-1"));
    }
}
