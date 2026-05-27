//! `POST /api/v1/reload` — force re-read cube YAMLs from disk.
//!
//! Per ADR-0032 Decision 5 + Amendment 4:
//! - Always returns HTTP 200 (unless malformed/unauthorized).
//! - Per-cube outcomes go in `reloaded[]` or `errors[]`.
//! - Explicitly named cold registered cubes get cold-loaded.
//! - Multi-cube reload runs sequentially.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::actor::CubeRequest;
use crate::error_envelope::{validate_schema_version, MosaicError};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ReloadRequest {
    pub schema_version: Option<String>,
    #[serde(default)]
    pub cubes: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub schema_version: &'static str,
    pub reloaded: Vec<ReloadedCube>,
    pub errors: Vec<ReloadError>,
}

#[derive(Debug, Serialize)]
pub struct ReloadedCube {
    pub cube: String,
    pub previous_revision: u64,
    pub new_revision: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ReloadError {
    pub cube: String,
    pub code: String,
    pub message: String,
}

pub async fn handle_reload(
    State(state): State<Arc<AppState>>,
    body: Result<Json<ReloadRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Json(req) = match body {
        Ok(j) => j,
        Err(e) => {
            return MosaicError::BadRequest {
                detail: format!("Invalid JSON: {e}"),
            }
            .into_response();
        }
    };

    if let Err(e) = validate_schema_version(&req.schema_version) {
        return e.into_response();
    }

    let mut cache = state.cache.lock().await;

    // Determine which cubes to reload.
    let cube_names: Vec<String> = match req.cubes {
        Some(names) if !names.is_empty() => names,
        _ => cache.warm_cube_names(),
    };

    let mut reloaded = Vec::new();
    let mut errors = Vec::new();

    for cube_name in &cube_names {
        let key = match cache.resolve_key(cube_name) {
            Some(k) => k,
            None => {
                errors.push(ReloadError {
                    cube: cube_name.clone(),
                    code: "UnknownCube".to_string(),
                    message: format!("Cube '{cube_name}' not registered in workspace"),
                });
                continue;
            }
        };

        // Get or load the cube (cold-loads if needed per Amendment 4).
        let tx = match cache.get_or_load(&key).await {
            Ok(tx) => tx,
            Err(e) => {
                errors.push(ReloadError {
                    cube: cube_name.clone(),
                    code: "LoadFailed".to_string(),
                    message: e,
                });
                continue;
            }
        };

        // Drop cache lock is not possible here since we're in a loop.
        // Send Reload request to the actor. The actor handles reload
        // sequentially via the channel (FIFO ordering per ADR-0029 Decision 2).
        let (reply_tx, reply_rx) = oneshot::channel();
        if tx
            .send(CubeRequest::Reload { reply: reply_tx })
            .await
            .is_err()
        {
            errors.push(ReloadError {
                cube: cube_name.clone(),
                code: "ActorUnavailable".to_string(),
                message: "cube actor channel closed".to_string(),
            });
            continue;
        }

        match reply_rx.await {
            Ok(Ok(result)) => {
                reloaded.push(ReloadedCube {
                    cube: cube_name.clone(),
                    previous_revision: result.previous_revision,
                    new_revision: result.new_revision,
                    duration_ms: result.duration_ms,
                });
            }
            Ok(Err(e)) => {
                errors.push(ReloadError {
                    cube: cube_name.clone(),
                    code: "ReloadFailed".to_string(),
                    message: e,
                });
            }
            Err(_) => {
                errors.push(ReloadError {
                    cube: cube_name.clone(),
                    code: "ActorUnavailable".to_string(),
                    message: "actor response dropped".to_string(),
                });
            }
        }
    }

    drop(cache);

    let response = ReloadResponse {
        schema_version: "1.0",
        reloaded,
        errors,
    };

    Json(response).into_response()
}
