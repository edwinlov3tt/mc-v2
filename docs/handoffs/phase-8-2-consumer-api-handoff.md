# Phase 8.2 Handoff — Consumer API Surface (`/whatif`, `/sweep`, `/reload`)

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0032](../decisions/0032-phase-8-2-consumer-api-surface.md) (Accepted with 7 acceptance amendments — read amendments BEFORE the body; amendments win on conflicts)
**Estimated effort:** 2–3 sessions (~700-900 LOC across 3 handlers + types + OpenAPI + tests)
**Crate:** `mc-daemon` (extends Phase 8.0 substrate — `2800d12`); no kernel changes, no model-layer changes
**Branch:** `phase-8.2/consumer-api`

---

## What this phase ships

Three new HTTP endpoints on the existing `mc-daemon`:

- `POST /api/v1/whatif` — query with per-call transient overrides
- `POST /api/v1/sweep` — vary an input override OR a fitted-model coefficient across a range; return per-step measures
- `POST /api/v1/reload` — force re-read of cube YAMLs from disk (for floating-pin policy)

Plus `GET /api/v1/openapi.json` as the machine-readable contract for codegen.

Carved narrowly out of ADR-0029's originally-planned Phase 8.1 bundle. The other 8.1 items (MCP server, org mode, Tessera schedule integration, warm restart with content hashes, `/diff`/`/narrate`/`/snapshot`/`/rollback`) stay deferred until consumers surface.

**Independent of Phase 3L (`nbinom_sf`).** Both can ship in parallel.

---

## Required reading (in this order)

1. **ADR-0032 Amendments (CRITICAL — read first).** All 7 amendments are binding. They override the body where they conflict. Most consequentially:
   - **Amendment 1**: `/sweep` is reshaped around a `vary` discriminated union (`kind: "override"` is the PRIMARY mode; coefficient sweep is secondary). The body's "model + coefficient" contract is INCOMPLETE — read Amendment 1 for the actual contract.
   - **Amendment 2**: `metric` becomes a structured `{measure, agg, where?}` object.
   - **Amendment 3**: Explicit override-coord merge rule (overlay `vary.at`/`override.at` onto `where`; resolve to exactly one cell).
   - **Amendment 4**: `/reload` always returns 200 with `errors[]` — no mode-switching on cardinality.
   - **Amendment 5**: OpenAPI promoted to Decision 10; utoipa-generated.
2. **ADR-0032 body** — context, scope rationale, decisions 1-9 (interpret through amendments)
3. **ADR-0029** (substrate this builds on): [`../decisions/0029-phase-8-service-daemon.md`](../decisions/0029-phase-8-service-daemon.md)
4. **Phase 8.0 ship commit** (`2800d12`) — `crates/mc-daemon/` is the existing scaffold to extend
5. **Research note** (downstream demand signal): [`../research-notes/claw-core-first-downstream-consumer.md`](../research-notes/claw-core-first-downstream-consumer.md)
6. **claw-core ADR-0001**: https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md — the consumer contract this unblocks
7. **CLAUDE.md** — §6 self-check gates; daemon crate rules (tokio + axum permitted per ADR-0025 Rule 1.6)

---

## Phase 8.2 scope

| # | Item | Approx LOC |
|---|---|---|
| 1 | `/whatif` handler + request/response types | ~120 |
| 2 | `/sweep` handler with `vary.{override,coefficient}` discriminated union | ~250 |
| 3 | `/reload` handler with per-cube outcome reporting | ~100 |
| 4 | Override coord resolution helper (Amendment 3 merge rule) | ~50 |
| 5 | New diagnostics: SWEEP_TOO_LARGE / UNKNOWN_COEFFICIENT / OVERRIDE_TYPE_MISMATCH / RELOAD_IN_PROGRESS / UNKNOWN_AGGREGATION / AMBIGUOUS_COORDINATE / UNKNOWN_COORDINATE | ~40 |
| 6 | `utoipa` integration + `/openapi.json` endpoint | ~80 |
| 7 | Tests (~10 per endpoint = 30+ test functions) | ~250 |
| 8 | `docs/specs/daemon-api.md` (authoritative contract document) | ~200 (doc) |
| 9 | `README.md` for `mc-daemon` updated with curl examples for new endpoints | ~30 (doc) |

