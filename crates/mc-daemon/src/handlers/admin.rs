//! Admin endpoints: health, status, cubes.
//!
//! Per ADR-0029 Decision 13:
//! - `GET /api/v1/health` — auth-exempt, minimal payload (status + uptime)
//! - `GET /api/v1/status` — auth-required, full diagnostics
//! - `GET /api/v1/cubes`  — list registered cubes with state

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::sync::Arc;

use crate::server::AppState;

/// `GET /api/v1/health` — minimal, auth-exempt.
///
/// Per ADR-0029 Decision 13: unauthenticated callers learn only whether
/// the service is up, not internal state.
pub async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime = state.started_at.elapsed().as_secs();
    let cache = state.cache.lock().await;
    let degraded = cache.degraded_cubes();
    let status = if degraded.is_empty() {
        "healthy"
    } else {
        "degraded"
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": status,
            "uptime_seconds": uptime
        })),
    )
}

/// `GET /api/v1/status` — full diagnostics, auth-required.
pub async fn handle_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime = state.started_at.elapsed().as_secs();
    let cache = state.cache.lock().await;
    let degraded = cache.degraded_cubes();
    let status = if degraded.is_empty() {
        "healthy"
    } else {
        "degraded"
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": status,
            "uptime_seconds": uptime,
            "cubes_registered": cache.registered_count(),
            "cubes_warm": cache.warm_count(),
            "cache_bytes_used": cache.current_bytes(),
            "cache_budget_bytes": cache.budget_bytes(),
            "pending_journal_entries": 0,
            "degraded_cubes": degraded
        })),
    )
}

/// `GET /api/v1/cubes` — list registered cubes.
pub async fn handle_cubes(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cache = state.cache.lock().await;
    let cubes: Vec<serde_json::Value> = cache
        .list_cubes()
        .into_iter()
        .map(|info| {
            serde_json::json!({
                "name": info.name,
                "state": info.state,
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!({ "cubes": cubes })))
}
