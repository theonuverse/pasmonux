//! Dynamic HTTP router — resolves arbitrary field paths from [`SystemStats`].
//!
//! New fields added to [`SystemStats`] (or its nested types) are automatically
//! exposed as endpoints without any routing changes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;
use tokio::sync::watch;

use crate::types::SystemStats;

// ─── Router construction ───────────────────────────────────────────────────

/// Builds the Axum router with fully dynamic endpoint resolution.
///
/// # Routes
///
/// | Method | Path                    | Description                        |
/// |--------|-------------------------|------------------------------------|
/// | `GET`  | `/`                     | API index — lists every endpoint   |
/// | `GET`  | `/stats`                | Full system stats snapshot         |
/// | `GET`  | `/<field>`              | Single top-level field             |
/// | `GET`  | `/cores/<name>`         | Single core by name                |
/// | `GET`  | `/cores/<name>/<field>` | Single field of a specific core    |
pub fn build(rx: watch::Receiver<SystemStats>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/stats", get(stats))
        .route("/*path", get(resolve))
        .with_state(rx)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /` — Returns the API index with every available endpoint.
async fn index(State(rx): State<watch::Receiver<SystemStats>>) -> Json<Value> {
    let stats = rx.borrow().clone();
    let tree = serde_json::to_value(&stats).unwrap_or_default();
    let mut endpoints = vec!["/stats".to_owned()];
    enumerate_endpoints(&tree, "", &mut endpoints);

    Json(serde_json::json!({
        "name": "asmo",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": endpoints,
        "usage": "GET any endpoint to retrieve its data."
    }))
}

/// `GET /stats` — Returns the full system stats snapshot.
async fn stats(State(rx): State<watch::Receiver<SystemStats>>) -> Json<SystemStats> {
    Json(rx.borrow().clone())
}

/// `GET /{path}` — Resolves an arbitrary path against the current stats.
async fn resolve(
    State(rx): State<watch::Receiver<SystemStats>>,
    Path(path): Path<String>,
) -> Response {
    let stats = rx.borrow().clone();
    let tree = match serde_json::to_value(&stats) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal serialization error"})),
            )
                .into_response();
        }
    };

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match resolve_path(&tree, &segments) {
        Some(v) => Json(v).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not found",
                "path": format!("/{path}"),
                "hint": "GET / for available endpoints"
            })),
        )
            .into_response(),
    }
}

// ─── Path resolution ───────────────────────────────────────────────────────

/// Walks the JSON value tree using the given URL path segments.
///
/// - **Objects** — keys map directly to child values.
/// - **Arrays** — items are matched by their `"name"` field (e.g. `"cpu0"`).
/// - **Leaf nodes** — returned wrapped as `{ "key": value }`.
fn resolve_path(value: &Value, segments: &[&str]) -> Option<Value> {
    if segments.is_empty() {
        return Some(value.clone());
    }

    let key = segments[0];
    let rest = &segments[1..];

    match value {
        Value::Object(map) => {
            let child = map.get(key)?;
            if rest.is_empty() {
                Some(serde_json::json!({ key: child }))
            } else {
                resolve_path(child, rest)
            }
        }
        Value::Array(arr) => {
            let item = arr
                .iter()
                .find(|v| v.get("name").and_then(Value::as_str) == Some(key))?;
            if rest.is_empty() {
                Some(item.clone())
            } else {
                resolve_path(item, rest)
            }
        }
        _ => None,
    }
}

// ─── Endpoint enumeration ──────────────────────────────────────────────────

/// Recursively discovers every addressable path in a JSON value tree.
fn enumerate_endpoints(value: &Value, prefix: &str, out: &mut Vec<String>) {
    let Value::Object(map) = value else { return };

    for (key, child) in map {
        let path = format!("{prefix}/{key}");
        out.push(path.clone());

        match child {
            Value::Object(_) => enumerate_endpoints(child, &path, out),
            Value::Array(arr) => {
                for item in arr {
                    let Some(name) = item.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let item_path = format!("{path}/{name}");
                    out.push(item_path.clone());

                    if let Value::Object(fields) = item {
                        for field_key in fields.keys().filter(|k| k.as_str() != "name") {
                            out.push(format!("{item_path}/{field_key}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
