# ADR-0027: Cross-Coordinate Dependency Graph Fix

**Status:** Proposed
**Date:** 2026-05-09
**Deciders:** project owner
**Phase:** Dedicated fix-it phase (per ADR-0018 Amendment §11: "scope within next 2 phase cycles after 3H.2")
**Crate(s) touched:** `mc-core` only (kernel-internal change; no API surface changes)
**Prerequisite reading:** `docs/research-notes/cross-coord-dep-graph.md`

---

## Context

Cross-coordinate functions (`prev`, `lag`, `actual_ref`, `scenario_ref`, `cumulative`, `rolling_avg`, adstock backward scan) read values at coordinates different from the rule's target coordinate. When `prev(M)` is evaluated at `Time=Feb`, it reads `M[Time=Jan]`. This produces a dependency: changes to `M[Jan]` should invalidate `prev(M)[Feb]`.

**Today, these dependencies are not registered in the graph.** The evaluation path routes through `Cube::resolve_cross_coord_read()` and returns a value, but the resolved source coordinate is NOT appended to `actual_reads`. Only same-coordinate (`EvalLookup::SelfRef`) reads register edges.

**Correctness is preserved** via a belt-and-suspenders mechanism: every write bumps the cube's global revision number. On the next read of ANY derived cell, if its cached revision is stale, the rule re-evaluates regardless of graph edges. This guarantees correct results at the cost of invalidating every derived cell in the cube on every write.

**This is a performance problem, not a correctness problem.** For a 300K-cell production cube (80 channels × 20 markets × 24 periods × 8 measures), a single-cell write invalidates ~180K cached derived values — even if only 15 cells actually depend on the written cell. At ~67ns per freshness check (per `PERF.md` benchmarks), that's ~12ms of wasted work per write, scaling with cube size rather than actual fan-out.

**Why this matters for Phase 8:** The service daemon's hot cube cache makes this problem acute. Interactive use cases (grid edits, what-if sliders) need "write to `Spend[Q1]` only recomputes the 15 derived cells that depend on it" — not "recomputes all 180K derived cells." Without this fix, the daemon's cache hit rate degrades proportional to cube size regardless of edit locality.

This ADR resolves the five architectural questions documented in `docs/research-notes/cross-coord-dep-graph.md` and commits to one approach.

---

## Decisions

### Decision 1: Lazy concrete edges (same pattern as existing graph)

Cross-coordinate dependency edges are registered as **concrete `(source_coord, target_coord)` pairs**, populated **lazily on first evaluation** — the same pattern the existing same-coordinate graph uses.

When `prev(M)` is evaluated at target coordinate `(Scenario=Plan, Version=Working, Time=Feb, Channel=Paid_Search, Market=Houston, Measure=Clicks)`:
- The eval resolves the source coordinate: `(... Time=Jan, ... Measure=Spend)` (or whatever the cross-coord read resolved to)
- The resolved source coordinate is appended to `actual_reads` (alongside the same-coord reads)
- The existing graph machinery registers the forward edge `source → target` and reverse edge `target → source`
- Future writes to the source coordinate find the target via the reverse edge and mark it dirty

**Why lazy concrete, not parametric:** Parametric edges (encoding "lag offset k" abstractly) reduce storage but require the dirty walk to reason about offsets — a significant complexity increase for the invalidation path, which is the hot path. Lazy concrete edges are simple, reuse the existing graph infrastructure, and keep the graph sparse (only cells that have been read get edges registered).

**Why this works at scale:** The graph is proportional to **evaluated cells**, not **total cells**. Users don't read all 300K cells — they read specific slices (one scenario, one version, specific time ranges, specific channels). The graph grows with usage, not with cube dimensionality. For Phase 8's daemon serving grid views: a typical grid shows ~500-2000 cells; those cells register their cross-coord edges; the graph stays in the 10K-50K edge range even for large cubes.

### Decision 2: Revision-bump retained as safety net

The global revision bump on write is **kept** as a correctness fallback. It no longer does the heavy lifting (the graph does), but it guarantees that any cell whose edges were never registered (because it was never read) still gets fresh results on its first future read.

The interaction:
1. Write to cell X → bump revision → walk graph edges → mark precise dirty set
2. Read cell Y (previously evaluated, has edges) → check dirty flag → if dirty, re-eval → correct
3. Read cell Z (never evaluated, no edges) → check cached revision → stale → eval from scratch → correct

This preserves the "no wrong answers" invariant while giving the graph's precise dirty set the performance win for the common case (re-reading previously-read cells).

### Decision 3: Register cross-coord reads in `actual_reads` during eval

