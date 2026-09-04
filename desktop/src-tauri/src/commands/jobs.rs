use llmusage::JobSnapshot;

use crate::{
    dto::SyncStartDto,
    error::{DesktopError, map_job_start_error, map_llmusage_error},
    state::AppState,
};

pub fn start_sync(state: &AppState, dto: SyncStartDto) -> Result<JobSnapshot, DesktopError> {
    let options = crate::dto::convert_sync(&dto)?;
    if let Some(meta) = state
        .store
        .current_worker_lock()
        .map_err(map_llmusage_error)?
    {
        return Err(DesktopError::new(
            "lock_busy",
            format!("worker lock busy: {}", meta.holder_identity()),
            Some(meta.holder_identity()),
        ));
    }
    let (job_id, _rx) = state
        .jobs
        .try_start(&state.store, options)
        .map_err(map_job_start_error)?;
    state
        .jobs
        .snapshot(&job_id)
        .ok_or_else(|| DesktopError::invalid_request(format!("job {job_id} missing after start")))
}

pub fn job_snapshot(state: &AppState, id: String) -> Option<JobSnapshot> {
    state.jobs.snapshot(&id)
}

pub fn cancel_job(state: &AppState, id: String) -> bool {
    state.jobs.cancel(&id)
}
