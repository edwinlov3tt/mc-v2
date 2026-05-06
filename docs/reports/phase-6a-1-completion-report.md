# Phase 6A.1 Completion Report — Review-Driven Fixes

**Branch:** `phase-6a-1/fixes`
**Inherits:** `e696379` (`fix(P0): MCP stdout corruption + 10 agent CLI integration tests`, 704/0 tests)
**Closes:** [`docs/reviews/phase-3-5-6-shipped-review.md`](../reviews/phase-3-5-6-shipped-review.md) findings selected for immediate fix
**Handoff:** [`docs/handoffs/phase-6a-1-fixes-handoff.md`](../handoffs/phase-6a-1-fixes-handoff.md)
**Implementer:** Claude Code (Opus 4.7)
**Date:** 2026-05-05

---

## Shipped

### Block 1 — Silent-correctness bugs

- **CRIT-1: name-keyed standardization in `predict()`.** `FittedModelData.coefficients: Vec<f64>` → `Vec<(String, f64)>`. `standardization: Option<Vec<(f64, f64)>>` → `Option<Vec<(String, f64, f64)>>`. Eval site at `crates/mc-core/src/cube.rs:1093-1138` builds an `ahash::AHashMap<&str, (f64, f64)>` keyed by feature name and pairs `(mean, std)` with each coefficient by name — declaration order in `standardization.params` no longer matters. `compile.rs:314-330` populates the new shape directly from parsed YAML; the only other in-tree consumer (`crates/mc-cli/src/sweep.rs:317`) was migrated to the tuple shape. Regression test `test_predict_with_out_of_order_standardization_params` in `crates/mc-model/tests/formula_integration.rs` proves the fix: with deliberately out-of-order params, the by-name eval gives `3.0` while the old positional code would have given `2391.015`.
- **MAJ-1: `time_format` now consumed at row-transform time.** New module `crates/mc-tessera/src/time_format.rs` (~340 lines including 16 unit tests) implements a hand-rolled strptime subset (`%Y %m %d %H %M %S %V %b %%`) plus `canonicalize_period` for `year`/`quarter`/`month`/`week`/`day` bucketing. `MappingTarget::Dimension` carries `is_time_dim`, `time_format`, `map_to_period`. `prepare.rs::resolve_column_plan` populates them from a Time-dim name set built off the validated model. `transform.rs` calls a new `maybe_canonicalize_time` helper before the `refs.element(...)` lookup at both call sites (wide-format and long-format batches). Per-row parse / canonicalize failures emit `MC5034` via the existing `TesseraErrorOwned` machinery, so `on_error: skip_row` / `quarantine` work as expected. Regression tests in `crates/mc-tessera/tests/time_format_ingest.rs` cover the happy path (US-locale `%m/%d/%Y` → monthly Time elements) and the parse-failure path.

### Block 2 — Envelope discipline (Phase 6A verb JSON)

- **CRIT-2: `schema_version: "1.0"` envelope on every Phase 6A verb.** New helper `crates/mc-cli/src/query.rs::push_json_envelope_header` writes `{"schema_version":"1.0", ` and is called by every JSON formatter — `query` (single-coord, where-filter, aggregate paths), `whatif` (live + dry-run), `trace` (tree wrapped under `"trace"`), `sweep`, `diff`, `write` (live + dry-run). New regression `test_all_phase_6a_verbs_emit_schema_version` in `agent_cli_integration.rs` runs all 7 verb invocations and asserts the field. `test_trace_returns_tree` was updated to navigate the new envelope shape (the trace tree now lives under `"trace"`).
- **CRIT-3: I/O errors return exit 3, model errors return exit 1.** Introduced `query::LoadModelError` enum (`Io | Model`) with `exit_code()` mapping I/O → 3 and Model → 1. The first `std::fs::read_to_string` in `load_model` is the only Io site; parse / validate / resolve_inputs / compile errors all surface as Model. Every Phase 6A verb's `run_captured` propagates the new code via `e.exit_code()`. New regressions in `agent_cli_integration.rs`: `test_query_returns_exit_3_when_model_file_missing`, `test_query_returns_exit_1_when_model_invalid`.
- **MIN-5: MCP responses carry `structured` for Phase 6A verbs.** New helper `mcp.rs::run_cli_verb_json` lifts captured stdout into `ToolOutcome.structured` when exit_code is 0 and stdout is non-empty. The 6 Phase 6A MCP tools (`mosaic.model.{query, whatif, trace, sweep, diff, write}`) now use it; `tool_transform` keeps the older `run_cli_verb` because its format defaults to CSV. New regression `test_mcp_query_returns_structured_envelope` confirms the structured field carries `schema_version: "1.0"`.

