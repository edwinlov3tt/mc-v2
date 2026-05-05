# Time Anchor — Runtime "Now" Reference for Formula Evaluation

> **Status:** Research note. Phase 3F.1 candidate (after Phase 3F ships `prev`/`lag`/`cumulative`/`period_index`).
> **Filed:** 2026-05-05, after the Tide Cleaners proof surfaced the "which period is current?" question.

## The gap

Mosaic's Time dimension elements are ordered strings (`"2025_01"`, `"2025_02"`, ...) with a positional index (`period_index()` from Phase 3F). The engine knows element ORDER but has no concept of "which period is NOW" — there's no anchor point that separates "past" from "future" at formula-evaluation time.

This matters for three common formula patterns:

### Pattern 1: Actual-vs-Plan switching

"Use actuals for past periods, plan for future periods."

```yaml
# WANT (requires knowing which periods are "past"):
body: "if(is_past(), actual_ref(Spend), Plan_Spend)"

# WORKAROUND TODAY (hardcoded period name):
body: "if(period_index() <= 4, actual_ref(Spend), Plan_Spend)"
# breaks every month when "4" needs to become "5"
```

### Pattern 2: Pacing against calendar

"What fraction of the flight period has elapsed?"

```yaml
# WANT:
body: "Total_Budget * (periods_elapsed() / total_periods())"

# WORKAROUND TODAY:
body: "Total_Budget * ((period_index() + 1) / 12)"
# works only if the model starts at the flight start; breaks for multi-year models
```

### Pattern 3: Rolling windows relative to "today"

"Average of the most recent 3 actuals" (not the 3 periods before the computed cell).

```yaml
# WANT:
body: "rolling_avg_to_anchor(Revenue, 3)"
# averages the 3 periods ending at the anchor, regardless of which cell is being computed

# WORKAROUND TODAY: external Python script computes this and injects as an input measure
```

## Proposed design

### Model-level declaration (optional; default = no anchor)

```yaml
metadata:
  name: "TideCleaners"
  time_anchor: "2025_10"  # optional; the "current" period
```

### CLI override (runtime parameter)

```bash
mc model test tide-cleaners.yaml --time-anchor 2025_10
mc tessera apply recipe.yaml --time-anchor 2025_10
mc model whatif --model tide.yaml --time-anchor 2026_01 --set "..."
```

The `--time-anchor` flag overrides the YAML declaration at runtime. This makes the model deterministic for any "as-of" date without editing the file. Monthly model updates = change the CLI flag, not the YAML.

If neither YAML nor CLI provides a time_anchor, formulas that reference it return Null (or fire a diagnostic).

### Formula primitives unlocked by time_anchor

| Function | Returns | Use case |
|---|---|---|
| `anchor_index()` | The `period_index()` of the time_anchor element (f64) | Basis for all relative calculations |
| `is_past()` | 1.0 if `period_index() < anchor_index()`, else 0.0 | Actual-vs-Plan switching |
| `is_current()` | 1.0 if `period_index() == anchor_index()`, else 0.0 | Highlighting "this month" |
| `is_future()` | 1.0 if `period_index() > anchor_index()`, else 0.0 | Forecast flagging |
| `periods_since_anchor()` | `period_index() - anchor_index()` (negative = past) | Elapsed/remaining calculations |
| `periods_to_end()` | `max_period_index() - period_index()` | "How much of the flight remains" |

### Example: the Tide Cleaners use case

```yaml
metadata:
  time_anchor: "2025_10"  # October 2025 is "now"

rules:
  # Use actual spend for past periods, plan spend for future:
  - target_measure: Effective_Spend
    body: "if(is_past() or is_current(), actual_ref(Ad_Spend), Plan_Spend)"

  # Pacing: what fraction of the year has elapsed?
  - target_measure: Pacing
    body: "(anchor_index() + 1) / 12"

  # YTD actual revenue:
  - target_measure: YTD_Revenue
    body: "if(is_past() or is_current(), cumulative(Matched_Revenue), Null)"
```

