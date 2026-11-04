//! The control API for the IDEA Keystage connector (ARCHITECTURE.md
//! section 14), on the same `127.0.0.1` server:
//!
//! * `GET /api/control/tools`: the tool catalog.
//! * `POST /api/control/call` with `{ "tool": name, "arguments": {...} }`:
//!   runs one tool and answers `{ "ok": true, "result": ... }` or
//!   `{ "ok": false, "error": "sentence" }`.
//!
//! Every request must pass [`gate`]: the routes exist only while the app
//! hands the server a [`Control`] with a token (404 otherwise, as for any
//! unknown path); `Host` is `127.0.0.1:<port>` or `localhost:<port>`; there
//! is no `Origin` and no `Sec-Fetch-Site` header, so no web page in a
//! browser can call it (browsers send them on every script request); and
//! `Authorization: Bearer <token>` matches, compared in constant time
//! (401 otherwise). A call's body is JSON (`Content-Type:
//! application/json`), at most [`MAX_BODY`] bytes. The overlay routes and
//! their Host and Origin rules are unchanged.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use serde_json::{json, Value};

use crate::server::{host_allowed, AppState};

/// Largest call body (a long `perform` script).
pub const MAX_BODY: usize = 1024 * 1024;

/// What the app provides.
pub trait Control: Send + Sync {
    /// The token, or `None` while the connector is turned off.
    fn token(&self) -> Option<String>;
    /// The tool catalog (`tools` array).
    fn tools(&self) -> Value;
    /// Runs one tool. Called on a blocking thread; may take minutes.
    fn call(&self, tool: &str, args: Value) -> Result<Value, String>;
}

/// Why a request was turned away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The connector is off: answer as if the route did not exist.
    Off,
    /// Wrong Host, or a browser's Origin or fetch metadata.
    Forbidden,
    /// Missing or wrong token.
    Unauthorized,
}

/// Checks a request's headers.
pub fn gate(headers: &HeaderMap, port: u16, token: Option<&str>) -> Result<(), Refusal> {
    let Some(token) = token.filter(|t| keystage_automation::token::is_token(t)) else {
        return Err(Refusal::Off);
    };
    if !host_allowed(headers, port)
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key("sec-fetch-site")
        || headers.contains_key("sec-fetch-mode")
    {
        return Err(Refusal::Forbidden);
    }
    let mut auth = headers.get_all(header::AUTHORIZATION).iter();
    let value = match (auth.next(), auth.next()) {
        (Some(v), None) => v.to_str().ok(),
        _ => None,
    };
    if keystage_automation::token::authorized(value, token) {
        Ok(())
    } else {
        Err(Refusal::Unauthorized)
    }
}

fn json_response(status: StatusCode, v: &Value) -> Response {
    let mut r = Response::new(Body::from(v.to_string()));
    *r.status_mut() = status;
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn refused(r: Refusal) -> Response {
    match r {
        Refusal::Off => json_response(
            StatusCode::NOT_FOUND,
            &json!({"ok": false, "error": "Not found"}),
        ),
        Refusal::Forbidden => json_response(
            StatusCode::FORBIDDEN,
            &json!({"ok": false, "error": "Forbidden"}),
        ),
        Refusal::Unauthorized => json_response(
            StatusCode::UNAUTHORIZED,
            &json!({"ok": false, "error": "The connector's key does not match. Restart IDEA Keystage and the connector."}),
        ),
    }
}

fn check(state: &AppState, headers: &HeaderMap) -> Result<Arc<dyn Control>, Refusal> {
    let Some(c) = state.control.clone() else {
        return Err(Refusal::Off);
    };
    let token = c.token();
    gate(headers, state.port, token.as_deref()).inspect_err(|r| {
        if *r != Refusal::Off {
            tracing::debug!("refused a control request");
        }
    })?;
    Ok(c)
}

pub(crate) async fn tools(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if method != Method::GET {
        return refused(Refusal::Off);
    }
    match check(&state, &headers) {
        Ok(c) => json_response(StatusCode::OK, &json!({ "tools": c.tools() })),
        Err(r) => refused(r),
    }
}

pub(crate) async fn call(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if method != Method::POST {
        return refused(Refusal::Off);
    }
    let c = match check(&state, &headers) {
        Ok(c) => c,
        Err(r) => return refused(r),
    };
    let json_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
        });
    if !json_type {
        return json_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &json!({"ok": false, "error": "Send JSON."}),
        );
    }
    let Ok(bytes) = to_bytes(body, MAX_BODY).await else {
        return json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            &json!({"ok": false, "error": "The request is too large."}),
        );
    };
    let Ok(req) = serde_json::from_slice::<Value>(&bytes) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            &json!({"ok": false, "error": "The request is not JSON."}),
        );
    };
    let Some(tool) = req["tool"].as_str().map(str::to_string) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            &json!({"ok": false, "error": "Name a tool."}),
        );
    };
    let args = req.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let out = tokio::task::spawn_blocking(move || c.call(&tool, args)).await;
    let v = match out {
        Ok(Ok(result)) => json!({"ok": true, "result": result}),
        Ok(Err(e)) => json!({"ok": false, "error": e}),
        Err(_) => {
            json!({"ok": false, "error": "IDEA Keystage hit an internal error running that tool."})
        }
    };
    json_response(StatusCode::OK, &v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 47820;

    fn h(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.append(*k, HeaderValue::from_str(v).expect("v"));
        }
        m
    }

    #[test]
    fn the_gate() {
        let t = "f".repeat(64);
        let bearer = format!("Bearer {t}");
        let good = h(&[("host", "127.0.0.1:47820"), ("authorization", &bearer)]);
        assert_eq!(gate(&good, PORT, Some(&t)), Ok(()));
        assert_eq!(
            gate(
                &h(&[("host", "localhost:47820"), ("authorization", &bearer)]),
                PORT,
                Some(&t)
            ),
            Ok(())
        );
        assert_eq!(gate(&good, PORT, None), Err(Refusal::Off), "connector off");
        assert_eq!(
            gate(&good, PORT, Some("short")),
            Err(Refusal::Off),
            "never a weak token"
        );
        assert_eq!(
            gate(
                &h(&[("host", "evil.example:47820"), ("authorization", &bearer)]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Forbidden)
        );
        assert_eq!(
            gate(
                &h(&[("host", "127.0.0.1:47821"), ("authorization", &bearer)]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Forbidden)
        );
        assert_eq!(
            gate(
                &h(&[
                    ("host", "127.0.0.1:47820"),
                    ("authorization", &bearer),
                    ("origin", "http://127.0.0.1:47820")
                ]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Forbidden),
            "no browser, not even our own pages"
        );
        assert_eq!(
            gate(
                &h(&[
                    ("host", "127.0.0.1:47820"),
                    ("authorization", &bearer),
                    ("sec-fetch-site", "same-origin")
                ]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Forbidden)
        );
        assert_eq!(
            gate(&h(&[("host", "127.0.0.1:47820")]), PORT, Some(&t)),
            Err(Refusal::Unauthorized)
        );
        let wrong = format!("Bearer {}", "e".repeat(64));
        assert_eq!(
            gate(
                &h(&[("host", "127.0.0.1:47820"), ("authorization", &wrong)]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Unauthorized)
        );
        assert_eq!(
            gate(
                &h(&[
                    ("host", "127.0.0.1:47820"),
                    ("authorization", &bearer),
                    ("authorization", &bearer)
                ]),
                PORT,
                Some(&t)
            ),
            Err(Refusal::Unauthorized),
            "one Authorization header"
        );
    }
}
