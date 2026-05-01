---
name: Null vs zero vs NaN
description: Three superficially-similar f64 states the engine treats as semantically distinct — Null is a first-class enum variant, division by (near-)zero returns Null, NaN/Inf are rejected at writeback and re-mapped to Null inside rule eval
type: research-note
---

# Null vs zero vs NaN

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2

---

## Conclusion (one sentence)

`ScalarValue::Null` is a first-class enum variant distinct from `F64(0.0)` and from `f64::NAN`; the engine enforces three rules — (1) NaN and ±Inf are rejected at the writeback boundary so they never reach storage, (2) any non-finite intermediate produced inside rule eval is re-mapped to `Null` instead of propagating, and (3) division by anything with `|y| < 1e-300` returns `Null`, never `f64::INFINITY`.

## Why this matters

Rust's `f64` type is forgiving: `0.0`, `NaN`, and `INFINITY` are all valid `f64`s, and `==` works on all of them (with the famous `NaN != NaN` exception). Without an enforced discipline, "no value here" easily ends up represented three different ways depending on which code path produced it, and downstream consumers can't reason about the result. The MarketingCubes engine commits to one canonical "absent / undefined" value (`Null`) and treats the other two states as either errors (writeback) or programming hazards to actively neutralize (rule eval). Get any one of these three rules wrong and the engine starts producing values that pass type checks but are semantically meaningless — `Inf * 0`, NaN-poisoned consolidations, etc.

CLAUDE.md §2.5 names this conflation as one of the recurring traps; this note exists so a future implementer can see the *whole* discipline in one place rather than rediscovering it from scattered enforcement sites.

## Evidence

### Null is a distinct enum variant

