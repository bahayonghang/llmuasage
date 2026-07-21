use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::{
    task::{JoinError, JoinHandle},
    time::{Instant, Interval, MissedTickBehavior},
};

const FILE_PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub(crate) struct FileProgressCounter {
    completed: Arc<AtomicU64>,
}

pub(crate) struct FileProgress {
    counter: FileProgressCounter,
    interval: Interval,
    last_emitted: u64,
}

impl FileProgress {
    pub(crate) fn new() -> (Self, FileProgressCounter) {
        let counter = FileProgressCounter {
            completed: Arc::new(AtomicU64::new(0)),
        };
        let mut interval = tokio::time::interval_at(
            Instant::now() + FILE_PROGRESS_INTERVAL,
            FILE_PROGRESS_INTERVAL,
        );
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        (
            Self {
                counter: counter.clone(),
                interval,
                last_emitted: 0,
            },
            counter,
        )
    }

    pub(crate) fn completed(&self) -> u64 {
        self.counter.completed.load(Ordering::Relaxed)
    }

    pub(crate) fn boundary_snapshot(&mut self) -> u64 {
        let completed = self.completed();
        self.last_emitted = self.last_emitted.max(completed);
        completed
    }

    pub(crate) async fn wait_for<T, F>(
        &mut self,
        mut task: JoinHandle<T>,
        mut report: F,
    ) -> Result<T, JoinError>
    where
        F: FnMut(u64),
    {
        loop {
            tokio::select! {
                result = &mut task => return result,
                _ = self.interval.tick() => {
                    if let Some(completed) = self.take_advanced() {
                        report(completed);
                    }
                }
            }
        }
    }

    fn take_advanced(&mut self) -> Option<u64> {
        let completed = self.completed();
        if completed <= self.last_emitted {
            return None;
        }
        self.last_emitted = completed;
        Some(completed)
    }
}

impl FileProgressCounter {
    pub(crate) fn advance_file(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_only_monotonic_advances() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let (mut progress, counter) = FileProgress::new();
            assert_eq!(progress.take_advanced(), None);

            counter.advance_file();
            assert_eq!(progress.take_advanced(), Some(1));
            assert_eq!(progress.take_advanced(), None);

            counter.advance_file();
            counter.advance_file();
            assert_eq!(progress.take_advanced(), Some(3));
            assert_eq!(progress.completed(), 3);

            counter.advance_file();
            assert_eq!(progress.boundary_snapshot(), 4);
            assert_eq!(progress.take_advanced(), None);
        });
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reports_progress_while_blocking_work_is_running() {
        let (mut progress, counter) = FileProgress::new();
        let worker_counter = counter.clone();
        let task = tokio::task::spawn_blocking(move || {
            worker_counter.advance_file();
            std::thread::sleep(Duration::from_millis(250));
            worker_counter.advance_file();
        });
        let mut snapshots = Vec::new();

        progress
            .wait_for(task, |completed| snapshots.push(completed))
            .await
            .expect("blocking task");

        assert_eq!(snapshots, vec![1]);
        assert_eq!(progress.completed(), 2);
    }
}
