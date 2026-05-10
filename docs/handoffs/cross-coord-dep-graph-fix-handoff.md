# Cross-Coordinate Dependency Graph Fix — Handoff

**Status:** Proposed (next to start)
**Date:** 2026-05-10
**ADR:** [ADR-0027](../decisions/0027-cross-coord-dependency-graph-fix.md) (Proposed — accept before implementation)
**Research note:** [cross-coord-dep-graph.md](../research-notes/cross-coord-dep-graph.md)
**Estimated effort:** 2–3 sessions
**Crate(s) touched:** `mc-core` ONLY (kernel-internal change; no API surface changes)

---

## What this phase does

Two-part fix that makes writes invalidate only the cells that actually depend on the written value — not every derived cell in the cube.

**Part 1:** Register cross-coordinate reads (`prev`, `lag`, `actual_ref`, `scenario_ref`, `cumulative`, `rolling_avg`, adstock) in the dependency graph during evaluation.

**Part 2:** Update cache freshness semantics so cells with registered dependencies trust the graph's dirty flag rather than the global revision number.

Together, these produce **precise invalidation** — a write to `Spend[Jan]` only recomputes `prev(Spend)[Feb]` and its dependents, not all 180K derived cells in the cube.

---

## Why this matters

Phase 8 daemon has a hot cube cache. Interactive grid editing needs sub-millisecond write-to-read latency for the visible grid. Without this fix, every write to any cell stales out the entire cache — defeating the purpose of caching.

---

## The two-part fix

### Part 1: Register cross-coord reads

**Location:** `crates/mc-core/src/cube.rs` — wherever `resolve_cross_coord_read()` returns a value

**Current behavior:**
```rust
EvalLookup::Cross { resolved_coord, .. } => {
    let value = self.read_inner(resolved_coord)?;
    // Returns value WITHOUT registering the dependency
    Ok(value)
}
```

**After fix:**
```rust
EvalLookup::Cross { resolved_coord, .. } => {
    let value = self.read_inner(resolved_coord)?;
    ctx.actual_reads.push(resolved_coord.clone());  // ← Register the edge
    Ok(value)
}
```

**Critical rules:**
- Register even when the read returns `Null` or `Missing` (Decision 4 in ADR-0027). Otherwise later Tessera imports that populate previously-null cells won't invalidate dependent formulas.
- Register for ALL cross-coord operators uniformly (no special cases per operator).
- The downstream graph machinery (`register_dependencies`, `closure_of_dependents`) already handles arbitrary `(source, target)` edges — it just needs to receive them.

Find ALL sites where cross-coord reads happen. Search for:
- `resolve_cross_coord_read`
- `EvalLookup::Cross`
- Any place where a coordinate is resolved for `prev`, `lag`, `actual_ref`, `scenario_ref`
- The adstock backward scan loop
- The `cumulative` / `rolling_avg` window loops

Each site must push to `actual_reads`.

### Part 2: Update cache freshness semantics

**Location:** `crates/mc-core/src/cube.rs` — the read path where cached values are checked for freshness (around line 396 per the research note)

**Current behavior (pseudocode):**
```rust
fn read_cell(&mut self, coord: &CellCoordinate) -> ScalarValue {
    if let Some(cached) = self.cache.get(coord) {
        if cached.revision == self.revision {
            return cached.value;  // Cache hit
        }
        // Stale — recompute (EVERY cached cell fails this on EVERY write)
    }
    self.evaluate(coord)  // Cold compute
}
```

**After fix (pseudocode):**
```rust
fn read_cell(&mut self, coord: &CellCoordinate) -> ScalarValue {
    if let Some(cached) = self.cache.get(coord) {
        if cached.is_dirty {
            // Graph-driven invalidation: a dependency changed → recompute
            self.evaluate(coord)
        } else if cached.has_registered_edges {
            // Cell has been evaluated before, edges are tracked, NOT dirty
            // → Safe to reuse. The graph guarantees freshness.
            return cached.value;  // Cache hit!
        } else if cached.revision == self.revision {
            return cached.value;  // Cache hit (legacy path for untracked cells)
        } else {
            // No edges yet (first access) — revision fallback → recompute
            self.evaluate(coord)
        }
    } else {
        self.evaluate(coord)  // Never seen this cell
    }
}
```

