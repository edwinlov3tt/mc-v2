---
name: Weighted-average consolidation
description: Five Acme ratio measures (CPC, CVR, Close_Rate, AOV, COGS_Rate) consolidate via weighted average — each weighted by a specific upstream measure forming a chain — while the other six use Sum; defaulting to Sum on a ratio is wrong by construction
type: research-note
---

# Weighted-average consolidation

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2

---

## Conclusion (one sentence)

Five of Acme's eleven measures (CPC, CVR, Close_Rate, AOV, COGS_Rate) are *ratios* and consolidate via `AggregationRule::WeightedAverage { weight_measure }` with a measure-specific weight that forms a deliberate chain — CPC weighted by Spend, CVR by Clicks, Close_Rate by Leads, AOV by Customers, COGS_Rate by Revenue — while the remaining six (Spend + 5 derived counts/dollar amounts) use `Sum`; *defaulting* to Sum is the canonical "obvious-but-wrong" mistake on this engine.

## Why this matters

The whole point of CPC at the Q1/Paid_Search/Florida level is "weighted-average cost per click across the markets that actually had clicks." Simple-summing CPCs across markets gives `1.50 + 1.50 + 1.50 = 4.50` — a number with no real-world meaning. Simple-averaging gives a number that ignores how lopsided spend was across markets. Only the weighted average — `Σ(CPC × Spend) / Σ(Spend)` — recovers the planning-finance answer.

The brief locks this down with a test that's specifically structured to fail under both alternatives (`t_acme_read_consolidated_cpc_uses_weighted_average` asserts the result is *not* equal to either simple sum or simple average). CLAUDE.md §2.10 names "default to Sum everywhere" as a recurring trap.

The choice of *which* measure weights *which* ratio is also load-bearing — it isn't arbitrary. The chain follows the funnel: each ratio is weighted by the measure that defines its denominator at the leaf level.

## Evidence

### The five weight pairings

