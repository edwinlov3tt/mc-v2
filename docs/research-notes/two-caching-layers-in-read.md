---
name: Two caching layers in read
description: `Cube::read` has two independent caches — derived-leaf and consolidated — both gated on the same triple (`!dirty && stored.revision == cube.revision && !request_trace`); trace requests deliberately bypass both
type: research-note
---

# Two caching layers in `read`

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2

---

## Conclusion (one sentence)

`Cube::read` has two independent value caches living in `HashMapStore` — one for derived-leaf cells (`Provenance::Rule`) and one for consolidated cells (`Provenance::Consolidation`) — both gated on the identical triple `!dirty && stored.revision == cube.revision && !request_trace`, with the dirty bit serving as the cache-invalidator and trace requests deliberately bypassing both because traces require walking the same tree as a recompute.

## Why this matters

The hot path for repeat reads at the same revision must be O(1), not O(rule-tree) and not O(consolidation-leaves). Phase 1A added these two caches specifically so:

- Repeated derived-leaf reads (e.g., reading Clicks at the same coord twice) skip the rule-eval recursion.
- Repeated consolidated reads (e.g., reading Q1/Paid_Search/Florida Spend twice) skip the Cartesian-product walk over hierarchy descendants — for the worst Acme consolidation that's 420 leaf reads.

Both caches are *correctness-preserving*, not opportunistic: they're invalidated by the same dirty-bit + revision pair that any other cache would use. Brief §10.3 includes `t_consolidation_caches_value_within_revision` which asserts a ≥10× speedup on the second consolidated read; the test is what made the consolidated cache mandatory in Phase 1A.

The reason this is worth a self-contained note is the *symmetry* — the two caches look almost identical and have the same gates, but they cache different `Provenance` shapes, are populated and invalidated through different paths, and have different cost profiles. Phase 1B will measure both. Phase 2 may replace either or both with a different mechanism (per-revision arena, snapshot-style version vectors, etc.). Knowing they're two distinct caches with the same shape is the grounding.

## Evidence

### The cache gate (identical in both)

