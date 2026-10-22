//! Invocation-local presentation language. Domain and wire values stay language neutral.
use std::{cell::Cell, future::Future};

pub mod commands;
pub mod logging;
pub(crate) mod quota;
pub mod sync;
pub(crate) mod tui;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Chinese,
}

impl Language {
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "zh" | "zh-CN" => Some(Self::Chinese),
            _ => None,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }

    pub const fn select<'a>(self, english: &'a str, chinese: &'a str) -> &'a str {
        match self {
            Self::English => english,
            Self::Chinese => chinese,
        }
    }
}

tokio::task_local! { static ASYNC_LANGUAGE: Language; }
thread_local! { static SYNC_LANGUAGE: Cell<Option<Language>> = const { Cell::new(None) }; }

pub fn current() -> Language {
    SYNC_LANGUAGE
        .with(|value| value.get())
        .or_else(|| ASYNC_LANGUAGE.try_with(|value| *value).ok())
        .unwrap_or_default()
}

/// Scope an asynchronous invocation; spawned tasks explicitly capture their own language.
pub async fn scope<F: Future>(language: Language, future: F) -> F::Output {
    ASYNC_LANGUAGE.scope(language, future).await
}

/// Scope synchronous rendering. The closure must not suspend or poll other async work.
pub fn with_language<T>(language: Language, render: impl FnOnce() -> T) -> T {
    struct Restore(Option<Language>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SYNC_LANGUAGE.with(|value| value.set(self.0));
        }
    }
    let _restore = Restore(SYNC_LANGUAGE.with(|value| value.replace(Some(language))));
    render()
}

pub fn text<'a>(english: &'a str, chinese: &'a str) -> &'a str {
    current().select(english, chinese)
}