[`crates/mc-fixtures/src/lib.rs:872-921`](../../crates/mc-fixtures/src/lib.rs#L872-L921) — Acme's measure dim is built with these aggregation rules verbatim:

| Measure      | AggregationRule                         | Why this weight                                                 |
|--------------|-----------------------------------------|------------------------------------------------------------------|
| CPC          | `WeightedAverage { weight_measure: spend }`     | CPC = Spend / Clicks; rolling up "cost per click" across markets where spend is uneven, you weight by spend |
| CVR          | `WeightedAverage { weight_measure: clicks }`    | CVR = Leads / Clicks; weight by clicks, the denominator of the rate |
| Close_Rate   | `WeightedAverage { weight_measure: leads }`     | Close_Rate = Customers / Leads; weight by leads |
| AOV          | `WeightedAverage { weight_measure: customers }` | AOV = Revenue / Customers; weight by customers |
| COGS_Rate    | `WeightedAverage { weight_measure: revenue }`   | COGS_Rate = COGS / Revenue (conceptually); weight by revenue |

The other six measures — `Spend`, `Clicks`, `Leads`, `Customers`, `Revenue`, `Gross_Profit` — all use `AggregationRule::Sum` ([`mc-fixtures/src/lib.rs:870, 929, 937, 945, 953, 961`](../../crates/mc-fixtures/src/lib.rs#L870)). These are dollar amounts and counts; simple-sum is the right rollup.

### How the engine picks the combinator

[`crates/mc-core/src/consolidation.rs:276-283`](../../crates/mc-core/src/consolidation.rs#L276-L283):

```rust
fn pick_combinator(meta: &MeasureMeta) -> Combinator {
    match &meta.aggregation {
        AggregationRule::Sum => Combinator::Sum,
        AggregationRule::WeightedAverage { .. } => Combinator::WeightedAverage,
        AggregationRule::Min => Combinator::Min,
        AggregationRule::Max => Combinator::Max,
    }
}
```

There is no default. Every measure's `MeasureMeta.aggregation` is set explicitly at construction; missing it would surface as a different error (the measure dim wouldn't build). A measure cannot accidentally fall through to Sum.

### The weighted-average reduction

[`crates/mc-core/src/consolidation.rs:335-358`](../../crates/mc-core/src/consolidation.rs#L335-L358) — `Combinator::observe_weighted`:

```rust
fn observe_weighted(
    self,
    state: &mut CombinatorState,
    value: ScalarValue,
    weight_value: ScalarValue,
    weight_product: f64,
) {
    // ...
    let v = match value { ScalarValue::F64(x) if x.is_finite() => x, _ => return };
    let w = match weight_value { ScalarValue::F64(x) if x.is_finite() => x, _ => return };
    let effective_weight = w * weight_product;
    if !effective_weight.is_finite() { return; }
    state.accum += v * effective_weight;
    state.denom += effective_weight;
    state.has_observation = true;
}
```

Two integration points worth flagging:

1. **Per-leaf weight read.** During a consolidated read, the `Consolidator` calls `read_at` *twice per leaf* under `WeightedAverage`: once for the value (e.g., CPC) and once for the sibling weight measure (e.g., Spend) at the same leaf coord ([`consolidation.rs:165-194`](../../crates/mc-core/src/consolidation.rs#L165-L194)). Both reads flow through `Cube::read_inner`, so dependency tracking applies to weight reads too.
2. **Hierarchy weight chains.** `effective_weight = w * weight_product` multiplies the measure's own weight value by the cumulative *hierarchy* weight (the product of per-dim hierarchy edge weights from the consolidated coord down to the leaf). For Acme, all hierarchy edge weights are 1.0, so this is the measure-weight alone. For a future hierarchy with `0.3/0.7` splits, the chain matters.

### Finish-time semantics

[`crates/mc-core/src/consolidation.rs:373-384`](../../crates/mc-core/src/consolidation.rs#L373-L384):

```rust
Combinator::WeightedAverage => {
    if !state.has_observation || state.denom.abs() < 1e-300 {
        ScalarValue::Null
    } else {
        let v = state.accum / state.denom;
        if v.is_finite() { ScalarValue::F64(v) } else { ScalarValue::Null }
    }
}
```

Zero total weight → Null (not zero, not NaN). See [`./null-vs-zero-vs-nan.md`](./null-vs-zero-vs-nan.md).

### Tests that lock the contract

- [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) — `t_acme_read_consolidated_cpc_uses_weighted_average` asserts CPC at Q1/Paid_Search/Florida ≈ 1.5202381, and explicitly checks the result is *not* the simple sum and *not* the simple average.
- [`crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs) — 12 §10.3 tests covering all four `AggregationRule` variants.
- [`crates/mc-core/src/consolidation.rs:631-749`](../../crates/mc-core/src/consolidation.rs#L631-L749) — module-internal weighted-average tests (`weighted_average_basic`, `weighted_average_zero_total_weight_returns_null`).

## Where it shows up in the engine

- **Source — combinator dispatch:** [`crates/mc-core/src/consolidation.rs::pick_combinator`](../../crates/mc-core/src/consolidation.rs#L276).
- **Source — weighted-average reduction:** [`crates/mc-core/src/consolidation.rs::observe_weighted`](../../crates/mc-core/src/consolidation.rs#L335) and `finish` ([line 373](../../crates/mc-core/src/consolidation.rs#L373)).
- **Source — measure-meta carrying the rule:** [`crates/mc-core/src/element.rs`](../../crates/mc-core/src/element.rs) — `MeasureMeta { aggregation: AggregationRule, ... }`.
- **Acme wiring:** [`crates/mc-fixtures/src/lib.rs:872-921`](../../crates/mc-fixtures/src/lib.rs#L872-L921) — five `WeightedAverage` definitions; remaining six use Sum.
- **Tests:** [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) (§10.1), [`crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs) (§10.3).
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.17, §4 (Acme measure list), §10.3.
- **Operating manual:** [`CLAUDE.md`](../../CLAUDE.md) §2.10 ("Wrong CPC consolidation (simple sum vs weighted average)").

## Edge cases / gotchas

- **The weight chain is a directed dependency.** CPC's weight is Spend; rolling up CPC therefore reads Spend at every leaf. If Spend at a leaf is `Null`, that leaf is *excluded* from the CPC weighted average (line 347 returns early). The dependency graph captures this: edges from the consolidated CPC coord point to every (leaf-coord, Spend) read it performed. Phase 2 invalidation needs to handle the chain — a write to Spend invalidates not only Spend's consolidations but also CPC's (via the weight-read).
- **Null poisoning is per-leaf, not per-result.** A single Null value at one leaf doesn't poison the whole consolidated CPC; it just drops that leaf from the average. All-Null leaves → Null result.
- **`weight_value` Null is treated identically to value Null.** Both cause the leaf to be excluded entirely. There's no "default the weight to 1.0 on missing" fallback. Spec §7 / §11 I-Cons-3.
- **The weight-measure must be writable.** All five Acme weight measures (Spend, Clicks, Leads, Customers, Revenue) are F64. Three are inputs (Spend) and four are derived (Clicks, Leads, Customers, Revenue). Reading a derived weight at a leaf triggers rule eval on the weight measure; that's load-bearing for cache discussion (see [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md)).
- **Hierarchy weights multiply with measure weights, not replace.** `effective_weight = w * weight_product`. For Acme this is moot (all hierarchy weights = 1.0); for a future weighted hierarchy it's the integration point. Don't accidentally drop one.
- **Adding a new ratio measure requires picking a weight.** The choice of which existing measure to use as the weight is design work, not an obvious mechanical step. The funnel-position heuristic (use the rate's denominator measure) works for the five existing pairings but isn't a general rule — a custom ratio with no obvious weight measure is a SPEC QUESTION per CLAUDE.md §11.
- **There is no `MeasureRole::Both` in Phase 1.** Brief excludes it, even though engine-semantics defines it. So a measure that's "Input at leaves but Derived at consolidated levels" can't exist yet; CPC etc. are `Input` and consolidate via the weighted-average reduction at runtime, not via a derived rule. Don't try to model ratios as derived measures with `Scope::AllConsolidated` rules — that's Phase 2 territory.

## Related notes

- [`./null-vs-zero-vs-nan.md`](./null-vs-zero-vs-nan.md) — zero total weight → Null.
- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — how the consolidated cache interacts with weighted-average reads.
- [`./lazy-dependency-graph.md`](./lazy-dependency-graph.md) — weight reads also generate edges.

## History

- 2026-05-01 — Created from Acme fixture wiring, consolidation.rs, brief §3.17, and CLAUDE.md §2.10, after Phase 1A ship.