**Out of scope (do NOT implement):**
- `/diff`, `/narrate`, `/narrate-trends`, `/snapshot`, `/rollback` (Decision 1 deferral)
- `GET /api/v1/cubes/:name` (deferred — consumer derives from `GET /cubes`)
- MCP server / `mc mcp` (ADR-0029 Decision 6 — separate ADR when an agent consumer surfaces)
- Org mode dispatch logic (schema reserves `workspace` field but logic ships in a separate phase)
- Tessera schedule integration via daemon (ADR-0029 Decision 9)
- Warm restart with content hashes (ADR-0029 Decision 4)
- Filesystem watcher / auto-reload
- Streaming `/sweep` (`/sweep/stream` is a future-additive endpoint)
- Multi-dimensional sweeps (`vary[]` array — see Amendment 1 future-extension note)
- Cancellation tokens through the actor channel (Phase 9+ kernel work)

---

## Pre-flight checklist (before writing any code)

```bash
# 1. Diagnostic code preflight (Amendment 7 of ADR-0032)
grep -RE "MC4015|MC4016|MC4017|MC4018|MC4019|MC4020|MC4021" docs/ crates/ 2>/dev/null
# Expected: no matches. For any allocated code, shift to next unallocated MC40xx
# and update Decision 8 + Amendments 2/3 + the dispatch table accordingly.

# 2. Verify Phase 8.0 substrate is in place
cargo build -p mc-daemon
ls crates/mc-daemon/src/api/  # expect: query.rs, write.rs, trace.rs (the 8.0 endpoints)

# 3. Verify the kernel exposes whatif/sweep machinery
grep -RE "query_with_overrides|sweep" crates/mc-core/src/ crates/mc-model/src/ 2>/dev/null | head -10
# If query_with_overrides doesn't exist yet, the kernel needs a small additive function.
# See Step 1 below for the API shape and where to add it.

# 4. utoipa dep audit (Amendment 5)
cargo tree -p mc-daemon 2>/dev/null | head -20  # baseline
# After adding utoipa, re-run to verify no nalgebra/rand/etc. surprises.
# If dep tree explodes, fall back to hand-written openapi.json + drift test (Amendment 5 note).

# 5. Verify claw-core's Worker client expectations (smoke test)
# (Optional — the Worker contract is documented in the research note; the
# acceptance test below verifies real end-to-end flow.)

# 6. Clean working tree
git status
```

Record diagnostic code allocations, utoipa dep-tree-delta, and git SHA in chat before Step 1.

---

## Implementation path

### Step 0: Kernel additive (if needed)

If `Cube::query_with_overrides` doesn't already exist in `mc-core`, add a minimal additive function:

```rust
// crates/mc-core/src/cube.rs
impl Cube {
    /// Evaluate measures at `coords` with `overrides` applied transiently.
    /// Overrides do NOT mutate the cube, do NOT bump revision, do NOT touch
    /// the write journal. After this call returns, the cube is byte-identical
    /// to its pre-call state.
    pub fn query_with_overrides(
        &mut self,
        coords: &[CellCoordinate],
        overrides: &[(CellCoordinate, ScalarValue)],
        show: &[MeasureId],
    ) -> Result<Vec<QueryResult>, EngineError> {
        // ... implementation: stash override values in an eval-scoped override map,
        // run the eval, drop the override map on return
    }
}
```

This is additive; existing `query()` is unchanged. If the function already exists under a different name, use the existing one — surface the name in chat so the daemon handler can match. Phase 8.0 likely already has this for `/whatif` to have been planned at all.

### Step 1: `/whatif` handler (simplest of the three)

**File:** `crates/mc-daemon/src/api/whatif.rs` (new)

Mirror `crates/mc-daemon/src/api/query.rs` line-for-line. The only deltas:

1. Request type adds `overrides: Vec<WhatifOverride>` field
2. Each override goes through `merge_override_coord` (Amendment 3 helper — implement in Step 4)
3. Hands off to `Cube::query_with_overrides(merged_coords, override_values, show)`
4. Response shape is identical to `/query` (`{schema_version, results[{coord, value}]}`)

