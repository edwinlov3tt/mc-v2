# ADR-0032: Phase 8.2 — Consumer API Surface (`/whatif`, `/sweep`, `/reload`)

**Status:** Proposed
**Date:** 2026-05-27
**Deciders:** project owner
**Phase:** 8.2 (carved narrowly out of ADR-0029's Phase 8.1; ships ahead of MCP/org/Tessera/warm-restart)
**Crate:** `mc-daemon` (extends Phase 8.0 substrate; no kernel or model-layer changes)
**Prerequisites:**
- [ADR-0029](./0029-phase-8-service-daemon.md) — Phase 8.0 substrate (shipped `2800d12`)
- [ADR-0026](./0026-org-workspace-resource-scope-capability-grants.md) — workspace/org scope (carried forward in request shape)
- [Research note](../research-notes/claw-core-first-downstream-consumer.md) — claw-core's specific asks
- [ADR-0001 in claw-core](https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md) — the downstream consumer contract this unblocks

---

## Context

Phase 8.0 (ADR-0029, shipped `2800d12`) put a daemon on the network with three endpoints: `/query`, `/write`, `/trace` (plus `/health`, `/status`, `/cubes`). claw-core is now driving NBA predictions through it via Cloudflare Tunnel (`mosaic-primary.edwinlovett.com`) and a `mosaic-runner-client.ts` Worker. The substrate works — actor model, cache, journal, auth, signal handling all clean.

Three endpoints from ADR-0029 Decision 5 didn't ship in 8.0 and are now blocking claw-core's production prediction loop:

- **`/whatif`** (CRIT) — the prediction loop wants per-call coordinate overrides without polluting cube state. Today the Worker would have to `POST /write` then `POST /query`, which accumulates stale "tonight's features" cells across an entire season.
- **`/sweep`** (HIGH) — the load-bearing motivator for ADR-0001's slider workflow (and ADR-0001 AC-11's `<5s` 30-step ceiling). CLI proof exists (22ms for an 11-step sweep on NBA cartridge); HTTP path doesn't.
- **`/reload`** (MEDIUM) — ADR-0001 Decision 5's floating-pin policy needs daemons to pick up rebuilt cartridges without `launchctl unload + load`. Today they fall back to plist kickstart, heavier than needed.

ADR-0029 framed all remaining endpoints (whatif, sweep, diff, narrate, narrate_trends, snapshot, rollback) plus MCP + org mode + Tessera schedule integration + `mc ps`/`mc reload` + warm restart as a single Phase 8.1 bundle. That's ~5 parallel tracks and the right scope for a major release, not the right scope when one consumer has concrete blocking demand and the other tracks have no consumer driving them yet.

**This ADR carves a narrow track out of Phase 8.1: ship the three endpoints claw-core needs, in the right priority order, with contracts pinned for downstream code generation.** The remaining 8.1 items (MCP, org mode, Tessera schedules, mc ps, warm restart, narrate/diff/snapshot/rollback endpoints) stay deferred until a consumer surfaces or the project owner sequences them in.

The numbering reflects this: Phase 8.0 = substrate (shipped), Phase 8.2 = consumer API surface (this ADR). Phase 8.1 as originally scoped in ADR-0029 is implicitly superseded — its remaining items will be re-scoped into smaller demand-driven sub-phases (or rolled together into a future bundle if multiple consumers converge).

---

## Decisions

### Decision 1: Scope — three endpoints only

**Ships in this phase:**
- `POST /api/v1/whatif`
- `POST /api/v1/sweep`
- `POST /api/v1/reload`

**Explicitly out of scope (deferred):**
- `POST /api/v1/diff`, `/narrate`, `/narrate-trends`, `/snapshot`, `/rollback`
- `GET /api/v1/cubes/:name` (single-cube detail)
- MCP server (ADR-0029 Decision 6)
- Org mode dispatch (ADR-0029 Decision 10) — request schema reserves the `workspace` field per Decision 6 below; org-mode dispatch logic itself ships separately
- Tessera schedule integration (ADR-0029 Decision 9)
- Warm restart with content hashes (ADR-0029 Decision 4)
- Auto-reload filesystem watcher
- Streaming sweep responses (see Decision 4)

