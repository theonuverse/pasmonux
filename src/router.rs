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
/// | Method | Path                          | Description                           |
/// |--------|-------------------------------|---------------------------------------|
/// | `GET`  | `/`                           | API index — lists every endpoint      |
/// | `GET`  | `/stats`                      | Full system stats snapshot            |
/// | `GET`  | `/<field>`                    | Single top-level field                |
/// | `GET`  | `/<f1>,<f2>,…`                | Multiple fields in one request        |
/// | `GET`  | `/cores/<name>`               | Single core by name                   |
/// | `GET`  | `/cores/<name>/<field>`       | Single field of a specific core       |
/// | `GET`  | `/cores/<name>/<f1>,<f2>,…`   | Multiple core fields                  |
/// | `GET`  | `/cores/*/<field>`            | Field from every core (wildcard)      |
/// | `GET`  | `/cores/all/<f1>,<f2>,…`      | Multiple fields from every core       |
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
    let tree = stats_to_value(&rx.borrow());
    let mut endpoints = vec!["/stats".to_owned()];
    enumerate_endpoints(&tree, "", &mut endpoints);

    Json(serde_json::json!({
        "name": "asmo",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": endpoints,
        "multi_field": "Combine fields with commas: /battery_level,cpu_temp,gpu_load",
        "wildcard": "Use * or 'all' for arrays: /cores/*/usage  /cores/all/usage,cur_freq",
        "usage": "GET any endpoint to retrieve its data."
    }))
}

/// `GET /stats` — Returns the full system stats snapshot.
async fn stats(State(rx): State<watch::Receiver<SystemStats>>) -> Json<SystemStats> {
    Json(rx.borrow().clone())
}

/// `GET /{path}` — Resolves an arbitrary path against the current stats.
///
/// Supports comma-separated fields in the last segment and wildcards (`*` / `all`)
/// for array expansion, e.g. `/cores/*/usage` or `/cores/all/usage,cur_freq`.
async fn resolve(
    State(rx): State<watch::Receiver<SystemStats>>,
    Path(path): Path<String>,
) -> Response {
    let tree = stats_to_value(&rx.borrow());

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match resolve_request(&tree, &segments) {
        Some(v) => Json(v).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "not found", &path),
    }
}

/// Build a JSON error response with a hint pointing to the index.
fn error_response(status: StatusCode, message: &str, path: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": message,
            "path": format!("/{path}"),
            "hint": "GET / for available endpoints"
        })),
    )
        .into_response()
}

// ─── Path resolution ───────────────────────────────────────────────────────

/// Serialize [`SystemStats`] into a JSON value tree with clean `f32` precision.
///
/// `serde_json::to_value` promotes `f32` → `f64`, introducing artifacts like
/// `556.7999877929688` instead of `556.8`.  This function walks the tree after
/// conversion and casts every float back through `f32` to recover the short
/// representation.
fn stats_to_value(stats: &SystemStats) -> Value {
    let mut tree = serde_json::to_value(stats).unwrap_or_default();
    clean_f32_precision(&mut tree);
    tree
}

/// Recursively round every float in a JSON tree to `f32` precision.
fn clean_f32_precision(value: &mut Value) {
    match value {
        Value::Number(n) => {
            // Only touch floats — leave integers untouched.
            if n.as_u64().is_none() && n.as_i64().is_none() {
                if let Some(f) = n.as_f64() {
                    if let Some(clean) = serde_json::Number::from_f64((f as f32) as f64) {
                        *n = clean;
                    }
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(clean_f32_precision),
        Value::Object(map) => map.values_mut().for_each(clean_f32_precision),
        _ => {}
    }
}

/// Returns `true` for wildcard tokens (`*` and `all`).
#[inline]
fn is_wildcard(s: &str) -> bool {
    s == "*" || s == "all"
}

/// Navigate the JSON tree and return the **raw** value at the given path.
fn navigate(value: &Value, segments: &[&str]) -> Option<Value> {
    if segments.is_empty() {
        return Some(value.clone());
    }

    let key = segments[0];
    let rest = &segments[1..];

    match value {
        Value::Object(map) => navigate(map.get(key)?, rest),
        Value::Array(arr) => {
            let item = arr
                .iter()
                .find(|v| v.get("name").and_then(Value::as_str) == Some(key))?;
            navigate(item, rest)
        }
        _ => None,
    }
}

/// Fully resolve a request path.  Handles all query patterns:
///
/// - Single field:      `/battery_level`           → `{"battery_level": 100}`
/// - Comma fields:      `/cpu_temp,gpu_temp`       → `{"cpu_temp": 34.4, …}`
/// - Wildcard:          `/cores/*/usage`            → `[{"name":"cpu0","usage":…}, …]`
/// - Wildcard + commas: `/cores/all/usage,cur_freq` → `[{"name":"cpu0","usage":…,"cur_freq":…}, …]`
fn resolve_request(value: &Value, segments: &[&str]) -> Option<Value> {
    if segments.is_empty() {
        return Some(value.clone());
    }

    let current = segments[0];
    let rest = &segments[1..];
    let is_last = rest.is_empty();

    // ── Comma-separated fields (last segment only) ──────────────────────
    if is_last && current.contains(',') {
        return resolve_comma_fields(value, current);
    }

    // ── Wildcard: expand over every item in an array ────────────────────
    if is_wildcard(current) {
        let Value::Array(arr) = value else { return None };
        let results: Vec<Value> = arr
            .iter()
            .filter_map(|item| {
                if is_last {
                    return Some(item.clone());
                }
                let resolved = resolve_request(item, rest)?;
                Some(attach_name(resolved, item.get("name").cloned()))
            })
            .collect();
        return if results.is_empty() { None } else { Some(Value::Array(results)) };
    }

    // ── Standard navigation ─────────────────────────────────────────────
    match value {
        Value::Object(map) => {
            let child = map.get(current)?;
            if is_last {
                Some(serde_json::json!({ current: child }))
            } else {
                resolve_request(child, rest)
            }
        }
        Value::Array(arr) => {
            let item = arr
                .iter()
                .find(|v| v.get("name").and_then(Value::as_str) == Some(current))?;
            if is_last {
                Some(item.clone())
            } else {
                resolve_request(item, rest)
            }
        }
        _ => None,
    }
}

/// Extract comma-separated fields from a value.
fn resolve_comma_fields(value: &Value, raw: &str) -> Option<Value> {
    let mut result = serde_json::Map::new();
    for field in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(val) = navigate(value, &[field]) {
            result.insert(field.to_string(), val);
        }
    }
    if result.is_empty() { None } else { Some(Value::Object(result)) }
}

/// Prepend a `"name"` key to an object for identification in wildcard results.
fn attach_name(value: Value, name: Option<Value>) -> Value {
    let Some(name_val) = name else { return value };
    let Value::Object(fields) = value else { return value };
    let mut out = serde_json::Map::with_capacity(fields.len() + 1);
    out.insert("name".to_string(), name_val);
    out.extend(fields);
    Value::Object(out)
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
