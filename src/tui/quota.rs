use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::subscription::{
    DiagnosticHint, FetchContext, MetricHint, QuotaPresentationHints, UsageFetchDiagnostic,
    UsageFetchReport, UsageMetric, fetch_all_with_presentation,
};
use crate::util::resolve_home_dir;

pub(super) struct QuotaController {
    runtime: tokio::runtime::Handle,
    tx: mpsc::Sender<(u64, QuotaUpdate)>,
    rx: mpsc::Receiver<(u64, QuotaUpdate)>,
    active_cancel: Option<CancellationToken>,
    fetching: bool,
    request: u64,
}

impl QuotaController {
    pub(super) fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(4);
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current().map_err(|err| {
                anyhow::anyhow!(
                    "{}: {err}",
                    crate::common::i18n::text(
                        "TUI requires a Tokio runtime",
                        "TUI 需要 Tokio 运行时"
                    )
                )
            })?,
            tx,
            rx,
            active_cancel: None,
            fetching: false,
            request: 0,
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
        let request = self.request;
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = tokio::select! {
                _ = cancel.cancelled() => return,
                result = fetch_all_with_presentation(&ctx, bypass_cache) => result,
            };
            let update = QuotaUpdate {
                report: result.outcome.report,
                presentation: result.presentation,
            };
            let _ = tx.send((request, update)).await;
        });
    }

    pub(super) fn try_recv(&mut self) -> Option<QuotaUpdate> {
        while let Ok((request, update)) = self.rx.try_recv() {
            if request == self.request {
                self.fetching = false;
                self.active_cancel = None;
                return Some(update);
            }
        }
        None
    }

    pub(super) fn cancel_active(&mut self) {
        self.request = self.request.wrapping_add(1);
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

pub(super) struct QuotaUpdate {
    pub report: UsageFetchReport,
    pub presentation: QuotaPresentationHints,
}

#[derive(Default)]
struct FramePresentation {
    metrics: Vec<(usize, MetricHint)>,
    diagnostics: Vec<(usize, DiagnosticHint)>,
}

thread_local! {
    static PRESENTATION: std::cell::RefCell<FramePresentation> = std::cell::RefCell::new(FramePresentation::default());
}

/// Render-only scope. Pointer identities are used only as keys during this frame.
pub(super) fn with_presentation<T>(
    report: Option<&UsageFetchReport>,
    hints: Option<&QuotaPresentationHints>,
    render: impl FnOnce() -> T,
) -> T {
    struct Restore(FramePresentation);
    impl Drop for Restore {
        fn drop(&mut self) {
            PRESENTATION.with(|value| {
                value.replace(std::mem::take(&mut self.0));
            });
        }
    }
    let mut frame = FramePresentation::default();
    if let (Some(report), Some(hints)) = (report, hints)
        && hints.matches(report)
    {
        for (output, metrics) in report.outputs.iter().zip(&hints.metrics) {
            frame.metrics.extend(
                output
                    .metrics
                    .iter()
                    .zip(metrics)
                    .map(|(metric, hint)| (metric as *const UsageMetric as usize, hint.clone())),
            );
        }
        frame.diagnostics.extend(
            report
                .diagnostics
                .iter()
                .zip(&hints.diagnostics)
                .filter_map(|(diagnostic, hint)| {
                    hint.as_ref().map(|hint| {
                        (
                            diagnostic as *const UsageFetchDiagnostic as usize,
                            hint.clone(),
                        )
                    })
                }),
        );
    }
    let _restore = Restore(PRESENTATION.with(|value| value.replace(frame)));
    render()
}

pub(super) fn metric_hint(metric: &UsageMetric) -> Option<MetricHint> {
    PRESENTATION.with(|value| {
        value
            .borrow()
            .metrics
            .iter()
            .find(|(key, _)| *key == metric as *const UsageMetric as usize)
            .map(|(_, hint)| hint.clone())
    })
}

pub(super) fn diagnostic_hint(diagnostic: &UsageFetchDiagnostic) -> Option<DiagnosticHint> {
    PRESENTATION.with(|value| {
        value
            .borrow()
            .diagnostics
            .iter()
            .find(|(key, _)| *key == diagnostic as *const UsageFetchDiagnostic as usize)
            .map(|(_, hint)| hint.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::{RemainingHint, UsageOutput, WindowHint};

    fn fixture() -> (UsageFetchReport, QuotaPresentationHints) {
        let report = UsageFetchReport {
            outputs: vec![UsageOutput {
                provider: "Kimi".into(),
                account: None,
                credential_source: None,
                plan: None,
                email: None,
                metrics: vec![UsageMetric {
                    label: "Weekly".into(),
                    used_percent: 0.0,
                    remaining_percent: 100.0,
                    remaining_label: Some("101/100 left".into()),
                    resets_at: None,
                }],
            }],
            diagnostics: vec![UsageFetchDiagnostic::error("Claude", "canonical")],
        };
        let mut hints = QuotaPresentationHints::default();
        hints.metrics.push(vec![MetricHint {
            window: WindowHint::Weekly,
            prefix: None,
            remaining: Some(RemainingHint::Integer {
                remaining: 101,
                limit: 100,
            }),
        }]);
        hints
            .diagnostics
            .push(Some(DiagnosticHint::Http { status: 403 }));
        hints.bind(&report);
        (report, hints)
    }

    #[test]
    fn presentation_scope_is_bound_nested_and_unwind_safe() {
        let (report, hints) = fixture();
        let metric = &report.outputs[0].metrics[0];
        assert!(metric_hint(metric).is_none());
        with_presentation(Some(&report), Some(&hints), || {
            assert_eq!(metric_hint(metric), Some(hints.metrics[0][0].clone()));
            assert_eq!(
                diagnostic_hint(&report.diagnostics[0]),
                Some(DiagnosticHint::Http { status: 403 })
            );
            let mut changed = report.clone();
            changed.outputs[0].metrics[0].remaining_percent = 5.0;
            with_presentation(Some(&changed), Some(&hints), || {
                assert!(metric_hint(metric).is_none())
            });
            assert!(metric_hint(metric).is_some());
            let _ =
                std::panic::catch_unwind(|| with_presentation(None, None, || panic!("scope test")));
            assert!(metric_hint(metric).is_some());
        });
        assert!(metric_hint(metric).is_none());
    }

    #[tokio::test]
    async fn superseded_and_cancelled_queued_results_do_not_clear_current_request() {
        let mut controller = QuotaController::new().unwrap();
        let (report, presentation) = fixture();
        let old_request = controller.request;
        controller
            .tx
            .try_send((
                old_request,
                QuotaUpdate {
                    report: report.clone(),
                    presentation: presentation.clone(),
                },
            ))
            .unwrap_or_else(|_| panic!("queue old result"));
        controller.cancel_active();
        controller.fetching = true;
        assert!(controller.try_recv().is_none());
        assert!(controller.is_fetching());
        controller
            .tx
            .try_send((
                controller.request,
                QuotaUpdate {
                    report: report.clone(),
                    presentation,
                },
            ))
            .unwrap_or_else(|_| panic!("queue current result"));
        let accepted = controller.try_recv().unwrap();
        assert_eq!(accepted.report, report);
        assert!(accepted.presentation.matches(&accepted.report));
        assert!(!controller.is_fetching());
        let (report, presentation) = fixture();
        controller
            .tx
            .try_send((
                controller.request,
                QuotaUpdate {
                    report,
                    presentation,
                },
            ))
            .unwrap_or_else(|_| panic!("queue cancelled result"));
        controller.cancel_active();
        assert!(controller.try_recv().is_none());
    }
}
