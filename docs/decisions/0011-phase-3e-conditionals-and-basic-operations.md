# ADR-0011: Phase 3E — Conditionals and Basic Operations

**Status:** Proposed
**Date:** 2026-05-04
**Deciders:** project owner
**Phase:** 3E (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3D shipped the formula parser over the existing 7-variant AST. Phase 3E is the first AST extension — adding conditionals, comparisons, logical operators, and basic math functions. This is the "80% unlock" that makes Mosaic formulas viable for real-world models beyond simple arithmetic chains.

---

## Context

Phase 3D (ADR-0007) deliberately limited the formula grammar to exactly the existing AST capability: `+ - * /`, parens, unary, and `if_null`. Decision 9 of ADR-0007 explicitly deferred `min`, `max`, `if`, comparison operators, and conditional expressions to "Phase 3E or later."

Real-world models (validated by the Tide Cleaners proof-of-concept) immediately hit the wall:
- Budget capping requires `min(Spend, Budget_Cap)` — currently impossible
- Variance analysis requires `if(abs(Actual - Budget) > Threshold, ...)` — currently impossible
- Safe division (denominator might be zero) requires `safe_div(a, b, default)` — currently worked around with `if_null` but that only catches Null, not zero
- Cross-scenario planning requires `actual_ref(Spend)` — currently done via external Python scripts

Phase 3E addresses all of these with a single coordinated extension to the AST, parser, evaluator, and serializer.

---

## Decisions

### Decision 1: AST nodes added to ParsedRuleBody

Phase 3E adds 17 new variants to `ParsedRuleBody`:

**Comparison operators (6):**
- `Gt { left, right }` — `a > b`
- `Lt { left, right }` — `a < b`
- `Gte { left, right }` — `a >= b`
- `Lte { left, right }` — `a <= b`
- `Eq { left, right }` — `a == b`
- `Neq { left, right }` — `a != b`

**Logical operators (3):**
- `And { left, right }` — `a and b`
- `Or { left, right }` — `a or b`
- `Not { operand }` — `not a`

**Functions (8):**
- `If { condition, then_branch, else_branch }` — `if(cond, then, else)`
- `Min { args: Vec }` — `min(a, b)` (variadic: 2+ args)
- `Max { args: Vec }` — `max(a, b)` (variadic: 2+ args)
- `Abs { operand }` — `abs(x)`
- `SafeDiv { numerator, denominator, default }` — `safe_div(a, b, fallback)`
- `Clamp { value, lo, hi }` — `clamp(x, lo, hi)`
- `Coalesce { args: Vec }` — `coalesce(a, b, c, ...)`
- `ActualRef { measure }` — `actual_ref(Measure_Name)`

### Decision 2: Precedence order

From lowest to highest binding:

1. `or` (lowest)
2. `and`
3. `not` (unary logical)
4. Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=` (non-associative — `a > b > c` is a parse error)
5. Addition: `+`, `-`
6. Multiplication: `*`, `/`
7. Unary arithmetic: `+`, `-`
8. Function call / primary / parentheses (highest)

**Why comparisons are non-associative:** `a > b > c` is ambiguous (does it mean `(a > b) > c` or `a > b AND b > c`?). Requiring explicit `and`/`or` prevents silent misinterpretation. Fire MC1008 on chained comparisons without explicit grouping.

### Decision 3: Boolean representation — f64-encoded (no ScalarValue change)

Comparisons return `1.0` (true) / `0.0` (false) as `ScalarValue::Scalar(f64)`. `if()` treats non-zero as truthy, zero as falsy, Null as falsy.

**Rationale:** Adding `ScalarValue::Bool` would require kernel changes (consolidation of booleans, dirty propagation of booleans, boolean arithmetic semantics). The f64-encoded approach is used by Excel, TM1, and most planning engines. It works because boolean results are almost always consumed by `if()` (which only needs truthy/falsy) or stored as flag measures (where 0/1 is the natural representation).

**Null in comparisons:** Any comparison involving Null returns `0.0` (falsy). `Null > 5` = 0.0. `Null == Null` = 0.0. This matches SQL's three-valued logic convention (NULL comparisons are false) and prevents surprising cascade behavior.

### Decision 4: `actual_ref` ships in 3E (not a separate sub-phase)

`actual_ref(Measure)` ships as part of Phase 3E despite being architecturally different (cross-coordinate read vs. local computation).

**Rationale:**
- The user need (cross-scenario planning) was the first gap identified in production validation
- The dep-graph machinery for cross-coordinate reads is the same infrastructure Phase 3F needs for `prev()`/`lag()`; building it in 3E amortizes the design cost
- Shipping it separately would add a release boundary with no architectural benefit
- The implementation is bounded: `actual_ref(X)` reads X at `[Scenario="Actual", same Version, same Time, same Channel, same Market, Measure=X]` — one targeted coordinate, not an arbitrary scan

**Scope constraint:** `actual_ref` in 3E reads ONLY from the "Actual" scenario. A more general `ref(Scenario: "X", Measure: "Y")` (equivalent to TM1's `DB()`) is explicitly NOT Phase 3E. If generalized cross-coordinate reads are needed, that's a separate ADR with performance analysis.

**Dependency graph implication:** `actual_ref(Spend)` in the Forecast scenario declares a cross-scenario dependency. Writing `Spend` at `[Actual, ...]` must dirty `Forecast_Spend` at `[Forecast, ...]`. The dep-graph's `reverse_edges` map must include cross-scenario entries.

### Decision 5: Diagnostic codes

| Code | Fires when |
|---|---|
| **MC1007** | Unknown function call (any identifier followed by `(` that isn't in the registered function table). Split from MC1004's catch-all now that the function table has grown beyond `if_null`. |
| **MC1008** | Wrong argument count for a function (e.g., `min(a)` with 1 arg, `if(a, b)` with 2 args, `safe_div(a, b)` with 2 args). Also fires on chained non-associative comparisons (`a > b > c`). |
| **MC1009** | `actual_ref` called with non-identifier argument (e.g., `actual_ref(Spend + 1)` — must be a bare measure name). |

**MC1004 narrows:** After Phase 3E, MC1004 still covers "unexpected token" (stray punctuation, etc.) but no longer covers unknown functions — that's MC1007 now. This is a diagnostic-code split, not a behavior change; existing MC1004 firings for unknown functions become MC1007 firings.

### Decision 6: Performance implications

**Local operations (no cross-coordinate reads):** `if`, comparisons, `and`/`or`/`not`, `min`, `max`, `abs`, `safe_div`, `clamp`, `coalesce` — these add branches to the eval loop but NO additional cell reads. Each evaluation still reads only the measures in `declared_dependencies` at the current coordinate. Performance impact: negligible (one additional match arm per eval).

**Cross-coordinate operation (`actual_ref`):** ONE additional cell read per evaluation (reading from the Actual scenario). This is bounded and predictable — the kernel already handles multiple reads per eval (any rule with 2+ declared_dependencies reads multiple cells). `actual_ref` adds exactly one more. The dep-graph correctly captures the cross-scenario edge for dirty propagation.

**No new performance gate needed.** The existing Phase 1A/1B benchmark ceilings remain valid — the new AST nodes don't change the algorithmic complexity class of evaluation.

---

## Out of scope

| Out of scope | Phase / disposition |
|---|---|
| Time-series functions (`prev`, `lag`, `cumulative`, `rolling_avg`) | Phase 3F |
| Reference-data blocks (`benchmarks:`, `lookup_tables:`) | Phase 3G |
| Fitted-model evaluation (`predict`) | Phase 3H |
| Math functions (`pow`, `sqrt`, `ln`, `exp`) | Phase 3I |
| ScalarValue changes (distributional cells) | Phase 3J (requires kernel ADR) |
| String operations (`concat`, `lower`, etc.) | Deferred indefinitely (recipe/transform layer) |
| Generalized `db()` / arbitrary cross-coordinate reads | Separate ADR if ever needed |
| `ScalarValue::Bool` variant | Not Phase 3E; revisit at Phase 3J |
| `actual_ref` with arbitrary scenario name (only "Actual" is supported) | Future extension if needed |
| Short-circuit evaluation for `and`/`or` | Nice-to-have optimization, not required for correctness |

---

## Alternatives considered

1. **Add `ScalarValue::Bool` for proper boolean typing.** Rejected — kernel change not justified for Phase 3E. The f64-encoded approach works for all 3E use cases. Revisit at Phase 3J when ScalarValue is already being modified.

2. **Ship `actual_ref` as a separate Phase 3E.1.** Rejected — the cross-coordinate read machinery is bounded (one targeted read), and the same dep-graph infrastructure serves Phase 3F. No architectural benefit to a separate release.

3. **Use `? :` ternary syntax instead of `if()` function syntax.** Rejected — ternary is familiar to programmers but opaque to business planners. `if(condition, then, else)` is readable by non-technical users and matches Excel's `IF()`.

4. **Make comparisons return a new `Bool` AST node (not `Gt`/`Lt`/etc.).** Rejected — separate nodes per comparison operator enable the serializer to round-trip the exact operator. A generic `Compare { op, left, right }` would work too but adds enum-within-enum complexity.

5. **Defer `coalesce` (can be expressed as nested `if_null`).** Rejected — `coalesce(a, b, c, d)` is dramatically more readable than `if_null(a, if_null(b, if_null(c, d)))`. The implementation cost is trivial (variadic null-check loop).

6. **Support `between(x, lo, hi)` as sugar for `x >= lo and x <= hi`.** Rejected for 3E — can be added later if demand surfaces. Users can write the expanded form.

---

## Cross-links

- [`0007-phase-3d-friendly-formula-syntax.md`](0007-phase-3d-friendly-formula-syntax.md) — Phase 3D (the parser this extends)
- [`../research-notes/formula-language-expansion.md`](../research-notes/formula-language-expansion.md) — full expansion research (3E through 3J)
- [`../research-notes/cross-coordinate-formulas.md`](../research-notes/cross-coordinate-formulas.md) — `actual_ref` original research note
- [`../../crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) — `ParsedRuleBody` enum (17 new variants added here)
- [`../../mosaic-plugin/skills/formulas/SKILL.md`](../../mosaic-plugin/skills/formulas/SKILL.md) — formula documentation (updated at 3E ship)

---

## Notes

Phase 3E is the largest single AST expansion (17 new variants from 7 existing = 24 total). After 3E, no subsequent phase adds more than 12. The implementation shape is straightforward: extend the recursive-descent parser with new precedence levels, add match arms to the evaluator, add serialization rules for round-trip. The cross-coordinate read (`actual_ref`) is the only piece with dep-graph implications.

**The parser remains hand-rolled recursive-descent.** No parser library. The grammar extension is mechanical: add precedence levels for `or` < `and` < comparisons between the existing `expr` (addition) and `term` (multiplication) levels. The function table grows from 1 entry (`if_null`) to 9 (`if`, `min`, `max`, `abs`, `safe_div`, `clamp`, `coalesce`, `actual_ref`, plus existing `if_null`).

**Backward compatibility:** All existing formulas parse and evaluate identically. The new operators/functions are additive. No existing diagnostic code changes behavior (MC1004 narrows, but previously-MC1004 unknown-function cases now get the more specific MC1007 — strictly better UX).
