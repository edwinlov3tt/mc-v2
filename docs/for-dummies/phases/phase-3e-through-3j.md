# Phases 3E – 3J — For Dummies

> **In one line:** we taught the YAML model files how to do math. By the end of these six phases, you can write almost anything you'd write in Excel — plus the planning-specific things Excel can't do (like "what was this number last month vs this month?", "what's the average across all our markets weighted by spend?", "what does our fitted regression model predict here?").

[Technical versions → ADRs 0011-0016](../../decisions/) · [Phase reports for 3E-3I](../../reports/) · [Phase 3J handoff](../../handoffs/phase-3j-formula-deferred-handoff.md)

---

## The big picture: what is a "formula engine"?

Imagine you're authoring a budget in Excel. You type a number into cell B5 (say, your January spend). In B6 you type `=B5*1.10` to project a 10% increase in February. That little `=B5*1.10` is a **formula** — it tells Excel "this cell's value is computed from other cells, using these operators."

Mosaic does the same thing, but at a much bigger scale. Instead of cell B5, you have a *coordinate* like "Spend at Houston, January 2026, Plan scenario." Instead of one formula per cell, you write ONE formula that applies to thousands of cells in a pattern. And instead of just `+` and `*`, the formula engine knows about time periods, scenarios, hierarchies, statistical models, and more.

Phase 3D was when Mosaic got its first formula syntax (you could write `body: "Customers * AOV"` instead of nested objects). But the **operators** available were tiny — just arithmetic. Phases 3E through 3J grew that operator vocabulary from "Excel circa 1985" all the way to "modern planning system."

By the end of 3J, the formula language is **complete**. Future formula additions only happen when a real customer asks for something specific, not because someone speculates we might need it.

---

## The analogy: leveling up your spreadsheet skills

Think of these phases as the journey from "I just learned what a formula is" to "I can build a real financial model":

- **Phase 3E:** Excel adds `IF()`. Now you can branch — "if revenue is over $1M, give a 15% bonus, else give 5%."
- **Phase 3F + 3F.1:** Excel adds time-aware functions. Now you can ask "what was this same metric last month?" without manually typing cell references.
- **Phase 3G:** Excel adds `VLOOKUP`. Now you can have a separate "tax rate by state" table and look things up.
- **Phase 3H:** Excel adds the ability to *predict* using a regression model you've already fitted in Python. Not Excel's wheelhouse, but planning models live or die by this.
- **Phase 3I:** Excel adds math primitives (square root, logarithm, normal distribution functions) plus a few quality-of-life features. Now you can compute Kelly criterion bets, NPV, safety stock — the math actually used in real planning.
- **Phase 3J:** Excel adds named constants (cell `=q1_anchor` instead of `=$B$2`), declarative indicator columns ("flag every Houston row with a 1"), and the ability to read from a *different scenario* of the same model. Plus the ability to fill forward "last known value" for forecasting.

Each phase builds on the last. By the end, the formula language covers ~98%+ of what real planning models need to express.

---

## Phase 3E — Conditionals and basic operations

