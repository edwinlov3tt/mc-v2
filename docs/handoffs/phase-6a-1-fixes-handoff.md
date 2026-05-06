# Phase 6A.1 Handoff — Review-Driven Fixes

> **Audience:** the Claude Code instance that implements Phase 6A.1.
> **You inherit `main` at `e696379` (`fix(P0): MCP stdout corruption + 10 agent CLI integration tests`), 704/0 tests.**
>
> **This is a small follow-up phase that closes findings from
> [`docs/reviews/phase-3-5-6-shipped-review.md`](../reviews/phase-3-5-6-shipped-review.md).**
> It's bundled work, not a new capability — Phase 4A.1 was the same
> shape (small fixes against a shipped phase). Three blocks of fix
> work, one block of read-only verification, then commit + tag
> `phase-6a-1-review-fixes`.
>
> **Hard rule:** Phase 6A.1 modifies the files explicitly listed in
> each block. It does NOT touch `mc-fixtures` (locked since 1A). It
> DOES touch `mc-core` (one type-shape change for CRIT-1, scoped to
> `FittedModelData`). It does NOT add deps. It does NOT bump the
> toolchain. The locked-surfaces grep `git diff e696379 -- crates/mc-fixtures/`
> must return zero lines at the end.
>
> **Scope discipline:** if you discover a bug not on this list, file
> it. Do not fix it. Phase 6A.1's value is precisely that it ships
> the review's findings without scope creep.

---

## The one paragraph you must internalize

A Sonnet code review of phases 3E–G, 3H, 5C, and 6A surfaced three
critical findings, four major findings, and six minor findings. The
project owner triaged with Claude Desktop and selected eleven of them
for immediate fix. **CRIT-1 is the most important.** It's a silent
data-correctness bug: if a model declares `standardization.params` in
a different order than `coefficients`, `predict()` produces silently
wrong predictions and nothing flags it. This is the kind of bug that
ships to a customer and produces wrong forecasts for months. Fix it
first. Everything else in this handoff is correctness polish or
envelope-discipline cleanup that should ship together.

---

## Block 1 — Silent-Correctness Bugs (~3 hours, P0)

These are the two highest-priority items. Land them first.

### Block 1.1 — CRIT-1: `predict()` standardization applied by position, not feature name