### Block 3 — Polish

- **MAJ-2: `ScheduleRegistry::save` is atomic.** `crates/mc-tessera/src/schedule/registry.rs::save` now writes to `<dir>/.tessera/schedules.json.tmp` and `fs::rename`s into place — same pattern as `incremental.rs::save_state`. Regression `save_uses_tmp_rename_atomically` plants a stale `schedules.json.tmp` from a simulated prior crash and verifies `save` produces a clean final file with no `.tmp` left behind.
- **MIN-6: `not()` and `if()` use 1e-9 epsilon.** `crates/mc-core/src/rule.rs:566` and `:574` swapped from `x == 0.0` to `x.abs() < 1e-9` — the established project convention from CLAUDE.md §3.1. Regression `test_not_handles_arithmetic_zero` runs `not(Spend - Spend)` and asserts the result is `1.0` (true).
- **MIN-1: `#[allow(unused_variables, unused_assignments)]` removed from 7 mc-cli modules.** All seven module declarations in `crates/mc-cli/src/main.rs:25-40` are bare `mod`. The previously-suppressed warnings were addressed by underscore-prefixing genuinely-unused parameters (8 sites across `query.rs`, `diff.rs`, `whatif.rs`, `transform.rs`) and removing two pieces of dead code in `query.rs::run_aggregate` (`matched_count` was assigned and read in the same function but the `mut` style triggered a warning) and `query.rs::run_single_coord` (`let measure = ...` that was unused). One genuinely-dead `baseline` Option in `sweep.rs::run` was removed since the true baseline is computed separately. `cargo clippy --all-targets --workspace -- -D warnings` is now clean.
- **MIN-4: daemon's `unsafe` exception documented in CLAUDE.md.** §3.1's `unsafe anywhere` row gained a "Sole sanctioned exception" note covering `crates/mc-tessera/src/schedule/daemon.rs:273-283` (POSIX signal-handler registration, atomic store, no stable-Rust alternative). Any new `unsafe` site requires an ADR.

### Block 4 — Verification (read-only)

- **4.1 HTTP-JSON rate limit** — driver makes a single `ureq::get` per `fetch_batch`. No rate-limiting code exists (no `sleep`, no `Duration::from_millis`, no throttle); the Phase 5C handoff's "4 req/s with 250ms sleep fallback" is **not enforced**. Single-request usage masks the gap; a future pagination loop would surface it. Filed as Phase 5C debt; no fix in 6A.1.
- **4.2 `predict()` `std <= 0` severity** — `validate.rs:1758-1766` pushes `ValidationError::Schema { ... }`, which is the **hard error vector** returned by `validate()`. Behavior is correct: a model with `std <= 0` fails validation outright before any cube is compiled. **No change needed.**
- **4.3 Write-log replay on `load_model`** — `crates/mc-cli/src/write.rs` appends to `<model_dir>/.tessera/writes.jsonl`, but `load_model` (used by query / whatif / trace / sweep / diff / write itself) reads only YAML + canonical_inputs and **never replays the write log**. So `mc model write` persists a value that subsequent `mc model query` calls **silently ignore**. This contradicts the four-source state model rule (process-notes Rule 9) which requires query / whatif / trace / sweep / diff to replay all four sources including post-hoc writes. Already flagged as Phase 6A debt; **not fixed in 6A.1** per the handoff's "do not implement replay" directive.

### Other deliverables

