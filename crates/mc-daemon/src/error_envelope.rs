//! Rich error envelope for Phase 8.2 endpoints.
//!
//! Per ADR-0032 Decision 8: consistent error envelope across `/whatif`,
//! `/sweep`, `/reload`. Phase 8.0 endpoints keep their existing thin
//! `{"error": "..."}` envelope (AC #10: unchanged).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;

/// Semantic error codes for Phase 8.2 endpoints.
///
/// Each variant maps to an HTTP status code + MC diagnostic code.
/// Semantic names are locked per ADR-0032 Amendment 7; numeric MC
/// codes were confirmed unallocated at preflight (commit ce09f55).
#[derive(Debug)]
pub enum MosaicError {
    /// Cube name not in workspace registry.
    UnknownCube { cube: String },
    /// Dimension name not registered in cube.
    UnknownDimension {
        cube: String,
        requested: String,
        available: Vec<String>,
    },
    /// Element name not in dimension.
    UnknownElement {
        cube: String,
        dimension: String,
        requested: String,
        available: Vec<String>,
    },
    /// Merged coord resolves to zero cells.
    UnknownCoordinate { coord: BTreeMap<String, String> },
    /// Merged coord resolves to multiple cells (under-specified).
    AmbiguousCoordinate {
        coord: BTreeMap<String, String>,
        match_count: usize,
    },
    /// Override value type doesn't match measure type.
    OverrideTypeMismatch { expected: String, got: String },
    /// Sweep range exceeds 1000-point cap.
    SweepTooLarge { requested: usize, max: usize },
    /// Coefficient name not in fitted model.
    UnknownCoefficient { model: String, name: String },
    /// Concurrent reload of same cube rejected.
    ReloadInProgress { cube: String },
    /// Unknown aggregation function in metric spec.
    UnknownAggregation { name: String },
    /// Unsupported schema_version value.
    UnsupportedSchemaVersion { version: String },
    /// Too many overrides (max 100).
    OverridesLimitExceeded { count: usize, max: usize },
    /// Cube is loaded but degraded (e.g., model load failed).
    CubeDegraded { cube: String, reason: String },
    /// Actor unavailable (channel closed or panic).
    ActorUnavailable { detail: String },
    /// Kernel-level error (EngineError passthrough).
    EngineError { detail: String },
    /// Malformed JSON or missing required fields.
    BadRequest { detail: String },
}

impl MosaicError {
    /// HTTP status code for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::UnknownCube { .. } => StatusCode::NOT_FOUND,
            Self::UnknownDimension { .. } => StatusCode::BAD_REQUEST,
            Self::UnknownElement { .. } => StatusCode::BAD_REQUEST,
            Self::UnknownCoordinate { .. } => StatusCode::BAD_REQUEST,
            Self::AmbiguousCoordinate { .. } => StatusCode::BAD_REQUEST,
            Self::OverrideTypeMismatch { .. } => StatusCode::BAD_REQUEST,
            Self::SweepTooLarge { .. } => StatusCode::BAD_REQUEST,
            Self::UnknownCoefficient { .. } => StatusCode::BAD_REQUEST,
            Self::ReloadInProgress { .. } => StatusCode::CONFLICT,
            Self::UnknownAggregation { .. } => StatusCode::BAD_REQUEST,
            Self::UnsupportedSchemaVersion { .. } => StatusCode::BAD_REQUEST,
            Self::OverridesLimitExceeded { .. } => StatusCode::BAD_REQUEST,
            Self::CubeDegraded { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::ActorUnavailable { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::EngineError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
        }
    }

