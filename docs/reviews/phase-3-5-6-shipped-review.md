# Code Review: Shipped Phases 3E-G, 3H, 5C, 6A

## Reviewer: Claude Sonnet 4.6
## Date: 2026-05-05
## Scope: 334e0aa..e696379 (phase-3e-3f-3g merge through MCP P0 fix)

---

## Build / Test Summary

- `cargo build --release --workspace`: clean, zero warnings
- `cargo clippy --all-targets --workspace -- -D warnings`: clean, exits 0
- `cargo test --workspace`: 704 tests pass, 0 fail, 5 ignored (all requiring live
  external services — Postgres, DuckDB Postgres scanner — acceptable)
- All `#[allow(unused_variables, unused_assignments)]` suppressions on Phase 6A
  CLI modules did not hide actual warnings at the time of this review

---

## Critical Issues (must fix before next phase ships)

### CRIT-1: `predict()` standardization applied by position, not by feature name

**File:** `crates/mc-core/src/cube.rs`, lines 1114–1120  
**File:** `crates/mc-model/src/compile.rs`, lines 316–325

**Expected behavior:** When `standardization.params` is declared in a different
order than `coefficients`, the mean/std for feature `X` must be looked up by
matching the feature name, not by positional index.

**Actual behavior:** `compile.rs` serializes standardization params as a
`Vec<(f64, f64)>` in declaration order (`sc.params.iter()`), and `cube.rs` applies
them to features with positional `zip`. If a user writes:

```yaml
coefficients:
  - { feature: "Spend", weight: 0.5 }
  - { feature: "CPC",   weight: 0.3 }
standardization:
  params:
    - { feature: "CPC",   mean: 2.0, std: 0.5 }   # listed first
    - { feature: "Spend", mean: 1000.0, std: 200.0 }
```

then `zip` pairs `Spend`'s weight with CPC's mean/std and vice versa. The model
produces silently wrong predictions.

**Validation does not catch this:** `validate.rs` lines 1744–1767 check only that
every feature name in `standardization.params` appears in `coefficients`; it does
not check that the lists are in the same order.

**Suggested fix:** In `compile.rs`, build standardization in coefficient order:
sort or re-index `sc.params` to match the ordering of `fm.coefficients` before
emitting the `Vec<(f64, f64)>`. Alternatively, store `Vec<(String, f64, f64)>` and
do name-keyed lookup in eval. Add a validation error if `std_params.len() !=
coefficients.len()` when standardization is present.

---

### CRIT-2: `schema_version: "1.0"` missing from all Phase 6A verb JSON output

**Files:** `crates/mc-cli/src/query.rs` line 1097, `crates/mc-cli/src/whatif.rs`
line 252, `crates/mc-cli/src/trace.rs` (JSON formatter), `crates/mc-cli/src/sweep.rs`,
`crates/mc-cli/src/diff.rs`, `crates/mc-cli/src/write.rs`

**Expected behavior (per phase-6a handoff §"4 Commitments"):** "All `--format json`
output follows Phase 3B's diagnostic-envelope discipline. Every JSON response has
a `schema_version` field."

**Actual behavior:** `query`, `whatif`, `trace`, `sweep`, `diff`, and `write` all
emit JSON without a `schema_version` field. Only `tessera` (line 626) and the
legacy `model validate/inspect/lint` verbs (main.rs line 682) include it. An agent
that pins to `schema_version: "1.0"` and validates its presence will silently get
`null` or fail a required-field check on every Phase 6A verb response.

**Suggested fix:** Add `"schema_version": "1.0"` as the first field in every
`--format json` response from the Phase 6A verbs. A shared `push_json_envelope_header`
helper would keep this DRY.

---

### CRIT-3: Exit code 1 returned for I/O errors (file not found) instead of exit code 3

