# Phase 2D Handoff — Bitset-Backed Dirty Tracker (§9.3)

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 2D.
> **You inherit a green Phase 2C** (commit `789db15`, tag
> `phase-2c-workload-baseline`, backfill at `96cca75`).
>
> **Branch picked from PERF.md §6.14: Branch A — §9.3.** The
> `load_canonical_inputs` super-linear cliff between 10× (4.33×/write)
> and 50× (19.7×/write) is the load-bearing finding, attributable to
> the `AHashSet<CellCoordinate>` dirty tracker's per-mark hash-and-insert
> cost growing nonlinearly as the set saturates. Phase 2D replaces it
> with a Cartesian-product flat bitset keyed by linearized coordinate
> index, giving O(1) mark/check independent of set size.
>
> **Hard rule:** This phase touches `crates/mc-core/src/`. The change
> is surgical (one or two files, ~150–250 lines) and gated behind a
> kernel unit test that proves observational equivalence with the
> existing tracker. The Phase 2B template (Arc<Hierarchy>) is the
> precedent: small, source-locked, justified by data, accompanied by
> a kernel unit test.

---

## Where Phase 2C ended

- **Phase 2C commit / tag:** `789db15` — *bench: complete Phase 2C workload-shaped benchmark baseline* — tag `phase-2c-workload-baseline`. Backfill commit at `96cca75`.
- **Test status:** 216 / 0 passing across all targets. 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build / fmt / clippy / test / demo / bench.
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval.**
- **Cargo.lock pins (still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **PERF.md §6.14 headline finding (verbatim):**
  > `load_canonical_inputs` shows a super-linear cliff between 10× (4.33×/write) and 50× (19.7×/write). Total ingest at 50× is **23× over the ADR-0003 patience-limit gate** (231 s vs 10 s). 100× was abandoned mid-run after criterion estimated > 38 minutes for a single 10-sample row. **This is the single data point Phase 2D's pick should anchor on.**
  >
  > **Why the cliff is §9.3 evidence, not §9.2.** §9.2 attacks per-write fixed cost (permission / type / lock / NaN / version / store-write / revision-bump). Those costs scale O(1) with cube size — fixing them drops every per-write cost by a constant amount but doesn't bend the curve. §9.3 attacks the per-mark hash-and-insert cost on the `AHashSet<CellCoordinate>` dirty tracker. As the dirty set grows (during bulk ingest, dirty grows from 0 to ~150 K entries at 10×, ~750 K at 50×, ~1.5 M at 100×), each subsequent insert costs more — AHashSet rehashes, cache locality drops, hash-collision probability climbs. This compounds nonlinearly. **A bitset-backed dirty tracker keyed by per-dim element index would make every insert O(1) and independent of set size, exactly the thing the cliff data names.**
  >
  > **Why combined-workflow flatness doesn't contradict.** The combined-workflow per-mark cost is flat **within a session** at 50× (422 → 419 → 422 ns across 100 edits). That's because the dirty set was *already* fully populated from the bulk-load that preceded the session — `final dirty_set = 305,039` at session start (after bulk-load), and stays in the same range across the session. The *cliff* is in the bulk-load itself, where the dirty set grows from 0; once it's saturated, per-mark cost stabilizes. So the two measurements reinforce each other: per-mark cost is dominated by set-size growth, and the dominant set-size growth happens during ingest, not within an interactive session.

- **ADR-0003** ([`../decisions/0003-workload-sketch.md`](../decisions/0003-workload-sketch.md)) — Accepted — Provisional. Defines the perception-threshold gates this optimization is measured against. The relevant gate for Phase 2D's acceptance is the ADR-0003 "patience limit" (≤ 10 s for bulk imports of typical-size datasets).

For the full Phase 2C audit read [`../reports/phase-2c-completion-report.md`](../reports/phase-2c-completion-report.md). For the bench baseline this phase diffs against, see [`../reports/bench-data/phase-2c/`](../reports/bench-data/phase-2c/) and its [README](../reports/bench-data/phase-2c/README.md).

---

## Phase 2D prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2D: Bitset-Backed Dirty Tracker.
>
> **Context.** PERF.md §6.14's `load_canonical_inputs` cliff (4.33×/write at 10× → 19.7×/write at 50×) attributes super-linear ingest cost to `AHashSet<CellCoordinate>` rehash + cache locality + hash-collision probability as the dirty set grows from 0 to 1.5M+ entries. Replace the AHashSet with a Cartesian-product flat bitset keyed by linearized coordinate index. Per-mark cost becomes O(1), independent of set size.
>
> **Goal.** Drop `load_canonical_inputs/50x` from 230.84 s (Phase 2C baseline) to ≤ 50 s — a ≥ 4.6× improvement that brings 50× ingest within ~5× of ADR-0003's 10 s patience limit. This is the load-bearing acceptance gate; secondary improvements on `write_input_leaf/10x` and any other write-shaped row are diagnostic, not gating.
>
> **Phase 2D scope:**
>
> 1. **Add a precomputed `CubeShape`** to the `Cube` struct, holding per-dimension element-index maps and per-dimension strides for linearizing a `CellCoordinate` to a `usize` index. Computed once at `CubeBuilder::build` from the existing dim/element data; immutable for the cube's lifetime. Held behind `Arc` for cheap propagation to the dirty tracker. Source: `crates/mc-core/src/cube.rs` (struct field) + a new private helper `cube_shape::CubeShape` (likely a new module `crates/mc-core/src/cube_shape.rs` or a struct inside `cube.rs`).
>
> 2. **Replace the `DirtyTracker` internal representation** in `crates/mc-core/src/dirty.rs`:
>
>    - Before: `pub struct DirtyTracker { set: AHashSet<CellCoordinate> }`
>    - After: `pub struct DirtyTracker { bits: BitVec, shape: Arc<CubeShape>, len: usize }` (or equivalent — the exact field set is yours, but the public API surface stays identical).
>
>    The public methods (`mark`, `mark_closure`, `is_dirty`, `clear`, `clear_all`, `len`, `is_empty`, `iter`, `snapshot_sorted`) keep their signatures verbatim. Internal implementations:
>    - `mark(coord)`: linearize coord → set bit at index. Increment `len` if the bit was previously zero.
>    - `is_dirty(coord)`: linearize coord → test bit at index. O(1).
>    - `clear(coord)`: linearize → clear bit. Decrement `len` if the bit was previously one.
>    - `clear_all()`: zero the bitset. `len = 0`.
>    - `iter()`: walk set bits, materializing each as a `CellCoordinate` via the inverse-linearize from `CubeShape`. **Allocation cost is paid only on `iter()`, not on `mark/check`.** Order is bit-set order (deterministic across runs), which is a stricter ordering than the current `AHashSet::iter()` (which is non-deterministic). Tests that depended on AHashSet's nondeterminism by sorting first will continue to pass; the stricter ordering is strictly stronger.
>    - `snapshot_sorted()`: walk set bits in order, materialize, return Vec.
>    - `mark_closure(root, graph)`: unchanged — calls into `graph.closure_of_dependents(root)` and feeds each into `mark`. The mark fast-path is what changes.
>
> 3. **Construct the tracker with the shape.** `DirtyTracker::new()` becomes `DirtyTracker::with_shape(shape: Arc<CubeShape>)`. Plumb through `CubeBuilder::build` so the cube's tracker has the shape from the start. The `iter()` API needs the shape to inverse-linearize.
>
> 4. **Add a kernel unit test** at `crates/mc-core/src/dirty.rs::tests::bitset_tracker_observationally_equivalent_to_ahashset` that builds a small cube, drives a sequence of mark/clear operations against both an old AHashSet-backed tracker (kept inline as a test-only struct, not retained in the kernel) and the new bitset tracker, and asserts they agree on `is_dirty` for every coord, `len()`, and `iter().sorted()`. This is the §10.1 dirty-set membership invariant proven exactly.
>
> 5. **Re-run the Phase 2C bench gate** at `--baseline phase-2c`. The §6.12.7 `load_canonical_inputs` rows + §6.12.1 `write_input_leaf` rows are the diagnostic targets. PERF.md §6.15 records the diff.
>
> **Hard rules:**
>
> - Source change confined to: `crates/mc-core/src/dirty.rs`, `crates/mc-core/src/cube.rs`, optionally a new file `crates/mc-core/src/cube_shape.rs` (or equivalent — `Arc<CubeShape>` lives somewhere internal). **No other source file may change.**
> - The public API surface in `crates/mc-core/src/lib.rs` MUST NOT lose or rename any re-export. `DirtyTracker`, `CellCoordinate`, `Cube`, `Snapshot` re-exports stay byte-for-byte.
> - The `DirtyTracker` public method signatures stay byte-for-byte. Internal repr changes are fine; signature changes are not.
> - No new external dependency. `bit-vec` is in std-adjacent crates; this phase uses `Vec<u64>` + manual bit-twiddling, NOT a new crate. (If the implementer makes a strong case for `bit-vec` or `bitvec` via SPEC QUESTION + ADR, that's reviewable, but the default is in-house.)
> - No async / threads / rayon / tokio / serde / external storage.
> - All 216 existing tests must still pass.
> - All Phase 1B / 2A / 2B / 2C benches must still build and run.
> - The §10.1 dirty-set assertions (which check exact dirty-set membership and iter content) MUST pass byte-for-byte.
> - Do not bump `rust-toolchain.toml`. Do not run `cargo update`. Do not touch `docs/specs/`. Do not amend ADR-0003 (Phase 2D consumes it; doesn't modify it).
>
> **Acceptance gate (the one thing that determines done):**
> PERF.md §6.12.7 `demo_path/load_canonical_inputs (126000 writes)` (the 50× row) drops from 230.84 s (phase-2c baseline) to ≤ 50 s. Higher-fan-out write rows (`write_input_leaf/10x`, etc.) should also improve materially; record those numbers in §6.15 but they are not gating.
>
> Secondary expectation: §6.13 combined-workflow per-mark cost stays flat (within ±10% of 422 ns) — confirms the change doesn't introduce per-edit regression in the saturated-set regime where the AHashSet was already efficient.
>
> **Validation gate before reporting done:**
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (must remain 216 / 0)
> - `cargo run --release --bin mc -- demo` (must match brief §4.6)
> - `cargo bench -p mc-core --bench demo_path -- --baseline phase-2c` — confirms the 50× ingest row hits the ≤ 50 s gate
> - `cargo bench -p mc-core --bench combined_workflow -- --baseline phase-2c` — confirms no within-session regression
> - Spot-run other Phase 1B/2A/2B/2C bench files at `--baseline phase-2c`; expect ±10% noise, no row beyond
> - 10 consecutive `cargo test --workspace -q` (still deterministic)
>
> **PERF.md update requirements:**
> - Append §6.15 "Phase 2D verification" with a before/after diff table for every row that improved.
> - Update §9.3 from "data-justified candidate" / "tracked" wording to "**closed in Phase 2D (commit `<hash>`)**". Cite the §6.15 diff.
> - Update §6.14's "Phase 2D pointer" paragraph to a closure note pointing at §6.15. Do not delete the pointer text — leave it as the historical record of what drove the pick.
> - Update §10's files-changed manifest to include the kernel source files Phase 2D touched.
>
> **Rollback plan (in case complexity explodes):**
> If the bitset implementation balloons beyond ~250 lines of source, or if a §10.1 invariant breaks under the new representation in a way that requires more than a one-line fix, **stop and write a SPEC QUESTION per CLAUDE.md §11**. Two recovery paths:
>
> 1. **Roaring Bitmap (Option B).** Add `roaring = "0.10"` as an mc-core runtime dep — requires an ADR documenting the new dep and confirming it doesn't pull in async/serde transitives on Rust 1.78.
> 2. **Hybrid: keep AHashSet, optimize hashing.** Replace the per-coord hash with a precomputed 64-bit hash stored on `CellCoordinate` at construction. Smaller win (probably 2–3×, not the 4–5× the cliff demands), but avoids the bitset complexity.
>
> Either fallback is a Phase 2D.1 amendment, not a Phase 2D scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 2D Bitset-Backed Dirty Tracker
>
> Build:    [command] ✓/✗
> Format:   [command] ✓/✗
> Lint:     [command] ✓/✗
> Tests:    cargo test --workspace 216 / 216
> Demo:     target/release/mc demo ✓/✗
> Bench:    cargo bench -p mc-core --bench demo_path -- --baseline phase-2c ✓/✗
>
> Acceptance gate:
> - load_canonical_inputs/50x: <BEFORE> → <AFTER> (target ≤ 50 s)
>
> Other write-shaped row deltas:
> - write_input_leaf/10x:        <BEFORE> → <AFTER>
> - write_input_leaf/Acme:       <BEFORE> → <AFTER>
> - dirty_propagation/spend:     <BEFORE> → <AFTER>
>
> Within-session combined-workflow check:
> - per-mark cost iter 1/50/100: <NUMBERS> (target: stays in ±10% of 422 ns)
>
> Source changes:
> - crates/mc-core/src/dirty.rs        (~N lines)
> - crates/mc-core/src/cube.rs         (~N lines)
> - crates/mc-core/src/cube_shape.rs   (new, ~N lines) [optional]
>
> Implementation summary:
> - <one paragraph: bitset width, linearization scheme, allocation pattern>
>
> §10.1 invariant proof:
> - <unit-test name + how it asserts equivalence>
>
> Memory footprint at 100× Acme:
> - <rough number — sanity check the bitset doesn't blow memory>
>
> Deviations:
> - <list any>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The exact code being optimized

[`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) currently holds:

```rust
pub struct DirtyTracker {
    set: AHashSet<CellCoordinate>,
}
```

And exposes the public methods `new`, `mark`, `mark_closure`, `is_dirty`, `clear`, `clear_all`, `len`, `is_empty`, `iter`, `snapshot_sorted`. Every mark is a `CellCoordinate.hash()` (which walks the SmallVec<[ElementId; 6]>, hashing 6 u64s) + an AHashSet insert (which does open-addressed probing + occasional rehash as the table grows past load-factor thresholds). At Acme, ~215 marks per write × ~95 µs per write of fixed cost = the row's ~165 µs total. At 50× scale, the dirty set saturates at ~750 K entries during bulk ingest; AHashSet's load-factor rehashes plus cache-line misses during probing put the per-mark cost at ~1.83 µs (vs ~712 ns at Acme).

### B. Why a Cartesian-product flat bitset is the right shape

Linearize `CellCoordinate { cube, elements: [e0, e1, ..., e_{D-1}] }` to a `usize index` via:

```rust
fn linearize(coord: &CellCoordinate, shape: &CubeShape) -> usize {
    let mut idx = 0;
    for (dim, eid) in coord.elements.iter().enumerate() {
        let local = shape.element_index_in_dim[dim][eid];   // u32 lookup
        idx += local as usize * shape.stride[dim];
    }
    idx
}
```

`shape.element_index_in_dim[dim]` is a `AHashMap<ElementId, u32>` precomputed at cube-build time (one lookup per dim, no allocation). `shape.stride[dim]` is precomputed (per-dim stride for the row-major linearization). Total cost: D = 6 hash lookups + 6 multiplies + 6 adds — ~50 ns on Acme hardware. **Independent of set size.** That's the win.

The bitset width = product of dim cardinalities. Memory at calibration scales:

| Scale | Cube cardinality | Bitset bytes |
|---|---:|---:|
| Acme (1×) | ~25 K | ~3 KB |
| 10× | ~250 K | ~31 KB |
| 50× | ~5 M | ~625 KB |
| 100× | ~25 M | ~3 MB |

All comfortable. At hypothetical real-production scale (100M+ cells), the flat bitset hits ~12 MB — still tractable; the memory wall is at ~1G+ cells, well past current calibration.

### C. The §10.1 dirty-set membership invariant

`crates/mc-core/tests/acme_demo.rs::t_acme_dirty_set_size_within_bound_after_one_spend_write` asserts the dirty-set delta after a single Spend write is bounded (≤ 215 marks at Acme). The bitset implementation must reproduce this exact membership — every coord that the AHashSet implementation marked as dirty after that write must be marked in the bitset, and no others. This is a strictly stronger guarantee than "len matches" because a bitset that marks the right *count* of cells but the wrong *cells* would still pass len assertions.

The kernel unit test at §"Phase 2D scope" item 4 is what proves this: build both representations side-by-side, drive identical operation sequences, assert byte-for-byte equivalence on the resulting set membership. If that test passes, every higher-level test (§10.1 etc.) inherits the equivalence.

### D. iter() ordering — strictly stronger, not weaker

The current `AHashSet::iter()` is non-deterministic across runs (it's a hash table). Any test that compares iter content for equality already collects + sorts first (per CLAUDE.md §2.11). The bitset's `iter()` is deterministic in bit-position order — strictly stronger. **Tests that worked under AHashSet keep working under bitset.** No test should need modification for ordering.

### E. The shape's lifetime relative to the cube

`CubeShape` is computed at `CubeBuilder::build` and held by `Cube` for its lifetime. `Snapshot` (which clones `HashMapStore`) does **not** need to clone the shape — the shape is structural metadata about the cube's dimensions, not part of the store. When `Cube::rollback_to(&snap)` runs, the shape is unchanged. `DirtyTracker::clear_all()` zeroes bits but keeps the shape. **No interaction with snapshot semantics.**

### F. Phase 2C regression guard

The phase-2c baseline at `docs/reports/bench-data/phase-2c/` is your forward baseline. After your change:

- `load_canonical_inputs` rows should improve dramatically (this is the gate).
- `write_input_leaf` rows should improve modestly (per-write fixed cost includes the AHashSet insert).
- `dirty_propagation` rows should improve (mark_closure walks more efficiently).
- Read rows (`read_input_leaf_warm/cold`, `read_derived_leaf_*`, `consolidation_warm/cold`) should be **unchanged or marginally faster** (read paths don't touch the dirty tracker write surface, but the `is_dirty` check is now O(1) bit-test instead of hash-and-probe).
- Snapshot/rollback rows should be unchanged.
- Within-session per-mark cost (combined_workflow §6.13.3) should stay flat — no regression in the saturated regime where AHashSet was already cheap.

Any **regression** beyond noise is a stop-the-line signal — investigate before recording.

### G. Phase 2E forecast (after Phase 2D ships)

If 2D succeeds:
- §9.3 closes; the §9 list narrows to §9.2 (opportunistic), §9.6 (leave it), §9.7 (toolchain).
- The next §6.14-driven candidate becomes whichever row Phase 2C's 50× / 100× env-gated benches surface when Phase 2D opts into them.
- **Phase 2E may not need to exist.** If the §6.14 cliff is the only super-linear row and Phase 2D closes it, Phase 2 can exit and Phase 3A becomes proposed.

If 2D's bitset hits a memory wall at 100× or beyond:
- Phase 2D.1 amendment: switch to Roaring Bitmap (under ADR), or switch to a hybrid that uses bitset for hot dim ranges and falls back to AHashSet for sparse coordinates.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| The optimization site | [`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) | replace internal repr; preserve public method signatures |
| Hold the precomputed shape | [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) | add `cube_shape: Arc<CubeShape>` field; populate in `CubeBuilder::build` |
| (optional) Define `CubeShape` | `crates/mc-core/src/cube_shape.rs` | new file (private module); or inline in `cube.rs` |
| (optional) Linearize helper | [`crates/mc-core/src/coordinate.rs`](../../crates/mc-core/src/coordinate.rs) | add `pub(crate) fn linearize(&self, shape: &CubeShape) -> usize` if it reads cleaner there |
| Phase 2D verification subsection + §9.3 closure note + §10 manifest | [`../PERF.md`](../PERF.md) | append §6.15 + targeted edits |
| Phase 2D completion report | `docs/reports/phase-2d-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Save phase-2d criterion baseline | [`../reports/bench-data/phase-2d/`](../reports/bench-data/) | new dir; use the [bench-data README](../reports/bench-data/README.md) workflow |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 2D from `proposed` → `complete` |

**Do not touch:**

- `crates/mc-core/src/` — any file other than `dirty.rs`, `cube.rs`, optionally `cube_shape.rs` and `coordinate.rs`. Touching `consolidation.rs`, `rule.rs`, `dependency.rs`, etc. is a signal that the scope has crept.
- `crates/mc-core/tests/` — the contract test suite is locked.
- `crates/mc-core/benches/` — Phase 2D does not need new bench code; the existing files run against `--baseline phase-2c`.
- `crates/mc-fixtures/src/lib.rs` — public fixtures are a shared contract.
- `docs/specs/` — locked.
- `rust-toolchain.toml` — pinned.
- ADR-0003 — Accepted; amendments go in `0003-amendment-N.md`.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-2D gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 216 / 0
cargo run --release --bin mc -- demo

# Restore phase-2c baseline locally so --baseline phase-2c works:
for bench in $(ls docs/reports/bench-data/phase-2c/); do
  [ "$bench" = "README.md" ] && continue
  mkdir -p "crates/mc-core/target/criterion/$bench"
  cp -R "docs/reports/bench-data/phase-2c/$bench/." "crates/mc-core/target/criterion/$bench/"
done

# Pre-2D bench check (sanity — every row should match phase-2c):
cargo bench -p mc-core -- --baseline phase-2c

# Quick smoke during 2D development:
cargo bench -p mc-core --bench dirty_propagation -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10
cargo bench -p mc-core --bench leaf_read_write -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10

# The acceptance-gate row — full sample:
cargo bench -p mc-core --bench demo_path -- --baseline phase-2c

# Save the post-2D baseline once at end of phase:
cargo bench -p mc-core --bench <name> -- --save-baseline phase-2d
mkdir -p docs/reports/bench-data/phase-2d
# then mirror the bench-data/README.md workflow to copy phase-2d JSON
```

---

## Final checklist before you call Phase 2D done

- [ ] Cartesian-product flat bitset implemented in `dirty.rs` (Option A from §B above).
- [ ] `CubeShape` struct added (in `cube.rs` or `cube_shape.rs`) and populated at `CubeBuilder::build`.
- [ ] Linearize / inverse-linearize implementations + tests for round-tripping coords.
- [ ] Source change confined to `dirty.rs`, `cube.rs`, optionally `cube_shape.rs` + `coordinate.rs`.
- [ ] No public API symbol from `crates/mc-core/src/lib.rs` removed or renamed.
- [ ] No new external dependency. (If a SPEC QUESTION authorized one, link the ADR.)
- [ ] Kernel unit test `bitset_tracker_observationally_equivalent_to_ahashset` lands in `dirty.rs::tests` and passes.
- [ ] All 216 existing tests still pass.
- [ ] §10.1 `t_acme_dirty_set_size_within_bound_after_one_spend_write` passes byte-for-byte (this is the load-bearing membership invariant).
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] **Acceptance gate met:** `load_canonical_inputs/50x` ≤ 50 s.
- [ ] Within-session combined-workflow per-mark cost flat (within ±10% of 422 ns).
- [ ] No Phase 1B / 2A / 2B / 2C bench row regressed beyond noise (~10%).
- [ ] PERF.md §6.15 written; §9.3 closure-noted; §6.14 pointer paragraph updated to closure note; §10 manifest updated.
- [ ] Completion report at `docs/reports/phase-2d-completion-report.md` written from template.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 2D from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 2E or any later phase.**

If you are uncertain at any point, the resolution order is:

1. The Phase 2D prompt above.
2. PERF.md §6.14 (the data justifying this phase) + §6.12.7 (the gate row).
3. ADR-0003 (Accepted — Provisional; sunset 2026-11-01).
4. Phase 2C completion report.
5. Earlier completion reports (1A / 1B / 2A / 2B).
6. `docs/specs/engine-semantics.md`, `docs/specs/phase-1-rust-kernel-build-brief.md`.
7. `CLAUDE.md`.
8. `docs/roadmap/MASTER_PHASE_PLAN.md`.
9. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (unchanged from Phase 2B / 2C)

**Measure before you optimize.** Phase 2D exists because Phase 2C measured what 2D should optimize. The §6.14 cliff data is the justification. Your kernel change must trace back to a §6.14 / §6.12.7 row.

**Source-locked between phases — except this one.** This is the rare phase that touches the kernel, surgically, with a unit test. Phase 2E (if any) is back to source-locked.

**A bench is a contract, not a draft.** Phase 2D's verification rows must be reproducible by anyone with the repo. Use the `phase-2c` baseline; save the `phase-2d` baseline; commit the JSON.

**Do not pick the next optimization.** Phase 2D's deliverable is the source change + its verification. If §9 still has rows after this phase, the next sub-phase is its own pick — driven by the post-2D bench data, not by the §9 list.

**Rollback is a real path.** If the bitset implementation outgrows ~250 lines or the §10.1 invariant breaks in a non-trivial way, **stop and write a SPEC QUESTION per CLAUDE.md §11.** The two recovery paths (Roaring Bitmap, hashed-CellCoordinate) are explicit. Don't ship a kernel change that broke a contract test "with one tiny tweak."

---

*Phase 2D handoff promoted from scaffold (`docs/reports/phase-2d-handoff-scaffold.md`) on 2026-05-02 after Phase 2C committed at `789db15` (tag `phase-2c-workload-baseline`). Branch A picked from PERF.md §6.14's `load_canonical_inputs` cliff finding.*
