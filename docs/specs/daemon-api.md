# Mosaic Daemon API Specification

**Version:** 1.0
**Date:** 2026-05-27
**Authority:** ADR-0029 (Phase 8.0 substrate) + ADR-0032 (Phase 8.2 consumer API surface)
**Machine-readable source of truth:** `GET /api/v1/openapi.json` on any running daemon

---

## Overview

The Mosaic daemon (`mc-daemon`) exposes an HTTP API for querying, writing, and analyzing cube data. Phase 8.0 shipped the substrate (query/write/trace + admin). Phase 8.2 adds the consumer API surface: whatif, sweep, reload.

**Base URL:** `http://<host>:<port>/api/v1/`

**Authentication:** Bearer token. All endpoints except `/health` require `Authorization: Bearer <key>` when `api_key` is configured in `daemon.toml`.

---

## Phase 8.0 Endpoints (unchanged in 8.2)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/query` | Read cell values from a loaded cube |
| POST | `/api/v1/write` | Write a single cell value |
| POST | `/api/v1/trace` | Read with evaluation trace |
| GET | `/api/v1/health` | Health check (auth-exempt) |
| GET | `/api/v1/status` | Daemon status |
| GET | `/api/v1/cubes` | List registered cubes |

---

## Phase 8.2 Endpoints

### POST /api/v1/whatif

Query with transient overrides. Overrides apply only to this request. No revision bump, no journal touch.

**Request:**
```json
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "workspace": null,
  "overrides": [
    { "at": { "Game": "LAL_at_BOS", "Measure": "avg_pace" }, "value": 102.4 }
  ],
  "where": {
    "Scenario": "Base", "Version": "Working",
    "Sportsbook": "Pinnacle", "Time": "2026_04_15",
    "Game": "LAL_at_BOS"
  },
  "show": ["Predicted_Total", "P_Over"]
}
```

**Response (200):**
```json
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "results": [
    { "coord": { "Game": "LAL_at_BOS", "Measure": "Predicted_Total", ... }, "value": 228.13 },
    { "coord": { ... "Measure": "P_Over", ... }, "value": 0.54 }
  ]
}
```

**Override coord resolution (Amendment 3):** Each `override.at` is overlaid onto the top-level `where`. The merged coordinate must resolve to exactly one cell. Zero matches -> `UnknownCoordinate` (MC4021). Multiple matches -> `AmbiguousCoordinate` (MC4020).

**Resource bounds:** Max 100 overrides per request.

---

### POST /api/v1/sweep

Vary a value across a range, returning per-step measures.

**Two modes via `vary` discriminated union (Amendment 1):**

**Override mode (primary):**
```json
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "vary": {
    "kind": "override",
    "at": { "Game": "LAL_at_BOS", "Measure": "Market_Line" },
    "range": { "start": 7.5, "stop": 10.5, "step": 0.5 }
  },
  "where": { "Scenario": "Base", "Version": "Working", "Sportsbook": "Pinnacle", "Time": "2026_04_15" },
  "overrides": [],
  "show": ["P_Over", "Calibrated_P_Over"],
  "metric": null,
  "goal": "none"
}
```

**Coefficient mode (secondary):**
```json
{
  "vary": {
    "kind": "coefficient",
    "model": "nba_v16_lasso",
    "coefficient": "avg_pace",
    "range": { "start": 2.5, "stop": 3.5, "step": 0.1 }
  },
  ...
}
```

**Response (200):**
```json
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "vary": { "kind": "override", "at": {...}, "range": {...} },
  "baseline": {
    "value": 8.5,
    "results": [{ "measure": "P_Over", "value": 0.54 }, ...],
    "metric": null
  },
  "best": null,
  "sweep": [
    { "value": 7.5, "results": [...], "metric": null },
    ...
  ]
}
```

**Range:** Closed-inclusive `[start, stop]`. Max 1000 points.

**Metric (Amendment 2):** Structured `{measure, agg, where?}` or null. Agg values: mean, sum, min, max, count.

**Goal:** `maximize`, `minimize`, or `none`. Populates `best` when metric is present.

---

### POST /api/v1/reload

Force re-read of cube YAMLs from disk.

**Request:**
```json
{
  "schema_version": "1.0",
  "cubes": ["nba-totals"]
}
```

Omit `cubes` to reload all warm cubes. Cold cubes named explicitly are cold-loaded.

**Response (always 200 per Amendment 4):**
```json
{
  "schema_version": "1.0",
  "reloaded": [
    { "cube": "nba-totals", "previous_revision": 47, "new_revision": 48, "duration_ms": 312 }
  ],
  "errors": []
}
```

Per-cube failures go in `errors[]`. Unknown cube -> `errors[]` entry, not HTTP 404.

**408 caveat (Amendment 6):** A 408 timeout does NOT cancel the underlying reload. The actor continues to completion. Verify via `GET /cubes` before retrying.

---

### GET /api/v1/openapi.json

Machine-readable OpenAPI 3.x spec covering all 8.0 + 8.2 endpoints. Generated via utoipa. Consumers codegen client types against this spec.

---

## Error Envelope (Decision 8)

Phase 8.2 endpoints use the rich error envelope:

```json
{
  "schema_version": "1.0",
  "error": {
    "code": "UnknownDimension",
    "message": "Dimension 'Marketing' not registered in cube 'nba-totals'",
    "diagnostic": "MC4012",
    "context": { "cube": "nba-totals", "requested": "Marketing", "available": [...] }
  }
}
```

**Phase 8.2 diagnostic codes:**

| Semantic name | MC Code | HTTP | Description |
|---|---|---|---|
| SWEEP_TOO_LARGE | MC4015 | 400 | Sweep range exceeds 1000 points |
| UNKNOWN_COEFFICIENT | MC4016 | 400 | Coefficient not in fitted model |
| OVERRIDE_TYPE_MISMATCH | MC4017 | 400 | Override value type mismatch |
| RELOAD_IN_PROGRESS | MC4018 | 409 | Concurrent reload conflict |
| UNKNOWN_AGGREGATION | MC4019 | 400 | Invalid metric aggregation |
| AMBIGUOUS_COORDINATE | MC4020 | 400 | Override coord matches multiple cells |
| UNKNOWN_COORDINATE | MC4021 | 400 | Override coord matches zero cells |
| UNSUPPORTED_SCHEMA_VERSION | MC4022 | 400 | Invalid schema_version |

---

## Timeouts and Resource Bounds

| Endpoint | Default timeout | Max | Source |
|---|---|---|---|
| /whatif | 60s | 100 overrides | daemon.toml |
| /sweep | 120s | 1000 points | daemon.toml |
| /reload | 60s | - | daemon.toml |