**Files:** `crates/mc-cli/src/query.rs` lines 126–127, `crates/mc-cli/src/whatif.rs`
lines 102, `crates/mc-cli/src/trace.rs` line 84, `crates/mc-cli/src/sweep.rs` line
167, `crates/mc-cli/src/diff.rs` line 94, `crates/mc-cli/src/write.rs` line 89

**Expected behavior (per phase-6a handoff §"4 Commitments"):** `1` = model/recipe
error (invalid YAML, validation failure). `2` = CLI usage error (bad flags/args).
`3` = I/O error (file not found, network failure).

**Actual behavior:** `load_model` failure for "could not read model file" returns
exit code `1`. Agents that distinguish "the model is broken" (`1`) from "the file
doesn't exist" (`3`) cannot route correctly. Both conditions currently return `1`.

**Suggested fix:** In `load_model`, distinguish `std::fs::read_to_string` I/O errors
from parse/validate errors. Return a typed error variant; callers map I/O errors to
exit code `3`.

---

## Major Issues (fix in current phase or next .x follow-up)

### MAJ-1: `time_format` field is parsed and validated but never consumed

**Files:** `crates/mc-recipe/src/schema.rs` line 258 (field declaration),
`crates/mc-recipe/src/error.rs` lines 224–228 (MC5030 definition)

**Expected behavior (per Phase 5C handoff + ADR-0014):** When a `column_mapping`
for the Time dimension references a non-ISO date column, `time_format` specifies
the strptime-style format string used to parse the source values. Example: `time_format:
"%m/%d/%Y"` for US-locale dates.

**Actual behavior:** `time_format` is a `pub time_format: Option<String>` on
`ColumnMappingConfig`. It is never read in `crates/mc-tessera/src/prepare.rs`,
`crates/mc-tessera/src/transform.rs`, or any driver. MC5030 (`TimeFormatRequired`)
fires (preventing ingestion) but the format string itself is silently ignored if
provided. Non-ISO date columns with a `time_format` configured are not parsed
correctly.

**Impact:** Models that set `time_format` believe they are configuring date parsing.
They are not. This is a silent data-correctness failure on any model with non-ISO
time columns.

**Suggested fix:** In `transform.rs` or the column-mapping resolution step, when
a Time-dimension column mapping is resolved, check for `time_format` and apply it
to convert source strings before matching against the Time dimension's element names.

---

### MAJ-2: Schedule registry `save()` is not atomic — non-atomic write could corrupt file

**File:** `crates/mc-tessera/src/schedule/registry.rs`, lines 76–81

**Expected behavior:** Schedule registry persistence survives daemon crash during
write, consistent with the watermark atomicity design (`incremental.rs` uses
temp-file + rename).

**Actual behavior:** `ScheduleRegistry::save()` calls `std::fs::write(&path, content)`
directly. If the daemon crashes mid-write (OOM, signal, etc.) the `.tessera/schedules.json`
file is truncated/corrupt and `ScheduleRegistry::load()` will deserialize-error,
silently creating a new empty registry and losing all schedules on the next start.
Compare to `incremental.rs:save_state` (lines 100–105) which correctly uses a
`.tmp` file then `fs::rename`.

**Suggested fix:** Mirror the `save_state` pattern: write to a `.tessera/schedules.json.tmp`
file, then `fs::rename` to the target path. This is atomic on POSIX file systems.

---

### MAJ-3: Cross-coordinate dependency graph not updated for `prev()`/`lag()`/`actual_ref()`

**File:** `crates/mc-core/src/cube.rs`, lines 431–508

**Expected behavior (per ADR-0011 §"Dependency graph implication", ADR-0012
§"Dirty propagation rule"):** Writing `M[T-1]` must dirty `prev(M)[T]`. The
dependency graph's reverse edges must include cross-time dependencies.

**Actual behavior:** `EvalLookup::Cross` calls route through `resolve_cross_coord_read`
(line 460) but their resolved coordinates are NOT added to `actual_reads` (line 457
only appends `EvalLookup::SelfRef` reads) and therefore NOT added to the dep-graph
forward/reverse edges (lines 494–508).