**The core code change:** In `cube.rs`, wherever `resolve_cross_coord_read()` returns a value for a `Cross`-variant lookup, the resolved source coordinate is pushed to the `actual_reads` vector in `EvalCtx`.

Currently (pseudocode):
```rust
EvalLookup::Cross { resolved_coord, .. } => {
    let value = self.read_inner(resolved_coord)?;
    // No edge registered — value returned directly
    Ok(value)
}
```

After fix:
```rust
EvalLookup::Cross { resolved_coord, .. } => {
    let value = self.read_inner(resolved_coord)?;
    ctx.actual_reads.push(resolved_coord.clone());  // ← THE FIX
    Ok(value)
}
```

The downstream graph machinery (`register_dependencies`, `closure_of_dependents`) already handles arbitrary `(source, target)` pairs. No structural changes to `DependencyGraph` needed — just feed it the cross-coord edges it's been missing.

### Decision 4: Null/missing/fallback reads MUST register dependency edges

Cross-coordinate reads must register the attempted source coordinate **even when the read returns Null, Missing, or triggers a fallback value.** The graph records "what coordinates were consulted," not "what coordinates returned populated values."

**Why this matters:**
```
actual_ref(Leads, fallback=0)
```
If Actuals is currently missing, the derived value falls back to 0. But if Actuals are loaded later (via Tessera import), the dependent Plan cell must invalidate. Without this rule, missing Actuals would not create edges, and later imports would silently leave stale fallback values in the cache.

Same concern applies to:
- `scenario_ref(M, "Actuals")` when Actuals scenario has no data yet
- Adstock scan stopping at missing/null data
- Rolling windows with null periods
- Cumulative windows with sparse inputs

**The rule:** Register the edge for every `resolve_cross_coord_read()` call regardless of the returned value. The dependency is "this cell consulted that coordinate" — whether the coordinate was populated is irrelevant to the invalidation question.

### Decision 5: Time-relative rules are query-context-dependent

`is_past()`, `is_current()`, `is_future()` are evaluated relative to `time_anchor`, which is a query-time parameter (set via `--time-anchor`), not a writable cell.

**Decision:** Rules that use time-relative functions (`is_past`, `is_current`, `is_future`) are marked as **query-context-dependent**. Their cached results are NOT reused across different `time_anchor` values. Implementation options (choose during implementation):

- **(Preferred) Include `time_anchor` in the cache key** for cells evaluated by time-relative rules. Different time_anchor = different cache entry. Simple, correct, no false sharing.
- (Alternative) Mark time-relative cells as always-recompute. Simpler but wastes cache for the common case where time_anchor doesn't change between queries.

**Why this is explicit:** The previous framing ("global revision bump handles time_anchor") was underspecified. There is no code path that bumps revision when time_anchor changes — it's a query parameter, not a write. The freshness guarantee must come from the cache key, not from revision mechanics.

### Decision 6: Freshness semantics — graph-tracked cells survive revision bumps

**This is the critical companion fix.** Without this decision, registering cross-coord edges is necessary but not sufficient — the global revision stale check would still force recomputation of every cached derived cell on every write.

**The new freshness rule for cached derived cells:**

```
On read of a cached derived cell:
  IF cell has registered dependency edges (has been evaluated at least once):
    IF cell is explicitly marked dirty (by graph-driven invalidation):
      → recompute
    ELSE:
      → reuse cached value (even if global revision has changed)
  ELSE (cell has never been evaluated / no edges registered):
    IF cached_revision != cube_revision:
      → evaluate from scratch (first eval; this populates edges)
    ELSE:
      → reuse cached value
```

**What this means:** The global revision bump is **demoted from primary invalidation mechanism to safety net.** For cells with registered dependencies, the graph's dirty flag is authoritative. The revision check only matters for cells that have never been read (and thus have no edges).

**Why this is load-bearing:** If the code keeps `if cached_revision != cube_revision → recompute` as an unconditional check, then every write still stales out all cached cells regardless of graph edges. The entire performance improvement depends on this semantic change.

**Correctness argument:** A cell with registered edges is reusable if and only if none of its dependencies have changed since it was last computed. The graph's dirty propagation guarantees this — if any dependency changes, the cell is marked dirty via `closure_of_dependents()`. A cell that is NOT dirty and HAS edges is provably fresh. The revision safety net covers the remaining case (no edges yet).

