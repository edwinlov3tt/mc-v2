# ADR-0012: Phase 3F — Time-Series and Period Operations

**Status:** Proposed
**Date:** 2026-05-04
**Deciders:** project owner
**Phase:** 3F (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3E adds conditionals and basic operations. Phase 3F adds time-awareness — the ability to reference prior periods, compute running totals, and calculate moving averages. This is the "reporting unlock" that eliminates the most common reason models pre-compute values externally.

---

## Context

Every reporting model needs period-over-period metrics: month-over-month revenue growth, year-to-date spend, 3-month rolling average CPC. Without time-series primitives, users pre-compute these externally (Python/Excel) and import them as input measures. This defeats the formula engine's core value: "change one input, see the entire cascade update instantly."

Phase 3F addresses this by introducing time-aware formula functions that read values at other positions in the Time dimension. These are **cross-coordinate reads** — the same architectural pattern as Phase 3E's `actual_ref` (which reads across the Scenario dimension). The dep-graph machinery built for `actual_ref` extends directly to time-series operations.

**Prerequisite:** Phase 3E must ship first. Time-series formulas commonly wrap their output in `safe_div`, `if`, or `coalesce` patterns (e.g., `safe_div(Revenue - prev(Revenue), prev(Revenue), 0)` for MoM growth).

---

## Decisions

### Decision 1: How does `prev()` know which dimension is Time?

**Decision: introduce `kind: "Time"` as a new dimension kind.**

Current legal kinds: `"Standard"`, `"Measure"`, `"Scenario"`, `"Version"`. Phase 3F adds `"Time"`.

```yaml
dimensions:
  - name: "Time"
    kind: "Time"          # NEW: identifies this as the temporal dimension
    elements:
      - { name: "Jan_2026" }
      - { name: "Feb_2026" }
      - { name: "Mar_2026" }
      # ...
```

**Validation rules:**
- Exactly ONE dimension with `kind: "Time"` must exist (MC2035 if missing, MC2036 if multiple)
- The Time-kind dimension may have any name (not required to be named "Time" — could be "Period", "Month", etc.)
- Time elements are ordered by their declaration order in the YAML. The validator does NOT enforce chronological ordering (dim elements have no intrinsic date semantics) — but lint MC3010 warns if elements have `date:` metadata that's non-chronological

**Backward compatibility:** Existing models that use `kind: "Standard"` for their time dimension continue to work — they simply cannot use `prev()`/`lag()`/`cumulative()`/`rolling_avg()`. The new kind is opt-in. Models that want time-series formulas must update their time dimension to `kind: "Time"`.

**Migration path:** a model can change `kind: "Standard"` to `kind: "Time"` without any other changes — it's purely additive. Existing hierarchies, elements, and rules continue to work identically.

### Decision 2: Boundary behavior (first period, no prev)

**Decision: return `Null` at boundaries.**

- `prev(Revenue)` at the first Time element → `Null`
- `lag(Revenue, 3)` at elements 1-3 → `Null`
- `lag(Revenue, -1)` at the last Time element → `Null` (lead past the end)
- `cumulative(Revenue)` at the first element → the element's own value (cumulative of 1 value = itself)
- `rolling_avg(Revenue, 3)` at elements 1-2 → average of available elements (partial window)

**Rationale:** Null at boundaries is consistent with the kernel's existing null semantics (§7 of the brief). Users who want a default value at boundaries use Phase 3E's `coalesce` or `if_null`:

```yaml
body: "if_null(prev(Revenue), 0)"    # treat first period's "no previous" as zero
body: "coalesce(prev(Revenue), Revenue)"  # fall back to current period
```

### Decision 3: Negative lag (lead)

**Decision: `lag(measure, n)` supports negative n.**

- `lag(Revenue, 1)` = previous period (same as `prev(Revenue)`)
- `lag(Revenue, 3)` = 3 periods ago
- `lag(Revenue, -1)` = next period (lead)
- `lag(Revenue, -3)` = 3 periods ahead
- `lag(Revenue, 0)` = current period (equivalent to just `Revenue`)

**Rationale:** Adding a separate `lead()` function is redundant when `lag` with negative n achieves the same result. Negative lag is standard in R (`lag(x, -n)`) and pandas (`shift(-n)`). Forecasting models frequently reference future periods for smoothing and target computation.

### Decision 4: Partial windows in `rolling_avg`

**Decision: partial windows compute the average of available data.**

- `rolling_avg(CPC, 3)` at period 1 → `CPC[period_1]` (average of 1 value)
- `rolling_avg(CPC, 3)` at period 2 → `(CPC[period_1] + CPC[period_2]) / 2`
- `rolling_avg(CPC, 3)` at period 3+ → `(CPC[i-2] + CPC[i-1] + CPC[i]) / 3`

**Rationale:** This matches Excel's AVERAGE behavior and is the least surprising default. If a user wants Null until the window is full, they use:

```yaml
body: "if(period_index() >= 2, rolling_avg(CPC, 3), Null)"
```

Where `period_index()` returns the 0-based position of the current Time element.

### Decision 5: Date functions and element metadata

**Decision: date functions operate on optional element metadata, not string parsing.**

Phase 3F introduces an optional `date:` field on dimension elements:

```yaml
dimensions:
  - name: "Time"
    kind: "Time"
    elements:
      - { name: "Jan_2026", date: "2026-01-01" }
      - { name: "Feb_2026", date: "2026-02-01" }
      - { name: "Mar_2026", date: "2026-03-01" }
```

**Date function behavior:**
- `days_between(Start_Date_Measure, End_Date_Measure)` — operates on measures that hold date-encoded values. **Deferred to Phase 3I** (requires date-as-value semantics, not date-as-metadata).
- `period_index()` — returns the 0-based index of the current Time element. Does NOT require `date:` metadata; operates on element order alone.

**Why metadata (not string parsing):** Parsing element names (`"Jan_2026"` → January 2026) is fragile, locale-dependent, and fails for non-standard naming. Explicit `date:` metadata is unambiguous and machine-readable. Elements without `date:` metadata simply cannot use date-aware functions (they return Null).

**Simplified scope for 3F:** Only `period_index()` ships in Phase 3F. Calendar-math functions (`days_between`, `month_of`, `quarter_of`) are deferred to Phase 3I where `date:` metadata is fully utilized. Phase 3F's time-series functions (`prev`, `lag`, `cumulative`, `rolling_avg`) operate on **element index order**, not calendar dates.

### Decision 6: AST nodes added

| Name | AST node | Arguments |
|---|---|---|
| `prev` | `Prev { measure: String }` | 1 (measure name) |
| `lag` | `Lag { measure: String, periods: Box<ParsedRuleBody> }` | 2 (measure name, period count expr) |
| `cumulative` | `Cumulative { measure: String }` | 1 (measure name) |
| `rolling_avg` | `RollingAvg { measure: String, window: Box<ParsedRuleBody> }` | 2 (measure name, window size expr) |
| `period_index` | `PeriodIndex` | 0 |

**Why measure names are Strings (not arbitrary expressions):** `prev(Spend + CPC)` would mean "evaluate Spend+CPC at the previous period" — this requires evaluating an arbitrary expression at a different coordinate, which is significantly more complex than reading a single cell. Phase 3F restricts time-series functions to bare measure references. `prev(Spend) + prev(CPC)` achieves the same result with clear semantics.

**Why `lag` periods argument is an expression:** Allows `lag(Revenue, Quarter_Length)` where the lag distance is itself a measure value. Validated at eval time: if the expression evaluates to a non-integer or negative value other than for lead, fire a runtime diagnostic.

### Decision 7: Dependency graph implications

**Cross-coordinate dependency rule:** `prev(Spend)` at time index N reads `Spend` at time index N-1. Therefore:
- Writing `Spend` at `Jan_2026` must dirty every measure that uses `prev(Spend)` at `Feb_2026`
- Writing `Spend` at `Feb_2026` must dirty every measure that uses `prev(Spend)` at `Mar_2026`
- Generally: writing to time index N dirties time index N+1 (and N+2, N+3, ... for `lag` and `cumulative`)

**`cumulative` is worst-case:** writing at period 1 dirties ALL subsequent periods' cumulative measures. For 12 monthly periods, this is 11 additional dirty entries per write. For 52 weekly periods, this is 51.

**Performance bound:** the Acme cube has 12 time periods. Cumulative dirty propagation adds at most 11 * (number of cumulative measures) entries to the dirty set per write. This is well within Phase 1A/1B benchmark ceilings. For models with hundreds of time periods, a lint warning (MC3012) advises against `cumulative` on high-cardinality time dimensions.

**`rolling_avg` with fixed window N:** writing at period K dirties periods K+1 through K+N-1. Bounded.

### Decision 8: Diagnostic codes

| Code | Fires when |
|---|---|
| **MC1010** | `lag` called with non-numeric period argument (e.g., `lag(Spend, "three")`) |
| **MC1011** | `rolling_avg` window resolves to non-positive integer at eval time |
| **MC1012** | Time-series function used but no `kind: "Time"` dimension declared in the model |
| **MC2035** | No dimension with `kind: "Time"` (fires only if time-series formulas exist; not an error for models without time-series functions) |
| **MC2036** | Multiple dimensions with `kind: "Time"` (ambiguous — which is the temporal axis?) |
| **MC3010** | Time dimension elements have `date:` metadata in non-chronological order (lint warning) |
| **MC3012** | `cumulative` used on a time dimension with > 52 elements (lint: potential performance concern) |

---

## Out of scope

| Out of scope | Phase / disposition |
|---|---|
| Calendar-math functions (`days_between`, `month_of`, `quarter_of`) | Phase 3I (requires date-as-metadata fully utilized) |
| Fiscal calendar support (custom year-start, 4-4-5 weeks) | Future — can be modeled with lookup tables in 3G |
| `prev` / `lag` on non-Time dimensions | Not Phase 3F; generalized cross-dim access is a separate concern |
| Time-series aggregations (`sum_ytd`, `avg_qtd`) | Expressible as `cumulative` + conditional on period_index; no dedicated function needed |
| Streaming/real-time append to Time dimension | Phase 5C (element auto-creation during import) |
| `date:` element metadata beyond the optional field declaration | Phase 3I |
| Time-series functions with arbitrary expression arguments (e.g., `prev(Spend + CPC)`) | Expressible as `prev(Spend) + prev(CPC)` |

---

## Alternatives considered

1. **Use the dimension named "Time" by convention (no new kind).** Rejected — fragile; models might name it "Period", "Month", "Week", "Date". An explicit kind is unambiguous and validator-enforceable.

2. **Return 0 at boundaries instead of Null.** Rejected — `0` is meaningful (zero revenue IS different from "no data for this period"). Null preserves the distinction; users explicitly choose their boundary behavior with `if_null`/`coalesce`.

3. **Add a separate `lead()` function.** Rejected — `lag(measure, -n)` is equivalent and avoids function-table bloat. Negative lag is standard in data analysis.

4. **Require full windows in `rolling_avg` (return Null for partial).** Rejected — partial windows match common expectations (Excel, pandas). Users who want strict full-window behavior use the `if(period_index() >= N-1, ...)` pattern.

5. **Parse element names to infer dates (no metadata).** Rejected — element naming conventions vary wildly across models ("Jan_2026", "2026-01", "M1", "Period_1"). Explicit `date:` metadata is the only reliable approach.

6. **Allow time-series functions on any dimension (not just Time-kind).** Rejected for Phase 3F — the concept of "previous" only has clear semantics on a linearly-ordered dimension. Time is the natural fit. Cross-dim reads with arbitrary ordering are a separate, harder problem.

7. **Ship `cumulative` and `rolling_avg` in a later phase (3F.1) due to dirty-propagation complexity.** Rejected — the dirty-prop machinery is the same for `prev`/`lag`; cumulative just has wider fan-out. The bound is manageable for typical cube sizes and linted for pathological cases.

---

## Cross-links

- [`0011-phase-3e-conditionals-and-basic-operations.md`](0011-phase-3e-conditionals-and-basic-operations.md) — Phase 3E (prerequisite; `safe_div`, `if`, `coalesce` used in time-series patterns)
- [`../research-notes/formula-language-expansion.md`](../research-notes/formula-language-expansion.md) — full expansion research (3E through 3J)
- [`../research-notes/cross-coordinate-formulas.md`](../research-notes/cross-coordinate-formulas.md) — cross-coordinate read architecture (same pattern)
- [`../../crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) — `ParsedRuleBody` enum (5 new variants) + `ParsedDimension.kind` (new legal value)
- [`../../mosaic-plugin/skills/schema-design/SKILL.md`](../../mosaic-plugin/skills/schema-design/SKILL.md) — dim kind documentation (updated at 3F ship)

---

## Notes

Phase 3F's implementation is architecturally simpler than it appears. The cross-coordinate read infrastructure built for `actual_ref` in Phase 3E handles the hard part (dep-graph entries for cross-coordinate dependencies, dirty propagation across coordinate positions). Phase 3F extends that same machinery to a different dimension axis (Time instead of Scenario).

**The `kind: "Time"` addition is a schema-level change only.** The kernel (`mc-core`) does not need to know about dimension kinds — it operates on `DimensionId` and `ElementId`. The model layer (`mc-model`) already validates dimension kinds; adding `"Time"` to the legal-kinds list is a one-line change in the validator. The formula evaluator (model layer) uses the kind to identify which dimension to offset when evaluating `prev`/`lag`.

**Element ordering is the definition of "time sequence."** There is no calendar algebra in Phase 3F. "Previous" means "element at index - 1 in the declaration order." This is sufficient for all common cases (months declared Jan-Dec, quarters declared Q1-Q4, weeks declared W01-W52). Models that need non-linear time (fiscal years starting in July, 4-4-5 retail calendars) handle this through element declaration order — list your fiscal months in fiscal order.