**Correctness impact:** Cells that use `prev()`/`lag()`/`actual_ref()`/`cumulative()`/
`rolling_avg()` WILL recompute correctly because the revision bump on any write
invalidates all previously-cached derived values (the belt-and-suspenders mechanism
at line 393–398). So this is NOT a silent wrong-answer bug in the current codebase.

**Performance impact:** Every write invalidates every cached derived cell regardless
of whether the write affected a cross-coordinate dependency. Writing `M[Jan]` should
only dirty `prev(M)[Feb]` but currently dirtied every derived cell in the cube.
For large cubes with many time periods, this over-invalidates widely.

**Why this is a Major rather than Critical:** Correctness is preserved via revision
checking. The cost is performance overhead only. However, the handoff documents
explicitly commit to tracking cross-coord deps in the dep graph. Phase 2D's dirty-
propagation work assumed granular dirty sets; this gap undermines that investment.

---

### MAJ-4: `sweep` with `--coefficient` reads and parses the model file 2N times for N parameter points

**File:** `crates/mc-cli/src/sweep.rs`, lines 162–165 (`load_model` per iteration),
lines 325–334 (`find_coefficient_index` calls `fs::read_to_string` + `parse` + `validate` per iteration)

**Expected behavior:** Model is loaded once; the sweep iterates over parameter
points on a single in-memory model clone.

**Actual behavior:** For a sweep with N points over a coefficient, the model YAML
is read from disk N times by `load_model` and an additional N times by
`find_coefficient_index`. A 20-point coefficient sweep reads and parses the model
file 40 times.

**Note:** The known issue "sweep reloads model N times" is already in the P1 queue,
but the `find_coefficient_index` double-parsing was not included in the queue
description.

---

## Minor Issues (technical debt, document and defer)

### MIN-1: `#[allow(unused_variables, unused_assignments)]` suppresses a whole warning category on Phase 6A modules

**File:** `crates/mc-cli/src/main.rs`, lines 25–40 (seven `#[allow(unused_variables,
unused_assignments)]` annotations on Phase 6A module declarations)

**Concern:** These annotations suppress all unused-variable warnings for the
`diff`, `query`, `sweep`, `trace`, `transform`, `whatif`, and `write` modules.
Legitimate warnings added during future development will be silently hidden.

**Suggested fix:** Remove the suppressions. If individual variables genuinely need
suppression in specific places, prefix them with `_` at the declaration site.

---

### MIN-2: `ScalarValue::Str(String)` added without `#[non_exhaustive]` on the enum

**File:** `crates/mc-core/src/value.rs`, lines 12–22

**Context:** `Str` was added in the Phase 3G/3H boundary work (commit `4e69e22`).
The comment says "Not stored in cells; only produced during eval by `DimElement`."
The type check in `CellDataType::matches` correctly rejects `Str` in writeback.

**Concern:** `ScalarValue` is a public type without `#[non_exhaustive]`. Adding
`Str` is technically a breaking change for external consumers that exhaustively match
on `ScalarValue`. It also has no matching `CellDataType` variant — the
`default_cell_data_type()` method maps `Str` to `CellDataType::F64` with a comment
"Str is transient; never stored" but this creates a dtype-vs-value mismatch that is
unintuitive. Consolidation match arms in `consolidation.rs` have no `Str` case and
will fall through to the `_` arm silently.

**Suggested fix:** Add `#[non_exhaustive]` to `ScalarValue` to signal that new
variants may be added. Document the `Str` invariant more prominently in the enum doc
comment.

---

### MIN-3: `lag()` and `rolling_avg()` period/window arguments cast to `i32`/`u32` with no overflow check

**File:** `crates/mc-core/src/rule.rs`, line 659 (`n as i32`), line 675 (`w as u32`)

