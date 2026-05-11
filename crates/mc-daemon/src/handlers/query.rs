//! `POST /api/v1/query` — read cell values from a loaded cube.
//!
//! Per ADR-0029 Decision 5: mirrors `mc model query` JSON format.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::actor::{self, CubeRequest, QueryResult};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub cube: String,
    #[serde(default, rename = "where")]
    pub where_clause: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub coord: Option<Vec<String>>,
    #[serde(default)]
    pub show: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub schema_version: String,
    pub results: Vec<QueryResultEntry>,
}

#[derive(Debug, Serialize)]
pub struct QueryResultEntry {
    pub coord: BTreeMap<String, String>,
    pub values: BTreeMap<String, serde_json::Value>,
}

pub async fn handle_query(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> impl IntoResponse {
    let mut cache = state.cache.lock().await;

    // Resolve cube key
    let key = match cache.resolve_key(&req.cube) {
        Some(k) => k,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("cube '{}' not found", req.cube)})),
            );
        }
    };

    // Get or load the cube actor
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
                Json(serde_json::json!({"error": "refs not found for warm cube"})),
            );
        }
    };
    let dim_order = cache.get_dimension_order(&key).unwrap_or(&[]).to_vec();

    // Drop the lock before doing I/O
    drop(cache);

    // Determine which measures to show
    let show_measures = req.show.unwrap_or_default();
    if show_measures.is_empty() && req.where_clause.is_none() && req.coord.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "either 'where' + 'show' or 'coord' is required"})),
        );
    }

    // Handle coord-array form (ordered element names)
    if let Some(coord_array) = &req.coord {
        let coord_names = actor::coord_names_from_array(coord_array, &dim_order);
        let coord = match actor::resolve_coord(&refs, &coord_names) {
            Some(c) => c,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "could not resolve coordinate"})),
                );
            }
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        if tx
            .send(CubeRequest::Query {
                coord,
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

        return match reply_rx.await {
            Ok(Ok(result)) => {
                let entry = build_result_entry(&coord_names, &result);
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "schema_version": "1.0",
                        "results": [entry]
                    })),
                )
            }
            Ok(Err(e)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            ),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "actor response dropped"})),
            ),
        };
    }

    // Handle where + show form
    let where_clause = req.where_clause.unwrap_or_default();
    let mut results = Vec::new();

    for measure in &show_measures {
        let mut coord_names = where_clause.clone();
        coord_names.insert("Measure".to_string(), measure.clone());

        let coord = match actor::resolve_coord(&refs, &coord_names) {
            Some(c) => c,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("could not resolve coordinate for measure '{measure}'")
                    })),
                );
            }
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        if tx
            .send(CubeRequest::Query {
                coord,
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
            Ok(Ok(result)) => {
                results.push((measure.clone(), result));
            }
            Ok(Err(e)) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e})),
                );
            }
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "actor response dropped"})),
                );
            }
        }
    }

    // Merge results into a single entry with all measure values
    let mut values = BTreeMap::new();
    for (measure, result) in &results {
        values.insert(measure.clone(), scalar_to_json(&result.value));
    }

    let entry = QueryResultEntry {
        coord: where_clause,
        values,
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "schema_version": "1.0",
            "results": [entry]
        })),
    )
}

fn build_result_entry(
    coord_names: &BTreeMap<String, String>,
    result: &QueryResult,
) -> serde_json::Value {
    let measure = coord_names
        .get("Measure")
        .cloned()
        .unwrap_or_else(|| "value".to_string());
    serde_json::json!({
        "coord": coord_names,
        "values": { measure: scalar_to_json(&result.value) }
    })
}

fn scalar_to_json(v: &mc_core::ScalarValue) -> serde_json::Value {
    match v {
        mc_core::ScalarValue::F64(f) => serde_json::json!(f),
        mc_core::ScalarValue::Null => serde_json::Value::Null,
        other => serde_json::json!(format!("{other:?}")),
    }
}
