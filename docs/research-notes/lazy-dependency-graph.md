---
name: Lazy dependency graph
description: Rule-edges in the cell-level dependency graph materialize on first read, not at cube construction; hierarchy edges are the exception
type: research-note
---

# Lazy dependency graph

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2

---

## Conclusion (one sentence)

The cell-level dependency graph is empty after `Cube::build()` and is populated incrementally — one forward/reverse edge per actual read — by `Cube::read_derived_leaf` after each rule evaluation; only hierarchy rollup edges are added eagerly at cube-build time, and even those don't appear in the rule-cell forward/reverse maps.

## Why this matters

Two reasons, one for correctness and one for performance:

1. **Correctness contract.** Brief §3.12 / §10.5 specify that the graph is empty immediately after `build_acme_cube()`. The Phase 1A test [`t_dependency_graph_empty_after_build`](../../crates/mc-core/tests/dependency.rs) asserts this. Pre-computing the graph at construction — the obvious "for safety" instinct — silently fails the test and is the trap [`CLAUDE.md §2.1`](../../CLAUDE.md) calls out by name.
2. **Operational consequence for benchmarks and writes.** `Cube::write` propagates dirty state by walking `deps.reverse_edges`. If no read has occurred yet, the reverse-edge index is empty, and the dirty mark only catches hierarchy ancestors via the parallel `compute_dirty_ancestors` walk — *not* derived dependents. This is why Phase 1B's dirty-propagation benchmark must call [`materialize_all_dependencies`](../../crates/mc-fixtures/src/lib.rs) before timing anything; otherwise the timing reflects the cost of doing almost nothing, not the cost of real propagation.

## Evidence

The dependency graph is structurally a `DependencyGraph` with two `AHashMap`s — a forward index (`from → Vec<DependencyEdge>`) and a reverse index (`to → Vec<from>`):

- [`crates/mc-core/src/dependency.rs:39-47`](../../crates/mc-core/src/dependency.rs#L39-L47) — the struct definition.
- [`crates/mc-core/src/dependency.rs:63-84`](../../crates/mc-core/src/dependency.rs#L63-L84) — `add_edge` is the only mutator; both forward and reverse are populated together, idempotent on the (from, to, via) triple.

Edges are added from exactly one place in production code:

- [`crates/mc-core/src/cube.rs:461-475`](../../crates/mc-core/src/cube.rs#L461-L475) — after `eval_expr` returns, `read_derived_leaf` walks the `actual_reads` it observed during eval and emits one `DependencySource::Rule(rule_id)` edge per read. Idempotent: re-evaluating the same coord doesn't accumulate duplicates.

The graph is initialized empty at construction:

- [`crates/mc-core/src/cube.rs:1330`](../../crates/mc-core/src/cube.rs#L1330) — `CubeBuilder::build` calls `DependencyGraph::new()` (which is `Default::default()`), period. No pre-population pass.

The empty-after-build invariant is locked by:

- [`crates/mc-core/tests/dependency.rs`](../../crates/mc-core/tests/dependency.rs) §10.5 tests — 7 tests total, including `t_dependency_graph_empty_after_build`, `t_dependency_graph_grows_after_one_derived_read`, and `t_dependency_graph_validates_full_fixture_when_forced` (which uses `materialize_all_dependencies` to force-populate).

The hierarchy-edge exception:

- [`crates/mc-core/src/dependency.rs:14-17`](../../crates/mc-core/src/dependency.rs#L14-L17) (module doc) — *"Hierarchy edges (consolidated coord → leaf descendants) are added by the cube builder when a hierarchy is bound. They're a fixed structural contribution to the graph rather than a lazy by-product of reads."*
- In Phase 1A, hierarchy invalidation is actually handled by [`compute_dirty_ancestors`](../../crates/mc-core/src/cube.rs#L916-L997) at write time (a parallel Cartesian-product walk), not by hierarchy edges in `DependencyGraph`. The module-doc comment describes the long-term shape; the as-shipped behavior keeps hierarchy invalidation out of `DependencyGraph` entirely. See the Phase 1 completion report §8 item 8 ("Phase 2 may fold hierarchy edges into the dep graph for unified invalidation accounting").

## Where it shows up in the engine

- **Source — graph data structure:** [`crates/mc-core/src/dependency.rs`](../../crates/mc-core/src/dependency.rs).
- **Source — only mutation site:** [`crates/mc-core/src/cube.rs:461-475`](../../crates/mc-core/src/cube.rs#L461-L475) inside `read_derived_leaf`.
- **Source — only read site (dirty propagation):** [`crates/mc-core/src/dirty.rs:42-46`](../../crates/mc-core/src/dirty.rs#L42-L46) `mark_closure`, called from [`crates/mc-core/src/cube.rs:877`](../../crates/mc-core/src/cube.rs#L877).
- **Tests:** [`crates/mc-core/tests/dependency.rs`](../../crates/mc-core/tests/dependency.rs) (§10.5).
- **Fixture helper for force-population:** [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) — `materialize_all_dependencies(cube, refs)` reads every (12 × 5 × 7 × 5) = 2,100 leaf-derived cells once.
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.12 (struct), §10.5 (test contract).
- **Operating manual:** [`CLAUDE.md`](../../CLAUDE.md) §2.1 ("Eager-when-the-spec-says-lazy") spells out the trap.

## Edge cases / gotchas

- **Reads of `Input` cells don't add edges.** Only `read_derived_leaf` populates the graph. Pure input reads bypass eval entirely (see `read_input_leaf` at [`cube.rs:307-358`](../../crates/mc-core/src/cube.rs#L307-L358)) and have no dependencies to record.
- **Consolidated reads recurse through `read_inner`.** A consolidated read of a derived measure walks leaf descendants and reads each leaf via `read_inner` — which routes derived leaves through `read_derived_leaf`, which adds edges. So consolidated reads *do* eventually populate the graph, but transitively, one leaf at a time.
- **`materialize_all_dependencies` is the canonical force-populate path.** It's a debug/benchmark helper, not a production API. Don't be tempted to call it during cube setup to "warm" the graph for production reads — it would defeat the laziness contract.
- **Dirty propagation has two orthogonal paths.** `mark_closure` (graph-edge based, depends on prior reads) and `compute_dirty_ancestors` (hierarchy-Cartesian based, runs every write). After a write to a freshly-built cube where nothing has been read, only the hierarchy path actually marks anything beyond the written cell.
- **Cycle detection runs on the populated graph.** Brief §15 I-Dep-1 says cycle detection runs when a new rule is registered, but in Phase 1 rule registration happens at `CubeBuilder::add_rule` *before* any cells exist, so the cycle scan there checks the rule-target → dep-measure graph (in [`rule.rs::detect_cycle_in_rule_graph`](../../crates/mc-core/src/rule.rs#L248)), not the cell-level `DependencyGraph::detect_cycle`. The cell-level cycle scan exists ([`dependency.rs:141-143`](../../crates/mc-core/src/dependency.rs#L141-L143)) but is only meaningful once the graph has cells in it.

## Related notes

- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — the dirty bit that gates cached-value freshness is set via the dependency graph's reverse edges.
- [`./dirty-propagation-as-per-write-delta.md`](./dirty-propagation-as-per-write-delta.md) — quantifying what propagation actually marks.

## History

- 2026-05-01 — Created from Phase 1A code + brief §3.12 + CLAUDE.md §2.1, after Phase 1A ship.
