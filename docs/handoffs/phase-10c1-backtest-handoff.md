# Phase 10C.1 Handoff — `mc model backtest` (the command; spike-gate passed GREEN)

**Status:** Accepted, ready to start — 10C.0 spike returned 🟢 GREEN (zero kernel change)
**Date:** 2026-05-27
**ADR:** [ADR-0036](../decisions/0036-phase-10c-model-backtest.md) (Accepted, 8 amendments — read amendments first; they win on conflicts)
**Spike verdict:** [phase-10c0-spike-report.md](../reports/phase-10c0-spike-report.md) — GREEN, AC #17 holds
**Estimated effort:** 3–4 sessions (~400-500 LOC, mostly composition)
**Crate:** `mc-cli` ONLY (spike confirmed zero kernel change); no daemon
**Branch:** `phase-10c.1/model-backtest`

---

## What this ships

`mc model backtest` — sweep a parameter across a grid; at each grid point
run the full grade-style holdout evaluation; report the metric surface +
flag the best setting. Composes `grade` (per-point evaluation) + `sweep`
(axis override mechanics). Multi-domain by mandate.

The 10C.0 spike already proved the load-bearing question: **the `param:`
axis recomputes correctly via the sweep pattern (snapshot → rollback_to →
set param → read).** This is a composition, not kernel work.

---

## Required reading (in order)