**Tag:** [`phase-3e-3f-3g-formula-expansion`](https://github.com/edwinlov3tt/mc-v2/releases/tag/phase-3e-3f-3g-formula-expansion) (shipped together with 3F+3G as one tag) · **ADR:** [`0011`](../../decisions/0011-phase-3e-conditionals-and-basic-operations.md)

### What it added

- **`if(condition, then, else)`** — branch on a condition. `body: "if(Spend > 1000, Spend * 0.15, Spend * 0.05)"` gives a higher commission rate when spend exceeds $1K.
- **Comparison operators:** `<`, `>`, `<=`, `>=`, `==`, `!=`. Returns `1.0` (true) or `0.0` (false).
- **Logical operators:** `and(...)`, `or(...)`, `not(...)`.
- **`min(a, b, c, ...)`** and **`max(a, b, c, ...)`** — pick the smallest or largest of several values.
- **`if_null(value, fallback)`** — if value is missing, use the fallback. Like Excel's `IFERROR`.

### Why we care

Without these, every branching decision had to happen *outside* the cube. You'd compute a forecast in your formula, then in Python say "if it's negative, replace with zero," then write the result back. Now that logic lives inside the model, where it belongs.

### One thing that's easy to get wrong

**Null is not zero.** If you write `if(Spend > 0, ...)` and Spend is missing (Null), the comparison returns Null, NOT false. The whole `if` then returns Null too. This is correct behavior — if you don't know the spend, you can't conclude anything — but it surprises Excel users who expect Null to silently behave like 0.

---

## Phase 3F — Time-series operations

**Tag:** bundled with 3E · **ADR:** [`0012`](../../decisions/0012-phase-3f-time-series-operations.md)

### What it added

- **`prev(measure)`** and **`next(measure)`** — read the same measure from the previous or next time period.
- **`lag(measure, n)`** and **`lead(measure, n)`** — like `prev`/`next` but you specify how many periods to shift. `lag(Revenue, 12)` reads "Revenue 12 months ago."
- **`cumsum(measure)`** — running total across time periods.
- **`period_delta(measure)`** — the difference vs the previous period. Equivalent to `measure - prev(measure)`.

### Why we care

Real planning is mostly comparison: this month vs last month, year-over-year, year-to-date. Without time-series operators, every YoY chart required hand-rolling Python that pulled the right cells. Now `body: "Revenue / lag(Revenue, 12) - 1"` gives you year-over-year growth as a one-liner.

### One thing that's easy to get wrong

**`prev` at the first time period returns Null, not zero.** This is the "Null is not zero" rule again. If your model uses `prev` extensively, you'll see Nulls at January 2024 (or whatever your earliest period is). Wrap with `if_null(prev(X), 0)` if you want explicit zero-fill.

---

## Phase 3F.1 — Runtime time anchor

**Tag:** bundled with 3E · **ADR:** [`0014`](../../decisions/0014-time-representation.md)

### What it added

A way to say "this time period is *now*." Models declare a `time_anchor: Apr_2026` on the Time dimension; runtime functions `is_past()`, `is_current()`, `is_future()` then return 1.0/0.0 based on whether the current coord's time period is before, equal to, or after the anchor.

### Why we care

Forecasting cubes need to know "where does the actual data end and the forecast begin?" Without a time anchor, you had to hard-code that boundary into every formula or compute it in Python. Now the model declares it once, and any formula can ask.

### One thing that's easy to get wrong

**The anchor is per-model, not global.** Two models open in the same session can have different anchors. If you're comparing across models, make sure the anchors line up.

---

## Phase 3G — Reference-data blocks

**Tag:** bundled with 3E · **ADR:** [`0013`](../../decisions/0013-phase-3g-reference-data-blocks.md)

### What it added

Three new top-level YAML blocks that hold "structured constants" — data that isn't part of the cell grid but the formulas need to read.

```yaml
benchmarks:
  - name: industry_cpc
    source: "WordStream 2025"
    last_updated: "2025-03-15"
    key_dimension: Channel
    values:
      Paid_Search: 5.50
      Paid_Social: 3.20

lookup_tables:
  - name: state_tax_rate
    key_dimension: State
    values:
      CA: 0.0725
      NY: 0.04

status_thresholds:
  - name: cpc_health
    bands:
      - { label: "Good",     max: 3.0 }
      - { label: "Warning",  max: 7.0 }
      - { label: "Critical", max: 999.0 }
```

Plus three new formula functions to read them:

- `lookup("state_tax_rate", State)` — reads from the lookup table at the current State.
- `bucket("cpc_health", CPC)` — returns the band index (0=Good, 1=Warning, 2=Critical) based on the value.
- `benchmark("industry_cpc", Channel)` — like `lookup` but with attribution metadata.

### Why we care

Before this, "the WordStream 2025 industry CPC for Paid Search is $5.50" had to be a measure with a hardcoded input value. You couldn't tell where it came from, when it was last updated, or whether it was operational data or external reference data. Now reference data is *labeled* as such, with source attribution — which matters when an audit asks "where did this number come from?"

### One thing that's easy to get wrong

**Lookup tables only support ONE key dimension.** (Phase 3I extends this to multi-key — see below.) If you want to look up "seasonality factor by Market AND Time," you'd need 4 separate single-key tables in 3G. Phase 3I fixes this.

---

## Phase 3H — Fitted-model evaluation

**Tag:** [`phase-3h-fitted-model-evaluation`](https://github.com/edwinlov3tt/mc-v2/releases/tag/phase-3h-fitted-model-evaluation) · **No ADR** (followed established 3E-3G pattern)

### What it added

Four new functions that bring statistical-model output into the cube:

- **`predict("model_name", feature1, feature2, ...)`** — looks up a fitted model by name, applies its coefficients to the features you pass in, returns the prediction.
- **`calibrate("calibration_map_name", raw_probability)`** — adjusts a raw probability through a calibration curve (PAVA or Platt scaling).
- **`exp(x)`** — Euler's number to the x. Used in logistic regressions.
- **`norm_cdf(x, mean, sigma)`** — cumulative density function of the normal distribution. Used in stats / probability work.

Plus two new YAML blocks: `fitted_models:` (model coefficients) and `calibration_maps:` (calibration curves), structurally similar to 3G's reference data.

### Why we care

This is the **"calculator + judge + investigator"** pattern: a fitted model is computed in Python (because Python has scikit-learn, etc.) and the *coefficients* land in the YAML. Mosaic then *evaluates* the model at planning coordinates without re-fitting. So the "model fit" stays in Python; the "model use" lives in the cube.

A real example from the NBA totals cartridge: `body: "predict('v16_lasso', avg_pace, off_efficiency, def_efficiency, ...)"` runs the Lasso regression model that was fit on 3,685 historical games. The model itself is in YAML as a list of coefficients; predict() does the dot product.

### One thing that's easy to get wrong

**Phase 6A.1 silently fixed a critical bug here.** The original `predict()` paired features and coefficients by *position*, so if your YAML listed coefficients in a different order than your formula's feature args, you got silently wrong predictions. Phase 6A.1 made the lookup name-keyed at eval time. The bug was caught by the Sonnet code review — see ADR-0015 Acceptance Amendment §1 for the back-story.

---

## Phase 3I — Formula language completion

**Tag:** [`phase-3i-formula-language-completion`](https://github.com/edwinlov3tt/mc-v2/releases/tag/phase-3i-formula-language-completion) · **ADR:** [`0015`](../../decisions/0015-phase-3i-formula-language-completion.md)

### What it added

Eight things that filled out the corners of the formula engine:

1. **`is_element(Dim, "Element")`** — returns 1.0 if the current coordinate is at the named element, else 0.0. Lets you write `if(is_element(Market, "Houston"), Spend * 1.2, Spend)` instead of pre-generating 464 indicator rows in CSV.
2. **9 math primitives:** `pow`, `sqrt`, `ln`, `log10`, `round`, `floor`, `ceil`, `mod`, `norm_inv`. Now you can compute NPV, Kelly criterion confidence intervals, safety stock — the math real planning needs.
3. **Multi-key `lookup_tables`:** the 3G "one key dimension" limitation goes away. Now `lookup_tables` can have `key_dimensions: [Market, Time, Measure]` for a single 3-key table instead of 5 separate single-key tables.
4. **`predict()` arity validation at load time.** If your formula passes 7 features but the fitted model expects 9, the model fails to load with a clear error (MC2057). Used to silently return Null at runtime.
5. **`avg_over` / `min_over` / `max_over` / `wavg_over`** — like 3G's `sum_over` but for averages, mins, maxes, and weighted averages. "What's the market-average CPC weighted by spend?" is one line.
6. **`ifs(c1, v1, c2, v2, ..., default)` and `switch(...)`** — multi-branch conditionals. Cleaner than nested `if(if(if(...)))`.
7. **Filter parser hyphen support** — `mc model query --where 'Time=Q1-2026'` now works (used to reject the hyphen).
8. **Filter-formula parser unification** — the agent CLI's `--where` filter now uses the same parser as the formula engine. Two parsers became one.

### Why we care

3I is the **"completion line."** After 3I, the formula language is good enough to express ~98% of what real planning models need without dropping to Python. The remaining 2% (cluster D below — string-aware functions, parameters, scoped rules) is Phase 3J.

By the end of 3I, the email-matchback project saw ~350 lines of Python eliminated from its "calculator" code. The remaining ~170 lines were waiting on Phase 3J's deferred items.

### One thing that's easy to get wrong

**The audit caught a real bug at the last minute.** The original ADR-0015 reserved diagnostic code MC2053 for a new validator. But MC2053 had been shipped in Phase 3H for an unrelated rule (per process-notes Rule 3, codes are forever once shipped — like CVE numbers). The implementer's self-audit caught the collision and remediated to MC2057 mid-phase. Outcome: ADR-0015 Acceptance Amendment §1 documents the fix; future ADRs sweep proposed codes against the baseline before publishing.

---

## Phase 3J — Formula authoring deferred items (in progress)

**Tag:** [pending] · **Branch:** `phase-3j/formula-deferred-items` · **ADR:** [`0016`](../../decisions/0016-phase-3j-formula-deferred-items.md) (Accepted with 8 amendments) · **Handoff:** [`phase-3j-formula-deferred-handoff.md`](../../handoffs/phase-3j-formula-deferred-handoff.md)

### What it adds

Seven things that close the formula-engine **deferred queue** from the post-Phase-6A audit:

1. **String literals in formulas:** Phase 3I taught the parser to accept `"Houston"` only as an argument to `is_element()`. Phase 3J makes string literals first-class within formula evaluation (NOT in storage — see "easy to get wrong" below). You can now write `current_element(Channel) == "Email"` directly.
2. **`current_element(Dim)`** — returns the name of the current coord's element in `Dim`. Useful for inline branching: `if(current_element(Channel) == "Email", 0.05, 0.10)`.
3. **`parameters:` block** — declare named constants once, reference everywhere: `param(q1_anchor)` instead of `1234.56` repeated. v1 is constants only; computed parameters can ship in 3J.1 if demand surfaces.
4. **`Indicator` measure role** — declarative indicator measures: `IsHouston: { role: Indicator, dimension: Market, element: Houston }`. Same effect as `is_element` from 3I but reusable across formulas.
5. **`Scope` enum extension** — rules can target only past / present / future leaves: `scope: FutureLeaves` runs the rule only at coords where the time anchor says "future."
6. **`scenario_ref(measure, "ScenarioName")`** and **`actual_ref(measure, fallback)`** — read from a different scenario, or fall back to a different scenario's value when actuals are missing.
7. **`extrapolate_last_value(measure)`** + LOCF (last-observation-carried-forward) — fills future-period gaps by carrying the last known value forward. Closes the "manually extend AdSpend to Nov/Dec" Python pre-processing pattern.

### Why we care

Phase 3J + Phase 3H.1 (a separate parallel ADR for fitted-model amendments) **close the deferred queue completely**. After both ship, every gap surfaced by the post-6A audit is either implemented or has its own future-phase ADR. Future formula additions become **demand-driven** — a real customer hits a need → write an ADR → ship the addition. No more speculative formula expansion.

That matters because it lets the project's attention shift from "what's missing in the formula engine?" to "what's missing in the *application*?" — UI (Phase 6B), distribution (Phase 6C), customer onboarding (Phase 7).

### One thing that's easy to get wrong

**Strings stay transient.** The biggest design decision in Phase 3J is that `ScalarValue::Str` flows through *expression evaluation* but never reaches *storage* (cells, snapshots, consolidation). Why? Because storing strings would require storage-layer changes throughout the kernel — variable-width data, type-aware consolidation, NaN-rejection that handles "can't NaN a string." That's Phase 4+ scope. By keeping strings transient, Phase 3J ships in a single phase instead of a multi-month rewrite.

The practical effect: you can write `current_element(Market) == "Houston"` (returns 1.0/0.0) but you cannot write a measure that *stores* a market name as a cell value. The cube already knows the market name via the coordinate; storing it as a cell would be redundant anyway.

---

## What you can do now (post-3I, soon-post-3J)

A planning model author can now express, in YAML alone:

- ✅ Branching logic (`if`, `ifs`, `switch`)
- ✅ Time-series comparisons (`prev`, `lag`, year-over-year, cumulative sums)
- ✅ Time-anchor-aware queries (`is_past`, `is_current`, `is_future`)
- ✅ Lookup tables (single-key and multi-key)
- ✅ Industry benchmarks with source attribution
- ✅ Status thresholds with health bands
- ✅ Fitted statistical models (`predict`, `calibrate`)
- ✅ Math primitives (`pow`, `sqrt`, `ln`, `norm_inv`, `norm_cdf`, ...)
- ✅ Cross-coordinate aggregations (`sum_over`, `avg_over`, `wavg_over`, ...)
- ✅ Inline indicators (`is_element`) and declarative ones (`Indicator` role) — *3J*
- ✅ Named constants (`parameters:`) — *3J, partial closure of M-14*
- ✅ Scope-restricted rules (`FutureLeaves`, `PastLeaves`, `CurrentLeaves`) — *3J*
- ✅ Cross-scenario reads (`scenario_ref`, `actual_ref(m, fallback)`) — *3J*
- ✅ Last-observation-carried-forward (`extrapolate_last_value`) — *3J*

What's still NOT in YAML (and probably never should be):

- ❌ Stochastic / random sampling — out of scope for a deterministic kernel
- ❌ Storing string values in cells — Phase 4+ kernel storage decision
- ❌ Computed `parameters:` (where the value is a formula) — Phase 3J.1 if demanded
- ❌ Scoped `parameters:` (per-Scenario, per-Market) — Phase 3J.1 if demanded
- ❌ `Indicator` over multiple dimensions simultaneously — future phase

The first item (stochastic) is a deliberate kernel design boundary. The others have escape clauses in their respective ADRs — they ship if real customer demand surfaces.

---

## Email-matchback Python reduction

The "ground-truth" benchmark for whether the formula engine is "complete" is an external project (the Tide Cleaners email-matchback model) that uses Mosaic for its forecasting. Its Python "calculator" code went from ~1,260 lines pre-Phase-6A to:

| Phase | Net Python eliminated | Cumulative |
|---|---|---|
| Phase 6A | ~350 lines (goldens-as-probes pattern → `mc model query`) | ~350 |
| Phase 6A.1 | (small; non-ISO date parsing) | ~360 |
| Phase 3I | ~80 lines (math primitives + indicators in MMM) | ~440 |
| Phase 3J (projected) | ~170 lines (string-literal-aware filtering, parameters, LOCF, scenario_ref) | ~610 |

Roughly half the original "calculator" Python code is replaced by YAML formulas after 3J ships. The remaining Python is correctly Python: model fitting (sklearn), Tessera-driver gaps (Excel ingestion), and reporting templates — none of which the formula engine is supposed to do.

---

## How to read the rest of the docs

If you want to go deeper on any of these phases:

- **The technical spec:** see the ADR linked in each section (`docs/decisions/0011-...md` through `0016-...md`).
- **The exact code that shipped:** see the completion report (`docs/reports/phase-3X-completion-report.md`).
- **The implementation contract:** see the handoff (`docs/handoffs/phase-3X-...handoff.md`).
- **The one-line summary in the master plan:** see `docs/roadmap/MASTER_PHASE_PLAN.md`.

If you want a similar "for-dummies" walkthrough of Phase 6A.x or Phase 5A-C, those are pending (the Phase 3 arc was the highest-priority for-dummies coverage gap). Ask the PM if you'd like one.

---

*Phase 3 is genuinely the most important arc the project has shipped so far. The kernel (Phase 1) is what runs computations; the formula engine (Phase 3E-3J) is what makes those computations expressible by humans. Without it, every Mosaic user would be writing Rust against the kernel API. With it, they write YAML.*
