//! `POST /api/v1/whatif` — query with transient overrides.
//!
//! Per ADR-0032 Decision 3: overrides are scoped to this request only.
//! No revision bump, no journal touch, no persistent side effects.
//! Response shape matches `/query` with `{schema_version, results[]}`.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mc_core::ScalarValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::actor::CubeRequest;
use crate::coord::merge_override_coord;
use crate::error_envelope::{validate_schema_version, MosaicError};
use crate::server::AppState;

/// Maximum overrides per request (Decision 9 resource bound).
const MAX_OVERRIDES: usize = 100;

#[derive(Debug, Deserialize)]
pub struct WhatifRequest {
    pub schema_version: Option<String>,
    pub cube: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub overrides: Vec<WhatifOverride>,
    #[serde(default, rename = "where")]
    pub where_clause: BTreeMap<String, String>,
    #[serde(default)]
    pub show: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WhatifOverride {
    pub at: BTreeMap<String, String>,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct WhatifResponse {
    pub schema_version: &'static str,
    pub cube: String,
    pub results: Vec<WhatifResultEntry>,
}

#[derive(Debug, Serialize)]
pub struct WhatifResultEntry {
    pub coord: BTreeMap<String, String>,
    pub value: serde_json::Value,
}

pub async fn handle_whatif(
    State(state): State<Arc<AppState>>,
    body: Result<Json<WhatifRequest>, axum::extract::rejection::JsonRejection>,
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

    // Validate schema_version.
    if let Err(e) = validate_schema_version(&req.schema_version) {
        return e.into_response();
    }

    // Resource bounds check.
    if req.overrides.len() > MAX_OVERRIDES {
        return MosaicError::OverridesLimitExceeded {
            count: req.overrides.len(),
            max: MAX_OVERRIDES,
        }
        .into_response();
    }

    if req.show.is_empty() {
        return MosaicError::BadRequest {
            detail: "'show' must contain at least one measure name".to_string(),
        }
        .into_response();
    }

    let mut cache = state.cache.lock().await;

    // Resolve cube.
    let key = match cache.resolve_key(&req.cube) {
        Some(k) => k,
        None => {
            return MosaicError::UnknownCube {
                cube: req.cube.clone(),
            }
            .into_response();
        }
    };

    let tx = match cache.get_or_load(&key).await {
        Ok(tx) => tx,
        Err(e) => {
            return MosaicError::CubeDegraded {
                cube: req.cube.clone(),
                reason: e,
            }
            .into_response();
        }
    };

    let refs = match cache.get_refs(&key) {
        Some(r) => r.clone(),
        None => {
            return MosaicError::ActorUnavailable {
                detail: "refs not found for warm cube".to_string(),
            }
            .into_response();
        }
    };

    // Drop the lock before I/O.
    drop(cache);

    // Resolve overrides: merge each override.at onto where_clause.
    let mut override_pairs: Vec<(mc_core::CellCoordinate, ScalarValue)> = Vec::new();
    for ov in &req.overrides {
        let (coord, _merged) =
            match merge_override_coord(&refs, &req.cube, &req.where_clause, &ov.at) {
                Ok(r) => r,
                Err(e) => return e.into_response(),
            };
        let value = match json_to_scalar(&ov.value) {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        override_pairs.push((coord, value));
    }

    // Build read coords: one per measure in `show`, each with where_clause.
    let mut read_coords = Vec::with_capacity(req.show.len());
    let mut read_coord_names: Vec<BTreeMap<String, String>> = Vec::new();
    for measure in &req.show {
        let mut coord_names = req.where_clause.clone();
        coord_names.insert("Measure".to_string(), measure.clone());
        match refs.coord_from_names(&coord_names) {
            Some(c) => {
                read_coords.push(c);
                read_coord_names.push(coord_names);
            }
            None => {
                return MosaicError::UnknownCoordinate { coord: coord_names }.into_response();
            }
        }
    }

    // Send WhatIf request to the actor.
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(CubeRequest::WhatIf {
            read_coords,
            overrides: override_pairs,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return MosaicError::ActorUnavailable {
            detail: "cube actor channel closed".to_string(),
        }
        .into_response();
    }

    match reply_rx.await {
        Ok(Ok(result)) => {
            let mut results = Vec::with_capacity(result.values.len());
            for (i, cv) in result.values.iter().enumerate() {
                results.push(WhatifResultEntry {
                    coord: read_coord_names[i].clone(),
                    value: scalar_to_json(&cv.value),
                });
            }
            let response = WhatifResponse {
                schema_version: "1.0",
                cube: req.cube,
                results,
            };
            Json(response).into_response()
        }
        Ok(Err(e)) => MosaicError::EngineError { detail: e }.into_response(),
        Err(_) => MosaicError::ActorUnavailable {
            detail: "actor response dropped".to_string(),
        }
        .into_response(),
    }
}

/// Convert a JSON value to a ScalarValue for overrides.
fn json_to_scalar(v: &serde_json::Value) -> Result<ScalarValue, MosaicError> {
    match v {
        serde_json::Value::Number(n) => {
            n.as_f64()
                .map(ScalarValue::F64)
                .ok_or_else(|| MosaicError::OverrideTypeMismatch {
                    expected: "number".to_string(),
                    got: format!("{v}"),
                })
        }
        serde_json::Value::Null => Ok(ScalarValue::Null),
        _ => Err(MosaicError::OverrideTypeMismatch {
            expected: "number or null".to_string(),
            got: format!("{v}"),
        }),
    }
}

fn scalar_to_json(v: &ScalarValue) -> serde_json::Value {
    match v {
        ScalarValue::F64(f) => serde_json::json!(f),
        ScalarValue::Null => serde_json::Value::Null,
        other => serde_json::json!(format!("{other:?}")),
    }
}
