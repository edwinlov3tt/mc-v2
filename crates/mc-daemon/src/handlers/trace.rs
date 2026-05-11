//! `POST /api/v1/trace` — return computation trace for a cell.
//!
//! Per ADR-0029 Decision 5: mirrors `mc model trace --format json` output.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use mc_core::{ScalarValue, TraceNode, TraceOp};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::actor::{self, CubeRequest};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct TraceRequest {
    pub cube: String,
    pub coord: Vec<String>,
    #[serde(default)]
    pub depth: Option<usize>,
}

pub async fn handle_trace(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TraceRequest>,
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

    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(CubeRequest::Trace {
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
            let max_depth = req.depth.unwrap_or(20);
            let trace_json = match &result.trace {
                Some(root) => trace_node_to_json(root, &dim_order, &refs, max_depth, 0),
                None => serde_json::json!({
                    "coord": coord_names,
                    "value": scalar_to_json(&result.value),
                    "source": "input",
                    "child_count": 0,
                    "formula": null,
                    "inputs": []
                }),
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "schema_version": "1.1",
                    "trace": trace_json
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
    }
}

fn trace_node_to_json(
    node: &TraceNode,
    dim_order: &[String],
    refs: &mc_model::ModelRefs,
    max_depth: usize,
    current_depth: usize,
) -> serde_json::Value {
    let coord_names = elements_to_names(&node.coord, dim_order, refs);
    let coord_str = coord_names
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");

    let (source, formula) = match &node.operation {
        TraceOp::InputLookup { .. } => ("input", None),
        TraceOp::RuleEvaluation { rule_id, .. } => {
            let formula = refs
                .rules
                .iter()
                .find(|(_, &id)| id == *rule_id)
                .map(|(name, _)| name.clone());
            ("rule", formula)
        }
        TraceOp::Consolidation { .. } => ("consolidation", None),
        TraceOp::DefaultFallback { .. } => ("default", None),
        TraceOp::NullPoison { .. } => ("null_poison", None),
    };

    let inputs: Vec<serde_json::Value> = if current_depth >= max_depth {
        Vec::new()
    } else {
        node.children
            .iter()
            .map(|child| trace_node_to_json(child, dim_order, refs, max_depth, current_depth + 1))
            .collect()
    };

    serde_json::json!({
        "coord": coord_str,
        "value": scalar_to_json(&node.value),
        "source": source,
        "child_count": inputs.len(),
        "formula": formula,
        "inputs": inputs
    })
}

fn elements_to_names(
    coord: &mc_core::CellCoordinate,
    dim_order: &[String],
    refs: &mc_model::ModelRefs,
) -> BTreeMap<String, String> {
    let elements = coord.elements();
    let mut map = BTreeMap::new();
    for (i, dim_name) in dim_order.iter().enumerate() {
        if i < elements.len() {
            let elem_id = elements[i];
            // Reverse-lookup: find element name from (dim_name, elem_name) → ElementId map
            let elem_name = refs
                .elements
                .iter()
                .find(|((d, _), &id)| d == dim_name && id == elem_id)
                .map(|((_, name), _)| name.clone())
                .unwrap_or_else(|| format!("?{:?}", elem_id));
            map.insert(dim_name.clone(), elem_name);
        }
    }
    map
}

fn scalar_to_json(v: &ScalarValue) -> serde_json::Value {
    match v {
        ScalarValue::F64(f) => serde_json::json!(f),
        ScalarValue::Null => serde_json::Value::Null,
        other => serde_json::json!(format!("{other:?}")),
    }
}
