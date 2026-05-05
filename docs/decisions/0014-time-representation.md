# ADR-0014: Time Representation in Mosaic

**Status:** Accepted
**Date:** 2026-05-05
**Deciders:** project owner
**Scope:** cross-cutting (engine, model metadata, Tessera ingestion, UI display)

> This ADR documents the architectural rules for how Mosaic handles time — from the kernel's perspective (positions only) to the model metadata (ISO 8601 / UTC only) to ingestion (explicit source format) to display (configurable rendering). The goal: **Mosaic itself cannot silently misinterpret time** because ambiguous forms never reach the engine. Interpretation happens at explicit, validated boundaries.
>
> This is an architectural-rule ADR (same shape as ADR-0001's canonical dimension order). It documents invariants that span multiple phases; implementation lands across Phase 3F.1 (time_anchor), Phase 5C (Tessera format enforcement), and Phase 6 (display rendering).

---

## Strategic centerpiece: the 3-layer architecture

```
┌────────────────────────────────────────────────────────────────────┐
│ LAYER 3: Display Rendering (Phase 6 UI / mc model inspect)         │
│   "Jan 2025" or "2025-01" or "01/2025" — user preference          │
│   Currency: $1,234.56 or €1.234,56 — locale setting                │
│   12hr/24hr: "1:30 PM" or "13:30" — user preference               │
│   The engine never produces these; the UI renders FROM canonical.   │
├────────────────────────────────────────────────────────────────────┤
│ LAYER 2: Model Metadata (YAML element declarations)                │
│   ISO 8601 only. UTC only. Half-open intervals.                    │
│   period_start: "2025-01-01"                                       │
│   period_end_exclusive: "2025-02-01"                               │
│   granularity declared at DIMENSION level.                         │
│   Validation rejects anything non-ISO at parse time.               │
├────────────────────────────────────────────────────────────────────┤
│ LAYER 1: Engine / Formulas (mc-core + mc-model evaluator)          │
│   Time = ordered element positions. period_index() = 0, 1, 2, ...  │
│   prev(X) = "the element before this one in the sequence"          │
│   anchor_index() = "the configured 'now' element's position"       │
│   NO date parsing. NO timezone math. NO 12hr/24hr handling.        │
│   The engine CANNOT get time wrong because it never sees dates.    │
└────────────────────────────────────────────────────────────────────┘

Data enters via Tessera (Layer 0):
┌────────────────────────────────────────────────────────────────────┐
│ LAYER 0: Tessera Ingestion (mc-tessera + mc-drivers)               │
│   Recipe declares source_format + source_timezone explicitly.      │
│   Tessera parses, normalizes to UTC, maps to Time element names.   │
│   ISO 8601 auto-detected (unambiguous); everything else requires   │
│   explicit declaration. No silent locale guessing.                 │
└────────────────────────────────────────────────────────────────────┘
```

**The invariant:** ambiguous time representations exist ONLY in external source data (CSVs, databases, APIs). They are resolved at the Tessera boundary (Layer 0) by explicit format declaration. By the time data reaches the engine (Layer 1), time is just element positions. The engine cannot misinterpret time because it never sees the ambiguous form.

---

## Decision 1: Engine layer — Time is position-only

Inside the kernel and formula evaluator, Time elements are an ordered sequence. The engine knows position 0, position 1, position 2. It does NOT know January, February, March. It does NOT know timezone, DST, calendar arithmetic, or leap years.

**Formula primitives (from Phase 3E/3F):**

| Function | What it returns | What it does NOT do |
|---|---|---|
| `period_index()` | 0-based position of the current Time element | Does NOT parse element names as dates |
| `prev(X)` | Value of X at position (current - 1) | Does NOT mean "previous calendar month" |
| `lag(X, n)` | Value of X at position (current - n) | Does NOT do calendar subtraction |
| `anchor_index()` | Position of the configured time_anchor element | Does NOT parse dates |
| `is_past()` | 1.0 if position < anchor_index | Does NOT compare against wall-clock time |

**What this eliminates:** DST drift bugs, calendar-arithmetic edge cases (end-of-month ambiguity, leap years), timezone confusion inside formulas. None of these can exist because the engine never encounters dates.

---

## Decision 2: Model metadata — ISO 8601, UTC, half-open intervals

When a model wants its Time elements to correspond to real calendar periods, it declares this via **optional** element metadata. The metadata is validated at parse time; models without it treat Time as pure abstract positions.

### Schema shape

```yaml
dimensions:
  - name: "Time"
    kind: "Time"
    granularity: "month"           # DIMENSION-LEVEL (not per-element)
    time_anchor: "2025_10"         # DIMENSION-LEVEL default anchor (optional)
    elements:
      - name: "2025_M01"
        period_start: "2025-01-01"
        period_end_exclusive: "2025-02-01"
      - name: "2025_M02"
        period_start: "2025-02-01"
        period_end_exclusive: "2025-03-01"
      # ...
```

### Hard rules (binding)

1. **Dates: `YYYY-MM-DD` only.** No `01/02/2025`, no `Jan 2 2025`, no `2025/1/2`. Validation rejects non-ISO with **MC2043**: "element date metadata must be ISO 8601 format YYYY-MM-DD."

2. **Timestamps (when used): `YYYY-MM-DDTHH:MM:SSZ` only.** The `Z` suffix (UTC) is REQUIRED. Timezone offsets (`-08:00`) are rejected. Local timestamps (missing `Z`) are rejected. **MC2044**: "timestamps in model metadata must be UTC (suffix Z required)."

3. **Half-open intervals: `[period_start, period_end_exclusive)`.** This convention means "January" is `[2025-01-01, 2025-02-01)` — no ambiguity about whether the last day is inclusive. Avoids the "does the month end at midnight or 23:59:59?" bug class entirely.

4. **Granularity at DIMENSION level.** Declared once: `granularity: "day" | "week" | "month" | "quarter" | "year"`. All elements in the dimension share the granularity. Per-element granularity would create mixed-resolution Time dimensions, which break `prev`/`lag` semantics (what's "one period back" when periods have different widths?). **MC2045**: "Time dimension granularity mismatch" if declared but element intervals don't match the declared granularity.

5. **Contiguous, non-overlapping, chronologically sorted.** If period metadata is present:
   - Each element's `period_end_exclusive` must equal the next element's `period_start`. **MC2046**: "gap between Time elements."
   - No overlaps. **MC2047**: "overlapping Time elements."
   - Elements must be in chronological order by `period_start`. **MC3016** (lint warning): "Time elements not in chronological order."

6. **Period metadata is optional.** A model that doesn't need calendar-aware features can omit `period_start`/`period_end_exclusive` entirely. The engine treats Time as pure positions. Models that want `time_anchor` still work without period metadata — the anchor is an element NAME, not a date.

7. **Element name and metadata are independent.** The element can be named `"Jan_2025"`, `"2025_01"`, `"M1"`, or anything. The engine matches by name (opaque string); the metadata establishes the calendar mapping for validation and future calendar-aware features.

---

## Decision 3: Dimension-level declarations (unified pattern)

Two dimension-kind-specific declarations are established as DIMENSION-LEVEL (not per-element) metadata, following the same pattern:

### For `kind: "Time"` dimensions:

```yaml
- name: "Time"
  kind: "Time"
  granularity: "month"         # what calendar unit each element represents
  time_anchor: "2025_10"       # the "current" period (optional; overridable via CLI)
```

### For `kind: "Scenario"` dimensions:

```yaml
- name: "Scenario"
  kind: "Scenario"
  actuals_element: "Actual"    # which element actual_ref() reads from
```

**Why dimension-level (not per-element):**
- These are invariants of the dimension, not properties of individual elements.
- Declaring once eliminates duplication and the "one element accidentally differs" failure mode.
- Validation is simpler (check one field) than scanning N elements for consistency.

**Diagnostic codes:**
- **MC2037** (already in ADR-0011): `actual_ref` used but no `actuals_element` on the Scenario-kind dimension.
- **MC1017**: anchor function (`is_past`, `anchor_index`, etc.) used but no `time_anchor` configured (neither in YAML metadata nor via `--time-anchor` CLI flag).
- **MC2048**: `time_anchor` names an element not in the Time dimension.

---

## Decision 4: Runtime time_anchor (Phase 3F.1 implementation)

The `time_anchor` is a **runtime parameter** — it can be set in YAML (dimension-level default) and overridden via CLI at invocation:

```bash
mc model test tide-cleaners.yaml --time-anchor 2025_10
mc tessera apply recipe.yaml --time-anchor 2025_11
```

**Semantics:**
- The anchor resolves to a Time element by **name-equality** (not date parsing).
- `--time-anchor 2025_10` means: find the element named `"2025_10"` in the Time dimension; use its index as `anchor_index()`.
- CLI override wins over YAML default.
- If neither is set AND a formula uses an anchor function → **MC1017** diagnostic (not silent Null).

**Formula primitives unlocked:**

| Function | Returns |
|---|---|
| `anchor_index()` | period_index() of the anchor element |
| `is_past()` | 1.0 if period_index() < anchor_index(), else 0.0 |
| `is_current()` | 1.0 if period_index() == anchor_index(), else 0.0 |
| `is_future()` | 1.0 if period_index() > anchor_index(), else 0.0 |
| `periods_since_anchor()` | period_index() - anchor_index() (negative = past) |
| `periods_to_end()` | max_period_index - period_index() |

**Snapshot/rollback interaction:** snapshots capture cube state (cells, revisions), NOT runtime context. Rolling back a snapshot while using a different `time_anchor` restores cells but keeps the current runtime anchor. The anchor is an eval-context parameter, like `now_unix_seconds` in `WritebackRequest`.

**Implementation effort:** ~half day after Phase 3F ships. 6 formula functions, all trivial eval (read from `EvalContext`), no cross-coordinate reads, no dep-graph implications.

---

## Decision 5: Tessera ingestion boundary — explicit format declaration

When Tessera (mc-tessera + mc-drivers) ingests time/date columns from external sources, the recipe MUST declare the source format explicitly for any non-ISO input. No silent locale guessing.

### Recipe schema (additions for Phase 5C):

```yaml
columns:
  - source: "created_at"
    dimension: "Time"
    time_format: "M/d/yyyy h:mm a"        # explicit source format
    time_timezone: "America/New_York"      # IANA timezone (required if source lacks tz)
    map_to_period: "month"                 # how to bucket into Time elements
```

### Rules (binding for Phase 5C):

1. **ISO 8601 is auto-detectable.** If a source column's values parse as `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ`, Tessera auto-detects without requiring `time_format`. ISO is unambiguous by design; auto-detection is safe.

2. **Everything else requires explicit `time_format`.** Slash-separated dates (`01/02/2025`), AM/PM time, timezone-less timestamps, named months (`Jan 2025`), custom formats (`YYYYMMDD`) — all require the recipe to declare the format string. **MC5030**: "non-ISO date column requires explicit time_format in recipe."

3. **Timezone-less timestamps require `time_timezone`.** If a source timestamp has no timezone indicator (`2025-01-01T13:30:00` without `Z` or offset), the recipe MUST declare `time_timezone` using an IANA timezone identifier (`America/New_York`, NOT `EST` or `-05:00`). **MC5031**: "timezone-less timestamp requires time_timezone in recipe."

4. **IANA identifiers only for timezone.** `America/New_York` handles DST transitions correctly (`EST` = `-05:00` always; `America/New_York` = `-05:00` in winter, `-04:00` in summer). **MC5032**: "use IANA timezone identifier (e.g., 'America/New_York'), not abbreviation or fixed offset."

5. **`map_to_period` for bucketing.** When a source has daily timestamps but the model has monthly Time elements, the recipe declares how to bucket: `map_to_period: "month"` means "take the month from the parsed date and map to the element for that month." Mapping failures (date doesn't correspond to any declared Time element) fire MC5033.

---

## Decision 6: Display formatting (Phase 6 UI)

Display rendering is NOT the engine's concern. The engine stores canonical values (f64 for measures, element names for dimensions). The rendering layer converts:

| Stored value | Possible displays | Where configured |
|---|---|---|
| `55.43` | `$55.43`, `€55,43`, `55.4` | Measure `format:` metadata (Phase 6) |
| `0.0888` | `8.88%`, `0.089`, `8.9%` | Measure `format:` metadata |
| `"2025_M01"` | `"Jan 2025"`, `"2025-01"`, `"01/2025"` | Dimension display config (Phase 6) |
| `13:30:00Z` | `"1:30 PM"`, `"13:30"` | User locale preference |

**Phase 6 adds an optional `format:` field on measures:**

```yaml
measures:
  - name: "Ad_Spend"
    format: { type: "currency", symbol: "$", decimals: 2 }
  - name: "ROI"
    format: { type: "multiplier", suffix: "×", decimals: 2 }
  - name: "CTR"
    format: { type: "percentage", decimals: 1 }
```

**This is a rendering directive, not a formula concept.** The engine computes `0.0888`; the UI renders it as `8.9%`. No formula-language change needed.

---

## Decision 7: What this solves

| Bug class | How Mosaic prevents it |
|---|---|
| String-sort of dates ("1/2/2025" sorts before "10/2/2025") | Engine never sorts by date strings; sorts by position index |
| Locale ambiguity ("01/02/2025" = Jan 2 or Feb 1?) | Model metadata is ISO only; ingestion requires explicit format |
| Timezone drift (UTC vs local crosses month/quarter boundary) | Model metadata is UTC only; ingestion normalizes to UTC |
| 12hr/24hr confusion ("1:30" = 01:30 or 13:30?) | Engine never sees clock times; ingestion requires explicit format |
| DST silent shift (weeks aren't always 168 hours) | Engine uses positions, not durations; granularity is declared |
| Calendar arithmetic edge cases (Jan 31 + 1 month = ?) | Engine never does date arithmetic; uses index math |
| "What is now?" ambiguity | time_anchor is explicit runtime parameter; MC1017 if missing |
| End-of-period boundary (does Jan end at midnight or 23:59:59?) | Half-open intervals: `[2025-01-01, 2025-02-01)` — no ambiguity |

---

## Implementation phasing

| Rule | Implemented in | Status |
|---|---|---|
| Engine = positions only | Phase 3D/3F | ✓ Already true |
| `period_index()`, `prev()`, `lag()` | Phase 3F | ✓ Shipped in 3E-3F-3G bundle |
| `time_anchor` + anchor functions | **Phase 3F.1** | Next (~half day) |
| `actuals_element` on Scenario dim | Phase 3E | ✓ Shipped |
| ISO/UTC metadata validation (MC2043-MC2048) | **Phase 3F.1** (alongside anchor) | Queued |
| `granularity` dimension-level field | **Phase 3F.1** | Queued |
| Half-open interval fields (`period_start`, `period_end_exclusive`) | **Phase 3F.1** | Queued |
| Tessera `time_format` + `time_timezone` enforcement | **Phase 5C** | Planned |
| Display formatting (`format:` on measures) | **Phase 6** | Planned |

---

## Diagnostic codes introduced

| Code | Layer | Fires when |
|---|---|---|
| MC1017 | Engine (parse/eval) | Anchor function used but no time_anchor configured |
| MC2043 | Model validation | Element date metadata not ISO 8601 YYYY-MM-DD |
| MC2044 | Model validation | Timestamp metadata not UTC (missing Z suffix) |
| MC2045 | Model validation | Time element intervals don't match dimension's declared granularity |
| MC2046 | Model validation | Gap between consecutive Time elements |
| MC2047 | Model validation | Overlapping Time elements |
| MC2048 | Model validation | time_anchor names a non-existent element |
| MC3016 | Lint (warning) | Time elements not in chronological order |
| MC5030 | Tessera (Phase 5C) | Non-ISO date column without explicit time_format |
| MC5031 | Tessera (Phase 5C) | Timezone-less timestamp without time_timezone |
| MC5032 | Tessera (Phase 5C) | Non-IANA timezone identifier used |
| MC5033 | Tessera (Phase 5C) | Date doesn't map to any declared Time element |

---

## Out of scope

| Topic | Disposition |
|---|---|
| Date arithmetic inside formulas (`add_months`, `add_days`) | Deferred indefinitely; use position math + lookups instead |
| Fiscal year support | Model it via hierarchy (FY elements as parents of month elements); no engine change |
| Multi-timezone within one model | Not supported; normalize to UTC at ingestion |
| Sub-daily granularity (hourly, minute) | Supported by the schema (granularity: "hour") but no special engine support |
| Calendar-aware `prev()` (e.g., "prev month" skipping holidays) | Use `lag()` with appropriate offset; don't embed calendar logic in the engine |
| `anchor_element_name()` returning a string | Requires Phase 3J string-value support; deferred |

---

## Alternatives considered

1. **Date-native Time dimension (parse element names as dates).** Rejected — makes the engine depend on date-parsing logic, introduces locale/timezone bugs, and breaks the "positions only" invariant that eliminates an entire class of errors.

2. **Per-element granularity.** Rejected — creates mixed-resolution Time dimensions where `prev()` semantics are ambiguous ("previous what?"). Dimension-level granularity is an invariant, not a per-element property.

3. **Automatic timezone inference in Tessera.** Rejected — silent inference is the root cause of timezone bugs in every data system that tries it. Explicit declaration is more typing but eliminates the bug class entirely.

4. **End-inclusive intervals (`date_start` + `date_end`).** Rejected in favor of half-open `[start, end_exclusive)`. The inclusive-end form creates "does Jan 31 end at midnight, 23:59:59, or 23:59:59.999?" ambiguity that half-open intervals avoid entirely.

5. **Embed time_anchor in the model file only (no CLI override).** Rejected — the anchor is a runtime parameter that changes monthly. Editing YAML every month defeats the purpose. CLI override + YAML default gives flexibility without sacrificing reproducibility.

---

## Cross-links

- [ADR-0011 (Phase 3E)](./0011-phase-3e-conditionals-and-basic-operations.md) — `actual_ref` + `actuals_element` (the Scenario-dim analog of time_anchor)
- [ADR-0012 (Phase 3F)](./0012-phase-3f-time-series-operations.md) — `period_index`, `prev`, `lag` (position-based time primitives)
- [Research note: time_anchor](../research-notes/time-anchor-runtime-parameter.md) — the original Phase 3F.1 proposal
- [Research note: cross-coordinate formulas](../research-notes/cross-coordinate-formulas.md) — `actual_ref` architecture
- [PERF.md §6.18](../PERF.md) — Tide Cleaners benchmark (43ms recompute) where the time-anchor gap surfaced
- [ADR-0010 (Phase 5 Tessera)](./0010-phase-5-tessera-architecture.md) — ingestion layer that enforces format rules
- [`CLAUDE.md`](../../CLAUDE.md) — project naming conventions (MC diagnostic code namespace)