**Files touched:**
- `crates/mc-core/src/cube.rs` (`FittedModelData` struct + `resolve_cross_coord_read`'s `PredictModel` arm)
- `crates/mc-model/src/compile.rs` (lines 314–330, where `FittedModelData` is populated)
- `crates/mc-model/tests/formula_integration.rs` (NEW regression test)

**The bug.** `FittedModelData.standardization` is currently
`Option<Vec<(f64, f64)>>` — positionally paired with `coefficients`.
`compile.rs:316-319` populates it from `sc.params.iter()` in
declaration order. `cube.rs:1115` zips the standardization with
`feature_values` positionally. If a user lists `standardization.params`
in any order other than the `coefficients` order, the means and stds
are paired with the wrong features. No error fires.

**The fix (LOCKED — name-keyed lookup at eval).** Change
`FittedModelData` so the engine looks up standardization by feature
name at eval time, not by position. This is a small intentional
amendment to a Phase-3H-shipped public type — the same lock-amendment
pattern as Phase 2D's `WritebackResult.invalidated` correction.

Specifically:

1. **`crates/mc-core/src/cube.rs`:**
   - Change `FittedModelData.coefficients: Vec<f64>` → `Vec<(String, f64)>` (feature name + weight, in declaration order).
   - Change `FittedModelData.standardization: Option<Vec<(f64, f64)>>` → `Option<Vec<(String, f64, f64)>>` (feature name + mean + std).
   - In `resolve_cross_coord_read`'s `PredictModel` arm (~line 1085 onward): when `standardization` is `Some`, build an `ahash::AHashMap<&str, (f64, f64)>` keyed by feature name; for each coefficient `(name, weight)` zipped with its `feature_value`, look up `(mean, std)` by `name` and apply `(val - mean) / std` if `std > 0.0`. The linear-combination loop iterates `model.coefficients.iter().zip(feature_values.iter())` using the weight from the tuple.

2. **`crates/mc-model/src/compile.rs`:**
   - Update the `fitted_models` population (lines 314–330) to emit `(c.feature.clone(), c.weight)` for coefficients and `(p.feature.clone(), p.mean, p.std)` for standardization params. **Do not** sort or re-order at compile time — the engine looks up by name.

3. **`crates/mc-model/tests/formula_integration.rs`:** add a regression test
   `test_predict_with_out_of_order_standardization_params`. Author a minimal model
   with two features (`Spend`, `CPC`) where `standardization.params` lists
   `CPC` first and `Spend` second. Assert the predicted value matches the
   value computed with the parameters paired by name (i.e., differs
   from the silently-wrong by-position result). Use chosen-on-purpose
   coefficient and standardization values that make the by-name and
   by-position outputs numerically distinct (e.g., very different
   means/stds for the two features).

4. **Optional nudge (recommended, not required):** add a lint warning
   (new `MC3xxx` code, only if it falls within the existing namespace
   discipline — confirm with a quick grep of `crates/mc-model/src/lint.rs`
   for the next free code) that fires when `standardization.params`
   is declared in a different order than `coefficients`. The bug is
   fixed regardless; this just nudges users toward the canonical
   order. **Skip if it requires inventing a new code namespace** —
   filing the lint as Phase 3I work is acceptable.

**Acceptance:**
- Existing tests still pass (the in-order Acme/NBA cases are unaffected by the rename — `compile.rs` just emits names).
- New regression test passes and would have failed against the old positional code.
- `mc-core` clippy clean. `cargo build --release --workspace` zero warnings.
- Other consumers of `FittedModelData` updated (search `Vec<(f64, f64)>` and `coefficients: Vec<f64>` in the workspace; the engine internals + any test fixtures that construct `FittedModelData` directly need to follow the new shape).

### Block 1.2 — MAJ-1: `time_format` parsed but never consumed

**Files touched:**
- `crates/mc-tessera/src/transform.rs` (or wherever Time-dim column matching lives)
- `crates/mc-tessera/src/prepare.rs` (if column mapping resolution happens here)
- `crates/mc-tessera/tests/` (NEW regression test using a fixture CSV with non-ISO dates)

**The bug.** `ColumnMappingConfig.time_format` is declared at
`crates/mc-recipe/src/schema.rs:258` and validated by MC5030
(`TimeFormatRequired` fires if a Time column has non-ISO values
without a `time_format` set). But the format string itself is never
**consumed** anywhere in `mc-tessera`. Models that set `time_format:
"%m/%d/%Y"` believe they're configuring date parsing; they're not.
Source values are matched against Time-dim element names as raw
strings, which means non-ISO dates land in element zero or fail to
match.

**The fix.** Wire `time_format` through the column-mapping resolution
step. When a column is mapped to the Time dimension and `time_format`
is set:

1. Parse each source string using the strptime-style format.
2. Convert the parsed timestamp to the Time dim's canonical form
   (e.g., `"YYYY-MM"` for monthly, `"YYYY-Qn"` for quarterly — driven
   by `map_to_period` if set, otherwise the existing inference path).
3. Match the canonicalized string against the Time dim's element names.

**Implementation guidance:**
- Use the standard library's `time` crate **only if it's already a workspace dep** — check `Cargo.lock`. If not, hand-roll a strptime subset matching the formats we already document (`%Y-%m`, `%Y-%m-%d`, `%m/%d/%Y`, `%d-%b-%Y`, `%Y-%m-%d %H:%M:%S`, `%Y-W%V`). The hand-rolled-wins rule applies (process-notes Rule 5). Keep it under ~150 lines.
- If a source string fails to parse against the configured `time_format`, emit a new diagnostic (extend the existing MC5030/MC5031 cluster — confirm next free code in `crates/mc-recipe/src/error.rs` or the equivalent in `mc-tessera`). Don't silently skip.

**Regression test:** create a fixture CSV where a column has
`%m/%d/%Y` dates (e.g., `01/15/2026`) and a recipe with `time_format:
"%m/%d/%Y"` and `map_to_period: "month"`. Ingest it. Assert the cells
land at the Time element `2026-01` (or whatever the canonical form
is). Without the fix, the test will land them somewhere wrong (or
fail to match at all).

**Acceptance:**
- New test passes.
- Existing Tessera tests unchanged (the ISO-date path is untouched).
- `cargo test --workspace` 100% pass.

---

## Block 2 — Envelope Discipline (~2 hours, P0/P1)

These three items all touch the Phase 6A verb JSON output. Bundle
them into one editing session because they share files and patterns.

### Block 2.1 — CRIT-2: `schema_version: "1.0"` missing from Phase 6A verb JSON

**Files touched:**
- `crates/mc-cli/src/query.rs` (line 1097 area)
- `crates/mc-cli/src/whatif.rs` (line 252 area)
- `crates/mc-cli/src/trace.rs` (JSON formatter)
- `crates/mc-cli/src/sweep.rs`
- `crates/mc-cli/src/diff.rs`
- `crates/mc-cli/src/write.rs`
- (optional) a new shared helper in `crates/mc-cli/src/main.rs` or `query.rs`

**The fix.** Add `"schema_version": "1.0"` as the **first** field of
every `--format json` envelope from Phase 6A verbs. Pattern matches
Phase 3B's diagnostic envelope and the existing `tessera` /
`validate` / `inspect` / `lint` outputs (line 626 + main.rs:682).

**Helper recommendation:** add `pub fn push_json_envelope_header(out: &mut String)`
in `query.rs` (since `query.rs` already exports `push_json_str`). It
writes `{"schema_version":"1.0",` and the per-verb JSON-emit code
appends its remaining fields. If you'd rather keep each emitter
self-contained, fine — just be consistent.

**Regression test:** add to `crates/mc-cli/tests/agent_cli_integration.rs`
a `test_all_phase_6a_verbs_emit_schema_version` that loops over the 6
verbs, runs each in a representative `--format json` mode, and asserts
the parsed JSON's `schema_version == "1.0"`.

### Block 2.2 — CRIT-3: I/O errors return exit 1 instead of exit 3

**Files touched:**
- `crates/mc-cli/src/query.rs` (`load_model` at line 240, callers at 123)
- `crates/mc-cli/src/whatif.rs` (line 98)
- `crates/mc-cli/src/trace.rs` (line 80)
- `crates/mc-cli/src/sweep.rs` (line 165 area)
- `crates/mc-cli/src/diff.rs` (line 92 area)
- `crates/mc-cli/src/write.rs` (line 87 area)

**The fix.** Today `load_model` returns `Result<LoadedModel, String>`
where every failure becomes a single `String`. Distinguish I/O
failures (file not found, permission denied) from parse/validate
failures.

**Suggested shape:**

```rust
pub enum LoadModelError {
    Io(String),       // exit 3
    Model(String),    // exit 1
}

pub fn load_model(path: &str) -> Result<LoadedModel, LoadModelError> { ... }
```

The first `std::fs::read_to_string` call maps to `LoadModelError::Io`;
parse / validate / resolve_inputs / compile failures map to `Model`.
Each Phase 6A verb's dispatch in `main.rs` maps `Io → 3`, `Model → 1`.

Per the Phase 6A handoff §"Agent-Readiness Invariants" rule 2:
`0` = success, `1` = model error, `2` = CLI usage, `3` = I/O.

**Regression test:** in `agent_cli_integration.rs`, add
`test_query_returns_exit_3_when_model_file_missing` (point to a
non-existent path, assert exit 3) and
`test_query_returns_exit_1_when_model_invalid` (point to a YAML with
a known parse error, assert exit 1).

### Block 2.3 — MIN-5: MCP responses missing `structured` field for Phase 6A verbs

**Files touched:**
- `crates/mc-cli/src/mcp.rs` (lines 844–856 — `run_cli_verb` for the new verbs)

**The fix.** When a Phase 6A MCP tool (`mosaic.model.query`,
`mosaic.model.whatif`, `mosaic.model.trace`, `mosaic.model.sweep`,
`mosaic.model.diff`, `mosaic.model.write`) is called with the
`format: "json"` argument and the verb succeeds, parse the captured
stdout back to a `JsonValue` and set it as `structured` on the
`ToolOutcome` (matching the existing `validate` / `inspect` / `lint`
pattern at lines 404 / 412 / 449). Today they all return
`structured: None`.

**Regression test:** extend `test_mcp_query_does_not_corrupt_stdout`
in `agent_cli_integration.rs` (or add a sibling
`test_mcp_query_returns_structured_envelope`) to assert the JSON-RPC
response includes a parsed `structured` field for `mosaic.model.query`
when called with `format: "json"`.

---

## Block 3 — Correctness & Safety Polish (~1.5 hours, P1)

### Block 3.1 — MAJ-2: Atomic schedule registry write

**File:** `crates/mc-tessera/src/schedule/registry.rs` (lines 71–83).

**The fix.** Mirror the watermark pattern at
`crates/mc-tessera/src/incremental.rs:88-107`: write to
`<path>.tmp`, then `fs::rename` to `<path>`. Atomic on POSIX. Fits in
the existing `save()` method body.

**Regression test:** if `crates/mc-tessera/src/schedule/registry.rs`
already has a tests module, add `test_save_creates_no_partial_file_on_simulated_crash`
that uses a directory containing a pre-existing `.tmp` to assert
`save()` cleans up after itself. If the existing tests are
end-to-end-only, skip the unit test and rely on the existing
`registry::tests::round_trip` (or equivalent) covering the rename
path.

### Block 3.2 — MIN-6: `not()` and `if()` use float `==` 0.0

**File:** `crates/mc-core/src/rule.rs` (lines 566 and 574).

**The fix.** Replace `x == 0.0` with `x.abs() < 1e-9` (the established
project epsilon convention — see CLAUDE.md §3.1). Both sites are the
falsy check inside `Expr::Not` and `Expr::If`.

**Regression test:** add to `crates/mc-model/tests/formula_integration.rs`
a `test_not_handles_arithmetic_zero` that runs `not(Spend - Spend)`
where `Spend` is a positive value. Without the fix, floating-point
noise from `Spend - Spend` could produce a value like `-2.7e-17` and
flip the boolean. With the fix, it correctly evaluates to `1.0` (true).

### Block 3.3 — MIN-1: Drop `#[allow(unused_variables, unused_assignments)]` from Phase 6A modules

**File:** `crates/mc-cli/src/main.rs` (lines 25–40, seven module declarations).

**The fix.** Remove the seven `#[allow(unused_variables, unused_assignments)]`
attributes. If any individual variable triggers a warning after
removal, prefix it with `_` at the declaration site or eliminate it.
Don't add the `allow` back.

**Acceptance:** `cargo clippy --all-targets --workspace -- -D warnings` clean.

### Block 3.4 — MIN-4: Document daemon.rs `unsafe` exception in CLAUDE.md

**Files touched:**
- `CLAUDE.md` (§2 or §3.1 — pick whichever fits cleanly)
- (Optional) `crates/mc-tessera/src/lib.rs` — add `#![forbid(unsafe_code)]` with a `#[allow(unsafe_code)] pub mod schedule { ... }` exception, or at the daemon module boundary.

**The fix.** CLAUDE.md §3.1 currently lists `unsafe` as forbidden
without exceptions. The daemon's signal-handler block at
`crates/mc-tessera/src/schedule/daemon.rs:273-283` is a legitimate
exception (POSIX signal handler registration; async-signal-safe atomic
store; no alternatives in stable Rust). Add a one-paragraph note to
CLAUDE.md §3.1 (or §2 — wherever fits) acknowledging the exception
with rationale, and either restrict it via `#[forbid]` annotations or
state plainly that this is the only sanctioned `unsafe` site in the
workspace.

---

## Block 4 — Verification (read-only, ~1 hour, P1)

These three items are checks, not edits. Confirm current behavior,
report findings, **only fix if a real bug is found** (and even then,
file it in the completion report's "discovered during 6A.1" section
rather than expanding scope).

### Block 4.1 — HTTP-JSON driver rate limiting

**File:** `crates/mc-drivers/src/http_json_driver.rs`.

**Verification:** the Phase 5C handoff specifies a 4-requests/second
rate limit with sleep-250ms fallback. Grep the driver for `sleep` /
`Duration::from_millis` / rate-limit logic. Confirm the limit is
enforced; if not, file as a finding in the completion report.

### Block 4.2 — `predict()` validation severity for `std <= 0`

**File:** `crates/mc-model/src/validate.rs:1758-1766`.

**Verification:** the eval at `cube.rs:1116` skips standardization if
`std > 0.0` (no division by zero), but the validation should hard-block
any model that ships `std <= 0`. Confirm the diagnostic for `std <= 0`
is `Severity::Error` (not `Warning`). If it's a warning, change it to
error (this is a bug fix; bundle it into the completion report).

### Block 4.3 — Write-log replay on `load_model`

**Files:** `crates/mc-cli/src/write.rs`, `crates/mc-cli/src/query.rs::load_model`.

**Verification:** the Phase 6A completion report flagged "write-log
replay not wired into `load_model`" as known debt. Run the following
check:

1. `mc model write` to write a single cell value to a model.
2. `mc model query` against the same model afterwards.
3. Does the queried value reflect the write, or is the write silently
   ignored?

Document the actual current behavior in the completion report. **Do
not implement replay** — that's queued for a future phase per the
four-source-state-model rule (process-notes Rule 9). Just confirm
which of "silent ignore" vs. "error on load" vs. "partial replay" is
the actual behavior so future maintainers know.

---

## Out of Scope (Explicitly Deferred)

These are review findings the project owner reviewed and chose to
defer. Do not address them in 6A.1.

| Finding | Why deferred | Future work |
|---|---|---|
| MAJ-3: cross-coord dep-graph not updated for `prev`/`lag`/`actual_ref`/`cumulative`/`rolling_avg` | Correctness preserved via revision-bump belt-and-suspenders. Performance-only impact. Proper fix needs an ADR — cross-coord edges may need parameterization (`lag(M, 3)` creates an edge from `M[T-3]` to `M[T]`, not `M[T-1]` to `M[T]`). | File a research note in `docs/research-notes/cross-coord-dep-graph.md` documenting the current behavior, the performance characteristic, and the architectural questions a fix-it phase needs to answer. ~1 page, file as part of 6A.1. |
| MAJ-4 second part: `find_coefficient_index` parses model file N more times | Bundles with the existing P1 "single-compile sweep" debt item. | Phase 6A.2 sweep refactor (already on the queue). |
| Sonnet's #3: cron DST/timezone audit | Cron + DST is famously bug-prone. Deserves a focused dedicated session with explicit spring-forward / fall-back test cases. | Future phase: `phase-5c-1-cron-dst-audit` or similar. |
| MIN-2: `#[non_exhaustive]` on `ScalarValue` | Mosaic has no external consumers yet — pre-V1 API hygiene pass is the right place. | Pre-V1 API freeze, not now. |
| MIN-3: `lag()` / `rolling_avg()` overflow bounds | Real but exotic. Bundle into Phase 3F polish. | Phase 3F polish or Phase 3I. |

---

## Hard Rules (binding)

1. **`mc-fixtures` is locked.** `git diff e696379 -- crates/mc-fixtures/` returns 0 lines at the end.
2. **`mc-core` change is scoped to `FittedModelData`.** No other public type changes. Other `mc-core` modifications limited to the rule.rs §3.2 epsilon fix.
3. **No new dependencies.** Hand-roll any parsing (e.g., strptime subset for Block 1.2).
4. **Toolchain stays at Rust 1.78.** No `rust-toolchain.toml` edit.
5. **No `Cargo.lock` pin churn** unless a hand-rolled subset can't satisfy a sub-task and an existing dep has a path that works without bumping pins.
6. **Backward compat:** the Acme YAML, NBA cartridge YAML, and every existing test fixture must compile and run unchanged after Block 1.1 (the `FittedModelData` shape change). The fix re-orders fields inside the struct, not the YAML schema.
7. **Determinism:** 10 consecutive `cargo test --workspace` runs identical pass/fail.

---

## Completion Report Expectations

Follow process-notes Rule 10 (self-audit pattern). The completion
report at `docs/reports/phase-6a-1-completion-report.md` should
include:

1. **Shipped** — what landed for each block.
2. **Acceptance gates** — checkbox list mirroring the headlines below.
3. **Verification findings (Block 4)** — what you discovered, even if "no bug found."
4. **Known debt** — anything you noticed but didn't fix (file follow-ups).
5. **Locked surfaces grep** — paste the output of `git diff e696379 -- crates/mc-fixtures/` (should be empty).

### Headline Acceptance Gates

- [ ] CRIT-1 fix lands; `test_predict_with_out_of_order_standardization_params` passes.
- [ ] MAJ-1 fix lands; non-ISO date ingest test passes.
- [ ] CRIT-2 fix lands; all 6 Phase 6A verbs emit `schema_version: "1.0"`.
- [ ] CRIT-3 fix lands; I/O errors return exit 3, model errors return exit 1.
- [ ] MIN-5 fix lands; MCP `mosaic.model.query` (and siblings) return parsed `structured`.
- [ ] MAJ-2 fix lands; `ScheduleRegistry::save()` is atomic via tmp+rename.
- [ ] MIN-6 fix lands; `not()` / `if()` use the `1e-9` epsilon convention.
- [ ] MIN-1 fix lands; the seven `#[allow(unused_variables, unused_assignments)]` annotations are gone.
- [ ] MIN-4 fix lands; CLAUDE.md documents the daemon's `unsafe` exception.
- [ ] Block 4 verification reported (HTTP-JSON rate limit, predict std<=0 severity, write-log replay behavior).
- [ ] MAJ-3 research note filed at `docs/research-notes/cross-coord-dep-graph.md`.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo test --workspace` passes (count up from 704; expect ~+8–12 new tests).
- [ ] 10 consecutive `cargo test` runs identical.
- [ ] Forbidden-pattern grep clean (`grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-core/src/` returns zero).

---

## Suggested Order of Operations

1. Read this handoff in full.
2. Read [`docs/reviews/phase-3-5-6-shipped-review.md`](../reviews/phase-3-5-6-shipped-review.md) for the original findings + line numbers.
3. Skim [`docs/process-notes.md`](../process-notes.md) Rules 9 + 10 (write-log replay rule + completion report self-audit).
4. **Block 1.1 first** — CRIT-1 is the highest-stakes fix. Get the type change + regression test green before anything else.
5. **Block 1.2 second** — MAJ-1.
6. **Block 2** — bundle the three envelope-discipline items in one editing session.
7. **Block 3** — polish.
8. **Block 4** — read-only verification. Report findings.
9. File the MAJ-3 research note.
10. Run all gates. Write the completion report. Surface anything ambiguous as a SPEC QUESTION before declaring done.

If anything in this handoff conflicts with the engine-semantics spec
or CLAUDE.md, surface as a SPEC QUESTION. The spec wins.

---

## SPEC QUESTION format

Use the existing format from [`CLAUDE.md`](../../CLAUDE.md) §11:

```
SPEC QUESTION: [one-line summary]

Context: [where in the handoff this came up]
Spec text: [literal quote from engine-semantics.md or brief]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

---

*End of handoff. The instance reading this should now have everything
needed to ship 6A.1 in a single focused session.*