**Context:** Rust's `as` cast for `f64 → i32` saturates (does not UB) on
overflow, but a very large `lag(Revenue, 1e18)` would silently produce
`offset = -(i32::MAX)`, which wraps on negation to `i32::MIN`, then produces a
large negative `target_idx` that returns Null rather than erroring. Not a crash,
but unexpected behavior that could confuse users.

**Suggested fix:** Add an in-range check before the cast: if `n.abs() > 1_000_000.0`,
return `ScalarValue::Null` (or `EngineError::InvalidArgument`).

---

### MIN-4: `unsafe` block in daemon.rs not documented as an exception in CLAUDE.md

**File:** `crates/mc-tessera/src/schedule/daemon.rs`, lines 273–283

**Context:** The daemon uses `unsafe` for POSIX signal handler registration.
The code is functionally correct (async-signal-safe atomic store, properly
commented). However, CLAUDE.md §3.1 lists `unsafe` as a "Forbidden pattern
(will fail review)."

**Suggestion:** Either add `#[forbid(unsafe_code)]` to `mc-tessera/src/lib.rs`
with an explicit module-level exception for daemon.rs, or document the daemon's
`unsafe` block in CLAUDE.md §2 as a known exception with rationale.

---

### MIN-5: MCP `run_cli_verb` responses have no `structured` JSON field for Phase 6A verbs

**File:** `crates/mc-cli/src/mcp.rs`, lines 844–856

**Context:** MCP tool responses for `mosaic.model.query`, `mosaic.model.whatif`,
`mosaic.model.trace`, `mosaic.model.sweep`, `mosaic.model.diff`, and
`mosaic.model.write` return `structured: None`. The legacy `validate`/`inspect`/
`lint` MCP tools return `structured: Some(envelope)` with parsed JSON. Agents
that call `mosaic.model.query --format json` receive their JSON only in the `stdout`
string, not as a structured object — requiring double-parse.

**Suggested fix:** For Phase 6A verbs, if the `--format json` flag is set and the
verb succeeds, parse the `stdout` string back to a `JsonValue` and set it as
`structured`. This is consistent with the existing pattern.

---

### MIN-6: `not(x)` and `if(cond == 0, ...)` use `x == 0.0` float equality

**File:** `crates/mc-core/src/rule.rs`, lines 566, 574

**Context:** `Expr::Not` at line 566 uses `x == 0.0`. `Expr::If` at line 574
uses `x == 0.0`. Both are comparing values that originated as `1.0` (true) or
`0.0` (false) from prior `bool_to_scalar` calls — so in practice the values are
always exactly representable. However, if a formula writer produces a "boolean"
via arithmetic (e.g., `Spend - Spend` which should be 0.0 but could have floating-
point noise), `not()` may give a wrong result.

**Suggested fix:** Change to `x.abs() < 1e-9` (matching the established epsilon
convention) for the falsy check, or document why the exact 0.0 comparison is safe
here.

---

## Confirmed Working (sanity checks that passed)

1. **Null propagation in comparisons (CRIT from methodology):** `eval_comparison`
   at `rule.rs:835–851` correctly returns `ScalarValue::Null` when either operand
   is Null. `Null > 5` returns Null, not `0.0` or `false`. Test
   `test_comparison_operators_return_null_on_null_input` (formula_integration.rs)
   covers this.

2. **`prev()` at first element returns Null:** `TimeOffset` boundary check at
   `cube.rs:803` — `target_idx < 0` returns `ScalarValue::Null`. Test
   `test_prev_at_first_period_returns_null` (formula_integration.rs:727) passes.

3. **`lag(m, n)` at out-of-bounds returns Null:** The same boundary check covers
   lag; `test_lag_positive` and `test_lag_negative_is_lead` cover both directions.

4. **`rolling_avg` partial windows:** Confirmed per-spec at `cube.rs:866–870`;
   start index falls back to 0 when fewer than W periods available. Per ADR-0012
   Decision 4 this is correct (Excel-compatible partial-window average).