    /// Canonical error code string (PascalCase, used in JSON `code` field).
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCube { .. } => "UnknownCube",
            Self::UnknownDimension { .. } => "UnknownDimension",
            Self::UnknownElement { .. } => "UnknownElement",
            Self::UnknownCoordinate { .. } => "UnknownCoordinate",
            Self::AmbiguousCoordinate { .. } => "AmbiguousCoordinate",
            Self::OverrideTypeMismatch { .. } => "OverrideTypeMismatch",
            Self::SweepTooLarge { .. } => "SweepTooLarge",
            Self::UnknownCoefficient { .. } => "UnknownCoefficient",
            Self::ReloadInProgress { .. } => "ReloadInProgress",
            Self::UnknownAggregation { .. } => "UnknownAggregation",
            Self::UnsupportedSchemaVersion { .. } => "UnsupportedSchemaVersion",
            Self::OverridesLimitExceeded { .. } => "OverridesLimitExceeded",
            Self::CubeDegraded { .. } => "CubeDegraded",
            Self::ActorUnavailable { .. } => "ActorUnavailable",
            Self::EngineError { .. } => "EngineError",
            Self::BadRequest { .. } => "BadRequest",
        }
    }

    /// MCxxxx diagnostic code. Per Amendment 7 preflight: MC4015-MC4021
    /// confirmed unallocated, plus MC4022 for UnsupportedSchemaVersion.
    pub fn diagnostic(&self) -> Option<&'static str> {
        match self {
            Self::SweepTooLarge { .. } => Some("MC4015"),
            Self::UnknownCoefficient { .. } => Some("MC4016"),
            Self::OverrideTypeMismatch { .. } => Some("MC4017"),
            Self::ReloadInProgress { .. } => Some("MC4018"),
            Self::UnknownAggregation { .. } => Some("MC4019"),
            Self::AmbiguousCoordinate { .. } => Some("MC4020"),
            Self::UnknownCoordinate { .. } => Some("MC4021"),
            Self::UnsupportedSchemaVersion { .. } => Some("MC4022"),
            _ => None,
        }
    }

    /// Human-readable error message.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownCube { cube } => {
                format!("Cube '{cube}' not registered in workspace")
            }
            Self::UnknownDimension {
                cube,
                requested,
                available,
            } => {
                format!(
                    "Dimension '{requested}' not registered in cube '{cube}'. Available: {available:?}"
                )
            }
            Self::UnknownElement {
                cube,
                dimension,
                requested,
                available,
            } => {
                let avail_display: Vec<&str> =
                    available.iter().take(10).map(|s| s.as_str()).collect();
                format!(
                    "Element '{requested}' not found in dimension '{dimension}' of cube '{cube}'. \
                     Available (first 10): {avail_display:?}"
                )
            }
            Self::UnknownCoordinate { coord } => {
                format!("Merged coordinate resolves to zero cells: {coord:?}")
            }
            Self::AmbiguousCoordinate { coord, match_count } => {
                format!(
                    "Merged coordinate resolves to {match_count} cells (expected exactly 1): {coord:?}"
                )
            }
            Self::OverrideTypeMismatch { expected, got } => {
                format!("Override value type mismatch: expected {expected}, got {got}")
            }
            Self::SweepTooLarge { requested, max } => {
                format!("Sweep range has {requested} points, maximum is {max}")
            }
            Self::UnknownCoefficient { model, name } => {
                format!("Coefficient '{name}' not found in model '{model}'")
            }
            Self::ReloadInProgress { cube } => {
                format!("Reload already in progress for cube '{cube}'")
            }
            Self::UnknownAggregation { name } => {
                format!("Unknown aggregation '{name}'. Valid: mean, sum, min, max, count")
            }
            Self::UnsupportedSchemaVersion { version } => {
                format!("Unsupported schema_version '{version}'. Supported: 1.0")
            }
            Self::OverridesLimitExceeded { count, max } => {
                format!("Too many overrides: {count}, maximum is {max}")
            }
            Self::CubeDegraded { cube, reason } => {
                format!("Cube '{cube}' is degraded: {reason}")
            }
            Self::ActorUnavailable { detail } => {
                format!("Cube actor unavailable: {detail}")
            }
            Self::EngineError { detail } => {
                format!("Kernel error: {detail}")
            }
            Self::BadRequest { detail } => detail.clone(),
        }
    }

    /// Context map for the error envelope (additional structured details).
    pub fn context(&self) -> serde_json::Value {
        match self {
            Self::UnknownCube { cube } => {
                serde_json::json!({ "cube": cube })
            }
            Self::UnknownDimension {
                cube,
                requested,
                available,
            } => {
                serde_json::json!({
                    "cube": cube,
                    "requested": requested,
                    "available": available,
                })
            }
            Self::UnknownElement {
                cube,
                dimension,
                requested,
                available,
            } => {
                serde_json::json!({
                    "cube": cube,
                    "dimension": dimension,
                    "requested": requested,
                    "available": available,
                })
            }
            Self::AmbiguousCoordinate { coord, match_count } => {
                serde_json::json!({
                    "coord": coord,
                    "match_count": match_count,
                })
            }
            Self::SweepTooLarge { requested, max } => {
                serde_json::json!({
                    "requested": requested,
                    "max": max,
                })
            }
            _ => serde_json::Value::Null,
        }
    }
}

/// The wire-format error envelope per ADR-0032 Decision 8.
#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    schema_version: &'static str,
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic: Option<&'static str>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    context: serde_json::Value,
}

impl IntoResponse for MosaicError {
    fn into_response(self) -> Response {
        let status = self.status();
        let envelope = ErrorEnvelope {
            schema_version: "1.0",
            error: ErrorBody {
                code: self.code(),
                message: self.message(),
                diagnostic: self.diagnostic(),
                context: self.context(),
            },
        };
        (status, Json(envelope)).into_response()
    }
}

/// Validate `schema_version` field per ADR-0032 Decision 6.
/// Missing/null → accept. `"1.0"` → accept. Other → error.
pub fn validate_schema_version(version: &Option<String>) -> Result<(), MosaicError> {
    match version {
        None => Ok(()),
        Some(v) if v == "1.0" => Ok(()),
        Some(v) => Err(MosaicError::UnsupportedSchemaVersion { version: v.clone() }),
    }
}