1. **ADR-0036 amendments (8, binding).** The body's design stands; the
   amendments correct it. Key ones:
   - A1: spike-gated — GREEN, so proceed (this handoff).
   - A2: per-cell clean-state — each grid cell does rollback_to first
     (the spike's guardrail — see below).
   - A3: `values:[...]` list axis + `--dry-run`.
   - A4: **`--simulate` is DEFERRED — do NOT implement it.** v1 is
     reduction-only, provably domain-neutral.
   - A5: EXP-021/026 walked back (training-hyperparam = refit, not in
     scope); `variant:` axis deferred. "replaces 5-6 no-refit scripts."
   - A6: `--best-by total|segment` for grouped objectives.
   - A7: add `rmse(m)` reduction (→10 total); objective Null/tie rules.
   - A8: lift grade's engine into shared `eval_common`; the load policy
     is ADR-0034/sweep precedent (NOT ADR-0035 A8).
2. **The spike report** ([phase-10c0-spike-report.md](../reports/phase-10c0-spike-report.md)) — the param-recompute mechanism + the guardrail.
3. **ADR-0034 (grade)** — the per-grid-point evaluation engine you reuse.
4. **`crates/mc-cli/src/sweep.rs`** — the `SweepTarget::Coefficient`
   override loop (sweep.rs ~273-320) is the TEMPLATE for
   `SweepTarget::Parameter`.
5. **`crates/mc-cli/src/grade.rs`** — the metric parser + reduction
   engine + Filter application to lift into `eval_common`.
6. **CLAUDE.md** — §2.15 (read mutates/caching — why rollback_to matters),
   §3.1 (no float ==), §4.5 (single-brace test YAML), §6.7 (quote real
   runs).

---

## The spike's guardrail (load-bearing — read this)

The 10C.0 spike found: a param override recomputes ONLY if the read
happens against a cube whose derived caches aren't already populated for
the prior value. `rollback_to` (cube.rs:2802) is the cache-bust — it
clears `derived_cache`, `consolidated_cache`, `trace_cache`, and bumps
the revision. So:

**Per grid cell, the order MUST be:** `snapshot/rollback_to` → apply the
swept value (param/coef/input) → evaluate → record. NEVER override a
param in place on an already-evaluated cube and re-read — that serves
stale cache (the spike's "naive-stale" test proves it). Follow sweep.rs's
coefficient loop exactly; it already does this correctly.

The spike also noted: `cube.parameters` is a `pub AHashMap` (one-line
`.insert()`), same as `override_coefficient` uses. There's no
`Cube::set_parameter` — you may add a ~5-line ergonomic one (additive, no
gate) OR insert directly. Also check `parameters_overlay` (cube.rs:2823) —
there may be a cleaner overlay path than direct insert; the spike didn't
fully explore it. Either works; direct insert mirrors sweep.

---

## Scope

| # | Item | Amendment |
|---|---|---|
| 1 | `crates/mc-cli/src/backtest.rs` (new) — command, parse, run | — |
| 2 | Wire `"backtest" =>` into main.rs model dispatch | — |
| 3 | `--sweep` axis-spec parser: `param:`/`coef:`/`input:`, range + `values:[...]` | A3 |
| 4 | `SweepTarget::Parameter { name }` mirroring Coefficient (sweep.rs template) | spike |
| 5 | Grid orchestration: cartesian product, fixed order, `--max-grid` (default 1000) | — |
| 6 | Per-cell: rollback_to → apply swept value → evaluate (the guardrail) | A2 |
| 7 | Lift grade's metric parser + reduction engine + Filter into `eval_common`; both verbs call it | A8 |
| 8 | Add `rmse(m)` reduction to the shared vocabulary (→10 reductions) | A7 |
| 9 | `--objective`/`--goal` + `--best-by total\|segment`; Null/tie rules | A6, A7 |
| 10 | `--dry-run` (print axes + grid count + first/last cells, no eval) | A3 |
| 11 | Output: surface table + JSON + `--emit-grid` jsonl | — |
| 12 | Tests incl. EXP-033 repro + the MANDATORY non-betting multi-domain test | A-spine |
| 13 | Cookbook: backtest section w/ BOTH betting + non-betting (rmse) examples | A7, A22 |

**Out of scope (do NOT build):** `--simulate` (A4 deferred); `variant:`
axis (A5 deferred); daemon endpoint; walk-forward/refit; parquet
`--emit-grid` (jsonl v1); parallel grid eval; zip-mode axes.

---

## Pre-flight (report before Step 1)
```
cd /Users/edwinlovettiii/Projects/mc-v2
git pull origin main   # has the merged spike + GREEN report
git worktree add ../mc-v2-phase-10c1 -b phase-10c.1/model-backtest main
cd ../mc-v2-phase-10c1

# 1. Confirm grade's reduction engine is liftable (A8) — is the metric
#    parser + reduction logic in grade.rs callable, or does it need
#    extracting into eval_common? Report the refactor surface.
grep -nE "fn parse_metric_expr|fn reduce|enum Reduction|pub fn" crates/mc-cli/src/grade.rs | head

# 2. Confirm the sweep coefficient template (your param: model)
grep -nE "SweepTarget|Coefficient|override_coef|rollback|snapshot" crates/mc-cli/src/sweep.rs | head

# 3. Confirm parameters insert path (spike finding)
grep -nE "pub parameters|parameters_overlay|set_parameter" crates/mc-core/src/cube.rs | head
```
If lifting grade's engine into `eval_common` is a big refactor, surface a
SPEC QUESTION before proceeding (A8 says shared code, not subprocess/dup —
but the extraction size matters).

---

## Implementation path

### Step 1: `eval_common` extraction (A8)
Lift grade's metric-expression parser + the reduction engine + Filter
application into a shared module both `grade` and `backtest` call. Add
`rmse(m)` here (→10 reductions) so grade gets it too. grade's existing
tests must still pass after the extraction (it now calls eval_common).

### Step 2: `--sweep` axis-spec parser (A3)
Parse `param:<name>=...`, `coef:<model>.<name>=...`, `input:<measure>@<coord>=...`.
Range form `start:stop:step` AND value-list form `[v1,v2,v3]`. Validate
the referenced param/coef/measure exists. Multi-axis → grid; enforce
`--max-grid`.

### Step 3: SweepTarget::Parameter + grid orchestration (Step 4-6 of scope)
Mirror sweep.rs's Coefficient loop. Per grid cell: rollback_to → apply
the cell's swept values (param.insert / coef override / input override) →
run eval_common's holdout evaluation → record. Fixed enumeration order
(first axis slowest, per grade's A12 convention).

### Step 4: Objective + output
`--objective`/`--goal`/`--best-by`. Null metrics excluded from selection;
all-Null objective hard-errors; ties → first cell. Surface table + JSON
(schema_version + run config) + `--emit-grid` jsonl. `--dry-run`.

### Step 5: Tests
- Axis parser: all 3 kinds, range + values-list forms, bad specs
- Single param sweep: known cube, assert metric surface matches hand-computed per-point
- **Param recompute via the command** (not the spike's isolated test):
  a backtest over a param-dependent derived metric shows DIFFERENT values
  per grid cell — proves the rollback_to guardrail is wired correctly
- Multi-axis cartesian: 2 axes → correct grid size + order
- `--max-grid` hard-errors
- Objective: maximize/minimize/Null-excluded/all-Null-error/tie→first
- `--best-by segment` with `--group-by`
- `rmse(m)` reduction correct (sqrt of mean)
- **EXP-033 reproduction:** edge-threshold sweep → optimal threshold +
  per-point metrics match claw-core within tolerance
- **MANDATORY multi-domain test (the spine):** a NON-betting fixture cube
  (forecasting or marketing) swept on a param, metrics via generic
  reductions incl. rmse, proving zero betting assumptions in the engine
- `--dry-run` prints without evaluating
- Determinism ×10; single-brace YAML (§4.5)

### Step 6: Cookbook + gates
backtest section with BOTH a betting example (threshold → ROI surface)
AND a non-betting example (forecasting smoothing → rmse surface, now
expressible per A7). All gates; **quote the real `cargo test --workspace`
line (§6.7)**; run the full gate as the LAST action before push.

---

## Acceptance gate (per ADR-0036 consolidated, 10C.1 portion)
- [ ] AC #1-6, #8-9, #11-14, #16, #18-21, #23: per body (axes, grid, eval, objective, output, determinism)
- [ ] AC #7: 10 reductions incl. `rmse`; no hardcoded domain metric
- [ ] AC #10: `--simulate` NOT present (deferred — A4)
- [ ] AC #15: mandatory non-betting multi-domain test passes; engine path has zero betting vocabulary
- [ ] AC #17: zero mc-core change (spike confirmed GREEN; param via rollback_to + pub insert)
- [ ] AC #22: cookbook has betting AND non-betting (rmse) worked examples
- [ ] AC #24: `values:[...]` axis + `--dry-run`
- [ ] AC #25: per-cell clean-state — rollback_to per cell; 2-axis grid gives independent results
- [ ] AC #26: `--best-by total|segment`
- [ ] AC #27: objective Null excluded / all-Null errors / ties→first
- [ ] AC #28: "replaces 5-6 no-refit scripts" (not 7); variant: deferred
- [ ] AC #29: grade + backtest share eval_common; grade's tests pass post-extraction
- [ ] Build gates: fmt, clippy -D warnings, build, `cargo test --workspace` (quoted, §6.7), determinism ×10
- [ ] No float == (§3.1); zero-checks abs() < 1e-300

---

## Common pitfalls
1. **Overriding a param without rollback_to first** → stale cache (the
   spike's naive-stale failure). Per cell: rollback_to → set → eval.
   Mirror sweep.rs's coefficient loop exactly.
2. **Building `--simulate`** — it's deferred (A4). Reduction-only v1.
3. **Claiming "replaces 7 scripts"** — it's 5-6 (A5); EXP-021/026 need
   the deferred variant: axis.
4. **Forgetting rmse** — the forecasting example (AC #22) is unwritable
   without it. Add it to eval_common (A7).
5. **Duplicating grade's reduction logic** instead of lifting to
   eval_common (A8). grade's tests must pass against the shared module.
6. **Hardcoding a domain metric** — every metric is a generic reduction
   over author measures. The multi-domain test (AC #15) is the guard.
7. **Double-brace YAML / unquoted "all green"** — §4.5, §6.7.

---

## Cross-links
- ADR-0036 (8 amendments): [`../decisions/0036-phase-10c-model-backtest.md`](../decisions/0036-phase-10c-model-backtest.md)
- Spike report (GREEN): [`../reports/phase-10c0-spike-report.md`](../reports/phase-10c0-spike-report.md)
- ADR-0034 (grade — the eval engine): [`../decisions/0034-phase-10b-model-grade.md`](../decisions/0034-phase-10b-model-grade.md)
- sweep.rs (the SweepTarget::Coefficient template), grade.rs (the engine to lift)
- cube.rs:2802 (rollback_to — the cache-bust), cube.rs:3069 (params outside dirty propagation — why rollback_to is needed)
- CLAUDE.md §2.15, §3.1, §4.5, §6.7

## Completion report
`docs/reports/phase-10c1-completion-report.md`: quoted test result; EXP-033 repro parity; the multi-domain test (which non-betting domain); confirmation zero mc-core change held; eval_common extraction (grade tests still green); effort vs estimate; recommended next (10D sweep --games or 10E walk-forward, demand-driven).
