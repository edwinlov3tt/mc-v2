//! `GET /api/v1/openapi.json` — machine-readable API contract.
//!
//! Per ADR-0032 Decision 10 / Amendment 5: generated via utoipa.
//! Consumers (claw-core Worker) codegen client types against this spec.

use axum::response::{IntoResponse, Response};
use axum::Json;
use utoipa::OpenApi;

use super::reload;
use super::sweep;
use super::whatif;

/// OpenAPI spec document covering all Phase 8.0 + 8.2 endpoints.
///
/// Per Amendment 5: bearer-token auth required when api_key configured.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Mosaic Daemon API",
        version = "1.0",
        description = "HTTP API for the Mosaic LNM platform daemon. Phase 8.0 (query/write/trace) + Phase 8.2 (whatif/sweep/reload)."
    ),
    paths(whatif::handle_whatif, sweep::handle_sweep, reload::handle_reload,),
    components(schemas(
        whatif::WhatifRequest,
        whatif::WhatifOverride,
        whatif::WhatifResponse,
        whatif::WhatifResultEntry,
        sweep::SweepRequest,
        sweep::VaryBlock,
        sweep::SweepRange,
        sweep::MetricSpec,
        sweep::SweepOverride,
        sweep::SweepResponse,
        sweep::SweepPoint,
        sweep::MeasureValue,
        sweep::SweepBest,
        reload::ReloadRequest,
        reload::ReloadResponse,
        reload::ReloadedCube,
        reload::ReloadError,
    ))
)]
struct ApiDoc;

pub async fn handle_openapi() -> Response {
    Json(ApiDoc::openapi()).into_response()
}