**Rationale.** claw-core's Worker (the only real downstream consumer in production today) needs these three endpoints. The others are speculative until a consumer asks. Scope discipline = ship sooner = consumer-validated contracts.

### Decision 2: Priority order (also the implementation order)

1. **`/whatif`** — unblocks the prediction loop; biggest production impact
2. **`/reload`** — unblocks the floating-pin daily-rebuild policy; smallest contract
3. **`/sweep`** — unblocks the slider workflow + ADR-0001 AC-11

Why this order: `/whatif` is the largest contract and the highest production criticality. `/reload` is the smallest and unblocks ops machinery — knock it out second. `/sweep` is the most architecturally interesting (range encoding, streaming question) and ships last so its design absorbs lessons from the first two.

Each endpoint is independently shippable. If `/sweep` discovers a design issue (e.g., needs streaming), `/whatif` and `/reload` don't have to wait.

### Decision 3: `/whatif` contract

```
POST /api/v1/whatif
Content-Type: application/json
Authorization: Bearer <key>   (if api_key configured)

Request:
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "workspace": null,                           // reserved for org mode; null/omit in single-workspace
  "overrides": [
    { "at": { "Game": "LAL_at_BOS", "Measure": "avg_pace" }, "value": 102.4 },
    { "at": { "Game": "LAL_at_BOS", "Measure": "combined_off_rating" }, "value": 225.1 }
  ],
  "where": {
    "Game": "LAL_at_BOS",
    "Scenario": "Base",
    "Version": "Working",
    "Sportsbook": "Pinnacle",
    "Time": "2026_04_15"
  },
  "show": ["Predicted_Total", "P_Over", "Calibrated_P_Over"]
}

Response (200):
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "results": [
    { "coord": { "Game": "LAL_at_BOS", "Measure": "Predicted_Total", ... }, "value": 228.13 },
    { "coord": { ... "Measure": "P_Over", ... },                              "value": 0.54  },
    { "coord": { ... "Measure": "Calibrated_P_Over", ... },                   "value": 0.52  }
  ]
}
```

**Sub-decisions:**

- **`overrides[]` is the canonical override format — not flat strings.** The CLI uses `--set 'Game=X,Measure=Y=value'` because shells make structured args hard; HTTP doesn't have that constraint, so we ship the structured form natively. Reasons: (1) no string parsing on the daemon side, (2) no ambiguity around commas in element names, (3) cleaner Worker codegen, (4) typed validation at parse time.
- **`overrides` is required but may be empty.** Empty `overrides[]` makes `/whatif` equivalent to `/query` (useful for code generators that route everything through one endpoint).
- **Overrides are transient.** They apply only to this request. The write journal is NOT touched. No revision bump. No dirty propagation persisted past the response. The kernel's existing whatif machinery (`Cube::query_with_overrides` or equivalent) already does this — the handler is a thin wrapper.
- **`where[]` and `show[]` semantics match `/query` verbatim.** Authors and consumers familiar with `/query` get `/whatif` for free with one extra field.
- **Override coords must match registered dimensions.** Coords mentioning unknown dimensions or unknown elements → 400 with `MosaicError::UnknownDimension` or `UnknownElement` (see Decision 8 below).
- **Override values must type-check against the target measure.** Numeric measure + string value → 400 with `MosaicError::TypeMismatch`. The kernel's existing override validation handles this; the endpoint surfaces the error.
- **No partial success.** If any override fails validation, the entire request fails. (Same semantic as `/write` — atomic-or-fail.)

### Decision 4: `/sweep` contract

```
POST /api/v1/sweep
Content-Type: application/json
Authorization: Bearer <key>   (if api_key configured)

Request:
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "workspace": null,
  "model": "nba_v16_lasso",                    // fitted model name (from model.yaml)
  "coefficient": "avg_pace",                   // name of coef in model.coefficients
  "range": { "start": 2.5, "stop": 3.5, "step": 0.1 },
  "metric": "mean",                            // mean | sum | min | max | count
  "metric_where": { "Time": "Q1_2026" },       // optional filter on which cells contribute
  "goal": "maximize",                          // maximize | minimize | none
  "overrides": [],                             // optional fixed overrides applied at every step
  "show": ["Predicted_Total"]                  // optional measures to surface alongside the metric
}

Response (200):
{
  "schema_version": "1.0",
  "cube": "nba-totals",
  "model": "nba_v16_lasso",
  "coefficient": "avg_pace",
  "metric": "mean",
  "goal": "maximize",
  "baseline": 226.43,                          // value at the model's published coefficient
  "best": { "value": 3.1, "metric": 230.12 },  // best point per goal (omitted when goal=none)
  "sweep": [
    { "value": 2.5, "metric": 226.10 },
    { "value": 2.6, "metric": 226.40 },
    ...
    { "value": 3.5, "metric": 228.05 }
  ]
}
```