Types in `crates/mc-daemon/src/api/types.rs`:

```rust
#[derive(Deserialize, ToSchema)]
pub struct WhatifRequest {
    pub schema_version: Option<String>,   // accept "1.0" or absent
    pub cube: String,
    pub workspace: Option<String>,         // reserved for org mode
    pub overrides: Vec<WhatifOverride>,    // may be empty
    pub r#where: BTreeMap<String, String>,
    pub show: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct WhatifOverride {
    pub at: BTreeMap<String, String>,
    pub value: ScalarValueJson,            // typed enum so JSON numbers stay numbers
}
```

Resource bounds: reject `overrides.len() > 100` with HTTP 400 + diagnostic (Amendment 7 will allocate the code; semantic name TBD — `OVERRIDES_LIMIT_EXCEEDED`?).

### Step 2: `/reload` handler

**File:** `crates/mc-daemon/src/api/reload.rs` (new)

Per Amendment 4:
- Always returns HTTP 200 (unless request malformed/unauthorized)
- Per-cube outcomes go in `reloaded[]` or `errors[]`
- Explicitly named cold registered cubes get cold-loaded (treated as reload)
- Multi-cube reload runs sequentially through each cube's actor

Pseudocode:
```rust
async fn handle_reload(req: ReloadRequest, state: AppState) -> ReloadResponse {
    let cubes_to_reload = if req.cubes.is_empty() {
        state.cache.warm_cubes()  // omitted = reload warm cubes only
    } else {
        req.cubes.clone()
    };

    let mut reloaded = vec![];
    let mut errors = vec![];

    for cube_name in cubes_to_reload {
        match state.registry.lookup(&cube_name) {
            None => errors.push(ReloadError {
                cube: cube_name,
                code: "UNKNOWN_CUBE",
                message: "...",
            }),
            Some(handle) => {
                match handle.send_reload().await {
                    Ok(outcome) => reloaded.push(outcome),
                    Err(e) => errors.push(...),
                }
            }
        }
    }

    ReloadResponse { schema_version: "1.0", reloaded, errors }
}
```

The per-cube reload-actor logic (drain in-flight, recompile, replace) already exists in Phase 8.0 via the `mc reload` CLI verb — surface the same handler over HTTP.

Concurrency: if two reload requests arrive for the same cube, the second gets 409 (per Decision 8) via a per-cube `reload_in_progress` flag on the actor.

**Document Amendment 6** in `crates/mc-daemon/README.md` operational notes: "A 408 timeout on /reload does NOT cancel the underlying reload. Verify via GET /cubes before retrying."

### Step 3: `/sweep` handler (most complex)

**File:** `crates/mc-daemon/src/api/sweep.rs` (new)

Per Amendment 1's reshaped contract:

```rust
#[derive(Deserialize, ToSchema)]
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

#[derive(Deserialize, ToSchema)]
pub struct SweepRange {
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

#[derive(Deserialize, ToSchema)]
pub struct MetricSpec {
    pub measure: String,
    pub agg: AggKind,                       // mean | sum | min | max | count
    pub r#where: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, ToSchema)]
pub struct SweepRequest {
    pub schema_version: Option<String>,
    pub cube: String,
    pub workspace: Option<String>,
    pub vary: VaryBlock,
    pub r#where: BTreeMap<String, String>,
    pub overrides: Vec<WhatifOverride>,     // fixed overrides applied at every step
    pub show: Vec<String>,                  // measures returned per-step
    pub metric: Option<MetricSpec>,         // optional; when null, no "best"
    pub goal: SweepGoal,                    // maximize | minimize | none
}
```

