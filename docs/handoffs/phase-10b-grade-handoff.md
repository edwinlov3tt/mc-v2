# Phase 10B Handoff — `mc model grade` (Segmented Holdout Evaluation)

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0034](../decisions/0034-phase-10b-model-grade.md) (Accepted with 12 acceptance amendments — read amendments BEFORE the body; amendments win on conflicts)
**Estimated effort:** 3–4 sessions (~400-500 LOC + tests + cookbook section)
**Crate:** `mc-cli` ONLY (no `mc-core` change unless a model-semantic primitive is surfaced — Amendment 4); no daemon
**Branch:** `phase-10b/model-grade`

---

## What this phase ships

`mc model grade` — segmented holdout evaluation. Group a holdout set by
an attribute (dimension, measure value, or bucketed measure), compute
per-segment metrics via the Phase 10A primitives, flag segments crossing
a threshold, emit a text table + JSON. Reproduces claw-core's
EXP-048/022/023/031c segment-table workflow in one command instead of a
~120-line Python script.

The canonical target (EXP-048): group line=9.0 bets by bet_side, surface
that 98.5% are UNDERs hitting at 65.70% with Wilson CI [61.19, 69.94].

---

## Required reading (in this order)

1. **ADR-0034 Amendments (CRITICAL — read first).** 12 binding amendments
   from dual review. They override the body. The ones that change
   implementation most:
   - **A1**: `--holdout` reuses the EXISTING `Filter` grammar
     (`query.rs:413`), NOT `--coord`. F64-measure equality is guarded.
   - **A2**: continuous-measure `--group-by` requires `--bucket`;
     `--max-segments` cap (default 50).
   - **A3**: Wilson Null indicator → HARD ERROR by default (not warning).
   - **A4**: grouped-reduction stays in `mc-cli`. No `mc-core` change.
   - **A5**: expanded JSON schema (status, null_counts, warnings, bucket
     metadata, denominator_zero_segments).
   - **A8**: `LoadPolicy::Reproducible` default.
2. **ADR-0034 body** — the map-reduce model, 7 decisions (interpret
   through amendments). The body's metric vocabulary is 7 reductions;
   Amendment 7 makes it 9 (adds min/max).
3. **ADR-0033 + metrics cookbook** — the primitives grade composes. The
   Wilson trial-count footgun (Amendment 5 there) is load-bearing here.
4. **The code grade reuses:**
   - `crates/mc-cli/src/sweep.rs` — the CLI command structure to mirror
     (parse → run, `LoadPolicy::Reproducible` at line 184)
   - `crates/mc-cli/src/query.rs:413` — the `Filter` enum (And/Or/Not/
     Compare/Expr; FilterAtom::{Measure,Dimension}; CmpOp) that
     `--holdout` reuses. Also `Filter::parse` at line 452.
   - The per-leaf eval traversal `count_over`/`avg_over` use in
     `mc-core` (grade *calls* this; does not extend it)
5. **CLAUDE.md** — §2.5 (Null), §3.1 (forbidden patterns incl. no float
   `==`), §6 (gates), **§6.7 (quote the real test run — Phase 10A
   lesson)**, **§4.5 (inline-YAML in tests uses single braces — Phase
   10A lesson)**.

---

## Phase 10B scope

| # | Item |
|---|---|
| 1 | `crates/mc-cli/src/grade.rs` (new) — `GradeCommand`, `parse`, `run` |
| 2 | Wire `"grade" =>` into `main.rs` model-verb dispatch |
| 3 | Metric-expression parser (formal grammar, Amendment 11) — 9 reductions |
| 4 | `--holdout` via existing `Filter` grammar + F64-equality guard (Amendment 1) |
| 5 | Group-key resolution: dimension / discrete-measure / bucketed-measure (Amendment 2) |
| 6 | `--bucket` parsing + assignment; `--max-segments` cap; out-of-range segment |
| 7 | Grouped-reduction engine (the core; in mc-cli) — restrict leaf set per segment, apply 10A primitive |
| 8 | Wilson Null hard-error + `--wilson-null drop\|error` (Amendment 3) |
| 9 | ratio denom-zero → Null + diagnostic (Amendment 6) |
| 10 | `--flag-if`, `--min-n` (inclusive TOTAL — Amendment 9) |
| 11 | Text table + expanded JSON (Amendment 5) |
| 12 | `LoadPolicy::Reproducible` default + `--include-writes` (Amendment 8) |
| 13 | Deterministic segment ordering (Amendment 12) |
| 14 | Tests (parser, bucketing, grouped reduction, EXP-048 repro, Wilson error, ratio denom-zero, determinism) |
| 15 | Metrics cookbook `mc model grade` section + reproducibility note |