[`crates/mc-core/src/value.rs:11-19`](../../crates/mc-core/src/value.rs#L11-L19):

```rust
pub enum ScalarValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    Category(usize),
    Null,
}
```

It is *not* `F64(0.0)` and not represented by any sentinel `f64`. `ScalarValue::is_null` (line 71) is a discriminant check, not a value comparison.

### NaN/Inf are rejected at the writeback boundary

[`crates/mc-core/src/value.rs:111-123`](../../crates/mc-core/src/value.rs#L111-L123) — `validate_finite_f64` returns `EngineError::InvalidValue` for both NaN and ±Infinity. It's called from:

- [`crates/mc-core/src/value.rs:28-31`](../../crates/mc-core/src/value.rs#L28-L31) — `ScalarValue::checked_f64`, the constructor used at the API boundary.
- [`crates/mc-core/src/cube.rs:828-832`](../../crates/mc-core/src/cube.rs#L828-L832) — `Cube::write` step (9) ("NaN / Inf reject. Per spec §3.18"). The check fires *after* type-check and *before* commit.

The integration test [`crates/mc-core/tests/value_nan.rs`](../../crates/mc-core/tests/value_nan.rs) (8 tests) exercises both NaN and Inf rejection paths, plus the rule-eval re-mapping (next section).

### Rule eval re-maps non-finite results to Null

[`crates/mc-core/src/rule.rs:396-405`](../../crates/mc-core/src/rule.rs#L396-L405):

```rust
fn finite_or_null(v: f64) -> ScalarValue {
    if v.is_finite() { ScalarValue::F64(v) } else { ScalarValue::Null }
}
```

Every arithmetic primitive funnels through this. `null_add` / `null_sub` / `null_mul` / `null_div` ([`rule.rs:407-452`](../../crates/mc-core/src/rule.rs#L407-L452)) all return `finite_or_null(...)` rather than the raw `f64`. The `Inf` / `NaN` reject at writeback only protects the input boundary; this is the dual at the rule-eval boundary, so a transient overflow during eval (huge `Mul`, etc.) produces `Null`, not a poison NaN that flows into storage on the next read-cache-write.

### Division semantics

[`crates/mc-core/src/rule.rs:441-452`](../../crates/mc-core/src/rule.rs#L441-L452):

```rust
fn null_div(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (a, b) {
        (ScalarValue::Null, _) | (_, ScalarValue::Null) => ScalarValue::Null,
        (ScalarValue::F64(_), ScalarValue::F64(y)) if y.abs() < 1e-300 => ScalarValue::Null,
        (ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x / y),
        _ => ScalarValue::Null,
    }
}
```

Three things to note:

1. **Null poisons division on either side**, including `Null / Null`. This is consistent with `null_mul` but differs from `null_add` / `null_sub`, where Null acts as identity. Per spec §7.
2. **`|y| < 1e-300` is the zero-ish threshold**, not `y == 0.0`. Float comparisons against literal zero are unreliable for inputs that are subnormal-but-nonzero; the engine treats them as zero for division. The threshold is documented in CLAUDE.md §7.6.
3. **Result is never `f64::INFINITY`.** A divide that *would* produce infinity returns `Null`. Spec §7 explicit.

### Null arithmetic table (per `null_add` / `null_sub` / `null_mul` / `null_div`)

| Op  | `Null op Null` | `Null op x`  | `x op Null`  |
|-----|----------------|--------------|--------------|
| Add | Null           | x            | x            |
| Sub | Null           | -x           | x            |
| Mul | Null           | Null         | Null         |
| Div | Null           | Null         | Null         |

This table is the runtime expression of brief §7. Worth keeping next to your editor (CLAUDE.md §2.5 advises printing it).

## Where it shows up in the engine

- **Source — value type:** [`crates/mc-core/src/value.rs`](../../crates/mc-core/src/value.rs) — `ScalarValue::Null`, `validate_finite_f64`, `checked_f64`.
- **Source — writeback boundary:** [`crates/mc-core/src/cube.rs:828-832`](../../crates/mc-core/src/cube.rs#L828-L832) `Cube::write` NaN/Inf reject step.
- **Source — rule eval boundary:** [`crates/mc-core/src/rule.rs:396-452`](../../crates/mc-core/src/rule.rs#L396-L452) `finite_or_null`, `null_add`, `null_sub`, `null_mul`, `null_div`.
- **Source — consolidation:** [`crates/mc-core/src/consolidation.rs:286-394`](../../crates/mc-core/src/consolidation.rs#L286-L394) — `Combinator::observe` / `observe_weighted` / `finish`. Sum returns Null when no leaf contributed; WeightedAverage returns Null when total weight is zero (`state.denom.abs() < 1e-300`); Min/Max return Null when every leaf was Null.
- **Tests — writeback rejection:** [`crates/mc-core/tests/value_nan.rs`](../../crates/mc-core/tests/value_nan.rs) (8 tests).
- **Tests — rule-eval Null semantics:** [`crates/mc-core/tests/correctness.rs`](../../crates/mc-core/tests/correctness.rs) (§10.7 / §10.8 doctrines).
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.3, §7 ("Null and arithmetic semantics"), §3.18 (writeback), §10.7 / §10.8.
- **Operating manual:** [`CLAUDE.md`](../../CLAUDE.md) §2.5 (the trap), §3.1 (forbidden patterns table — `value == 0.0`, `value == f64::NAN`, `f64::INFINITY` returns), §7.6 (decision tree for float comparison).

## Edge cases / gotchas

- **Null is compatible with every dtype.** [`value.rs:96`](../../crates/mc-core/src/value.rs#L96) — `CellDataType::matches` returns true for any `(_, ScalarValue::Null)` pair. So `Null` can sit in an F64-typed measure, an I64-typed measure, or a Category-typed measure interchangeably. It is *not* coerced to `0`, `0.0`, or `false`.
- **`ScalarValue::Null::dtype()` returns `CellDataType::F64` as a placeholder** ([`value.rs:66`](../../crates/mc-core/src/value.rs#L66)). This is fine because the validation primitive (`CellDataType::matches`) treats Null as universal regardless. Don't read this placeholder as "Null is secretly an F64" — the comment at line 56-67 is the source of truth.
- **Division by literal zero is rare in practice; near-zero is the real hazard.** The 1e-300 threshold catches subnormals that round to zero in `1/y`. If you "fix" the threshold to `y == 0.0`, certain rule chains (CPC reads in markets with no spend) will produce `f64::INFINITY` — which gets re-mapped to Null by `finite_or_null` anyway, but takes a longer path with measurable cost. Leave the threshold alone.
- **`null_sub` is asymmetric.** `Null - x = -x` but `x - Null = x`. Spec §7's intent: subtracting "nothing" from x leaves x; subtracting x from "nothing" produces -x because we're computing `0 - x` semantically. Get this backwards and Sub gives wrong signs in chains.
- **Float equality in tests.** Per CLAUDE.md §3.1 / §4.3 / §7.6: tests use `(a - b).abs() < 1e-9`, never `==`. Production code never compares floats for equality at all (the closest is the `1e-300` zero-check inside `null_div` and `Combinator::finish`).
- **NaN propagation in consolidation is silent.** [`consolidation.rs:295-301`](../../crates/mc-core/src/consolidation.rs#L295-L301) — Sum's `observe` skips contributions whose `value * weight` is non-finite. If a hierarchy's weights sum to a non-finite product (very deep hierarchies × extreme weights), individual leaves get silently dropped. For Acme this can't happen (weights are all 1.0); a future hierarchy with skewed weights will need an audit.
- **Rule eval doesn't error on type mismatch.** [`rule.rs:416, 428, 437, 450`](../../crates/mc-core/src/rule.rs#L416) — every `_ =>` arm returns `Null`. The well-typedness check at `CubeBuilder::add_rule` ([`cube.rs:1336-1383`](../../crates/mc-core/src/cube.rs#L1336-L1383)) is the only place type mismatches surface. Don't move that check to runtime "for safety" — null-poisoning on type mismatch is the deliberate fallback so a malformed rule that bypassed the structural check still produces meaningful output rather than panicking.

## Related notes

- [`./weighted-average-consolidation.md`](./weighted-average-consolidation.md) — Null semantics in WeightedAverage's denominator (zero-weight → Null).
- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — what happens when a cached cell holds Null vs F64(0.0).

## History

- 2026-05-01 — Created from brief §7, value.rs/rule.rs source, and CLAUDE.md §2.5, after Phase 1A ship.