Run with `--time-anchor 2025_10` in October, `--time-anchor 2025_11` in November — same model, formulas adapt automatically.

## Implementation considerations

### Where it lives

The time_anchor is a **runtime context parameter** (like the `now_unix_seconds` field in `WritebackRequest`). It flows through the evaluation context, not through the model schema.

```rust
// In the eval context (existing EvalCtx or similar):
pub struct EvalContext {
    // ... existing fields ...
    pub time_anchor_index: Option<usize>,  // None = no anchor configured
}
```

Formula functions (`is_past()`, `anchor_index()`, etc.) read from `EvalContext`. If `time_anchor_index` is None and a formula references an anchor function → Null (or diagnostic, depending on design choice).

### Schema validation

- If `metadata.time_anchor` is specified, validate that it names a real element in the Time-kind dimension. Fire MC2038 if not.
- `--time-anchor` CLI flag overrides the YAML value (runtime wins over static declaration).
- Multiple Time-kind dims: Phase 1 restricts to exactly one per ADR-0001's canonical 6-dim order, so this isn't a real concern today. If multi-Time-dim models are ever supported, the anchor needs scoping.

### Dirty propagation

Anchor-dependent formulas (`is_past()`, `anchor_index()`, etc.) don't dirty-propagate from data changes — they only change when the anchor itself changes (which is a full-model-reload event, not a per-cell event). So they're "constants" within a single evaluation pass. No dep-graph complexity.

### Interaction with Phase 3E `actual_ref`

`actual_ref` is exact-period (reads the actuals value at the SAME time coordinate). The `time_anchor` doesn't change `actual_ref`'s semantics — it just gives users the conditional-switch power to say "use `actual_ref` for past periods, something else for future periods."

A future `latest_actual_ref(measure)` (the "most recent actuals" variant) WOULD interact with the anchor: it could mean "the actuals at the anchor period" (most natural) or "the most recent non-Null actuals scanning backward from the anchor." That's a Phase 3F.1+ design decision.

## When to schedule

**After Phase 3F ships.** The time_anchor primitives are simple formula functions that read from the eval context — no new parser machinery, no AST complexity, no cross-coordinate reads. They're just context-lookups (`anchor_index()` is literally `ctx.time_anchor_index.map(|i| i as f64).unwrap_or(f64::NAN)`).

**Prerequisites:**
- Phase 3F's `period_index()` exists (the anchor is defined in terms of it)
- Phase 3E's `if()` exists (anchor functions are most useful inside conditionals)
- Phase 3F's `prev`/`lag`/`cumulative` exist (anchor-relative rolling windows build on them)

**Effort:** ~half a day. 5-6 new formula functions, all trivial eval (read from context), no dep-graph implications, no cross-coordinate reads.

## What this is NOT

- **NOT a scheduling/cron feature.** The anchor is a parameter, not an automated trigger. "Run this model every month with a new anchor" is an ops concern (cron, CI/CD), not a model concern.
- **NOT a date-parsing library.** The anchor is an ELEMENT NAME (`"2025_10"`), not a date object. No date arithmetic, no timezone handling, no calendar awareness. Those are Phase 3I candidates if ever.
- **NOT display formatting.** "Render October 2025 as 'Oct 2025' vs '2025-10' vs '10/2025'" is a Phase 6 UI concern (measure/dim format metadata), not a formula-engine concern.

## Cross-links

- [ADR-0012 (Phase 3F)](../decisions/0012-phase-3f-time-series-operations.md) — `period_index()` which the anchor builds on
- [ADR-0011 (Phase 3E)](../decisions/0011-phase-3e-conditionals-and-basic-operations.md) — `if()` + `actual_ref()` which anchor-switching uses
- [PERF.md §6.18](../PERF.md) — Tide Cleaners benchmark where the gap surfaced (the Python `build_forecast.py` script is the current workaround)
- [cross-coordinate-formulas.md](./cross-coordinate-formulas.md) — the broader "read from a different coordinate" research
- Phase 3D formula syntax — the parser infrastructure all anchor functions build on