Handler flow:
1. Validate point count: `((stop - start) / step).abs().floor() as usize + 1` must be ≤ 1000; else 400 `SWEEP_TOO_LARGE`
2. For `vary.kind: "override"`: merge `vary.at` over `where` (Amendment 3 helper). For `"coefficient"`: validate model + coefficient names.
3. Compute baseline: evaluate `show[]` at the original (non-overridden) cell or with model's published coefficient
4. For each sweep step value `v`:
   - For override mode: add `(merged_coord, v)` to the override list for this eval pass
   - For coefficient mode: substitute `v` into the model's coefficient
   - Evaluate `show[]` → `results: Vec<MeasureValue>`
   - If `metric` is present: aggregate `metric.measure` over `metric.where`-filtered cells → `metric_value`
   - Append `{value, results, metric}` to `sweep[]`
5. If `goal != "none"` and `metric` is present: find best step by goal
6. Return unified response shape from Amendment 1

The range iteration is closed-inclusive on `[start, stop]` (Decision 4 of body — unchanged). Floating-point step accumulation may drift; the last point should always be `stop` exactly (clamp before the final eval).

**Single-point range** (`start == stop`): sweep has one step, baseline + sweep[0] are computed; `best` = sweep[0] if `goal != none`.

**Descending range** (`start > stop`): step is interpreted as negative magnitude; iteration goes `start, start-|step|, ..., stop`.

### Step 4: Override coord resolution helper

**File:** `crates/mc-daemon/src/api/coord.rs` (new) — shared by `/whatif` and `/sweep`

```rust
/// Merge `override_at` onto `base_where` per ADR-0032 Amendment 3.
///
/// Returns a fully-qualified single coord, or one of:
/// - `UnknownDimension` — override mentions a dim not in the cube
/// - `UnknownElement` — override mentions an element not in the dim
/// - `AmbiguousCoordinate` — merged coord matches multiple cells
/// - `UnknownCoordinate` — merged coord matches zero cells
pub fn merge_override_coord(
    cube: &Cube,
    base_where: &BTreeMap<String, String>,
    override_at: &BTreeMap<String, String>,
) -> Result<CellCoordinate, MosaicError> {
    let mut merged: BTreeMap<String, String> = base_where.clone();
    for (dim, elem) in override_at {
        merged.insert(dim.clone(), elem.clone());
    }
    // Validate every dim mentioned exists in the cube
    for dim in merged.keys() {
        if !cube.has_dimension(dim) {
            return Err(MosaicError::UnknownDimension { ... });
        }
    }
    // Validate every element exists in its dim
    for (dim, elem) in &merged {
        if !cube.has_element(dim, elem) {
            return Err(MosaicError::UnknownElement { ... });
        }
    }
    // Resolve to cells matching the merged coord
    let matches = cube.find_cells(&merged);
    match matches.len() {
        0 => Err(MosaicError::UnknownCoordinate { coord: merged }),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(MosaicError::AmbiguousCoordinate {
            coord: merged,
            match_count: matches.len(),
        }),
    }
}
```

Tested independently in `crates/mc-daemon/tests/coord_resolution.rs`.

### Step 5: Diagnostic codes

In `crates/mc-daemon/src/error.rs` add the new variants:

```rust
pub enum MosaicError {
    // ... existing 8.0 variants ...
    SweepTooLarge { requested: usize, max: usize },          // MCxxxx — preflight
    UnknownCoefficient { model: String, name: String },       // MCxxxx — preflight
    OverrideTypeMismatch { expected: String, got: String },   // MCxxxx — preflight
    ReloadInProgress { cube: String },                        // MCxxxx — preflight (409)
    UnknownAggregation { name: String },                      // MCxxxx — preflight
    AmbiguousCoordinate { coord: BTreeMap<String, String>, match_count: usize }, // MCxxxx
    UnknownCoordinate { coord: BTreeMap<String, String> },    // MCxxxx
}
```

Each variant maps to:
- An HTTP status code (per Decision 8 table)
- A diagnostic code (allocated post-preflight per Amendment 7)
- A canonical error message format

Update the error-envelope formatter (`crates/mc-daemon/src/api/error_envelope.rs` or similar) to handle the new variants.

### Step 6: `utoipa` + `/openapi.json`

**Decision 10 (Amendment 5).** Add to `crates/mc-daemon/Cargo.toml`:

```toml
[dependencies]
utoipa = { version = "5", features = ["axum_extras"] }
```

Audit the new transitive deps:
```bash
cargo tree -p mc-daemon | grep -E "utoipa|indexmap|serde_json"
```

