//! `POST /api/v1/write` — write a single cell value.
//!
//! Per ADR-0029 Decision 8: write path is journal "pending" → apply to cube →
//! append to .tessera/writes.jsonl → journal "committed" → reply to client.
//! Phase 8.0 ships single-cell writes only.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::actor::{self, CubeRequest};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct WriteRequest {
    pub cube: String,
    pub coord: Vec<String>,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct WriteResponse {
    pub schema_version: String,
    pub status: String,
    pub revision_after: u64,
    pub dirty_count: usize,
    pub write_id: u64,
}

pub async fn handle_write(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WriteRequest>,
) -> impl IntoResponse {
    let mut cache = state.cache.lock().await;

    let key = match cache.resolve_key(&req.cube) {
        Some(k) => k,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("cube '{}' not found", req.cube)})),
            );
        }
    };

    let tx = match cache.get_or_load(&key).await {
        Ok(tx) => tx,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": e})),
            );
        }
    };

    let refs = match cache.get_refs(&key) {
        Some(r) => r.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "refs not found"})),
            );
        }
    };
    let dim_order = cache.get_dimension_order(&key).unwrap_or(&[]).to_vec();

    drop(cache);

    // Resolve coordinate
    let coord_names = actor::coord_names_from_array(&req.coord, &dim_order);
    let coord = match actor::resolve_coord(&refs, &coord_names) {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "could not resolve coordinate"})),
            );
        }
    };

    // Build canonical coord string for tessera writes.jsonl
    let coord_string = actor::coord_to_string(&coord_names, &dim_order);

    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(CubeRequest::Write {
            coord,
            coord_names: req.coord.clone(),
            coord_string,
            value: req.value,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "cube actor unavailable"})),
        );
    }

    match reply_rx.await {
        Ok(Ok(result)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "schema_version": "1.0",
                "status": "ok",
                "revision_after": result.revision_after,
                "dirty_count": result.dirty_count,
                "write_id": result.write_id,
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "actor response dropped"})),
        ),
    }
}