**Sub-decisions:**

- **Range is a structured `{start, stop, step}` object — not a `"2.5:3.5:0.1"` string.** Same reasoning as `/whatif` overrides: no parsing on the daemon side, no locale-format ambiguity (some locales use `,` for decimal), explicit types.
- **`range` is closed-inclusive in `[start, stop]`.** Matches CLI behavior: `--range 2.5:3.5:0.1` produces 11 points (2.5, 2.6, …, 3.5). The contract specifies inclusive endpoints to lock this behavior even if floating-point step accumulation drifts slightly — the last point is always `stop` exactly.
- **Step direction follows `start → stop`.** If `start > stop`, `step` is treated as negative (sweep descends). If `start == stop`, sweep is a single-point query.
- **`metric_where` is optional.** When present, the metric aggregates only over cells matching the filter. When absent, the metric aggregates over the cube's full cartesian product at the model's input scope.
- **`goal: "none"` skips the `best` field in the response.** Useful when the consumer wants the raw curve without the daemon picking a winner.
- **Response is NOT streamed in 8.2.** Buffered JSON. Rationale: typical sweeps are 10-50 points and complete in <100ms (per claw-core's 22ms measurement on 11 points). Streaming adds chunked transfer + parser complexity for a feature nobody's asked for yet. If a future consumer needs streaming (e.g., 1000-point sweeps), file a separate ADR for the streaming variant — don't bake it in speculatively. The existing 120s `sweep_ms` timeout from `daemon.toml` is the bound.
- **Maximum point count: 1000.** Request with more than 1000 points (computed as `floor((stop - start) / step) + 1`) → 400 with `MosaicError::SweepTooLarge`. Prevents accidental DoS via huge ranges; this can be raised when streaming lands.
- **The `model` + `coefficient` pair must reference a real fitted model.** Unknown model name → 404 with `MosaicError::UnknownModel`. Unknown coefficient → 400 with `MosaicError::UnknownCoefficient`.

### Decision 5: `/reload` contract

```
POST /api/v1/reload
Content-Type: application/json
Authorization: Bearer <key>   (if api_key configured)

Request:
{
  "schema_version": "1.0",
  "cubes": ["nba-totals"]                      // omit to reload all warm cubes
}

Response (200):
{
  "schema_version": "1.0",
  "reloaded": [
    { "cube": "nba-totals", "previous_revision": 47, "new_revision": 48, "duration_ms": 312 }
  ],
  "errors": []                                 // populated on per-cube failures
}
```

**Sub-decisions:**

- **`cubes[]` may be omitted (or `null`).** Omitting means "reload every cube currently warm in the cache." Cold cubes stay cold. Rationale: a daily-pull rebuild may have changed multiple cartridges; one HTTP call covers all of them. Cold cubes will pick up the new YAML on their next cold-load — no work needed.
- **Reload semantics match ADR-0029 Decision 12 verbatim.** In-flight queries for cube X drain first; new requests queue behind the reload; if a Tessera import is active for X, reload blocks until the import completes. The endpoint blocks until reload finishes, then returns. No async / job-token pattern.
- **Per-cube failure is reported in the response — not in HTTP status.** A request that reloads 3 cubes where 1 fails returns 200 with `errors[]` populated for the failing cube. Rationale: the consumer needs to know which cubes succeeded (so they can retry only the failures). HTTP-level error would lose that granularity. Use 5xx only for daemon-internal failures (e.g., out of memory during recompile).
- **Reload is workspace-scoped.** In single-workspace mode (Phase 8.0 reality), all cubes belong to one workspace, so the cube name is unambiguous. In org mode (deferred), the request will add a `workspace` field. The schema reserves this for forward compatibility.
- **No filesystem watcher.** Manual reload only. Auto-reload remains a future enhancement; explicit reload makes the floating-pin policy auditable (the daily-pull script's HTTP call is the moment of truth, not a magic background watcher).
- **Revision numbers in the response are informational.** They expose Phase 8.0's existing per-cube revision counter so consumers can confirm the reload actually happened (defensive check against silent no-ops).

### Decision 6: Schema versioning — `"schema_version": "1.0"` carries forward

All three new endpoints carry the same `schema_version: "1.0"` field in request and response, matching Phase 8.0's existing endpoints. Future breaking changes bump to `"2.0"`; additive changes don't bump (consumers MUST ignore unknown fields).

The daemon validates `schema_version` on incoming requests:
- Missing or `null` → accept (lenient for early consumers)
- `"1.0"` → accept
- Other value → 400 with `MosaicError::UnsupportedSchemaVersion`

The same envelope shape (`schema_version` + endpoint-specific payload) is consistent across all endpoints — Workers can write a single envelope helper and reuse it everywhere.

### Decision 7: Authentication — inherits from Phase 8.0 verbatim

No new auth surface in 8.2:
- Bearer token (Decision 7 of ADR-0029) protects all three new endpoints when `api_key` is configured
- Without `api_key`, daemon refuses to bind to non-localhost (Phase 8.0 behavior unchanged)
- The endpoints are NOT exempt from auth (only `/health` is auth-exempt; everything else requires the bearer token when configured)

When org mode lands (deferred), capability-grant scoping (ADR-0026) will apply to these endpoints. For 8.2 it's the same single-key model as 8.0.

### Decision 8: Error envelope — consistent across all endpoints

```json
{
  "schema_version": "1.0",
  "error": {
    "code": "UnknownDimension",
    "message": "Dimension 'Marketing' not registered in cube 'nba-totals'",
    "diagnostic": "MC4012",
    "context": { "cube": "nba-totals", "requested": "Marketing", "available": ["Game", "Scenario", "Version", "Sportsbook", "Time", "Measure"] }
  }
}
```

| HTTP status | When |
|---|---|
| 400 | Malformed JSON, unknown coord/dimension/element, invalid override type, range too large, `goal` value invalid, unknown model/coefficient |
| 401 | Missing or invalid bearer token |
| 404 | Cube not found in workspace (`cube` field doesn't match a registered cube) |
| 408 | Request exceeded its timeout (default 60s; `/sweep` 120s — bounded by `daemon.toml`) |
| 409 | Reload requested while another reload of the same cube is in flight |
| 500 | Daemon-internal failure (panic, OOM, journal I/O error during whatif's transient evaluation) |
| 503 | Cube is loaded but degraded (e.g., model load failed); request can't proceed |

`MosaicError::*` error codes reuse the Phase 8.0 vocabulary; new codes added by this phase:
- **`SweepTooLarge`** — 400 — sweep range exceeds 1000 points
- **`UnknownCoefficient`** — 400 — coefficient name not in fitted model
- **`OverrideTypeMismatch`** — 400 — override value type doesn't match measure type
- **`ReloadInProgress`** — 409 — concurrent reload of same cube rejected

Each error code maps to an existing `MCxxxx` diagnostic where applicable, surfaced in the `diagnostic` field. New diagnostics needed in this phase:
- **MC4015** — sweep range exceeds maximum point count
- **MC4016** — unknown coefficient in fitted model
- **MC4017** — override value type mismatch
- **MC4018** — concurrent reload conflict

(Pre-flight: verify MC4015-MC4018 are unallocated on `main` before implementation; shift to next free codes if collisions exist.)

### Decision 9: Timeouts and resource bounds

| Endpoint | Default timeout | Source |
|---|---|---|
| `/whatif` | 60s | `daemon.toml` `default_ms` |
| `/sweep` | 120s | `daemon.toml` `sweep_ms` |
| `/reload` | 60s | `daemon.toml` `default_ms` |

Timeout enforcement happens inside the per-cube actor (the request times out from the daemon's perspective; the cube operation continues to completion in the actor and the result is discarded). Rationale: aborting a partially-applied whatif mid-evaluation would require kernel changes (Phase 9+); buffering the wasted work for 100ms-1min is cheaper than the alternative.

Resource bounds:
- Maximum request body: 10MB (inherited from `daemon.toml` `max_request_body_mb`)
- Maximum sweep points: 1000 (Decision 4)
- Maximum overrides per `/whatif`: 100 (new; prevents pathological bulk-edits via the wrong endpoint — `/write` is the right tool for bulk updates)

---

## Implementation plan

Each endpoint is a thin axum handler around an existing CLI evaluator. Total work: ~600-800 LOC across handlers + tests + request/response types.

### Step 1: `/whatif` handler

**File:** `crates/mc-daemon/src/api/whatif.rs` (new)

Pattern: mirror `crates/mc-daemon/src/api/query.rs` exactly. Differences:
- Request type adds `overrides: Vec<WhatifOverride>` field
- Hands off to `Cube::query_with_overrides(...)` (or equivalent — check existing kernel API; if missing, the kernel layer needs a small additive function that takes overrides, evaluates transiently, returns results, drops the override scratchpad)
- Response shape identical to `/query`

Tests (`crates/mc-daemon/tests/whatif.rs`):
1. Empty overrides → equivalent to `/query` (parity test)
2. Single override changes one cell → only dependent cells reflect override
3. Multiple overrides → all reflected
4. Override on unknown dimension → 400 `UnknownDimension`
5. Override on unknown element → 400 `UnknownElement`
6. Override with wrong type → 400 `OverrideTypeMismatch` (MC4017)
7. Overrides do NOT persist (verify revision unchanged after request)
8. Overrides do NOT touch write journal (verify journal byte-count unchanged)
9. >100 overrides → 400 (resource bound)
10. Concurrent `/whatif` requests on same cube serialize through actor (existing 8.0 behavior — verify it still holds)

### Step 2: `/reload` handler

**File:** `crates/mc-daemon/src/api/reload.rs` (new)

Pattern: similar to `/status` but takes cube list as input. Flow:
1. Parse request, extract `cubes[]` (or null = all warm)
2. For each cube: dispatch a `ReloadRequest` to its actor
3. Each actor: drain in-flight requests, hold new requests in channel, re-read YAML, recompile, replace `self.cube`, bump revision
4. Collect results across all actors, return JSON

Tests:
1. Reload single cube → revision bumps, new YAML applied
2. Reload all warm cubes (cubes omitted) → all reload
3. Reload non-existent cube → 404 in `errors[]`, other cubes still reload
4. Reload during in-flight query → query completes first, then reload runs
5. Two simultaneous reloads of same cube → second gets 409 `ReloadInProgress`
6. Reload fails (YAML now invalid) → error in `errors[]`, cube state unchanged (rollback)

### Step 3: `/sweep` handler

**File:** `crates/mc-daemon/src/api/sweep.rs` (new)

Pattern: mirror the CLI `mc model sweep` implementation. The eval loop already exists — wrap it in a handler.

Tests:
1. 11-point sweep matches CLI JSON output byte-for-byte (golden test)
2. `goal: "maximize"` returns correct best point
3. `goal: "none"` omits `best` field
4. Unknown model → 404 `UnknownModel`
5. Unknown coefficient → 400 `UnknownCoefficient` (MC4016)
6. Range >1000 points → 400 `SweepTooLarge` (MC4015)
7. Descending range (`start > stop`) works correctly
8. Single-point range (`start == stop`) returns 1-element sweep
9. With `metric_where` filter → metric aggregates only filtered cells
10. With `overrides[]` → overrides applied at every sweep point

### Step 4: Error mapping + new diagnostic codes

**File:** `crates/mc-daemon/src/error.rs` (extend)

Add the four new error variants + their MC codes. Verify against `main` that MC4015-MC4018 are unallocated; if collisions exist, shift to next free codes.

### Step 5: Request/response types

**File:** `crates/mc-daemon/src/api/types.rs` (extend)

Add `WhatifRequest`, `WhatifResponse`, `SweepRequest`, `SweepResponse`, `ReloadRequest`, `ReloadResponse`, `WhatifOverride`, `SweepRange`. All derive `Serialize`/`Deserialize` via the existing axum + serde pattern.

JSON schemas for these types are exposed via `GET /api/v1/openapi.json` (new endpoint) so claw-core's Worker can codegen client types. Optional but recommended — small lift, big payoff for downstream codegen.

### Step 6: Documentation

**Files:**
- `crates/mc-daemon/README.md` — add endpoint reference for the three new endpoints with curl examples
- `docs/specs/daemon-api.md` (new) — the OpenAPI-style spec that the JSON endpoint serves; this becomes the authoritative contract document

---

## Acceptance criteria

**Functional:**
1. `POST /api/v1/whatif` accepts the contract in Decision 3, returns results matching `/query` semantics with transient overrides applied
2. `POST /api/v1/sweep` accepts the contract in Decision 4, returns sweep results matching CLI `mc model sweep` output byte-for-byte for the same inputs
3. `POST /api/v1/reload` accepts the contract in Decision 5, reloads named cubes (or all warm cubes), reports per-cube success/failure
4. Overrides do NOT persist past the request — cube revision unchanged, journal unchanged
5. Reload drains in-flight requests before reloading; concurrent reload returns 409
6. Sweep with >1000 points returns 400 `SweepTooLarge`
7. All three endpoints respect bearer-token auth from Phase 8.0
8. All three endpoints return the same error envelope (Decision 8) on failure
9. New diagnostic codes MC4015-MC4018 (or next-free equivalents) registered in the diagnostic catalog

**Compatibility:**
10. Phase 8.0 endpoints (`/query`, `/write`, `/trace`, `/health`, `/status`, `/cubes`) unchanged in behavior or shape
11. `daemon.toml` schema unchanged (no new config fields required for 8.2; uses existing `default_ms`, `sweep_ms`, `max_request_body_mb`)
12. No `mc-core` changes (kernel additions, if any, are additive and pass existing tests unchanged)
13. claw-core's `mosaic-runner-client.ts` `whatifCartridge()` function transitions from "throws MosaicError: endpoint not yet implemented" to "returns the response" on daily-pull update — no Worker code changes required

**Build gates:**
14. `cargo test --workspace` passes
15. `cargo clippy --all-targets --workspace -- -D warnings` clean
16. `cargo fmt --check --all` clean

**Performance:**
17. `/whatif` warm-path latency: < 50ms p99 for typical NBA-cartridge-sized request (matches `/query` ceiling + override-application overhead)
18. `/sweep` 11-point sweep: < 100ms (current CLI baseline: 22ms; HTTP overhead budget: 78ms)
19. `/reload` single-cube: < 2s (matches Phase 8.0 cold-load ceiling)

**Documentation:**
20. `docs/specs/daemon-api.md` exists and documents all three endpoint contracts verbatim with examples
21. `GET /api/v1/openapi.json` returns valid OpenAPI 3.x for all 8.0 + 8.2 endpoints (consumers can codegen client types)

---

## Alternatives considered

### Alt 1: Bundle into ADR-0029 (amend, don't create new ADR)

Considered. Adding a "§Decision 15: HTTP endpoint contracts" section to ADR-0029 with the three endpoint specs.

**Rejected because:**
- ADR-0029 is already a long doc (substrate + 14 decisions); endpoint contracts would balloon it
- Independent ship gates are cleaner — 8.0 substrate + 8.2 endpoints are reviewed and tested separately
- The endpoint contracts pin a downstream code-generation surface (claw-core's Worker types). That deserves its own ADR for grep-ability later — when a Phase 9 consumer asks "where's the `/whatif` contract documented?" they should find ADR-0032, not buried in a substrate ADR
- ADR-0029 was written before Phase 8.0 shipped. Its Decision 5 listed endpoints aspirationally. Now that the substrate exists, the consumer-facing API surface deserves first-class treatment

### Alt 2: Ship all of Phase 8.1 (whatif/sweep/diff/narrate/snapshot/rollback + MCP + org + Tessera + warm restart)

Considered. The original ADR-0029 scope.

**Rejected because:**
- Five parallel tracks for a single "phase" is unreviewable. The phase becomes a marketing label, not a coherent shipping unit
- Only three of those endpoints have a real consumer (claw-core). The rest are speculative until someone asks
- Each deferred track (MCP, org mode, Tessera schedules, warm restart) deserves its own ADR with consumer-driven contracts. Building them all on speculation produces APIs that don't fit consumers
- Phase 8.2 shipping in 1-2 days unblocks claw-core's production prediction loop. Bundling with the other tracks delays that unblock by weeks

### Alt 3: JSON-RPC over WebSocket instead of REST

Considered. claw-core's research note explicitly mentioned this: "if a Mosaic LLM session writes a counter-research-note (e.g. 'we should ship these as JSON-RPC over `mc mcp` instead of REST'), claw-core will adapt."

**Rejected because:**
- Phase 8.0 already shipped REST. The existing endpoints (`/query`, `/write`, `/trace`) are REST. The Worker, Cloudflare Tunnel, and operational tooling (curl, http clients, logs) are all REST-shaped
- WebSocket adds connection-lifecycle complexity (reconnect, heartbeat, message ordering across reconnects) that REST doesn't have
- JSON-RPC's main appeal is bidirectional + streaming. We don't need bidirectional (consumer always initiates). Streaming is rejected separately (Decision 4) for /sweep
- MCP (Phase 8.1, deferred) is the right home for JSON-RPC-shaped tools for AI agents. The REST surface stays for HTTP consumers; both can coexist
- Switching transports mid-flight (during claw-core's production deployment) breaks ADR-0001 Decision 6's stability commitment

### Alt 4: Streaming `/sweep` responses (NDJSON / SSE)

Considered. For very large sweeps (1000+ points), buffered JSON might be slow to first byte.

**Rejected for 8.2 because:**
- claw-core's largest current use case is ~30 points. Buffered completes in <100ms
- The 1000-point cap (Decision 4) bounds latency: at ~2ms per point, worst case is ~2s of work + transmission
- Streaming adds NDJSON or SSE handling on the consumer side; adds complexity at both ends for a use case nobody's hit yet
- If a consumer needs >1000-point sweeps, file a separate ADR for the streaming variant. The buffered endpoint stays and the streaming endpoint is additive

If/when streaming lands, the right contract is probably `/api/v1/sweep/stream` as a separate endpoint with NDJSON response, so the buffered `/sweep` doesn't break for existing consumers.

### Alt 5: Flat string syntax for overrides + range (matching CLI)

Considered. claw-core's research note used the CLI-style string syntax:
```json
"set": ["Game=LAL_at_BOS,Measure=avg_pace=102.4"],
"range": "2.5:3.5:0.1"
```

**Rejected because:**
- Strings require parsing on the daemon side; structured forms don't
- Strings ambiguate on element names containing commas, equals signs, or colons (rare but possible in user-authored cubes)
- Structured forms enable better validation errors ("element 'avg_pace' has type number, got string" is clearer than "couldn't parse value '102.4'")
- The CLI string syntax is a shell ergonomic — it exists because zsh/bash make structured args painful. HTTP doesn't have that constraint
- Worker codegen on structured JSON is cleaner (Worker authors don't write string-formatters)

### Alt 6: Include `/diff`, `/narrate`, `/snapshot`, `/rollback`

Considered. They're all in ADR-0029 Decision 5's planned surface.

**Rejected for 8.2 because:**
- No active consumer asking for them
- Each has nontrivial contract decisions (e.g., `/snapshot` needs to decide: full clone or COW? in-memory or persisted? scoped to a workspace or cube?) that should be made when a consumer surfaces, not speculatively
- Each can land as its own micro-phase (8.3, 8.4, …) when demand surfaces

---

## Out of scope (explicit)

- `/diff`, `/narrate`, `/narrate-trends`, `/snapshot`, `/rollback` endpoints (no consumer demand)
- `GET /api/v1/cubes/:name` (consumer can derive from `GET /cubes`)
- MCP server (ADR-0029 Decision 6 — Phase 8.x, separate ADR when an agent consumer surfaces)
- Org mode request dispatch (ADR-0029 Decision 10 — schema reserves the field; logic ships separately)
- Tessera schedule integration via daemon (ADR-0029 Decision 9 — when Tessera authors need it)
- Warm restart with content hashes (ADR-0029 Decision 4 — operational nice-to-have, not blocking)
- Filesystem watcher / auto-reload (manual reload is explicit and auditable)
- Streaming `/sweep` responses (rejected above; future ADR if a consumer needs it)
- Multi-tenant / RBAC / user sessions (Phase 9)
- Kernel changes to support `&self` reads (Phase 9 exploration)
- Cartridge migration to formula-derived `P_Over_NB` for MLB (claw-core-side action, blocked on ADR-0031 `nbinom_sf` shipping)

---

## Cross-links

- **ADR-0029:** [`./0029-phase-8-service-daemon.md`](./0029-phase-8-service-daemon.md) — Phase 8.0 substrate (the foundation this builds on); Decision 5 listed these endpoints aspirationally
- **ADR-0031:** [`./0031-nbinom-sf-formula-function.md`](./0031-nbinom-sf-formula-function.md) — companion phase; once both ship, MLB cartridge's slider workflow works end-to-end
- **ADR-0026:** [`./0026-org-workspace-resource-scope-capability-grants.md`](./0026-org-workspace-resource-scope-capability-grants.md) — workspace scope reserved in request schema for forward compatibility
- **ADR-0025:** [`./0025-kernel-discipline-and-deployment-architecture.md`](./0025-kernel-discipline-and-deployment-architecture.md) — Shape 4 (this is the API surface for it)
- **Research note:** [`../research-notes/claw-core-first-downstream-consumer.md`](../research-notes/claw-core-first-downstream-consumer.md) — claw-core's demand signal
- **claw-core ADR-0001:** https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md — the downstream substrate contract
- **Phase 8.0 ship commit:** `2800d12` — the substrate this extends
- **Worker client:** `claw-core/workers/<name>/src/mosaic-runner-client.ts` — the first downstream code that codegens against this contract

---

## Notes

**Why now.** Phase 8.0 substrate works in production (Cloudflare Tunnel + bearer auth + Worker calls). The three endpoints are 1-2 days of work each (handler + tests + docs) for a multi-week production unblock. No reason to wait.

**Why 8.2, not 8.1.** ADR-0029's Phase 8.1 was scoped as a 5-track mega-bundle (endpoints + MCP + org + Tessera + warm restart). Phase 8.2 carves out the consumer-driven track. The other 8.1 items are deferred to demand-driven micro-phases. This is the same pattern as Phase 3K → 3L: small, focused, consumer-validated.

**On contract permanence.** The three endpoint contracts in this ADR will be referenced by claw-core's Worker code via codegen (`OpenAPI.json` → TypeScript types). Once codegen is wired, breaking these contracts breaks the Worker. Treat the request/response shapes as load-bearing — additive changes only after this ADR is Accepted.

**On the OpenAPI spec endpoint.** Phase 8.2 also ships `GET /api/v1/openapi.json` as an implicit Decision (called out in Implementation Step 5 but not numbered as a decision). This is the artifact downstream consumers codegen against. Treating it as part of the surface means the daemon owns the contract document, not a separate static JSON file that can drift from reality.

**Effort estimate.** ~2-3 days of focused work. Each endpoint is ~1 day including tests and docs. The OpenAPI spec endpoint is ~half a day. Total LOC ~600-800 across handlers + types + tests, mostly mechanical.

**Coordination with ADR-0031.** ADR-0031 (`nbinom_sf`) and this ADR are independent — neither blocks the other. If shipped in parallel:
- ADR-0031 alone: MLB cartridge gains live `P_Over_NB` computation via local CLI `mc model whatif`, but the Worker still ships baked values because no HTTP `/whatif`
- ADR-0032 alone: NBA cartridge gains over-HTTP slider workflow (already works since NBA uses `norm_cdf`); MLB Worker still ships baked `P_Over_NB`
- Both shipped: the full vision — MLB Worker `POST /api/v1/whatif` with overridden features, daemon computes `P_Over_NB` live via `nbinom_sf`, slider workflow works end-to-end for both sports

If sequencing one before the other, ship **this ADR first**. Rationale:
1. Larger production impact (unblocks production prediction loop for ALL sports, not just MLB)
2. NBA cartridge gets immediate benefit even without `nbinom_sf` (NBA uses `norm_cdf`)
3. claw-core's Worker can ship the Mosaic-driven prediction loop end-to-end for NBA, with MLB following when `nbinom_sf` lands

---

## Acceptance amendments

*(None as of authoring. Project owner review pending.)*