**Out of scope:** daemon `/grade` endpoint; free-form formula metrics;
`median`/`percentile`; `max_drawdown`/`recovery_bets` (Phase 10F);
cross-segment significance tests; subtotals (reserve JSON room only);
`--at-revision` snapshot pinning (Amendment 10 defers it); any `mc-core`
change.

---

## Pre-flight checklist (report results in chat before Step 1)

```bash
# Worktree
cd /Users/edwinlovettiii/Projects/mc-v2
git worktree add ../mc-v2-phase-10b -b phase-10b/model-grade main
cd ../mc-v2-phase-10b

# 1. Confirm Filter grammar supports measure predicates (Amendment 1 grounds)
sed -n '413,450p' crates/mc-cli/src/query.rs
# Expect: FilterAtom::{Measure,Dimension}, CmpOp::{Eq,Neq,Gt,...}

# 2. Confirm Filter::parse signature (what grade calls)
grep -n "fn parse" crates/mc-cli/src/query.rs | head -3
# Expect: Filter::parse(input, refs, cube) -> Result<Filter, String>
# Verify it's pub(crate) or make grade live in a module that can call it.

# 3. Confirm LoadPolicy::Reproducible precedent (Amendment 8)
grep -n "LoadPolicy::Reproducible" crates/mc-cli/src/sweep.rs

# 4. Confirm the per-leaf eval path 10A primitives use (grade calls, not extends)
grep -n "count_over\|avg_over\|OverKind" crates/mc-core/src/rule.rs | head -10

# 5. How is a measure marked discrete/low-cardinality? (Amendment 2 needs this)
grep -rn "discrete\|cardinality\|is_discrete" crates/mc-model/src/schema.rs | head -5
# If NO such field exists: surface a SPEC QUESTION. Amendment 2 assumes
# cartridge metadata can mark a measure discrete. If the field doesn't
# exist, the fallback is: continuous F64 ALWAYS requires --bucket (no
# discrete-exemption), and grouping by a non-F64 (string/category) measure
# is allowed directly. Confirm which before building group-key resolution.

# 6. Diagnostic codes if grade introduces parse errors (MC4xxx range)
grep -RE "MC40[0-9]{2}" docs/ crates/ | tail -10
# Reuse the shared wrong-arg / parse-error helpers where possible.

# 7. Clean tree
git status
```

The Step-5 discrete-measure question is the one most likely to need a
SPEC QUESTION — resolve it before building group-key resolution.

---

## Implementation path

### Step 1: CLI scaffold (mirror sweep.rs)
`GradeCommand` struct with fields for: path, unit, holdout (String,
parsed to `Filter`), group_by (Vec<String>), metrics (Vec<MetricExpr>),
buckets (Map<measure, Vec<f64>>), flag_if (Option<String>), min_n
(usize, default 0), max_segments (usize, default 50), wilson_null (enum,
default Error), include_writes (bool, default false), format. `parse` +
`run`. Wire into `main.rs` (`"grade" => grade::parse / grade::run`).

### Step 2: Metric-expression parser (Amendment 11 grammar)
Parse `name=reduction(ingredients)`. 9 reductions. Validate arity (ratio
= 2, rest = 1) and that each ingredient is a measure in the cartridge.
Error UX per Amendment 11 (`"unknown reduction 'avgg'; expected one of:
..."`). Whitespace-tolerant around delimiters.

