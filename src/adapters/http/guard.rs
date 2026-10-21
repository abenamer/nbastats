//! Request guard for a locally hosted, unauthenticated application (REAS-TEST-003).
//!
//! Two checks protect the local-only assumption:
//! * The `Host` header must match the configured allow-list. This stops DNS-rebinding pages
//!   from reaching the API through a name that resolves to 127.0.0.1.
//! * A state-changing request (anything but GET, HEAD, OPTIONS) that a browser marks as
//!   cross-site (`Sec-Fetch-Site`) or whose `Origin` differs from the `Host` is rejected. This
//!   stops cross-site request forgery from other pages, including other local ports. Requests
//!   without these headers (curl, scripts) are allowed, as before.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use super::State as AppState;

/// Outcome of the checks, kept separate from Axum types so it can be unit tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    BadHost,
    CrossSite,
}

pub fn check(method: &Method, headers: &HeaderMap, host_allowed: impl Fn(&str) -> bool) -> Verdict {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !host_allowed(host) {
        return Verdict::BadHost;
    }
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Verdict::Allow;
    }
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if !matches!(site.trim(), "same-origin" | "none") {
            return Verdict::CrossSite;
        }
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        if origin != "null" {
            let origin_host = origin
                .split("://")
                .nth(1)
                .unwrap_or(origin)
                .trim_end_matches('/');
            if !origin_host.eq_ignore_ascii_case(host) {
                return Verdict::CrossSite;
            }
        } else {
            return Verdict::CrossSite;
        }
    }
    Verdict::Allow
}

pub async fn browser_request_guard(
    State(app): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    match check(req.method(), req.headers(), |h| app.config.host_allowed(h)) {
        Verdict::Allow => next.run(req).await,
        Verdict::BadHost => (
            StatusCode::MISDIRECTED_REQUEST,
            Json(json!({ "error": "host not allowed; set ALLOWED_HOSTS to accept this name" })),
        )
            .into_response(),
        Verdict::CrossSite => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "cross-site requests are not accepted" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }
    fn local(h: &str) -> bool {
        h.starts_with("localhost") || h.starts_with("127.0.0.1")
    }

    #[test]
    fn reads_pass_with_an_allowed_host() {
        assert_eq!(
            check(&Method::GET, &headers(&[("host", "localhost:3000")]), local),
            Verdict::Allow
        );
        assert_eq!(
            check(&Method::GET, &headers(&[("host", "evil.example")]), local),
            Verdict::BadHost
        );
        assert_eq!(check(&Method::GET, &headers(&[]), local), Verdict::BadHost);
    }

    #[test]
    fn writes_from_other_sites_are_rejected() {
        let h = headers(&[("host", "localhost:3000"), ("sec-fetch-site", "cross-site")]);
        assert_eq!(check(&Method::POST, &h, local), Verdict::CrossSite);
        let h = headers(&[
            ("host", "localhost:3000"),
            ("sec-fetch-site", "same-site"),
            ("origin", "http://localhost:4000"),
        ]);
        assert_eq!(check(&Method::POST, &h, local), Verdict::CrossSite);
        let h = headers(&[
            ("host", "localhost:3000"),
            ("origin", "http://attacker.example"),
        ]);
        assert_eq!(check(&Method::DELETE, &h, local), Verdict::CrossSite);
        let h = headers(&[("host", "localhost:3000"), ("origin", "null")]);
        assert_eq!(check(&Method::POST, &h, local), Verdict::CrossSite);
    }

    #[test]
    fn same_origin_and_non_browser_writes_pass() {
        let h = headers(&[
            ("host", "localhost:3000"),
            ("sec-fetch-site", "same-origin"),
            ("origin", "http://localhost:3000"),
        ]);
        assert_eq!(check(&Method::POST, &h, local), Verdict::Allow);
        let h = headers(&[("host", "127.0.0.1:3000")]);
        assert_eq!(check(&Method::POST, &h, local), Verdict::Allow);
    }
}