**The key semantic change:** Cells with registered edges are REUSABLE across global revision bumps as long as they're not explicitly dirty. The global revision is DEMOTED to a safety net for cells without edges.

**Implementation notes:**
- You may need to add a `has_registered_edges: bool` or `edge_count > 0` check on the cached cell entry.
- Or: the dirty tracker already knows which cells are dirty (bitset from Phase 2D). If a cell is in the dirty set → recompute. If NOT in the dirty set AND has been evaluated at least once → reuse.
- Examine how `DirtyTracker` / the existing dirty set interacts with the cache. The existing `is_dirty()` mechanism may already provide what's needed — the change might be: "trust is_dirty() as authoritative for tracked cells; only fall back to revision for untracked cells."

### Part 3: Edge deduplication

Ensure the `DependencyGraph` doesn't accumulate duplicate edges. If a cell is evaluated multiple times (e.g., read from cache miss, then later re-evaluated due to a write), the same edges shouldn't be re-registered.

Check the `register_dependencies` function. If it uses `Vec<DependencyEdge>` for reverse edges, either:
- Switch to a `HashSet` or `SmallVec` with dedup
- Or add a check-before-insert

### Part 4: Time-anchor handling

Rules using `is_past()`, `is_current()`, `is_future()` depend on `time_anchor`. Decision 5 in ADR-0027 says: include `time_anchor` in the cache key for these cells, OR mark them as never-reuse-across-anchors.

**Simplest approach:** If the `EvalCtx` detects that a time-relative function was called during evaluation, mark the result as `time_anchor_dependent: true`. On cache read, if `time_anchor_dependent && current_anchor != cached_anchor` → recompute.

If no test cubes use time-relative functions, this can be a simple flag check with a TODO for Phase 8 testing. But the mechanism must exist.

---

## Tests to write

### Precise invalidation tests

```rust
#[test]
fn t_cross_coord_prev_precise_dirty() {
    // Cube with: Derived = prev(Input)
    // 3 time periods: Jan, Feb, Mar
    // Read Derived[Feb] → populates edge Input[Jan] → Derived[Feb]
    // Read Derived[Mar] → populates edge Input[Feb] → Derived[Mar]
    // Write Input[Jan] = new value
    // Assert: Derived[Feb] is dirty
    // Assert: Derived[Mar] is NOT dirty (its source is Input[Feb], not Input[Jan])
}

#[test]
fn t_cross_coord_cumulative_precise_dirty() {
    // Cube with: Total = cumulative(Input)
    // 4 time periods: Q1, Q2, Q3, Q4
    // Read all Total cells (populate edges)
    // Write Input[Q1]
    // Assert: Total[Q1], Total[Q2], Total[Q3], Total[Q4] are ALL dirty
    //         (cumulative depends on all prior periods)
    // Write Input[Q3]
    // Clear dirty, re-read all
    // Assert: Total[Q3], Total[Q4] are dirty; Total[Q1], Total[Q2] are NOT
}

#[test]
fn t_cross_coord_actual_ref_precise_dirty() {
    // Cube with: Plan_Ref = actual_ref(Measure)
    // Two scenarios: Actual, Plan
    // Read Plan_Ref[Plan] → edge from Measure[Actual] → Plan_Ref[Plan]
    // Write Measure[Actual]
    // Assert: Plan_Ref[Plan] is dirty
    // Assert: Other derived cells NOT using actual_ref are NOT dirty
}

#[test]
fn t_cross_coord_null_read_still_registers_edge() {
    // Cube with: Plan_Ref = actual_ref(Leads, fallback=0)
    // Leads[Actual] is NULL (not written yet)
    // Read Plan_Ref[Plan] → returns 0 (fallback), BUT edge is registered
    // Write Leads[Actual] = 100
    // Assert: Plan_Ref[Plan] is dirty (edge existed despite null source)
}

#[test]
fn t_cross_coord_derived_source_chain() {
    // Cube with: Revenue = Customers * AOV; Prev_Revenue = prev(Revenue)
    // Read Prev_Revenue[Feb] → edge Revenue[Jan] → Prev_Revenue[Feb]
    //   + Revenue[Jan] has edges to Customers[Jan] and AOV[Jan]
    // Write Customers[Jan]
    // Assert: Revenue[Jan] is dirty (same-coord dep)
    // Assert: Prev_Revenue[Feb] is dirty (transitive: Customers[Jan] → Revenue[Jan] → Prev_Revenue[Feb])
}

#[test]
fn t_edge_dedup_no_growth() {
    // Cube with prev(M)
    // Read Derived[Feb] 5 times
    // Count edges registered for Derived[Feb] → should be exactly 1, not 5
}

#[test]
fn t_visible_grid_unrelated_write_preserves_cache() {
    // Cube with 100 cells: 50 input (Spend), 50 derived (Revenue = Spend * 2)
    // Read all 50 Revenue cells (populate graph)
    // Write Spend[one specific coord]
    // Re-read all 50 Revenue cells
    // Assert: only 1 Revenue cell was recomputed; other 49 are cache hits
    // (This is the daemon grid-editing use case)
}
```