If the audit surfaces unexpected heavy deps (rand, nalgebra, full async stack), fall back to hand-written `openapi.json` + drift test. Decision in chat before proceeding.

Annotate every request/response type with `#[derive(ToSchema)]` and every endpoint handler with `#[utoipa::path(...)]`. Then expose:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        api::query::handle_query,
        api::write::handle_write,
        api::trace::handle_trace,
        api::whatif::handle_whatif,
        api::sweep::handle_sweep,
        api::reload::handle_reload,
    ),
    components(schemas(/* all request/response types */)),
)]
struct ApiDoc;

async fn openapi_json() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}
```

Mount at `GET /api/v1/openapi.json` with bearer-token auth (Amendment 5).

### Step 7: Tests

**Files:**
- `crates/mc-daemon/tests/whatif.rs` (~10 test fns)
- `crates/mc-daemon/tests/sweep.rs` (~12 test fns — both vary modes)
- `crates/mc-daemon/tests/reload.rs` (~8 test fns)
- `crates/mc-daemon/tests/coord_resolution.rs` (~6 test fns — Amendment 3 helper)
- `crates/mc-daemon/tests/openapi.rs` (~3 test fns — schema present, endpoints covered, auth-required)

Critical test cases:

**`/whatif`:**
- Empty overrides == `/query` parity
- Override that resolves to a single cell → applied; result reflects override
- Override with partial coord that merges with `where` to one cell → applied (Amendment 3 happy path)
- Override that merges to zero cells → 400 `UnknownCoordinate`
- Override that merges to multiple cells → 400 `AmbiguousCoordinate`
- Override on Derived cell → applied (substitutes formula result for this request)
- Overrides do NOT bump cube revision (verify before/after)
- Overrides do NOT touch write journal (verify journal size before/after)
- > 100 overrides → 400

**`/sweep` (override mode — Amendment 1):**
- Override sweep on `Market_Line` produces decreasing `P_Over` (sanity test against NBA cartridge)
- Override sweep with `goal: maximize` returns correct `best`
- Override sweep with `metric: null` returns null `best` and null per-step metric
- Override sweep with multi-measure `show` returns all measures per step
- Override coord that under-specifies → 400 `AmbiguousCoordinate`

**`/sweep` (coefficient mode — Amendment 1 secondary):**
- 11-point coefficient sweep matches CLI `mc model sweep` byte-for-byte (golden)
- Unknown model → 404 `UnknownModel` (existing 8.0 error)
- Unknown coefficient → 400 `UNKNOWN_COEFFICIENT`
- Unknown aggregation in `metric.agg` → 400 `UNKNOWN_AGGREGATION`

**`/sweep` general:**
- Range >1000 points → 400 `SWEEP_TOO_LARGE`
- Descending range works
- Single-point range works
- Step accumulation: last point is exactly `stop`

**`/reload`:**
- Single warm cube reload bumps revision
- Multi-cube reload runs sequentially
- Reload unknown cube → 200 with `errors[].UNKNOWN_CUBE` (Amendment 4)
- Reload omitted = reload warm only (cold cubes stay cold)
- Explicitly named cold cube → cold-loaded and reloaded (Amendment 4)
- Concurrent reload of same cube → 409 `RELOAD_IN_PROGRESS`
- Reload during in-flight query → query completes first, then reload runs (carries forward from 8.0 behavior)
- Mixed success/failure response shape is the same as all-success (Amendment 4)

**`/openapi.json`:**
- Endpoint returns valid OpenAPI 3.x JSON
- All 8.0 + 8.2 endpoint paths are covered
- All request/response types are in components.schemas
- Auth: requires bearer token when `api_key` configured

### Step 8: Build gates (CLAUDE.md §6)

```bash
cargo fmt --check --all
cargo clippy --all-targets --workspace -- -D warnings
cargo build --release --workspace
cargo test --workspace
cargo tree -p mc-daemon | wc -l                # delta from baseline; record in completion report
```

Then a manual smoke test against the running daemon:

```bash
# Start daemon
cargo run --release -p mc-daemon -- up --port 8787 --api-key test-key