### Step 3: Load (Amendment 8)
`load_model_with_policy(path, LoadPolicy::Reproducible)` unless
`--include-writes`. Mirror sweep.rs:184.

### Step 4: Holdout filter (Amendment 1)
`Filter::parse(holdout_expr, refs, &cube)`. Apply per unit leaf to decide
inclusion. Before parsing, scan for F64-measure equality and reject per
the guard (discrete-marked / range / tolerance required).

### Step 5: Group-key resolution (Amendment 2)
For each group-by key: classify as dimension / discrete-measure /
bucketed-measure. Continuous-F64-without-bucket → hard error. Evaluate
the key per unit leaf, assign to a segment. Compute the cartesian product
for multi-level. Enforce `--max-segments` (count check → hard error).
Out-of-range bucket values → `(out-of-range)` segment (surfaced, counted
in TOTAL).

### Step 6: Grouped-reduction engine (the core — in mc-cli)
For each segment: collect its unit leaves, then for each metric apply the
reduction by restricting the 10A primitive's evaluation to that leaf set.
The per-leaf eval already exists in mc-core — call it over the segment's
leaves, accumulate, reduce. Wilson reductions: compute mean + count over
the segment; Null-indicator → hard error (Amendment 3). ratio: sum/sum
with denom-zero → Null + diagnostic (Amendment 6).

### Step 7: TOTAL + flags + min-n (Amendments 9)
TOTAL = ungrouped aggregate over ALL holdout units (inclusive of min-n-
excluded segments). `--min-n` marks small segments `below_min_n` and
excludes from flag eval, NOT from TOTAL. `--flag-if` parses a simple
`<metric> <op> <value>` predicate, evaluated per non-excluded segment.

### Step 8: Output (Amendment 5 + 12)
Text table (EXP-048 shape, aligned, flag markers, TOTAL row). Expanded
JSON: segments with status/null_counts/flagged, warnings, bucket
metadata, denominator_zero_segments, total, flagged_count,
reserve subtotals. Segment ordering lexicographic by group-by flag order
(first slowest — Amendment 12).

### Step 9: Tests
- Metric parser: valid forms, each reduction, wrong arity, unknown
  reduction error message, whitespace tolerance
- Bucket assignment: edges, out-of-range, left-closed/right-open
- F64-equality guard: `line == 9.0` on unmarked F64 → error; on range → ok
- Continuous group-by without bucket → error; with bucket → ok
- `--max-segments` exceeded → error
- Grouped reduction: build a small cube (single-brace YAML per §4.5!),
  group by a 2-value measure, assert per-segment values hand-computed
- **EXP-048 reproduction**: fixture cube, group by bet_side, assert
  Wilson bounds match metrics.rs fixtures (the headline parity)
- Wilson Null indicator → hard error; `--wilson-null drop` → excludes
- ratio denom-zero → Null + diagnostic, never inf/NaN
- `--min-n`: small segment excluded from flagging, still in TOTAL
- Determinism: 10 runs identical (ordering locked)

### Step 10: Cookbook + gates
Add `mc model grade` section to `docs/specs/metrics-cookbook.md` with the
EXP-048 worked example + reproducibility note. Run all gates
(CLAUDE.md §6) and **quote the real `cargo test --workspace` result line
per §6.7**.

---

## Acceptance gate (binding — body 23 ACs + amendment revisions)

Report each explicitly when claiming done. Consolidated per the ADR's
"Consolidated acceptance-criteria revisions" section:

