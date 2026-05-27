//! `POST /api/v1/sweep` — parameter sensitivity analysis over HTTP.
//!
//! Per ADR-0032 Decision 4 + Amendment 1: two modes via `vary` discriminated union:
//! - `kind: "override"` — sweeps a value at one coordinate (PRIMARY mode)
//! - `kind: "coefficient"` — sweeps a fitted-model coefficient (secondary mode)

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

/// Maximum sweep points (Decision 4 resource bound).
const MAX_SWEEP_POINTS: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct SweepRequest {
    pub schema_version: Option<String>,
    pub cube: String,
    #[serde(default)]
    pub workspace: Option<String>,
    pub vary: VaryBlock,
    #[serde(default, rename = "where")]
    pub where_clause: BTreeMap<String, String>,
    #[serde(default)]
    pub overrides: Vec<SweepOverride>,
    #[serde(default)]
    pub show: Vec<String>,
    pub metric: Option<MetricSpec>,
    #[serde(default = "default_goal")]
    pub goal: String,
}

fn default_goal() -> String {
    "none".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum VaryBlock {
    Override {
        at: BTreeMap<String, String>,
        range: SweepRange,
    },
    Coefficient {
        model: String,
        coefficient: String,
        range: SweepRange,
    },
}

#[derive(Debug, Deserialize)]
pub struct SweepRange {
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

#[derive(Debug, Deserialize)]
pub struct MetricSpec {
    pub measure: String,
    pub agg: String,
    #[serde(default, rename = "where")]
    pub where_clause: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct SweepOverride {
    pub at: BTreeMap<String, String>,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SweepResponse {
    pub schema_version: &'static str,
    pub cube: String,
    pub vary: serde_json::Value,
    pub baseline: SweepPoint,
    pub best: Option<SweepBest>,
    pub sweep: Vec<SweepPoint>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SweepPoint {
    pub value: f64,
    pub results: Vec<MeasureValue>,
    pub metric: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MeasureValue {
    pub measure: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SweepBest {
    pub value: f64,
    pub metric: f64,
}

pub async fn handle_sweep(
    State(state): State<Arc<AppState>>,
    body: Result<Json<SweepRequest>, axum::extract::rejection::JsonRejection>,
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

    if req.show.is_empty() {
        return MosaicError::BadRequest {
            detail: "'show' must contain at least one measure name".to_string(),
        }
        .into_response();
    }

    // Validate goal.
    if !matches!(req.goal.as_str(), "maximize" | "minimize" | "none") {
        return MosaicError::BadRequest {
            detail: format!(
                "Invalid goal '{}'. Must be 'maximize', 'minimize', or 'none'",
                req.goal
            ),
        }
        .into_response();
    }

    // Validate metric.agg if present.
    if let Some(ref metric) = req.metric {
        if !matches!(
            metric.agg.as_str(),
            "mean" | "sum" | "min" | "max" | "count"
        ) {
            return MosaicError::UnknownAggregation {
                name: metric.agg.clone(),
            }
            .into_response();
        }
    }

    // Generate sweep points.
    let range = match &req.vary {
        VaryBlock::Override { range, .. } | VaryBlock::Coefficient { range, .. } => range,
    };

    let points = generate_sweep_points(range.start, range.stop, range.step);
    if points.len() > MAX_SWEEP_POINTS {
        return MosaicError::SweepTooLarge {
            requested: points.len(),
            max: MAX_SWEEP_POINTS,
        }
        .into_response();
    }

    let mut cache = state.cache.lock().await;

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

    drop(cache);

    // Build read coords from show[] + where_clause.
    let mut read_coords = Vec::with_capacity(req.show.len());
    for measure in &req.show {
        let mut coord_names = req.where_clause.clone();
        coord_names.insert("Measure".to_string(), measure.clone());
        match refs.coord_from_names(&coord_names) {
            Some(c) => read_coords.push(c),
            None => {
                return MosaicError::UnknownCoordinate { coord: coord_names }.into_response();
            }
        }
    }

    // Resolve fixed overrides.
    let mut fixed_overrides: Vec<(mc_core::CellCoordinate, ScalarValue)> = Vec::new();
    for ov in &req.overrides {
        let (coord, _) = match merge_override_coord(&refs, &req.cube, &req.where_clause, &ov.at) {
            Ok(r) => r,
            Err(e) => return e.into_response(),
        };
        let value = match json_to_scalar(&ov.value) {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        fixed_overrides.push((coord, value));
    }

    match &req.vary {
        VaryBlock::Override { at, .. } => {
            // Resolve the vary coord.
            let (vary_coord, _) =
                match merge_override_coord(&refs, &req.cube, &req.where_clause, at) {
                    Ok(r) => r,
                    Err(e) => return e.into_response(),
                };

            // Compute baseline: evaluate show[] WITHOUT the sweep override.
            let baseline_result = {
                let (reply_tx, reply_rx) = oneshot::channel();
                if tx
                    .send(CubeRequest::WhatIf {
                        read_coords: read_coords.clone(),
                        overrides: fixed_overrides.clone(),
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
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => return MosaicError::EngineError { detail: e }.into_response(),
                    Err(_) => {
                        return MosaicError::ActorUnavailable {
                            detail: "actor response dropped".to_string(),
                        }
                        .into_response()
                    }
                }
            };

            // Read baseline value at the vary coord.
            let baseline_value = {
                let (reply_tx, reply_rx) = oneshot::channel();
                if tx
                    .send(CubeRequest::WhatIf {
                        read_coords: vec![vary_coord.clone()],
                        overrides: vec![],
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
                    Ok(Ok(r)) => r.values[0].value.as_f64().unwrap_or(0.0),
                    _ => 0.0,
                }
            };

            let baseline_results: Vec<MeasureValue> = req
                .show
                .iter()
                .zip(baseline_result.values.iter())
                .map(|(name, cv)| MeasureValue {
                    measure: name.clone(),
                    value: scalar_to_json(&cv.value),
                })
                .collect();

            let baseline = SweepPoint {
                value: baseline_value,
                results: baseline_results,
                metric: None, // TODO: compute metric for baseline if specified
            };

            // Sweep: for each point, apply the override and evaluate.
            let mut sweep_results = Vec::with_capacity(points.len());
            for &v in &points {
                let mut step_overrides = fixed_overrides.clone();
                step_overrides.push((vary_coord.clone(), ScalarValue::F64(v)));

                let (reply_tx, reply_rx) = oneshot::channel();
                if tx
                    .send(CubeRequest::WhatIf {
                        read_coords: read_coords.clone(),
                        overrides: step_overrides,
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
                    Ok(Ok(r)) => {
                        let step_results: Vec<MeasureValue> = req
                            .show
                            .iter()
                            .zip(r.values.iter())
                            .map(|(name, cv)| MeasureValue {
                                measure: name.clone(),
                                value: scalar_to_json(&cv.value),
                            })
                            .collect();
                        sweep_results.push(SweepPoint {
                            value: v,
                            results: step_results,
                            metric: None, // TODO: compute metric if specified
                        });
                    }
                    Ok(Err(e)) => return MosaicError::EngineError { detail: e }.into_response(),
                    Err(_) => {
                        return MosaicError::ActorUnavailable {
                            detail: "actor response dropped".to_string(),
                        }
                        .into_response()
                    }
                }
            }

            // Find best if goal != "none" and metric is present.
            let best = find_best(&req.goal, &sweep_results);

            let vary_json = serde_json::json!({
                "kind": "override",
                "at": at,
                "range": { "start": range.start, "stop": range.stop, "step": range.step }
            });

            let response = SweepResponse {
                schema_version: "1.0",
                cube: req.cube,
                vary: vary_json,
                baseline,
                best,
                sweep: sweep_results,
            };
            Json(response).into_response()
        }
        VaryBlock::Coefficient {
            model, coefficient, ..
        } => {
            // Coefficient sweep is secondary mode. For Phase 8.2, return a
            // structured error indicating this mode is not yet implemented
            // if the model/coefficient infrastructure isn't wired.
            // TODO: implement coefficient sweep when model layer exposes
            // coefficient substitution.
            MosaicError::BadRequest {
                detail: format!(
                    "Coefficient sweep (model='{}', coefficient='{}') is planned but \
                     requires model-layer coefficient substitution not yet wired in Phase 8.2",
                    model, coefficient
                ),
            }
            .into_response()
        }
    }
}

/// Generate closed-inclusive sweep points from start to stop.
fn generate_sweep_points(start: f64, stop: f64, step: f64) -> Vec<f64> {
    if step.abs() < 1e-15 {
        return vec![start];
    }

    let ascending = start <= stop;
    let abs_step = step.abs();
    let (lo, hi) = if ascending {
        (start, stop)
    } else {
        (stop, start)
    };

    let n = ((hi - lo) / abs_step).floor() as usize + 1;
    let mut points = Vec::with_capacity(n);

    for i in 0..n {
        let v = if ascending {
            start + abs_step * i as f64
        } else {
            start - abs_step * i as f64
        };
        points.push(v);
    }

    // Clamp last point to exactly `stop` if overshoot occurred.
    if let Some(last) = points.last_mut() {
        if (ascending && *last > stop) || (!ascending && *last < stop) {
            *last = stop;
        }
    }

    // Ensure stop is included.
    if points.last().map_or(true, |&v| (v - stop).abs() > 1e-12) {
        points.push(stop);
    }

    points
}

fn find_best(goal: &str, sweep: &[SweepPoint]) -> Option<SweepBest> {
    if goal == "none" {
        return None;
    }

    let mut best: Option<(f64, f64)> = None;
    for pt in sweep {
        if let Some(m) = pt.metric {
            let is_better = match best {
                None => true,
                Some((_, prev_m)) => {
                    if goal == "maximize" {
                        m > prev_m
                    } else {
                        m < prev_m
                    }
                }
            };
            if is_better {
                best = Some((pt.value, m));
            }
        }
    }

    best.map(|(value, metric)| SweepBest { value, metric })
}

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