5. **`bucket()` returns 0-based index:** `cube.rs:1038` returns `ScalarValue::F64(i as f64)`
   where `i` is 0-indexed. Per ADR-0013 Decision 6 this is correct.

6. **Null propagation in `if()` condition:** `if(Null, then, else)` evaluates to
   `else` per ADR-0011 Decision §"if() with Null condition". Confirmed at
   `rule.rs:572–573`.

7. **Watermark atomicity:** `incremental.rs:save_state` writes to a `.tmp` file
   then `fs::rename` — atomic on POSIX. Correct.

8. **Credential values not leaked in error messages:** `prepare.rs:281` includes
   only the key name in the error path, not the resolved value.

9. **No async/await outside mc-drivers source:** Grepped all non-driver crates.
   Comments mentioning "async" in daemon.rs are safety-comment prose, not code.

10. **No `unsafe` in mc-core, mc-model, mc-recipe, mc-fixtures, mc-cli:** Only
    `mc-tessera/src/schedule/daemon.rs` uses `unsafe`, for signal handler
    registration. `mc-drivers` uses `#![forbid(unsafe_code)]`.

11. **`ScalarValue::Str` is transient and blocked from storage:** `CellDataType::matches`
    returns `false` for `Str`, so writeback correctly rejects it. The NaN-reject
    check (line 1319) correctly skips non-F64 values.

12. **PAVA calibration raw-order validation:** `validate.rs:1807–1822` checks
    ascending `raw` values. Interpolation at `cube.rs:1156–1180` correctly clamps
    below first point and above last point.

13. **Logistic link function:** `cube.rs:1133` — `1.0 / (1.0 + (-linear_result).exp())`
    is the standard sigmoid. Correct.

14. **Schedule registry survives restart:** `ScheduleRegistry::load` reads from
    `.tessera/schedules.json`; `ScheduleRegistry::save` writes to the same path.
    Cron entries persist across daemon restarts (subject to MAJ-2 atomicity caveat).

15. **MCP stdout corruption fix:** `mcp.rs:40–64` holds a named `stdout` lock for
    the full request loop and flushes explicitly. The prior bug (interleaved writes
    from multiple code paths) is resolved.

---

## Things You Couldn't Verify Without More Info

1. **HTTP-JSON driver rate limiting:** `crates/mc-drivers/src/http_json_driver.rs`
   was not examined in detail. The Phase 5C handoff specifies a 4-requests/second
   limit with a sleep-250ms fallback. Could not confirm this is implemented without
   reading the full driver file.

2. **DuckDB/SQLite drivers on Rust 1.78:** Both drivers depend on native C libraries.
   Confirmed they build on this machine, but could not run integration tests without
   live databases. The 2 ignored DuckDB-Postgres tests and 3 ignored Postgres tests
   are the only test gap.

3. **Cron expression DST/timezone edge cases:** `cron_expr.rs` was not fully audited
   for leap-second and DST-boundary behavior. The implemented algorithm appears
   correct for the tested cases (`@every`, `@hourly`, `*/N`), but monthly/yearly
   cron expressions that cross DST transitions could schedule incorrectly.

4. **`predict()` with zero `std` in standardization:** The eval at `cube.rs:1116`
   skips standardization when `*std > 0.0` (correct — no division by zero). But
   the validation at `validate.rs:1758–1766` emits an error for `std <= 0`. If the
   validation error surfaces only as a warning (not blocking), the eval silently
   skips the standardization step. The severity level of the MC-code for this
   condition was not verified.

5. **`transform --source` HTTP authentication:** The known issue (curl subprocess
   instead of ureq) means auth headers, TLS client certs, and proxy settings cannot
   be configured without shell-level workarounds. Could not verify whether any
   recipe credential interpolation applies to curl arguments.

6. **Write-log replay not wired into `load_model`:** This is in the known-issues
   queue. Could not determine what the actual runtime behavior is when a model that
   has a write-log is loaded via `mc model query` — does it silently ignore the log
   or error?