- [ ] AC #1-5, #8, #10-11, #13, #16-23: per body (parsing, grouping, output, EXP-048 repro, etc.)
- [ ] AC #6: 9 reductions including min/max (Amdt 7)
- [ ] AC #7: ratio denom-zero/Null → Null + diagnostic, never inf/NaN/0 (Amdt 6)
- [ ] AC #9: Wilson Null indicator hard-errors by default; `--wilson-null drop` escape (Amdt 3)
- [ ] AC #12: TOTAL inclusive of min-n-excluded segments (Amdt 9)
- [ ] AC #14: JSON validates against expanded schema (Amdt 5)
- [ ] AC #15: lexicographic ordering, first group-by slowest (Amdt 12)
- [ ] AC #17: CLI-only; zero mc-core change (Amdt 4)
- [ ] AC #24: `--holdout` reuses Filter grammar; F64-equality guarded (Amdt 1)
- [ ] AC #25: continuous group-by without `--bucket` → error; `--max-segments` cap (Amdt 2)
- [ ] AC #26: `LoadPolicy::Reproducible` default; `--include-writes` opt-in (Amdt 8)
- [ ] AC #27: metric grammar + error UX (Amdt 11)
- [ ] AC #28: JSON exposes status/null_counts/warnings/bucket/denominator_zero_segments (Amdt 5)
- [ ] AC #29: reproducibility note documented (Amdt 10)
- [ ] Build gates: fmt, clippy -D warnings, build, **`cargo test --workspace` with quoted result line (§6.7)**, determinism ×10
- [ ] Forbidden-pattern grep clean (no float `==`, no unwrap in src)

---

## Common pitfalls (forewarned)

1. **Inventing a filter grammar.** Don't. Reuse `Filter` (`query.rs:413`)
   — it already does measure predicates. Amendment 1.
2. **Adding an mc-core helper.** Don't. The grouped-reduction engine is
   mc-cli. Amendment 4. If you think you need a kernel primitive, surface
   a SPEC QUESTION — don't silently add.
3. **Warning instead of erroring on Wilson Null.** Hard error by default.
   Amendment 3. A too-narrow CI in betting context is the wrong failure.
4. **Grouping a continuous F64 measure by distinct value.** Float `==`
   is forbidden (CLAUDE.md §3.1) and produces thousands of singletons.
   Require `--bucket`. Amendment 2.
5. **`ratio` producing inf/NaN on zero denominator.** Return Null +
   diagnostic. Amendment 6. Use `abs() < 1e-300`, not `== 0.0`.
6. **TOTAL excluding min-n segments.** TOTAL is inclusive. Amendment 9.
7. **Double-brace `{{ }}` in test YAML.** Single braces. CLAUDE.md §4.5
   (the exact Phase 10A bug). Run each construct-then-assert test by name.
8. **Claiming done without a quoted test run.** CLAUDE.md §6.7. Quote the
   real `test result: ok. N passed; 0 failed` line.
9. **Thin JSON.** Expand it per Amendment 5 — it's the codegen contract.

---

## Cross-links

- ADR-0034 (this phase): [`../decisions/0034-phase-10b-model-grade.md`](../decisions/0034-phase-10b-model-grade.md)
- Review request that produced the 12 amendments: [`../reviews/adr-0034-review-request.md`](../reviews/adr-0034-review-request.md)
- ADR-0033 (Phase 10A — the primitives): [`../decisions/0033-phase-10a-evaluation-metrics-library.md`](../decisions/0033-phase-10a-evaluation-metrics-library.md)
- [Metrics cookbook](../specs/metrics-cookbook.md): per-unit measure patterns + the grade section this phase adds
- `crates/mc-cli/src/sweep.rs`: CLI structure + LoadPolicy::Reproducible
- `crates/mc-cli/src/query.rs:413`: the `Filter` grammar to reuse
- claw-core EXP-048: `claw-core/docs/reports/exp-048-line9-deep-dive.md` — reproduction target
- CLAUDE.md §4.5 + §6.7: the Phase 10A process lessons

---

## Completion report template

Write `docs/reports/phase-10b-completion-report.md`:
1. Final diagnostic codes (reused vs new)
2. Test count + **quoted `cargo test --workspace` result line**
3. Build gate results
4. SPEC QUESTION resolutions (especially the discrete-measure metadata question)
5. EXP-048 reproduction parity (Wilson bounds vs metrics.rs fixtures)
6. JSON schema — final shape
7. Any mc-core change (should be none; if any, the surfaced justification)
8. Effort actual vs estimate (3-4 sessions)
9. Recommended next phase from consumer demand (10C backtest / 10D batch sweep)
