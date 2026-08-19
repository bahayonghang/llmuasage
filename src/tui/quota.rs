use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::subscription::{FetchContext, UsageFetchReport, fetch_all};
use crate::util::resolve_home_dir;

pub(super) struct QuotaController {
    runtime: tokio::runtime::Handle,
    tx: mpsc::Sender<UsageFetchReport>,
    rx: mpsc::Receiver<UsageFetchReport>,
    active_cancel: Option<CancellationToken>,
    fetching: bool,
}

impl QuotaController {
    pub(super) fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(4);
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|err| anyhow::anyhow!("TUI requires a Tokio runtime: {err}"))?,
            tx,
            rx,
            active_cancel: None,
            fetching: false,
        })
    }

    pub(super) fn is_fetching(&self) -> bool {
        self.fetching
    }

    pub(super) fn fetch_if_needed(&mut self, ctx: FetchContext) {
        self.start(ctx, false);
    }

    pub(super) fn force_fetch(&mut self, ctx: FetchContext) {
        self.start(ctx, true);
    }

    fn start(&mut self, ctx: FetchContext, bypass_cache: bool) {
        if self.fetching && !bypass_cache {
            return;
        }
        self.cancel_active();
        let cancel = CancellationToken::new();
        self.active_cancel = Some(cancel.clone());
        self.fetching = true;
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let report = tokio::select! {
                _ = cancel.cancelled() => return,
                report = fetch_all(&ctx, bypass_cache) => report,
            };
            let _ = tx.send(report).await;
        });
    }

    pub(super) fn try_recv(&mut self) -> Option<UsageFetchReport> {
        match self.rx.try_recv() {
            Ok(report) => {
                self.fetching = false;
                self.active_cancel = None;
                Some(report)
            }
            Err(_) => None,
        }
    }

    pub(super) fn cancel_active(&mut self) {
        if let Some(cancel) = self.active_cancel.take() {
            cancel.cancel();
        }
        self.fetching = false;
    }

    pub(super) fn shutdown(&mut self, _timeout: Duration) {
        self.cancel_active();
    }
}

pub(super) fn production_context(cache_path: std::path::PathBuf) -> FetchContext {
    FetchContext::production(resolve_home_dir(), cache_path)
}