[`crates/mc-core/src/cube.rs:368-374`](../../crates/mc-core/src/cube.rs#L368-L374) — `read_derived_leaf`:

```rust
let cached_fresh = !self.dirty.is_dirty(coord)
    && self
        .store
        .read(coord)
        .map(|s| s.revision == self.revision)
        .unwrap_or(false);
if cached_fresh && !request_trace {
    let stored = self.store.read(coord).expect("checked above");
    return Ok(CellValue { value: stored.value.clone(), /* ... */ });
}
```

[`crates/mc-core/src/cube.rs:544-563`](../../crates/mc-core/src/cube.rs#L544-L563) — `read_consolidated` (additionally requires `Provenance::Consolidation` so a derived-leaf entry can't masquerade as a consolidated one):

```rust
let cached_fresh = !self.dirty.is_dirty(coord)
    && self
        .store
        .read(coord)
        .map(|s| {
            s.revision == self.revision
                && matches!(s.provenance, Provenance::Consolidation { .. })
        })
        .unwrap_or(false);
if cached_fresh && !request_trace { /* return stored */ }
```

The gates differ in one bit only: the consolidated path additionally pattern-matches on provenance.

### Cache population

Both caches are populated as a side-effect of *the same call that satisfies a cache-miss*:

- [`cube.rs:482-491`](../../crates/mc-core/src/cube.rs#L482-L491) — `read_derived_leaf` writes `Provenance::Rule { rule_id, computed_at: self.revision }` after eval.
- [`cube.rs:624-633`](../../crates/mc-core/src/cube.rs#L624-L633) — `read_consolidated` writes `Provenance::Consolidation { hierarchies, child_count }` after the Consolidator returns.

Both call `self.dirty.clear(coord)` immediately after — the freshly-recomputed value is by definition not dirty.

### Cache invalidation

Both caches are invalidated by the same two mechanisms:

1. **`Cube::write` step (11)** ([`cube.rs:873-882`](../../crates/mc-core/src/cube.rs#L873-L882)) calls `self.dirty.mark_closure(...)` (rule-edge dependents) + `self.compute_dirty_ancestors(...)` (hierarchy ancestors). Anything in the dirty set fails the `!self.dirty.is_dirty(coord)` half of the gate on the next read.
2. **Revision bump.** `Cube::write` increments `self.revision` ([`cube.rs:846`](../../crates/mc-core/src/cube.rs#L846)) on every successful write. A previously-cached entry whose `stored.revision` no longer matches `self.revision` fails the gate even if the dirty bit was somehow missed. This is a belt-and-suspenders correctness check: in principle, dirty-bit tracking alone is sufficient; in practice, the revision field catches any propagation bug that leaves a stale entry without dirtying it.

### Trace requests bypass

Both gates explicitly include `&& !request_trace`. Reasoning: a trace records the recursive structure of the computation that produced the value. A cached value has no associated trace (we'd have to store the whole tree, which is what we were trying to avoid). The cleanest fix is to recompute when a trace is requested — semantically the same cost as a cache miss, which is exactly what trace consumers are signing up for.

There is no eviction path. Both caches grow monotonically within a revision; on revision bump, stale entries fail the gate and are over-written by the next miss. `Cube::rollback_to` ([`cube.rs:1027-1037`](../../crates/mc-core/src/cube.rs#L1027-L1037)) explicitly prunes `Provenance::Rule` entries because they're meaningless at the rolled-back revision; consolidated entries are not pruned (they sit in the store, fail the gate due to revision mismatch, and get overwritten on next read).

## Where it shows up in the engine

- **Source — derived-leaf cache hit:** [`crates/mc-core/src/cube.rs:368-387`](../../crates/mc-core/src/cube.rs#L368-L387).
- **Source — derived-leaf cache populate:** [`crates/mc-core/src/cube.rs:482-491`](../../crates/mc-core/src/cube.rs#L482-L491).
- **Source — consolidated cache hit:** [`crates/mc-core/src/cube.rs:544-563`](../../crates/mc-core/src/cube.rs#L544-L563).
- **Source — consolidated cache populate:** [`crates/mc-core/src/cube.rs:624-633`](../../crates/mc-core/src/cube.rs#L624-L633).
- **Source — invalidation (dirty marks):** [`crates/mc-core/src/cube.rs:873-882`](../../crates/mc-core/src/cube.rs#L873-L882) inside `Cube::write`.
- **Source — invalidation (revision bump):** [`crates/mc-core/src/cube.rs:845-847`](../../crates/mc-core/src/cube.rs#L845-L847).
- **Source — rollback prune of derived entries:** [`crates/mc-core/src/cube.rs:1027-1037`](../../crates/mc-core/src/cube.rs#L1027-L1037).
- **Tests — consolidated cache speedup:** `t_consolidation_caches_value_within_revision` in [`crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs) (§10.3).
- **Tests — derived cache invalidation by write:** `t_acme_write_invalidates_dependents` in [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs).
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §10.3, §11 I-Cons (consolidation invariants), §16 (dirty/cache invariants).

## Edge cases / gotchas

- **The store double-duties as a cache.** There's no separate `Cache` data structure — `HashMapStore` holds both Input cells (written by users) and cached Rule/Consolidation cells (written by reads). Distinguishing them is `provenance`, not location. A consequence: `store.iter()` in tests will include cached entries, not just user-written ones. Tests that need "user inputs only" filter by provenance.
- **The consolidated cache requires the Provenance check.** Without it, a stale `Provenance::Rule` entry at the same coord (after, e.g., a hierarchy change between phases) could falsely satisfy the consolidated gate. The check is defensive — there's no normal way to get a Rule entry at a consolidated coord today, but the cache wouldn't notice if there were one.
- **Rollback prunes Rule entries but not Consolidation entries.** Asymmetric on purpose: Rule entries reference a specific rule version that may not be valid at the rolled-back revision, but Consolidation entries are pure functions of hierarchy structure + leaf values and stay valid as long as the snapshot's data is still in the store. They'll fail the revision gate on next read anyway.
- **`request_trace` is per-call, not a Cube setting.** `read` and `read_with_trace` are two methods that route to `read_inner` with the flag. Caching state is identical regardless of the flag — a trace request doesn't poison the cache, it just doesn't consume from it.
- **Cache-hit paths return early with no edge updates.** A cache hit on `read_derived_leaf` doesn't add edges to the dependency graph (the original miss already did). This is correct: the second read is *factually* dependent on the same cells the first one was, and those edges already exist. But it means: if you populate the dep graph by repeated reads, only the first read of each derived coord contributes — a fact useful when reasoning about benchmark setup.
- **Trace cost is unbounded vs. cached read cost is O(1).** Every `read_with_trace` call recomputes from scratch. For a benchmark, "warm trace" is conceptually nonsense — there is no trace cache. PERF.md §7 should treat trace timings as a separate axis.
- **Hot-path hierarchy clone is *separate* from the cache.** Even on a consolidated cache hit we don't pay the hierarchy clone cost (because we return at line 553-562 before the clone at lines 576-582). The clone cost only hits on misses. That changes the picture for Phase 1B benchmarking — the hierarchy-clone overhead noted as a Phase 2 follow-up is a *miss-path* concern, not a cache-hit one. Cache-hit consolidated reads should already be fast.

## Related notes

- [`./lazy-dependency-graph.md`](./lazy-dependency-graph.md) — the dirty bit (cache-invalidation key) is set by walking the dependency graph's reverse edges.
- [`./dirty-propagation-as-per-write-delta.md`](./dirty-propagation-as-per-write-delta.md) — the per-write marginal cost of cache invalidation.
- [`./snapshot-as-deep-clone.md`](./snapshot-as-deep-clone.md) — snapshot captures the store including cached entries; rollback prunes Rule entries but not Consolidation entries.

## History

- 2026-05-01 — Created from cube.rs cache sites and §10.3 cache test, after Phase 1A ship.