**Reference:** This is the same pattern used by Salsa (Rust's incremental computation framework) and rustc's query system — tracked dependencies mean reuse across revisions; untracked means recompute. Dynamic dependency systems like Pluto record actual requirements during execution and reuse results when requirements are unchanged.

### Decision 7: Edge deduplication and budget

**Deduplication:** Re-reading the same source coordinate multiple times during a single evaluation (or across repeated evaluations) must NOT produce duplicate edges in the graph. The graph stores edges as a set, not a list. Implementation: check before insert, or use a `HashSet` for the edge collection.

**Edge-count proportionality:** Window operators must produce edge counts proportional to their actual window size:
- `cumulative(M)` over 24 periods → ~24 edges per target cell (not hundreds from repeated eval artifacts)
- `rolling_avg(M, 3)` → 3 edges per target cell
- Adstock with `max_lookback: 8` → up to 8 edges per target cell

**Why this matters for the daemon:** In long-running daemon mode, repeated queries should not grow the graph unboundedly. Edge dedup prevents memory leaks. Edge-count tests prove the graph stays proportional to actual dependency structure.

### Decision 8: Benchmark-driven acceptance (no ceiling regression)

The fix must:
1. **Not regress** existing benchmark ceilings in `PERF.md` (same-coord write-read path must not get slower)
2. **Demonstrate improvement** on a new synthetic benchmark with heavy cross-coord ops (write-then-read latency should be proportional to actual fan-out, not cube size)
3. **Pass all existing tests** including `t_dirty_set_after_write` family (Acme has no cross-coord ops; its dirty sets should be unchanged)
4. **Include a visible-grid benchmark** that validates the daemon use case: populate a large slice → write an unrelated input → re-read the slice → assert most cells reuse cache and latency does not scale with total derived cells

### Decision 9: Scope — all cross-coord operators, one approach

### Decision 9: Scope — all cross-coord operators, one approach

All cross-coordinate operators are handled uniformly by Decision 3. No operator-specific special cases:

| Operator | Source coord | Edges per eval |
|---|---|---|
| `prev(M)` | `M[T-1]` | 1 |
| `lag(M, k)` | `M[T-k]` | 1 |
| `actual_ref(M)` | `M[Scenario=Actuals]` | 1 |
| `scenario_ref(M, "X")` | `M[Scenario=X]` | 1 |
| `cumulative(M)` | `M[T_first] .. M[T]` | O(T) — all prior periods |
| `rolling_avg(M, w)` | `M[T-w+1] .. M[T]` | O(w) — window width |
| Adstock backward scan | `M[T-lookback] .. M[T-1]` | O(lookback) |

For `cumulative` and `rolling_avg`, the eval loop already iterates over the window; each iteration does a `read_inner()` that is now tracked. No new iteration logic needed — just stop suppressing the reads.

---

## Implementation plan

### Step 1: Register cross-coord reads in `actual_reads`

In `crates/mc-core/src/cube.rs`, find every `EvalLookup::Cross` handler (or equivalent `resolve_cross_coord_read` call) and ensure the resolved coordinate is appended to `actual_reads`. Register even when the read returns Null/Missing (per Decision 4). This is likely 1-3 code sites.

### Step 2: Update cache freshness semantics (the companion fix)

This is the critical second part. In the read path (`cube.rs` around line 396), change the cache-hit logic:

**Before:**
```rust
// Unconditional revision check — every cached cell stales on every write
if cached_revision != self.revision {
    // recompute
}
```

**After:**
```rust
if cell.is_dirty() {
    // Graph-driven invalidation: this cell's dependency changed → recompute
} else if cell.has_registered_edges() {
    // Cell has been evaluated and its dependencies are tracked
    // Not dirty → safe to reuse (graph guarantees freshness)
    return cached_value;
} else if cached_revision != self.revision {
    // No edges yet (never evaluated, or pre-fix cached cell)
    // Fall back to revision check → recompute
}
```

The exact implementation shape depends on what `CellValue` / cache entry looks like in the kernel. The implementer should examine the current `read()` path and modify the freshness gate accordingly. The key semantic: **cells with registered edges trust the graph's dirty flag, not the global revision.**

### Step 3: Edge deduplication

Ensure the `DependencyGraph` deduplicates edges. If it uses `Vec<DependencyEdge>`, switch reverse-edge storage to a `HashSet` or add a check-before-insert. Verify that repeated evaluations of the same cell don't grow the edge count.

### Step 4: Verify existing tests pass

`cargo test --workspace` must pass unchanged. Critical tests:
- `t_dirty_set_after_write_*` — Acme dirty sets should be identical (Acme has no cross-coord ops)
- `t_acme_*` — all Acme golden values unchanged
- `doctrine_*` — all invariant doctrines pass

### Step 5: Add cross-coord dirty-set tests

New tests that verify precise invalidation:

```rust
#[test]
fn t_dirty_set_cross_coord_prev() {
    // Build cube with prev(M) rule
    // Read derived cell to populate graph edges
    // Write to M[T-1]
    // Assert: only the prev(M)[T] cell is dirtied, not ALL derived cells
}

#[test]
fn t_dirty_set_cross_coord_cumulative() {
    // Build cube with cumulative(M) rule
    // Read derived cells for all periods to populate edges
    // Write to M[T_first]
    // Assert: cumulative[T_first..T_last] are dirtied, but unrelated derived cells are NOT
}

#[test]
fn t_dirty_set_cross_coord_actual_ref() {
    // Build cube with actual_ref(M) rule
    // Read derived cell in Plan scenario to populate edges
    // Write to M[Actual]
    // Assert: only the actual_ref cell in Plan is dirtied
}

#[test]
fn t_dirty_set_cross_coord_prev_derived_chain() {
    // Build cube where prev(Revenue) depends on Revenue which depends on Customers * AOV
    // Read prev(Revenue)[Feb] to populate full chain
    // Write to Customers[Jan] (underlying input of the previous period)
    // Assert: Revenue[Jan] is dirtied, AND prev(Revenue)[Feb] is dirtied
    // (the chain propagates through derived sources)
}

#[test]
fn t_dirty_set_cross_coord_null_read_registers_edge() {
    // Build cube where actual_ref(Leads) reads from Actuals scenario
    // Actuals has no data (read returns Null/fallback)
    // Read derived cell in Plan → edge registered despite null source
    // Write to Leads[Actuals] (data appears)
    // Assert: the Plan cell is dirtied (edge existed even though source was null)
}

#[test]
fn t_edge_dedup_no_growth_on_repeated_reads() {
    // Build cube with prev(M) rule
    // Read derived cell 10 times
    // Assert: edge count for that cell is exactly 1 (not 10)
}
```

### Step 6: Create synthetic benchmarks

New benchmarks in `crates/mc-core/benches/`:

```rust
// cross_coord_write_precision.rs
//
// Bench 1: write-then-read-dependent
// Cube: 20 channels × 10 markets × 24 time periods × 6 measures
// (2 input, 4 derived-with-prev/cumulative)
// Populate full graph by reading all derived cells
// Write one input cell → read one dependent derived cell
// Measure: latency proportional to fan-out (~5-20 cells), not cube size (~19K cells)
//
// Bench 2: visible-grid-unrelated-write (daemon use case)
// Read a "visible grid" of ~500 cells to populate graph
// Write an input cell that affects NONE of those 500 cells
// Re-read the same 500 cells
// Measure: all 500 should be cache hits (zero recomputation)
// This directly validates Phase 8 daemon grid-editing performance
```

### Step 7: Verify PERF.md ceilings hold

Run `cargo bench --workspace`. All existing ceilings must pass. The new cross-coord benchmarks establish new ceilings.

---

## Alternatives considered

### Alt 1: Parametric edges (encode "lag offset k" in the graph)

Considered. Store edge metadata like `CrossCoordEdge::Lag { measure, offset: 3 }` instead of concrete `(source_coord, target_coord)` pairs. More compact storage; fewer edges.

**Rejected because:**
- Dirty walk becomes operator-specific (must interpret parametric edges differently per operator type)
- The invalidation path is the hot path — adding interpretation logic slows it down
- Lazy concrete registration already keeps the graph sparse (proportional to accessed cells, not cube size)
- Implementation complexity is 5-10× higher for marginal storage savings

### Alt 2: Remove revision-bump entirely (trust the graph fully)

Considered. If the graph has all edges, the revision-bump is redundant.

**Rejected because:**
- Never-evaluated cells would silently return stale values if their edges aren't registered yet
- The lazy graph pattern means edges only exist for cells that have been read at least once
- The revision-bump safety net costs nothing for cells that have been read (they already have edges; the freshness check is a single integer comparison)
- Removing the safety net risks subtle correctness bugs in edge cases (new operators that forget to register reads)

### Alt 3: Eager edge population at compile time

Considered. When a rule body contains `prev(M)`, pre-register all possible cross-coord edges at compile time (for every possible target coordinate).

**Rejected because:**
- Explodes graph size at startup (every possible coord pair, not just accessed ones)
- For a 300K-cell cube with 50% cross-coord rules: ~150K edges registered before any query runs
- Startup time regresses (graph population is O(cube_size), not O(1))
- Contradicts the lazy-graph design principle (`t_dependency_graph_empty_after_build` test)

### Alt 4: Defer to Phase 8 (ship daemon with revision-bump)

Considered. Accept the performance cost in the daemon and fix later.

**Rejected because:**
- Phase 8 daemon's cache hit rate depends on precise invalidation
- Interactive grid editing (Phase 6B consuming the daemon) needs low write-to-read latency
- The fix is surgical (1-3 code sites in cube.rs); the risk is low
- ADR-0018 Amendment §11 committed to fixing this "within next 2 phase cycles" — that deadline has passed

---

## Out of scope

- Cross-cube dependency edges (Tier C, Phase 5+ per spec; requires kernel changes beyond this fix)
- Graph compaction or optimization (if the graph gets large, optimize later; not blocking)
- Custom extension operators (future operators must follow the same "register reads" pattern; document in operator-authoring guide)
- Parallelized dirty propagation (Phase 2E+ territory)

---

## Acceptance criteria

1. Cross-coord reads (`prev`, `lag`, `actual_ref`, `scenario_ref`, `cumulative`, `rolling_avg`, adstock) register dependency edges in the graph
2. Null/missing/fallback reads ALSO register edges (not just successful reads)
3. Cache freshness semantics updated: cells with registered edges trust the dirty flag, not global revision
4. Writes produce precise dirty sets proportional to actual fan-out, not cube size
5. All existing tests pass unchanged (Acme dirty-set sizes identical)
6. New cross-coord dirty-set tests verify precise invalidation (including derived-source chains and null-read edges)
7. Edge deduplication: repeated reads do not grow edge count; window operators produce O(window) edges
8. New benchmarks demonstrate: (a) write-then-read-dependent is proportional to fan-out; (b) visible-grid-unrelated-write produces zero recomputation
9. Existing PERF.md ceilings not regressed
10. Revision-bump safety net retained for never-evaluated cells only
11. Time-relative rules (`is_past`/`is_current`/`is_future`) include `time_anchor` in cache key or are marked never-reuse-across-anchors
12. `cargo test --workspace` passes
13. `cargo clippy --all-targets --workspace -- -D warnings` passes

---

## Cross-links

- **Research note:** `docs/research-notes/cross-coord-dep-graph.md` (full technical analysis; this ADR resolves its open questions)
- **ADR-0018 Amendment §11:** Cumulative debt tracking; commits to this fix-it phase
- **ADR-0016 Amendment §12:** Performance note tied to `scenario_ref` + `actual_ref`
- **ADR-0011:** Phase 3E — `prev`/`lag`/`actual_ref` semantics (first introduction of cross-coord ops)
- **ADR-0025 Decision 3:** Caching strategy (coordinate+revision); this fix makes the cache precise
- **Phase 8 research note:** `docs/research-notes/mosaic-service-daemon.md` — daemon cache depends on this fix
- **PERF.md:** Benchmark ceilings; new cross-coord bench establishes new ceiling
- **MASTER_PHASE_PLAN:** "Cross-coord dep-graph fix" row

---

## Notes

**This is a two-part fix, not a one-line fix.** The research note's complexity was real. GPT review correctly identified that registering cross-coord reads (Part 1) is necessary but not sufficient — the cache freshness semantics (Part 2) must also change. Without Part 2, the global `cached_revision != cube_revision` check defeats the precision that Part 1 provides. Both parts must ship together.

**The Phase 8 dependency.** This fix should ship BEFORE Phase 8 implementation begins. The daemon's hot cube cache is designed around precise invalidation. Shipping the daemon without this fix means the cache provides less value than designed, and retrofitting the fix into a live daemon is harder than fixing the kernel first.

**The test confidence story.** Acme has no cross-coord operators. All existing Acme tests pass unchanged — the fix only adds edges for cubes that actually use cross-coord ops. The new tests specifically verify that cross-coord-heavy cubes get precise dirty sets. The derived-source-chain test and null-read-edge test catch the subtlest failure modes. Both dimensions of confidence are covered.

**Inspiration from incremental computation literature.** The freshness semantics in Decision 6 mirror Salsa (Rust's incremental computation framework for rustc) and Pluto (incremental build system with dynamic dependencies). Both systems track actual dependencies during execution and reuse results when dependencies are unchanged — regardless of global "epoch" or revision. Mosaic's pattern is the same: the dirty flag is authoritative for tracked cells; the revision is the fallback for untracked cells.

**Estimated effort with both parts.** 2-3 sessions. Part 1 (edge registration) is surgical. Part 2 (freshness semantics) requires careful modification of the read path's cache-hit logic, but the change is localized and the existing test suite provides strong regression coverage.
