//! Invocation-local parser facts without changing the public parser contract.
use std::{cell::RefCell, future::Future};

use crate::models::SourceKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InventoryIssue {
    RootNotDirectory,
    ExternalIo { detail: String },
    Traversal,
    Unknown { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryDiagnostic {
    pub(crate) samples: Vec<InventoryIssue>,
    pub(crate) total: usize,
    pub(crate) omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParserFeedback {
    SqliteOpenFailed {
        source: SourceKind,
    },
    Inventory {
        source: SourceKind,
        diagnostic: InventoryDiagnostic,
    },
}

tokio::task_local! {
    static FEEDBACK: RefCell<Vec<ParserFeedback>>;
}

pub(crate) fn emit(feedback: ParserFeedback) {
    let _ = FEEDBACK.try_with(|collector| collector.borrow_mut().push(feedback));
}

pub(crate) async fn collect<F: Future>(future: F) -> (F::Output, Vec<ParserFeedback>) {
    FEEDBACK
        .scope(RefCell::new(Vec::new()), async {
            let value = future.await;
            let facts = FEEDBACK.with(|collector| collector.take());
            (value, facts)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(source: SourceKind) -> ParserFeedback {
        ParserFeedback::SqliteOpenFailed { source }
    }

    #[tokio::test]
    async fn nested_collectors_and_concurrent_tasks_are_isolated_and_restored() {
        let (_, outer) = collect(async {
            emit(fact(SourceKind::Codex));
            let (_, inner) = collect(async {
                emit(fact(SourceKind::Opencode));
            })
            .await;
            assert_eq!(inner, vec![fact(SourceKind::Opencode)]);
            tokio::spawn(async {
                emit(fact(SourceKind::Claude));
            })
            .await
            .unwrap();
            emit(fact(SourceKind::Zcode));
        })
        .await;
        assert_eq!(
            outer,
            vec![fact(SourceKind::Codex), fact(SourceKind::Zcode)]
        );
        let (left, right) = tokio::join!(
            collect(async {
                tokio::task::yield_now().await;
                emit(fact(SourceKind::Opencode));
            }),
            collect(async {
                emit(fact(SourceKind::Zcode));
                tokio::task::yield_now().await;
            })
        );
        assert_eq!(left.1, vec![fact(SourceKind::Opencode)]);
        assert_eq!(right.1, vec![fact(SourceKind::Zcode)]);
        assert!(FEEDBACK.try_with(|_| ()).is_err());
    }

    #[tokio::test]
    async fn dropped_or_failed_collection_restores_scope_and_direct_emit_is_noop() {
        emit(fact(SourceKind::Codex));
        let mut future = Box::pin(collect(async {
            emit(fact(SourceKind::Opencode));
            std::future::pending::<()>().await;
        }));
        std::future::poll_fn(|context| {
            assert!(future.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(future);
        assert!(FEEDBACK.try_with(|_| ()).is_err());
        let (result, facts) = collect(async {
            emit(fact(SourceKind::Zcode));
            Err::<(), _>("fixture failure")
        })
        .await;
        assert!(result.is_err());
        assert_eq!(facts, vec![fact(SourceKind::Zcode)]);
        assert!(FEEDBACK.try_with(|_| ()).is_err());
    }
}
