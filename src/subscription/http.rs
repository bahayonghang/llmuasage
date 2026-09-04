use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;
use reqwest::redirect::Policy;

pub(crate) fn redirect_policy() -> Policy {
    Policy::none()
}

pub fn client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(timeout)
        .redirect(redirect_policy())
        .user_agent("llmusage")
        .build()?)
}

pub fn status_error(provider: &str, status: StatusCode) -> anyhow::Error {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        anyhow::anyhow!(
            "{provider} usage unavailable: stored access token was rejected (HTTP {status}). \
             Run the provider CLI so it can refresh its own login, then retry."
        )
    } else {
        anyhow::anyhow!("{provider} usage request failed (HTTP {status})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, header};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::net::TcpListener;

    #[test]
    fn client_builder_uses_visible_no_redirect_policy() {
        let policy = format!("{:?}", redirect_policy());
        assert!(
            policy.contains("None"),
            "subscription HTTP client must disable redirects, got {policy}"
        );
    }

    #[tokio::test]
    async fn cross_host_redirect_does_not_forward_authorization() {
        let foreign_hits = Arc::new(AtomicBool::new(false));
        let foreign_saw_authorization = Arc::new(AtomicBool::new(false));
        let origin_saw_authorization = Arc::new(AtomicBool::new(false));
        let foreign_hits_handler = Arc::clone(&foreign_hits);
        let foreign_auth_handler = Arc::clone(&foreign_saw_authorization);
        let foreign_app = axum::Router::new().route(
            "/steal",
            get(move |headers: HeaderMap| {
                let hits = Arc::clone(&foreign_hits_handler);
                let saw_authorization = Arc::clone(&foreign_auth_handler);
                async move {
                    hits.store(true, Ordering::SeqCst);
                    if headers.get(header::AUTHORIZATION).is_some() {
                        saw_authorization.store(true, Ordering::SeqCst);
                    }
                    "stolen"
                }
            }),
        );
        let foreign_listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 2), 0))
            .await
            .expect("bind foreign host");
        let foreign_addr: SocketAddr = foreign_listener.local_addr().expect("foreign addr");
        let foreign_server = tokio::spawn(async move {
            axum::serve(foreign_listener, foreign_app)
                .await
                .expect("foreign serve");
        });
        let foreign_url = format!("http://{foreign_addr}/steal");

        let origin_app = axum::Router::new().route(
            "/usage",
            get({
                let foreign_url = foreign_url.clone();
                let origin_auth = Arc::clone(&origin_saw_authorization);
                move |headers: HeaderMap| {
                    let foreign_url = foreign_url.clone();
                    let origin_auth = Arc::clone(&origin_auth);
                    async move {
                        if headers.get(header::AUTHORIZATION).is_some() {
                            origin_auth.store(true, Ordering::SeqCst);
                        }
                        (StatusCode::FOUND, [(header::LOCATION, foreign_url)]).into_response()
                    }
                }
            }),
        );
        let origin_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind origin");
        let origin_addr: SocketAddr = origin_listener.local_addr().expect("origin addr");
        let origin_server = tokio::spawn(async move {
            axum::serve(origin_listener, origin_app)
                .await
                .expect("origin serve");
        });

        let http = client(Duration::from_secs(2)).expect("client");
        let response = http
            .get(format!("http://{origin_addr}/usage"))
            .header(header::AUTHORIZATION, "Bearer secret-token")
            .send()
            .await
            .expect("origin response");

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some(foreign_url.as_str())
        );
        assert_ne!(origin_addr.ip(), foreign_addr.ip());
        assert!(
            origin_saw_authorization.load(Ordering::SeqCst),
            "origin must receive Authorization so the leak check is not vacuous"
        );
        assert!(
            !foreign_hits.load(Ordering::SeqCst),
            "cross-host redirect must not be followed"
        );
        assert!(
            !foreign_saw_authorization.load(Ordering::SeqCst),
            "Authorization must not be sent to the redirect target"
        );

        origin_server.abort();
        foreign_server.abort();
    }

    #[test]
    fn status_error_unauthorized_mentions_rejected_token() {
        let error = status_error("Codex", StatusCode::UNAUTHORIZED).to_string();
        assert!(
            error.contains("stored access token was rejected"),
            "{error}"
        );
    }

    #[test]
    fn status_error_internal_error_is_generic_failure() {
        let error = status_error("Codex", StatusCode::INTERNAL_SERVER_ERROR).to_string();
        assert!(error.contains("usage request failed"), "{error}");
        assert!(
            !error.contains("stored access token was rejected"),
            "{error}"
        );
    }
}
