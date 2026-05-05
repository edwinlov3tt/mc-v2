# Cross-Coordinate Formulas (Phase 3E Candidate)

> **Status:** Research note. Not scheduled. Filed after the Tide Cleaners real-world validation surfaced the gap (2026-05-04).

## The gap

Mosaic's current formula engine (Phase 3D) evaluates rules at a **single leaf coordinate**. A rule body like `Spend / Clicks` reads `Spend` and `Clicks` at the SAME coordinate the rule is computing — same Scenario, same Version, same Time, same Channel, same Market.

There is no way to read a cell at a DIFFERENT coordinate position within a rule body. Specifically:

```yaml
# WANT (but currently impossible):
- name: forecast_spend_rule
  target_measure: Forecast_Spend
  body: "db(Scenario: 'Actual', Drop_Period: 'Q1_2026') * Plan_Seasonality"
  # "read the Actual Q1 value, multiply by my scenario's plan factor"
```

This is the equivalent of TM1's `DB()` function — the most-used TI/rule function in TM1 production models.

## Where it surfaced

During the Tide Cleaners proof-of-concept, the implementer wanted to write a Forecast scenario where:

```
Forecast_AdSpend(market, period) = Actual_AdSpend(market, Q1_avg) * Plan_Seasonality(market, period)
```

This requires reading from the `Actual` scenario while computing the `Forecast` scenario — a cross-coordinate read. The workaround was a Python script (`build_forecast.py`) that:
1. Reads actual Q1 ratios (ROAS, new-customers-per-dollar)
2. Reads planned spend
3. Computes forecast values externally
4. Writes them to the CSV as pre-computed inputs

This works but defeats the purpose of a formula engine — the "change spend → see forecast update" loop requires re-running the Python script instead of getting instant recalculation from the kernel.

## What TM1 does

TM1's `DB(cube_name, element1, element2, ..., elementN)` function reads any cell in any cube. Rules can reference arbitrary coordinates:

```
['Forecast'] = DB('Sales', !Region, !Product, 'Actual', !Month) * DB('Plan', !Region, !Product, 'Seasonality', !Month);
```

This is both TM1's greatest power (arbitrary cross-references) AND its greatest performance trap (every DB() call is a potential cache miss; deeply nested DB() chains cause exponential eval).

## Design options for Mosaic

### Option A: `db()` function (TM1-equivalent)

```yaml
body: "db(Scenario: 'Actual') * Plan_Factor"
```

- **Pro:** familiar to TM1 users; maximum expressiveness
- **Con:** performance implications (each `db()` is a read at a different coordinate; dependency graph becomes complex); hard to declare dependencies statically (the `declared_dependencies` field currently only names measures, not coordinate positions)

### Option B: `if_scenario()` / `select()` conditional

```yaml
body: "select(Scenario, 'Actual', Actual_Spend, 'Forecast', Plan_Spend * Seasonality)"
```

- **Pro:** limited and analyzable; the kernel knows exactly which scenarios are referenced
- **Con:** verbose for the common case; doesn't generalize to cross-Time or cross-Market reads

### Option C: "Reference measures" pattern

Define a measure that's populated differently per scenario:

```yaml
measures:
  - name: Base_Spend
    role: Input  # populated with Actual in Actual scenario; Plan in Forecast scenario
  - name: Projected_Spend
    role: Derived
    body: "Base_Spend * Seasonality"
```

- **Pro:** no formula-language extension needed; the conditional logic moves to the recipe/import layer
- **Con:** shifts complexity to the data-loading step; less transparent in the model

### Option D: "Scenario feed rules" (Mosaic-native pattern)

A new rule scope that only fires for specific scenarios:

```yaml
- name: forecast_spend_from_actual
  target_measure: Spend
  scope: { scenario: "Forecast" }  # only fires in Forecast; Actual keeps its input value
  body: "actual_ref(Spend) * Seasonality"
```

Where `actual_ref(measure)` is a new built-in that reads from the "Actual" scenario at the same non-Scenario coordinates.

- **Pro:** Mosaic-native; dependency graph is still analyzable (the kernel knows which scenarios feed which); no arbitrary cross-coordinate reads
- **Con:** new concept (scope narrowing by scenario); `actual_ref()` is somewhat magical

## Recommendation

**Option D** is the most Mosaic-native. It preserves the kernel's single-coordinate evaluation model (the rule still fires at one leaf) while adding a controlled escape hatch for the single most common cross-scenario pattern. The dependency graph stays analyzable because `actual_ref(Spend)` declares an explicit dependency on `Spend` in the `Actual` scenario — the kernel can dirty-propagate correctly.

**Do NOT implement Option A (full `db()`)** without a performance ADR. TM1's `db()` is the root cause of most TM1 performance problems in production; Mosaic should learn from that history rather than repeating it.

## When to schedule

This is a **Phase 3E** candidate (formula-language extensions). Prerequisites:
- Phase 5A shipped (it's the first consumer of this pattern via Tessera recipes)
- Phase 6 UI may surface additional demand (planners want "change Actual → see Forecast update" in the UI)
- The Tide Cleaners workaround (external Python script) is acceptable for the POC/Phase 6 demo

**File as a future ADR when Phase 6 surfaces concrete demand for cross-scenario planning UX.** The Python-script workaround is functional (43ms round-trip proven); the formula extension is a UX improvement, not a capability gap.

## Cross-links

- [PERF.md §6.18](../PERF.md) — the 43ms recompute benchmark that proves the Python-script workaround is fast enough for interactive use
- [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) — Phase 5A Tessera architecture (the data-ingestion side of the forecast story)
- `skills/formulas/SKILL.md` — current formula grammar (no cross-coordinate support)
- TM1 reference: `DB()` function documentation in the research/ folder
