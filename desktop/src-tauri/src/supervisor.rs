use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use llmusage::{Dashboard, LlmusageError, Result as LlmusageResult};
use rusqlite::InterruptHandle;
use serde::Serialize;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{error::DesktopError, state::AppState};

const READ_BUSY_TIMEOUT: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SupervisorSnapshot {
    pub inflight: usize,
    pub timed_out_tasks: u64,
    pub orphaned_tasks: usize,
}

struct QuerySlot {
    cancelled: AtomicBool,
    interrupt: Mutex<Option<InterruptHandle>>,
    cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
}

pub struct DesktopQuerySupervisor {
    inflight: AtomicUsize,
    timed_out_tasks: AtomicU64,
    orphaned_tasks: AtomicUsize,
    slots: Mutex<HashMap<u64, Arc<QuerySlot>>>,
}

struct QueryWorkGuard {
    supervisor: Arc<DesktopQuerySupervisor>,
}

impl Drop for QueryWorkGuard {
    fn drop(&mut self) {
        self.supervisor.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

impl DesktopQuerySupervisor {
    pub fn new() -> Self {
        Self {
            inflight: AtomicUsize::new(0),
            timed_out_tasks: AtomicU64::new(0),
            orphaned_tasks: AtomicUsize::new(0),
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub fn snapshot(&self) -> SupervisorSnapshot {
        SupervisorSnapshot {
            inflight: self.inflight.load(Ordering::Acquire),
            timed_out_tasks: self.timed_out_tasks.load(Ordering::Acquire),
            orphaned_tasks: self.orphaned_tasks.load(Ordering::Acquire),
        }
    }

    fn begin_work(self: &Arc<Self>) -> QueryWorkGuard {
        self.inflight.fetch_add(1, Ordering::AcqRel);
        QueryWorkGuard {
            supervisor: Arc::clone(self),
        }
    }

    fn register(
        self: &Arc<Self>,
        request_id: u64,
        cancel_tx: oneshot::Sender<()>,
    ) -> Arc<QuerySlot> {
        let slot = Arc::new(QuerySlot {
            cancelled: AtomicBool::new(false),
            interrupt: Mutex::new(None),
            cancel_tx: Mutex::new(Some(cancel_tx)),
        });
        if let Ok(mut slots) = self.slots.lock() {
            slots.insert(request_id, Arc::clone(&slot));
        }
        slot
    }

    fn unregister(&self, request_id: u64) {
        if let Ok(mut slots) = self.slots.lock() {
            slots.remove(&request_id);
        }
    }

    pub fn cancel(&self, request_ids: &[u64]) {
        let slots = self.slots.lock().ok();
        let Some(slots) = slots else {
            return;
        };
        for request_id in request_ids {
            if let Some(slot) = slots.get(request_id) {
                interrupt_slot(slot);
            }
        }
    }

    pub fn cancel_all(&self) {
        let slots = self.slots.lock().ok();
        let Some(slots) = slots else {
            return;
        };
        for slot in slots.values() {
            interrupt_slot(slot);
        }
    }

    fn supervise<T>(self: &Arc<Self>, task: JoinHandle<LlmusageResult<T>>)
    where
        T: Send + 'static,
    {
        self.timed_out_tasks.fetch_add(1, Ordering::AcqRel);
        self.orphaned_tasks.fetch_add(1, Ordering::AcqRel);
        let supervisor = Arc::clone(self);
        let _supervisor_task = tokio::spawn(async move {
            let _ = task.await;
            supervisor.orphaned_tasks.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

fn interrupt_slot(slot: &QuerySlot) {
    slot.cancelled.store(true, Ordering::SeqCst);
    if let Ok(guard) = slot.interrupt.lock()
        && let Some(handle) = guard.as_ref()
    {
        handle.interrupt();
    }
    if let Ok(mut tx) = slot.cancel_tx.lock()
        && let Some(tx) = tx.take()
    {
        let _ = tx.send(());
    }
}

pub async fn run_query<T, F>(
    state: &AppState,
    request_id: u64,
    timeout: Duration,
    f: F,
) -> Result<T, DesktopError>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
{
    let started = Instant::now();
    let permit =
        match tokio::time::timeout(timeout, state.query_semaphore.clone().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(DesktopError::invalid_request(
                    "dashboard query semaphore is closed",
                ));
            }
            Err(_) => return Err(DesktopError::timeout(timeout)),
        };
    let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
        return Err(DesktopError::timeout(timeout));
    };
    if remaining.is_zero() {
        return Err(DesktopError::timeout(timeout));
    }

    let supervisor = Arc::clone(&state.supervisor);
    let work_guard = supervisor.begin_work();
    let store = state.store.clone();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let slot = supervisor.register(request_id, cancel_tx);
    let blocking_slot = Arc::clone(&slot);
    let mut task = tokio::task::spawn_blocking(move || {
        let _work_guard = work_guard;
        let _permit = permit;
        let dashboard = Dashboard::open_with_busy_timeout(&store, READ_BUSY_TIMEOUT)?;
        let interrupt = dashboard.interrupt_handle();
        if blocking_slot.cancelled.load(Ordering::SeqCst) {
            interrupt.interrupt();
            return Err(LlmusageError::Cancelled {
                operation: "dashboard query",
            });
        }
        if let Ok(mut guard) = blocking_slot.interrupt.lock() {
            *guard = Some(interrupt);
        }
        if blocking_slot.cancelled.load(Ordering::SeqCst) {
            if let Ok(guard) = blocking_slot.interrupt.lock()
                && let Some(handle) = guard.as_ref()
            {
                handle.interrupt();
            }
            return Err(LlmusageError::Cancelled {
                operation: "dashboard query",
            });
        }
        f(&dashboard)
    });

    let joined = tokio::select! {
        joined = &mut task => joined,
        _ = cancel_rx => {
            interrupt_slot(&slot);
            supervisor.supervise(task);
            supervisor.unregister(request_id);
            return Err(DesktopError::cancelled());
        }
        _ = tokio::time::sleep(remaining) => {
            interrupt_slot(&slot);
            supervisor.supervise(task);
            supervisor.unregister(request_id);
            return Err(DesktopError::timeout(timeout));
        }
    };
    supervisor.unregister(request_id);
    if slot.cancelled.load(Ordering::SeqCst) {
        return Err(DesktopError::cancelled());
    }
    dashboard_join_result(joined)
}

fn dashboard_join_result<T>(
    joined: std::result::Result<LlmusageResult<T>, tokio::task::JoinError>,
) -> Result<T, DesktopError> {
    match joined {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(crate::error::map_llmusage_error(error)),
        Err(error) => Err(DesktopError::invalid_request(format!(
            "blocking dashboard task failed: {error}"
        ))),
    }
}
