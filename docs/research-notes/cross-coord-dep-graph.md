---
status: open question
filed: 2026-05-05 (Phase 6A.1)
filed-from: Sonnet code review (`docs/reviews/phase-3-5-6-shipped-review.md` MAJ-3)
disposition: deferred — needs ADR before implementation
---

# Cross-coordinate dependency graph: current behavior + open architectural questions

## Context

The kernel's `Cube` carries a [`DependencyGraph`](../../crates/mc-core/src/dependency.rs) — forward and reverse edges between rule cells and their declared dependencies. The graph is built lazily during `read()` (see [`docs/research-notes/lazy-dependency-graph.md`](lazy-dependency-graph.md)) and used by `write()` to compute the dirty set.

For **same-coordinate** dependencies the graph is correct. A rule like `Clicks = Spend / CPC` reads `Spend` and `CPC` at the same `(Scenario, Version, Time, Channel, Market)` coord; the dependency edges connect those measures at that coord; writing `Spend[T1]` correctly dirties `Clicks[T1]` and any consolidated parents.

For **cross-coordinate** dependencies — `prev(M)`, `lag(M, k)`, `actual_ref(M)`, `cumulative(M)`, `rolling_avg(M, w)` — the situation is different. These calls walk to a *different* coordinate (`M[T-1]`, `M[T-k]`, `M[Actual]`, etc.) inside the rule body. The eval path routes through [`Cube::resolve_cross_coord_read`](../../crates/mc-core/src/cube.rs) at `cube.rs:751` and returns a value, but the resolved coordinate is **not** added to `actual_reads`. As of e696379, only `EvalLookup::SelfRef` reads are appended at `cube.rs:457`; cross-coord results are returned without registering an edge.

## Why this is not a wrong-answer bug today

The Phase 2D dirty-propagation invariant — "any write bumps the cube's revision, and any cached derived value tagged with a stale revision is recomputed" — covers correctness end to end. The revision bump on write at [`cube.rs:393–398`](../../crates/mc-core/src/cube.rs) invalidates every cached derived cell regardless of which cell was written. So even though `prev(M)[T+1]` has no recorded dependency on `M[T]` in the graph, a write to `M[T]` bumps the revision, the next read of `prev(M)[T+1]` finds its cached value stale, and the rule re-evaluates against the new `M[T]`.

Sonnet's review classified this as a **performance concern, not a correctness concern**, and the Phase 6A.1 handoff confirmed: "correctness preserved via revision-bump belt-and-suspenders." Phase 6A.1 does not change this behavior.

## Why it's a real performance concern

Phase 2D's dirty propagation work invested specifically in *granular* dirty sets — only the cells that depend on a write are marked dirty. The `t_dirty_set_after_write` family of tests (brief §10.1) asserts exact dirty-set sizes derived from the graph topology.

For cross-coord rules, the granular-dirty-set property is not currently delivered. Writing `M[Jan]` should dirty:

- `prev(M)[Feb]` (because the rule body at `Feb` reads `M[Jan]`)
- `lag(M, 3)[Apr]` (because the rule body at `Apr` reads `M[Jan]`)
- `cumulative(M)[Jan]`, `cumulative(M)[Feb]`, ... `cumulative(M)[Dec]` (all months from `Jan` onward)
- `rolling_avg(M, 3)[Jan]`, `[Feb]`, `[Mar]` (windows that include `Jan`)
- `actual_ref(M)` if the write is to a cell named in the Scenario dim's `actuals_element`

But the current implementation invalidates **every cached derived cell in the cube** instead. For Acme (~2,500 cells) the overhead is negligible. For a quarterly model with 80 channels × 20 markets × 8 measures × 24 time periods (~300K cells, of which maybe 60% are derived), every write triggers ~180K cache invalidations even if only one downstream cell actually depends on the write. At ~67ns per cache check (`cargo bench` numbers from PERF.md §6), that's ~12ms of wasted overhead per write, scaling with cube size rather than with the actual fan-out of the dependency.

## Why the fix is non-trivial — open architectural questions

A naive fix ("just append cross-coord reads to `actual_reads`") under-specifies what the graph edges should encode. The kernel currently models dependencies as `(target_coord, dep_coord)` pairs — both fully resolved. For cross-coord deps that's correct for `prev(M)` (the resolved coord is `M[T-1]` and the edge is `M[T-1] → prev(M)[T]`), but it doesn't generalize cleanly:

1. **Parameterized offsets.** `lag(M, k)` should record an edge from `M[T-k]` to `lag(M, k)[T]`. The graph today has no notion of "edge with offset k" — every edge is between concrete coords. If a rule body is `lag(M, 3)`, the resolved coord at eval time is `M[T-3]`, so a single eval gives one concrete edge — which is correct *for that particular target T*. The question is whether the graph stores per-target-coord edges (granular but expensive: one edge per `(target T, source T-k)` pair) or whether it abstracts the relationship parametrically.

2. **Window-shaped deps.** `rolling_avg(M, w)[T]` reads `M[T-w+1] .. M[T]`. That's a fan-in of `w` source cells per target. Again storing concrete edges works (`w` edges per target) but blows up O(N·w) for an N-period cube. Storing parametric edges ("rolling window of width w") shrinks to O(N) edges but requires the dirty-walk to reason about windows.

3. **Cross-scenario deps (`actual_ref`).** The rule body `actual_ref(M)` reads `M` from the Scenario element named in `Scenario.actuals_element`. That's a write-to-Actuals → dirty-non-Actuals dependency. Not strictly cross-time but cross-coordinate in the Scenario axis.

4. **Cumulative.** `cumulative(M)[T]` reads `M[T-1], M[T-2], ..., M[T_first]`. Fan-in is O(T) — the largest of the cross-coord operators. A write to `M[T_first]` would dirty `cumulative[T_first]` through `cumulative[T_last]` — every cumulative cell.

5. **Time-anchor coupling.** `is_past(T)`, `is_current(T)`, `is_future(T)` are conceptually time-anchor-relative. The graph today doesn't track time-anchor as a dependency; an `mc model query --time-anchor` override changes results without going through `write()`. A future graph design needs to decide whether time-anchor counts as a "writeable" thing that should participate in dirty propagation.

## Possible directions (not endorsed; ADR territory)

A future fix-it phase needs to choose along these axes:

- **Edge granularity:** concrete-only (simple, blows up storage on time-heavy cubes) vs. parametric (compact, complicates the dirty walk).
- **Eviction on write:** keep the current revision-bump-bulk-invalidate as a fallback for any deps the graph can't precisely model, vs. commit fully to graph-driven invalidation and accept that some operators (e.g., custom-extension cross-coord operators in a future phase) might need to opt into bulk invalidation explicitly.
- **Lazy vs. eager edge population:** the same-coord graph today is lazily built; cross-coord edges could follow the same pattern (registered on first eval) or be precomputed at compile time when the rule body's `Cross` lookups have statically resolvable offsets.
- **Storage cost:** if cross-coord edges multiply graph size by 10×–50× on time-heavy cubes, the dependency-graph data structure may need re-evaluation. The current `DependencyGraph` is a `HashMap` of forward/reverse edges; that scales fine to ~10K edges, less well to ~1M.

## What Phase 6A.1 does NOT do

Per the handoff (§"Out of Scope"): MAJ-3 is **deferred**. This note exists to preserve the analysis so the next phase has somewhere to start.

Phase 6A.1's only contribution to this area is this research note. The kernel's cross-coord behavior at e696379 is unchanged: cross-coord deps are not graph-tracked, and bulk revision-bump invalidation covers correctness.

## Triggering a fix-it phase

When a future phase tackles this:

1. Open an ADR with the questions above. The five operators (`prev`, `lag`, `actual_ref`, `cumulative`, `rolling_avg`) span the design space; resolving the ADR should pick one approach for all of them, not five.
2. Add benchmarks measuring write-then-read latency on a synthetic large cube (~500K cells, 50% derived, heavy use of cross-coord operators). The Phase 1B benches at PERF.md §6 are warm-cache-heavy and don't expose this.
3. Confirm that the brief §10.1 dirty-set tests still pass under the new edge representation.
4. Check that the `t_acme_*` family is unaffected (Acme has no cross-coord operators, so its dirty sets shouldn't change).

## References

- [`docs/reviews/phase-3-5-6-shipped-review.md`](../reviews/phase-3-5-6-shipped-review.md) §MAJ-3 — the original finding.
- [`docs/decisions/0011-formula-language-extension.md`](../decisions/0011-formula-language-extension.md) — `prev`/`lag`/cumulative semantics.
- [`docs/decisions/0012-formula-extensions-dirty-propagation.md`](../decisions/0012-formula-extensions-dirty-propagation.md) — the dirty-propagation rule the current behavior partially undermines.
- [`docs/research-notes/cross-coordinate-formulas.md`](cross-coordinate-formulas.md) — earlier note on the cross-coord operator family.
- [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) lines 431–508 — the read/eval path where cross-coord reads bypass `actual_reads`.