# In another terminal — exercise each endpoint
curl -X POST http://127.0.0.1:8787/api/v1/whatif \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{ "schema_version": "1.0", "cube": "nba-totals", "overrides": [], "where": {...}, "show": ["P_Over"] }'

curl -X POST http://127.0.0.1:8787/api/v1/sweep \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{ "schema_version": "1.0", "cube": "nba-totals", "vary": {"kind":"override", "at":{"Game":"...","Measure":"Market_Line"}, "range":{"start":7.5, "stop":10.5, "step":0.5}}, "where": {...}, "show": ["P_Over"], "metric": null, "goal": "none" }'

curl -X POST http://127.0.0.1:8787/api/v1/reload \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{ "schema_version": "1.0", "cubes": ["nba-totals"] }'

curl http://127.0.0.1:8787/api/v1/openapi.json -H "Authorization: Bearer test-key" | jq .info.title
```

### Step 9: Documentation

**Create:** `docs/specs/daemon-api.md` — the human-readable contract.

Mirror the structure of OpenAPI but written for humans: one section per endpoint with request shape, response shape, error envelope, examples. Reference the live `/api/v1/openapi.json` as the machine-readable source of truth.

**Update:** `crates/mc-daemon/README.md` — add the operational notes section (Amendment 6 `/reload` 408 caveat goes here) and curl examples for the new endpoints.

---

## Acceptance gate (binding — body criteria 1-21 + Amendment AC #22-#28)

Implementer reports each of these explicitly when claiming done:

**From the body of the ADR:**
- [ ] AC #1: `/whatif` accepts contract per Decision 3 (as amended); transient overrides; no revision bump; no journal touch
- [ ] AC #2: `/sweep` accepts contract per Decision 4 + Amendment 1; both vary modes work
- [ ] AC #3: `/reload` accepts contract per Decision 5; reloads named cubes or all warm cubes
- [ ] AC #4: Overrides do NOT persist (revision unchanged, journal unchanged)
- [ ] AC #5: Reload drains in-flight; concurrent reload → 409
- [ ] AC #6: Sweep >1000 points → 400 SWEEP_TOO_LARGE
- [ ] AC #7: All three endpoints honor bearer-token auth
- [ ] AC #8: Consistent error envelope across all endpoints
- [ ] AC #9: New diagnostic codes registered (final codes recorded in completion report)
- [ ] AC #10: Phase 8.0 endpoints unchanged
- [ ] AC #11: `daemon.toml` schema unchanged
- [ ] AC #12: No `mc-core` breaking changes (additive only if needed for `query_with_overrides`)
- [ ] AC #13: claw-core's `mosaic-runner-client.ts` `whatifCartridge()` works against the new endpoint on next daily-pull
- [ ] AC #14: `cargo test --workspace` passes
- [ ] AC #15: `cargo clippy --all-targets --workspace -- -D warnings` clean
- [ ] AC #16: `cargo fmt --check --all` clean
- [ ] AC #17: `/whatif` warm-path < 50ms p99
- [ ] AC #18: `/sweep` 11-point < 100ms
- [ ] AC #19: `/reload` single-cube < 2s
- [ ] AC #20: `docs/specs/daemon-api.md` exists and documents all three endpoint contracts verbatim
- [ ] AC #21: `GET /api/v1/openapi.json` returns valid OpenAPI 3.x

**From amendments:**
- [ ] AC #22: `/sweep` supports `vary.kind: "override"` AND `vary.kind: "coefficient"` (Amendment 1)
- [ ] AC #23: `metric` accepts structured `{measure, agg, where?}` object (Amendment 2)
- [ ] AC #24: Override coord merge per Amendment 3; AmbiguousCoordinate / UnknownCoordinate / UnknownDimension / UnknownElement diagnostics all fire correctly
- [ ] AC #25: `/reload` returns 200 + `errors[]` for per-cube failures; cold cubes named explicitly are reloaded (Amendment 4)
- [ ] AC #26: `/openapi.json` generated via utoipa OR documented fallback (Amendment 5)
- [ ] AC #27: `/reload` 408 caveat documented in daemon README (Amendment 6)
- [ ] AC #28: Diagnostic codes assigned post-preflight; semantic names locked (Amendment 7)

---

## Effort and shape

- 2-3 sessions including build-gate self-check and manual smoke
- ~700-900 LOC of handler + types + helpers + tests (mostly mechanical — the kernel does the real work)
- One new direct dep (`utoipa`) — verify transitive tree before committing

---

## Common pitfalls (forewarned, forearmed)

1. **Implementing `/sweep` as coefficient-only.** The body of the ADR reads that way; Amendment 1 reshapes it. Read Amendment 1 before writing `sweep.rs`. The override-sweep mode is the PRIMARY workflow.
2. **Treating `metric: "mean"` as a valid string.** Amendment 2 made it a structured object. Reject string-form requests with 400.
3. **Forgetting the Amendment 3 merge rule.** Override coord = `where` overlaid with `at`. Validate the merged result resolves to exactly one cell. Test with under-specified coords (must return AmbiguousCoordinate).
4. **Returning HTTP 404 for an unknown-cube reload.** Amendment 4 says always 200 with `errors[]`. The single-cube vs multi-cube mode-switch was rejected.
5. **Implementing cold-cube reload as a no-op.** Amendment 4: explicitly named cold registered cubes get reloaded (cold-load → warm + revision bump).
6. **Streaming `/sweep` responses.** Rejected. Buffered only. Cap at 1000 points.
7. **Adding `nbinom_pmf` or other math primitives.** Wrong phase. That's ADR-0031 (Phase 3L).
8. **Implementing cancellation through the actor channel.** Phase 9+ kernel work. For 8.2, document Amendment 6's caveat and move on.
9. **Hardcoding MC4015-MC4021.** Preflight before allocation per Amendment 7.
10. **Treating utoipa as decided.** If the dep audit surfaces heavy transitives, fall back to hand-written + drift-test. Decision in chat.

---

## Cross-links

- ADR-0032: [`../decisions/0032-phase-8-2-consumer-api-surface.md`](../decisions/0032-phase-8-2-consumer-api-surface.md)
- ADR-0029 (substrate): [`../decisions/0029-phase-8-service-daemon.md`](../decisions/0029-phase-8-service-daemon.md)
- Research note: [`../research-notes/claw-core-first-downstream-consumer.md`](../research-notes/claw-core-first-downstream-consumer.md)
- claw-core ADR-0001 (downstream consumer contract): https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md
- claw-core Worker client (the codegen target): `claw-core/workers/<name>/src/mosaic-runner-client.ts`
- Sibling phase: [`./phase-3l-nbinom-sf-handoff.md`](./phase-3l-nbinom-sf-handoff.md) — independent; both can ship in parallel
- Phase 8.0 ship commit: `2800d12` (the substrate this extends)

---

## Sequencing recommendation

If both Phase 3L and 8.2 are in queue and only one can ship at a time, ship **this phase first**. Rationale:

1. Larger production impact — unblocks NBA immediately (NBA uses `norm_cdf` which already works); MLB Worker still ships baked `P_Over_NB` until Phase 3L also lands
2. The HTTP surface is the load-bearing piece for ADR-0001's slider workflow
3. Phase 3L without Phase 8.2 helps only local CLI users; Phase 8.2 without Phase 3L helps the Worker for any sport using continuous distributions

If both ship: MLB Worker gets the full live-prediction loop with native `nbinom_sf` computation over HTTPS.

---

## Completion report template

When done, write `docs/reports/phase-8-2-completion-report.md` covering:

1. Final MC diagnostic code assignments (post-preflight)
2. Dep-tree delta from utoipa (cargo tree before/after)
3. Test count per endpoint + total pass status
4. Performance numbers (whatif p99, sweep 11-pt latency, reload single-cube)
5. Manual smoke test results (curl outputs for each endpoint against running daemon)
6. claw-core integration verification: did `whatifCartridge()` Worker test pass on the next daily-pull tick?
7. Effort actual vs estimate
8. OpenAPI spec validation result (against an OpenAPI 3.x validator)
9. Anything surprising or worth amending in the ADR
