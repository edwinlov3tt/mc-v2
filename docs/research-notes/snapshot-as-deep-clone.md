---
name: Snapshot as deep-clone
description: Phase 1 snapshots are full deep-clones of `HashMapStore` plus revision metadata; rollback replaces the live store with a clone of the snapshot's; no copy-on-write, no version vectors, no persistence — the deliberately-naive shape that ships first
type: research-note
---

# Snapshot as deep-clone

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2/3

---

## Conclusion (one sentence)

A `Snapshot` in Phase 1 is a full O(N) deep-clone of `HashMapStore` plus a `Revision` and optional label; `Cube::snapshot` calls `self.store.clone()` and `Cube::rollback_to` replaces the live store with `snap.store.clone()` plus drops every `Provenance::Rule` cell — no copy-on-write, no version vectors, no persistence, no cleverness, all per spec §3.16's explicit "No COW. No persistence. No cleverness."

## Why this matters

Snapshots are the engine's primitive for "approved version" workflows: take a snapshot, label it, mutate the cube, then optionally roll back to the snapshot. The temptation when implementing this in a memory-conscious systems language is to reach for copy-on-write (`Arc<HashMap<...>>` with structural sharing, version trees, persistent data structures). Phase 1 says no — the brief specifically mandates the naive shape. This is one of the cleanest examples in the codebase of "ship the obvious thing first, optimize when measurement justifies."

The justification for the discipline:

1. **Acme is small enough to not matter.** ~25K cells × `(CellCoordinate + StoredCell)` per cell is well under a megabyte; cloning it is microseconds. Phase 1A correctness gates ship without ever exercising the snapshot at scale.
2. **Cleverness has a debugging cost.** A COW snapshot scheme has at least three places where a bug could hide a stale-value read; the deep-clone scheme has zero. Phase 1 cannot afford concurrent unknowns.
3. **Phase 2/3 has the right framing to revisit.** Once Phase 1B benchmarks are in PERF.md, the snapshot-take cost is a known number; if it shows up as a bottleneck for any real workload, the optimization is *measured*, not speculative.

CLAUDE.md §2.18 names "Snapshot misimplementation" as a recurring trap and instructs: *"`Snapshot` holds a `HashMapStore` by value. Taking a snapshot is `store.clone()`. Rolling back is `cube.store = snapshot.store`. Done. No COW. No persistence. No cleverness."*

## Evidence

### The struct