- **MAJ-3 research note:** [`docs/research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md) — ~1 page documenting (a) why cross-coord eval bypasses `actual_reads` at `cube.rs:457`, (b) why the revision-bump belt-and-suspenders covers correctness so this is performance-only, (c) the open architectural questions a future fix-it phase needs to answer (parameterized vs. concrete edges, window-shaped deps, fan-in for `cumulative`, time-anchor coupling). No kernel change in 6A.1.
- **CLAUDE.md amendment:** §3.1 documents the `daemon.rs` `unsafe` exception (MIN-4 above).

---

## Headline Acceptance Gates

| # | Gate | Status |
|---|---|---|
| 1 | CRIT-1 fix lands; `test_predict_with_out_of_order_standardization_params` passes | ✓ |
| 2 | MAJ-1 fix lands; non-ISO date ingest test passes | ✓ |
| 3 | CRIT-2 fix lands; all 6 Phase 6A verbs emit `schema_version: "1.0"` | ✓ |
| 4 | CRIT-3 fix lands; I/O errors return exit 3, model errors return exit 1 | ✓ |
| 5 | MIN-5 fix lands; MCP `mosaic.model.query` (and siblings) return parsed `structured` | ✓ |
| 6 | MAJ-2 fix lands; `ScheduleRegistry::save()` is atomic via tmp+rename | ✓ |
| 7 | MIN-6 fix lands; `not()` / `if()` use the `1e-9` epsilon convention | ✓ |
| 8 | MIN-1 fix lands; the seven `#[allow(unused_variables, unused_assignments)]` annotations are gone | ✓ |
| 9 | MIN-4 fix lands; CLAUDE.md documents the daemon's `unsafe` exception | ✓ |
| 10 | Block 4 verification reported (HTTP-JSON rate limit, predict std<=0 severity, write-log replay behavior) | ✓ |
| 11 | MAJ-3 research note filed at `docs/research-notes/cross-coord-dep-graph.md` | ✓ |
| 12 | `cargo build --release --workspace` zero warnings | ✓ |
| 13 | `cargo clippy --all-targets --workspace -- -D warnings` exits 0 | ✓ |
| 14 | `cargo fmt --check --all` exits 0 | ✓ |
| 15 | `cargo test --workspace` passes (count up from 704; expect ~+8–12 new tests) | ✓ — **704 → 729 (+25 / 0 failed / 5 ignored)** |
| 16 | 10 consecutive `cargo test` runs identical | ✓ — `729/0/5` × 10 |
| 17 | Forbidden-pattern grep clean | ✓ — every match in `mc-core/src/` is inside `#[cfg(test)]` |

### Test count delta

```
baseline (e696379):  704 passed / 0 failed / 5 ignored
HEAD (Phase 6A.1):   729 passed / 0 failed / 5 ignored
delta:               +25
```

Breakdown of the 25 new tests:

| Block | New tests | Notes |
|---|---:|---|
| 1.1 (CRIT-1) | 1 | `test_predict_with_out_of_order_standardization_params` |
| 1.2 (MAJ-1) | 18 | 16 unit tests in `mc-tessera::time_format`; 2 integration tests in `time_format_ingest.rs` |
| 2.1 (CRIT-2) | 1 | `test_all_phase_6a_verbs_emit_schema_version` |
| 2.2 (CRIT-3) | 2 | exit-3 missing-file, exit-1 invalid-yaml |
| 2.3 (MIN-5)  | 1 | `test_mcp_query_returns_structured_envelope` |
| 3.1 (MAJ-2)  | 1 | `save_uses_tmp_rename_atomically` |
| 3.2 (MIN-6)  | 1 | `test_not_handles_arithmetic_zero` |

### Locked-surfaces grep

```
$ git diff e696379 -- crates/mc-fixtures/
$ # (zero lines — mc-fixtures is untouched)
```

### `mc-core` change scope (Hard Rule 2)

```
$ git diff e696379 -- crates/mc-core/ | grep -E "^---|^\+\+\+" | sort -u
--- a/crates/mc-core/src/cube.rs
--- a/crates/mc-core/src/rule.rs
+++ b/crates/mc-core/src/cube.rs
+++ b/crates/mc-core/src/rule.rs
```

`cube.rs`: `FittedModelData` reshape + `PredictModel` arm rewrite (CRIT-1 only).
`rule.rs`: lines 562–576 epsilon swap (Block 3.2 only).
No other `mc-core` files touched.

### Forbidden-pattern grep

```
$ grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
crates/mc-core/src/hierarchy.rs:339:            .expect("hierarchy must build");          # in #[cfg(test)] mod tests
crates/mc-core/src/hierarchy.rs:358:            .expect("hierarchy must build");          # in #[cfg(test)] mod tests
crates/mc-core/src/element.rs:198:            .expect("measure constructor populates measure_meta");   # in #[cfg(test)]
crates/mc-core/src/consolidation.rs: ...                                                  # all in #[cfg(test)] mod tests
```

Per CLAUDE.md §3.1: "Tests, benches, fixtures, and `mc-cli` may use `expect("static reason")`. `mc-core/src/` may not." All matches above are inside `#[cfg(test)] mod tests` blocks; production paths are clean.

---

## Verification Findings (Block 4)

### 4.1 HTTP-JSON driver rate limiting — gap confirmed

`crates/mc-drivers/src/http_json_driver.rs:78` issues a single `ureq::get` and never paginates. There is no rate-limiting code. The Phase 5C handoff specified "4 requests/second with sleep-250ms fallback" — that is **not implemented**. Today the gap is invisible because the driver only makes one request per `fetch_batch` (the entire JSON response is returned in full and sliced via the `cursor` field). A future driver expansion that adds pagination must enforce the rate limit. **Filed as Phase 5C debt; not fixed in 6A.1.**

### 4.2 `predict()` `std <= 0` validation severity — confirmed correct

`validate.rs:1758-1766` pushes `ValidationError::Schema`, which is hard error-class. A model with `std <= 0` cannot reach the cube layer. **No fix needed.**

### 4.3 Write-log replay — confirmed silent ignore

`crates/mc-cli/src/write.rs:175-194` writes successfully to `<model_dir>/.tessera/writes.jsonl`. `crates/mc-cli/src/query.rs::load_model` performs:

```
read_to_string → parse → validate → resolve_inputs → compile → apply_canonical_inputs
```

— and stops. The `.tessera/writes.jsonl` log is **never read**. So:

```
$ mc model write model.yaml --coord ... --value 999
Written: ... = 999 (was ...)        # ← persists to writes.jsonl

$ mc model query model.yaml --coord ...
... = (the original canonical_inputs value, NOT 999)   # ← silently ignored
```

This contradicts the four-source state model rule (process-notes Rule 9): query / whatif / trace / sweep / diff are supposed to replay post-hoc writes from `.tessera/writes.jsonl`. The Phase 6A completion report already flagged this as known debt; **6A.1 documents it but does not implement replay** per the handoff's explicit directive.

---

## Known Debt (self-audit per process-notes Rule 10)

These are the things 6A.1 deliberately did **not** fix, ordered by priority for a future phase. The `Out of Scope` table in the handoff covers some of these; this section adds detail and groups by maintenance-priority.

### P0 — silent operational gaps the user will hit soon

1. **Write-log replay not wired into `load_model`** (Block 4.3 above). Today `mc model write` looks like it works (`writes.jsonl` is appended), but subsequent reads silently ignore the write. This is the single most confusing user-facing gap in Phase 6A. A user running `mc model write` followed by `mc model query` will see no change and will reasonably conclude `write` is broken. **Fix priority: high.** The `four-source state model rule` (process-notes Rule 9) already specifies the contract — implementing it is mostly plumbing.

### P1 — performance debt that doesn't affect correctness today

2. **MAJ-3: cross-coord deps not in the dep graph** — see [`docs/research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md). Bulk revision-bump invalidation covers correctness; the cost is over-invalidation on writes. Performance-only impact, but undermines Phase 2D's granular-dirty-set work. ADR required before fix.
3. **MAJ-4: `sweep` reloads model + parses YAML 2N times for N points.** The `find_coefficient_index` function at `sweep.rs:325` re-reads + re-parses the YAML on every iteration. The known "sweep reloads model N times" debt item already covers the load side; this completion report adds the find_coefficient_index doubling. A 20-point coefficient sweep parses Acme YAML 40 times. Phase 6A.2 (already on the queue) should refactor to a single compile + in-memory mutate-and-rerun loop.
4. **HTTP-JSON driver rate limiting** (Block 4.1 above). Gap is benign for single-fetch usage; a pagination expansion would surface it.

### P2 — robustness / API hygiene, not user-visible today

5. **MIN-2: `ScalarValue::Str` added without `#[non_exhaustive]`.** Pre-V1 API freeze territory. Mosaic has no external consumers yet. Add at the next API-hygiene pass.
6. **MIN-3: `lag()` / `rolling_avg()` cast bounds.** Real but exotic. Bundle into a Phase 3F polish or 3I.
7. **Cron DST/timezone audit.** Already acknowledged as deserving a focused dedicated session. Future `phase-5c-1-cron-dst-audit`.

### Trade-offs taken deliberately

- **`time_format` strptime tokens are a subset.** Implemented `%Y %m %d %H %M %S %V %b %%` only. `%a` (weekday name), `%A` (full weekday), `%p` (AM/PM), and timezone tokens (`%z`, `%Z`) are not in scope — the project's recipe schema documents these tokens are recipe-driven and the existing column-mapping examples don't use them. If a future recipe needs them, extend `time_format.rs` rather than pulling in `chrono`.
- **`map_to_period: "week"` requires `%V` in the format string.** ISO week computation from a Y-M-D triple (Zeller's-variant) is non-trivial and not implemented. Recipes that need week-bucketing must include `%V`. The error message points this out.
- **Default canonical period when `time_format` set without `map_to_period` is `"day"` (YYYY-MM-DD).** This is the most general form. Daily Time dims will match; coarser dims will fail to find an element and emit MC5034. The Time dim's declared `granularity` could in principle drive this default, but threading granularity through `prepare → resolve → transform` was out of scope for a 30-line helper. Future enhancement.
- **MCP `structured` field is a stringified JSON envelope, not a parsed `JsonValue`.** Matches the existing `validate` / `inspect` / `lint` shape (line 449 sets `structured: Some(body)` where `body` is the raw string). Agents that want a parsed object call `JSON.parse(structured)`. A future API-cleanup pass might switch to `Option<JsonValue>` workspace-wide, but doing it for just the 6 new verbs would be inconsistent.
- **Block 1.1 LINT not added.** The handoff offered an "optional nudge" lint that fires when `standardization.params` is declared in a different order than `coefficients`. Skipped because (a) the bug is fully fixed by name-keyed lookup so the lint is purely stylistic, (b) the next-free MC code in `mc-model::lint` would have required digging through the namespace, and (c) the handoff explicitly said "Skip if it requires inventing a new code namespace." Filed as ergonomic polish for a future phase.
- **Schedule-registry crash test.** The MAJ-2 regression confirms the tmp+rename invariant by planting a stale `.tmp` file before a save. A more aggressive test (kill the process mid-write) would require subprocess machinery; the current test is enough to lock the invariant against silent regressions.

---

## SPEC QUESTIONs raised during implementation

**One**, resolved before any code was written:

> SPEC QUESTION: Phase 6A.1 binding-contract docs are missing from the repo

The kickoff prompt referenced `docs/handoffs/phase-6a-1-fixes-handoff.md` and `docs/reviews/phase-3-5-6-shipped-review.md` as the binding contract. Neither was committed to `phase-6a-1/fixes`. Resolved when the user pointed at the parallel `mc-v2` worktree (on `main`), where both docs existed as untracked files. Implementation proceeded from those. The completion report assumes those docs will be committed by the PM/spec-maintainer at integration time; if not, the references in this report and the research note will need updating.

No further SPEC QUESTIONs surfaced. The handoff was unambiguous on every decision.

---

## Files changed

```
 19 files changed, 885 insertions(+), 119 deletions(-)
```

| Layer | Files |
|---|---|
| Kernel (mc-core) | `crates/mc-core/src/cube.rs`, `crates/mc-core/src/rule.rs` |
| Model layer (mc-model) | `crates/mc-model/src/compile.rs`, `crates/mc-model/tests/formula_integration.rs` |
| Tessera (mc-tessera) | `crates/mc-tessera/src/lib.rs`, `crates/mc-tessera/src/prepare.rs`, `crates/mc-tessera/src/transform.rs`, `crates/mc-tessera/src/schedule/registry.rs`, `crates/mc-tessera/src/time_format.rs` *(new)*, `crates/mc-tessera/tests/time_format_ingest.rs` *(new)* |
| CLI (mc-cli) | `crates/mc-cli/src/main.rs`, `crates/mc-cli/src/query.rs`, `crates/mc-cli/src/whatif.rs`, `crates/mc-cli/src/trace.rs`, `crates/mc-cli/src/sweep.rs`, `crates/mc-cli/src/diff.rs`, `crates/mc-cli/src/write.rs`, `crates/mc-cli/src/transform.rs`, `crates/mc-cli/src/mcp.rs`, `crates/mc-cli/tests/agent_cli_integration.rs` |
| Process docs | `CLAUDE.md` (§3.1 unsafe-exception note), `docs/research-notes/cross-coord-dep-graph.md` *(new)* |

**`mc-fixtures` untouched** (Hard Rule 1 — locked surface).
**No new dependencies added** (Hard Rule 3).
**Toolchain unchanged** (Hard Rule 4).
**No `Cargo.lock` pin churn.**

---

## What's next

This report is filed as the bookkeeping artifact for `phase-6a-1/fixes` at HEAD. Per the kickoff prompt:

> **Stop. Do not commit or tag — that happens after PM review.**

Branch is left clean (working tree matches the changes documented above), gates green, ready for review. After PM acceptance:

1. Squash or curate commits per the project's commit-style preference.
2. Tag `phase-6a-1-review-fixes` (or similar — final tag name is the PM's call).
3. Promote the handoff and review docs from the `mc-v2` worktree's untracked state into the canonical `main` branch alongside this completion report.
4. Open follow-up issues for the P0 / P1 known debt enumerated above (write-log replay first).