### Existing test regression

- `t_dirty_set_after_write_*` — Acme tests must pass with IDENTICAL dirty set sizes (Acme has no cross-coord ops)
- `t_acme_*` — All golden values unchanged
- `doctrine_*` — All doctrines pass

---

## Benchmarks to add

File: `crates/mc-core/benches/cross_coord_write_precision.rs`

**Bench 1: write-dependent (precision test)**
```
Cube: 10 channels × 5 markets × 24 time periods × 4 measures
  - 2 input measures (Spend, CPC)
  - 2 derived with prev() (Prev_Spend, Prev_CPC)
Total cells: ~4,800. Derived: ~2,400.
Setup: Read all derived cells (populate graph).
Benchmark: Write Spend[one coord] → read Prev_Spend[next period].
Target: latency proportional to ~5 cells (fan-out), not ~2,400 (all derived).
```

**Bench 2: visible-grid-unrelated-write (daemon use case)**
```
Same cube as above.
Setup: Read a "visible grid" of 200 cells.
Benchmark: Write an input cell that affects NONE of those 200 → re-read all 200.
Target: All 200 should be cache hits (zero recomputation).
```

---

## Files to modify

| File | Change |
|---|---|
| `crates/mc-core/src/cube.rs` | Register cross-coord reads in actual_reads; update cache freshness semantics |
| `crates/mc-core/src/dependency.rs` | Edge deduplication (if not already present) |
| `crates/mc-core/benches/cross_coord_write_precision.rs` | NEW — precision benchmarks |
| `crates/mc-core/tests/cross_coord_dirty.rs` (or similar) | NEW — precise invalidation tests |

**No other crates change.** This is entirely internal to `mc-core`.

---

## What NOT to change

- No API surface changes (Cube::read, Cube::write signatures unchanged)
- No new public types
- No changes outside `mc-core`
- No changes to the Acme demo or any existing test fixtures
- Don't remove the revision-bump mechanism — keep it as safety net for untracked cells

---

## Acceptance criteria

1. Cross-coord reads register edges in the graph
2. Null/missing reads also register edges
3. Cache freshness: cells with edges trust dirty flag, not global revision
4. Precise dirty sets: write to X only dirties cells that depend on X (transitively)
5. Derived-source chains propagate correctly (prev of derived measure)
6. Edge dedup: repeated reads don't grow edge count
7. Visible-grid benchmark: unrelated write produces zero recomputation on cached cells
8. All existing tests pass unchanged (Acme dirty sets identical)
9. Existing PERF.md benchmark ceilings not regressed
10. Time-relative rules handle time_anchor correctly
11. `cargo test --workspace` passes
12. `cargo clippy --all-targets --workspace -- -D warnings` passes

---

## Cross-links

- **ADR-0027:** All binding decisions for this fix
- **Research note:** `docs/research-notes/cross-coord-dep-graph.md` (background analysis)
- **Phase 8 daemon:** depends on this fix for cache precision
- **cube.rs lines ~396, ~457, ~751:** Key locations per research note (verify against current HEAD)
- **DirtyTracker / bitset:** Phase 2D work; examine interaction with new freshness semantics
- **PERF.md:** Existing benchmark ceilings that must not regress

---

**End of handoff. Two parts: register reads (surgical) + update freshness semantics (the part that actually delivers the performance win). Ship together.**