[`crates/mc-core/src/snapshot.rs:19-33`](../../crates/mc-core/src/snapshot.rs#L19-L33):

```rust
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cube: CubeId,
    pub revision: Revision,
    pub captured_at: u64,         // wall-clock seconds; diagnostic only
    pub label: Option<String>,    // advisory; two snapshots can share a label
    pub(crate) store: HashMapStore,
}
```

The `store` field is `HashMapStore` by value, not `Arc<HashMapStore>`, not `Rc<RefCell<...>>`, not a structurally-shared map. `Snapshot: Clone` itself implies cloning the inner store on every snapshot clone.

### Take

[`crates/mc-core/src/cube.rs:1001-1009`](../../crates/mc-core/src/cube.rs#L1001-L1009):

```rust
pub fn snapshot(&self, label: Option<&str>) -> Snapshot {
    Snapshot {
        cube: self.id,
        revision: self.revision,
        captured_at: 0,
        label: label.map(str::to_string),
        store: self.store.clone(),
    }
}
```

The `store: self.store.clone()` is the entire cost. `HashMapStore` is just an `AHashMap<CellCoordinate, StoredCell>` so this is `O(N)` over the number of stored cells (including cached derived/consolidated entries — see [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md)).

### Rollback

[`crates/mc-core/src/cube.rs:1011-1039`](../../crates/mc-core/src/cube.rs#L1011-L1039):

```rust
pub fn rollback_to(&mut self, snap: &Snapshot) -> Result<Revision, EngineError> {
    if snap.cube != self.id {
        return Err(EngineError::SnapshotCubeMismatch);
    }
    self.store = snap.store.clone();
    self.revision = self.revision.next();
    self.dirty.clear_all();
    // Prune Provenance::Rule cells that came along on the clone:
    let stale: Vec<CellCoordinate> = self.store.iter().filter_map(|(c, s)| match s.provenance {
        Provenance::Rule { .. } => Some(c.clone()),
        _ => _,
    }).collect();
    for c in stale { self.store.remove(&c); }
    Ok(self.revision)
}
```

Three concrete actions:

1. Clone the snapshot's store into the live store.
2. Bump the live revision (rollback is a state change).
3. Drop every cached derived-leaf entry.

The third step is asymmetric with consolidated cache entries (see [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) on why) and is documented inline at [`cube.rs:1019-1027`](../../crates/mc-core/src/cube.rs#L1019-L1027).

### Sanity tests

[`crates/mc-core/src/snapshot.rs:113-148`](../../crates/mc-core/src/snapshot.rs#L113-L148) — `snapshot_clone_is_independent_of_live_store` writes a value, takes a snapshot, mutates the live store, and asserts the snapshot still sees the old value. This test is the pin: any "clever" snapshot impl that shared state with the live store would fail it.

### Spec text

Brief §3.16 is short and explicit:

> *"`Snapshot` is a clone of the store. No COW. No persistence. No cleverness."*

CLAUDE.md §2.18 quotes this verbatim and codifies it as a rule.

## Where it shows up in the engine

- **Source — struct:** [`crates/mc-core/src/snapshot.rs`](../../crates/mc-core/src/snapshot.rs).
- **Source — take:** [`crates/mc-core/src/cube.rs::snapshot`](../../crates/mc-core/src/cube.rs#L1001).
- **Source — rollback:** [`crates/mc-core/src/cube.rs::rollback_to`](../../crates/mc-core/src/cube.rs#L1011).
- **Tests:** [`crates/mc-core/src/snapshot.rs`](../../crates/mc-core/src/snapshot.rs#L51) module tests, plus integration coverage via `t_acme_snapshot_then_rollback_restores_state` in [`tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) and snapshot tests in [`tests/writeback.rs`](../../crates/mc-core/tests/writeback.rs).
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.16 + engine-semantics §19.
- **Operating manual:** [`CLAUDE.md`](../../CLAUDE.md) §2.18.
- **Phase 2 follow-up:** [`docs/reports/phase-1-completion-report.md` §8 item 3](../reports/phase-1-completion-report.md).

## Edge cases / gotchas

- **Snapshot-take is not free, but is acceptable for Acme.** ~25K cells × cell size is well under a millisecond on contemporary hardware. Phase 1B will measure this exactly and put a number in PERF.md. If the demo path's snapshot-take cost shows up meaningfully, that's a Phase 2 case for COW; until then the cost is fine.
- **Rollback prunes `Provenance::Rule` but keeps `Provenance::Consolidation`.** Both are technically "cached values," but Rule entries reference a specific rule version while Consolidation entries are pure functions of hierarchy + leaf values; the latter remains valid post-rollback as long as the snapshot's data is in place. The remaining Consolidation entries fail the revision-gate on next read and get overwritten — see [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) for the cache-symmetry analysis.
- **Snapshot store is shared between the snapshot and any clones via `Snapshot: Clone`.** That is, *cloning a snapshot* doesn't clone the store again until rollback runs `snap.store.clone()`. So `let s2 = snap.clone()` is cheap; only `cube.rollback_to(&snap)` pays the clone cost. This is fine because rollback is the only path that needs an independent store.
- **`captured_at` is a wall-clock timestamp but tests fix it to 0.** Determinism (CLAUDE.md §4.2): real timestamps are fine in `mc-cli` but never in tests. The field is purely diagnostic; reads use `revision` as the freshness key.
- **`label` is advisory.** Per spec §19 I-Snap-7 two snapshots can share a label. The snapshot module-internal test `snapshot_label_is_optional` ([`snapshot.rs:91-110`](../../crates/mc-core/src/snapshot.rs#L91-L110)) just confirms the field is nullable.
- **There's no snapshot registry on the cube.** `Cube::snapshot` returns the snapshot to the caller; the cube does not retain a list of outstanding snapshots. Lifetimes are entirely caller-managed. If the caller drops the snapshot, it's gone — there's no way to recover it from cube state.
- **`Snapshot::store` is `pub(crate)`, not `pub`.** External callers can read cells via `Snapshot::read` (a single-cell hashmap lookup) and inspect metadata, but cannot directly inspect the store contents. The accessor `Snapshot::store(&self)` is `pub(crate)` and currently `#[allow(dead_code)]` — reserved for slice-via-snapshot reads in step 15. The Phase 1 surface is intentionally minimal.
- **Snapshot can't span cubes.** `rollback_to` returns `EngineError::SnapshotCubeMismatch` when `snap.cube != self.id`. This is the only failure mode in rollback.
- **Phase 2 candidates if benchmarks justify it:** structural sharing via `im::HashMap` or similar, version-vector storage where snapshots are just `Revision` numbers and reads check a version filter, or per-cell COW. Each has tradeoffs documented in [`docs/external-conversations/`](../external-conversations/) and worth recapturing in an ADR when the time comes — not yet.

## Related notes

- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — the asymmetric pruning of `Provenance::Rule` vs `Provenance::Consolidation` on rollback.
- [`./lazy-dependency-graph.md`](./lazy-dependency-graph.md) — rollback also drops `dirty` state; the dep graph itself is *not* rolled back (it survives), so post-rollback reads work against the pre-rollback graph until they re-populate.

## History

- 2026-05-01 — Created from snapshot.rs, cube.rs::{snapshot,rollback_to}, brief §3.16, and CLAUDE.md §2.18, after Phase 1A ship.
