# Phase 2A Handoff — Cold-Path Benchmark Expansion

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 2A.
> **You inherit a green Phase 1B.** Your job is **measurement, not
> behavior change.** Read this whole file before touching code.
>
> **Why this phase exists.** Phase 1B closed the criterion tooling gap
> and shipped a baseline, but two measurement holes remained:
>
> 1. The §11.2 consolidation rows were warm-cache hits (~67 ns), not
>    cold-walk costs. The brief's 50 µs / 1 ms / 20 ms / 5 ms / 2 ms
>    ceilings were calibrated against cold reads and cannot be claimed
>    "passed" by today's numbers.
> 2. `bench_write_input_leaf_no_deps` (165 µs, 1A ceiling 50 µs) measures
>    the same thing as `bench_write_input_leaf` on Acme because the
>    fixture's hierarchy fan-out dominates per-write cost. The brief's
>    "no-dependents" condition implicitly assumes a synthetic
>    no-hierarchy cube.
>
> **Phase 2A's job is to close both holes** so Phase 2B (kernel
> optimization) can prioritize from data instead of guesswork.

---

## Where Phase 1B ended

- **Last Phase 1A commit:** `bee2812` — *mc-core: update lib.rs doc-comment to point at docs/specs/* (kernel at `4aa674a`).
- **Last Phase 1B commit:** _the `phase-1b-benchmark-baseline` tag_ — see [`CURRENT_STATE.md`](../CURRENT_STATE.md).
- **Test status:** 203 / 203 passing across all targets. 10/10 determinism gate runs identical.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** `cargo build --release --workspace`, `cargo fmt --check --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo bench --workspace`.
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval** (PERF.md §9.7 has the procedure when you're authorized).
- **Phase 1B baseline:** [`docs/PERF.md`](../PERF.md). Read it cover-to-cover; the `Two important caveats` banner at the top is what this phase exists to address.

For the full Phase 1A audit read [`reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md). Read sections 0, 1, 1.1, 2 (especially 2.6, 2.7, 2.12), 3, 5.1, 5.5, 6, 8, and 12 before writing any code. They override anything below if there is a conflict.

---

## Phase 2A prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2A: Cold-Path Benchmark Expansion.
>
> **Context:**
> Phase 1B is complete. The criterion bench harness is wired up under
> `crates/mc-core/benches/`, the kernel passes all correctness and
> determinism gates, and `cargo bench --workspace` is part of the
> standard pipeline. PERF.md §6 contains the warm-state baseline. Two
> measurement gaps remain — see PERF.md's top-of-file caveats banner.
>
> **Goal:**
> Close those two measurement gaps by adding cold-path benchmarks. Do
> not optimize the kernel. Do not change kernel behavior. The deliverable
> is more measurement, plus an updated PERF.md, so Phase 2B (optimization)
> can prioritize from data.
>
> **Phase 2A scope:**
> 1. **Cold consolidation benchmarks.** Extend
>    `crates/mc-core/benches/consolidated_read.rs` with cold-state
>    variants of every existing row (3 leaves, 27 leaves with weighted
>    avg, 27 leaves with rule chain, 420 leaves). Cache must be cold
>    when timing starts. Cache state must be verified before timing.
> 2. **Synthetic minimal-hierarchy fixture for `no_deps` writes.** Add
>    a new public builder to `mc-fixtures` (e.g.
>    `build_minimal_cube()`) that produces a cube with no hierarchies
>    on any dimension and no derived measures. Use it to add a
>    `synthetic_no_deps` benchmark that measures the true 1A
>    `bench_write_input_leaf_no_deps < 50 µs` ceiling — i.e. a write
>    with no rule rev-edges and no hierarchy ancestors.
> 3. **Snapshot clone benchmark.** Add
>    `crates/mc-core/benches/snapshot_clone.rs` that measures
>    `Cube::snapshot(None)` and `Cube::rollback_to(&snap)` at varying
>    store cardinalities (e.g. 0 cells, 100 cells, 2,520 cells, fully
>    materialized).
> 4. **Hierarchy ancestor mark microbench.** Isolate the dominant write
>    cost from other write fixed costs. Approach: build a series of
>    fixtures with graduated hierarchy depth (no hierarchy → 1-deep →
>    2-deep → 3-deep on one dim) and measure write cost; the marginal
>    cost between rows is the hierarchy mark contribution. Land in
>    `crates/mc-core/benches/hierarchy_mark.rs` (or extend the
>    synthetic fixture file from item 2).
>
> **Hard rules:**
> - Do not modify any file under `crates/mc-core/src/`.
> - Do not modify any file under `crates/mc-core/tests/`.
> - Do not modify the locked spec inputs in `docs/specs/`.
> - Do not loosen, rewrite, or remove the existing 5 Phase 1B bench files;
>   only extend them.
> - Do not add async, threads, rayon, tokio, serde, or external storage.
> - Do not add a `CellStore` trait or rewrite `HashMapStore`.
> - Do not bump `rust-toolchain.toml` without explicit approval. If a new
>   benchmark dep needs a newer Rust, **stop and report** before changing
>   the pin.
> - All 203 existing tests must still pass.
> - All 5 Phase 1B benches must still run and produce numbers in the same
>   shape as PERF.md §6 (small drift is fine; substantial drift means
>   you've changed something you shouldn't have).
> - **Do not start Phase 2B kernel optimization.** No matter what
>   numbers come out of these benches, the deliverable is the data, not
>   a fix.
>
> **Cold-state verification (mandatory for the consolidation benchmarks):**
> Before each timed read, the bench must demonstrate that the cache is
> actually cold. Concrete approaches (pick one and document):
> - Per-iteration setup that issues a Spend write at one of the
>   consolidation's child leaves and verifies the consolidated coord
>   is now `dirty()` before timing the read.
> - Per-iteration setup that calls `Cube::rollback_to` against a
>   pre-write snapshot to revert state, bumping revision and clearing
>   caches.
> If the consolidated coord is found NOT dirty at timing-start, the
> bench must `assert!` that fact so a future maintainer cannot
> accidentally measure a warm hit.
>
> **Sanity checks before timing (per category):**
> - Cold consolidation: golden-value match per brief §4.5.1 (same set
>   `consolidated_read.rs` already verifies for the warm path) on the
>   first cold read of each iteration before timing recording.
> - Synthetic no_deps write: the cube has no hierarchies (`assert!` on
>   `dim.default_hierarchy().edges.is_empty()` for every non-Measure
>   dim) AND no derived measures (`assert_eq!(measures with role ==
>   Derived, 0)`). After a write, the dirty set delta is exactly 1
>   (just the written coord; no ancestors, no rev-edges).
> - Snapshot clone: round-trip — take a snapshot, mutate, rollback,
>   read; values must match pre-mutation values. Phase 1B's existing
>   `tests/writeback.rs` and snapshot tests are the model.
> - Hierarchy mark microbench: emit a one-time stderr line per fixture
>   with `dirty_set delta` and `invalidated.len()` so the marginal
>   cost is auditable.
>
> **PERF.md update requirements:**
> Append (do not rewrite) the following sections to `docs/PERF.md`:
> - A new §6.7 "Cold consolidation reads — Phase 2A" with the new
>   table. The §6.3 §11.2 ceiling rows that were marked "not directly
>   comparable" should now be evaluated against the cold numbers.
> - A new §6.8 "Synthetic no-deps write — Phase 2A" reporting the
>   isolated `bench_write_input_leaf_no_deps` against the < 50 µs 1A
>   ceiling (the brief's original intent).
> - A new §6.9 "Snapshot clone" with rows by store cardinality.
> - A new §6.10 "Hierarchy mark cost (microbench)" with rows by
>   hierarchy depth.
> - Update §7 (interpretation), §8 (hot spots), and §9
>   (recommendations) with the new findings. Specifically: collapse
>   the §6.3 banner's deferral note ("Phase 2's first bench-side task")
>   to a "closed in Phase 2A — see §6.7" pointer.
> - Update §10 (behavior change statement) to confirm no
>   `crates/mc-core/src/` files were modified.
>
> **Validation gate before reporting done:**
> Run:
> - `cargo fmt --check --all`
> - `cargo clippy --workspace --all-targets -- -D warnings`
> - `cargo build --release --workspace`
> - `cargo test --workspace` — must still pass 203/203
> - `cargo run --release --bin mc -- demo` — must still match brief §4.6
> - `cargo bench --workspace` — every Phase 1B bench still produces
>   numbers consistent with PERF.md §6 (drift expected; substantial
>   drift requires investigation, not adjustment)
>
> **Completion report format:**
> ```
> DONE: Phase 2A Cold-Path Benchmark Expansion
>
> Build:    [command] ✓/✗
> Format:   [command] ✓/✗
> Lint:     [command] ✓/✗
> Tests:    cargo test --workspace [N]/[N]
> Demo:     target/release/mc demo ✓/✗
> Bench:    cargo bench --workspace ✓/✗
>
> Files changed:
> - list files
>
> Cold consolidation results:
> - 3 leaves Spend (cold):
> - 27 leaves Spend (cold):
> - 27 leaves CPC weighted avg (cold):
> - 27 leaves Revenue rule chain (cold):
> - 420 leaves Spend (cold):
> - Vs 1A ceilings:
>
> Synthetic no_deps write result:
> - measured:
> - vs 1A ceiling 50 µs:
>
> Snapshot clone results:
> - 0 cells:
> - 100 cells:
> - 2520 cells (post-load):
> - materialized:
>
> Hierarchy mark microbench results:
> - 0 hierarchies:
> - 1 hierarchy 1-deep:
> - 1 hierarchy 2-deep:
> - 1 hierarchy 3-deep:
> - marginal cost per ancestor:
>
> Findings:
> - 1-3 sentences per measurement on what the data says
>
> Phase 2B candidates this data justifies (from PERF.md §9):
> - list candidates the new data points at, with magnitudes
>
> Deviations:
> - list any deviations from Phase 2A instructions
> ```
>
> Do not start Phase 2B features.

---

## Context the prompt above does NOT spell out

These are the landmarks the receiving instance will need that the user-facing prompt does not include. Pull them in instead of re-reading the kernel from scratch.

### A. The consolidation cache mechanics — exactly how to force a cold read

[`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs#L526-L563) decides cache hit using:

```rust
let cached_fresh = !self.dirty.is_dirty(coord)
    && self.store.read(coord).map(|s| {
            s.revision == self.revision
                && matches!(s.provenance, Provenance::Consolidation { .. })
        })
        .unwrap_or(false);
if cached_fresh && !request_trace {
    // … return stored value …
}
```

So a consolidated coord is a **cache hit** iff all three are true:
- not in `cube.dirty()`
- store has an entry with `revision == cube.revision`
- the stored entry has `Provenance::Consolidation`

To force a **cold** read for a consolidated coord, the bench's per-iteration setup must invalidate at least one of those. The cleanest paths:

- **Write to a child leaf.** The write path calls `cube.dirty.mark_closure(&req.coord, &self.deps)` and `compute_dirty_ancestors` ([`cube.rs:877-881`](../../crates/mc-core/src/cube.rs#L877-L881)). The Spend writes used by Phase 1B's `dirty_propagation.rs` already invalidate every consolidation that aggregates over that leaf. Issue one such write, assert the consolidated coord is now dirty, then time the read.
- **`Cube::rollback_to(&pre_write_snapshot)`.** [`cube.rs:1011`](../../crates/mc-core/src/cube.rs#L1011) bumps the revision and calls `dirty.clear_all()`, then strips Rule-provenance cells. Consolidated cache entries (by Provenance::Consolidation) survive but their `revision` field is now stale, so the cached_fresh check fails. This is heavier but produces a "fresh build" cold state.

Use `criterion::Criterion::iter_batched_ref` (Phase 1B's pattern) for the per-iteration setup so the timed body excludes the invalidation cost.

### B. mc-fixtures — adding a sibling to `build_acme_cube`

[`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) currently exposes:

```rust
pub fn build_acme_cube() -> Result<(Cube, AcmeRefs), EngineError>
pub fn write_canonical_inputs(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError>
pub fn materialize_all_dependencies(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError>
pub fn coord(cube_id, refs, scenario, version, time, channel, market, measure) -> CellCoordinate
pub fn canonical_inputs_for(time_idx, channel_idx, market_idx) -> CanonicalInputs
```

Add new public functions next to those. Suggested shape:

```rust
pub fn build_minimal_cube() -> Result<(Cube, MinimalRefs), EngineError>
pub fn build_graduated_hierarchy_cube(depth: u8) -> Result<(Cube, GraduatedRefs), EngineError>
```

The `MinimalRefs` / `GraduatedRefs` are sibling `pub struct`s to `AcmeRefs`. Recommended `MinimalRefs` shape: `root_principal: PrincipalId`, `time_dim: DimensionId`, `time_only_element: ElementId`, `measure_dim: DimensionId`, `spend: ElementId`, `cube_id: CubeId` — minimum required to build a coord. Two dims (Time-but-no-hierarchy + Measure) keep the dim_count > 1 (engine expects ≥ 2; check brief §3.5) and let writes go through the same `Cube::write` code path.

For the graduated fixture, parametrize by `depth: u8 ∈ {0, 1, 2, 3}`:
- depth 0: no hierarchy, 1 element on Time
- depth 1: Time has a 1-deep hierarchy (1 leaf → 1 parent)
- depth 2: 1 leaf → 1 parent → 1 grandparent
- depth 3: 1 leaf → 1 parent → 1 grandparent → 1 great-grandparent

Tip: copy `build_acme_cube`'s `Dimension::builder` / `Hierarchy::builder` patterns. The mc-fixtures unit tests in lib.rs's `mod tests` are the model for sanity-checking fixture shape.

### C. iter_batched_ref pattern — reuse Phase 1B's setup helpers

[Phase 1B's `dirty_propagation.rs`](../../crates/mc-core/benches/dirty_propagation.rs) is the closest analogue for what cold consolidation will look like. Its `iter_batched_ref` over `build_materialized()` runs setup once per ~10-iter batch but the timed inner closure pays only the body cost. Use the same shape:

```rust
b.iter_batched_ref(
    || {
        let (mut cube, refs) = build_for_consolidation();   // build + load + materialize
        // Invalidate the consolidation cache for the target coord:
        let leaf_coord = at(&cube, &refs, /* leaf in Q1×Paid_Media×Florida */, ...);
        cube.write(/* Spend = 50_000 at leaf_coord */).expect("write");
        let consolidated_coord = at(&cube, &refs, /* Q1, Paid_Media, Florida, Spend */);
        assert!(cube.dirty().is_dirty(&consolidated_coord),
                "cold-read setup failed: target coord is not dirty");
        (cube, refs, consolidated_coord)
    },
    |(cube, refs, coord)| {
        let v = cube.read(coord, refs.root_principal).expect("read");
        black_box(v);
    },
    BatchSize::SmallInput,
);
```

The dirty-bit assertion is the cold-state verification the prompt requires. Without it, a bench that silently warmed the cache would still pass (and silently report wrong numbers).

### D. Hierarchy ancestor walk — the cost being isolated

[`cube.rs::compute_dirty_ancestors`](../../crates/mc-core/src/cube.rs#L912-L998) is private. It walks the Cartesian product of per-dim `{self} + ancestors_in_default_hierarchy(coord)` × all derived measures. The result is the set of consolidated coords that need dirty-marking after a leaf write.

Because `compute_dirty_ancestors` is not in the public API, you cannot bench it directly without modifying `mc-core/src` (which is forbidden). The graduated-depth fixture approach is the indirect path:

| Fixture | Hierarchies | Derived measures | Expected ancestor walk size |
|---|---|---|---|
| `build_minimal_cube()` | 0 | 0 | 1 (just self) |
| `build_graduated_hierarchy_cube(1)` | 1 dim, depth 1 | 0 | 2 (self + 1 ancestor) |
| `build_graduated_hierarchy_cube(2)` | 1 dim, depth 2 | 0 | 3 (self + 2 ancestors) |
| `build_graduated_hierarchy_cube(3)` | 1 dim, depth 3 | 0 | 4 (self + 3 ancestors) |

The marginal cost per ancestor is `(depth-N timing) - (depth-(N-1) timing)`. With 0 derived measures the inner walk is purely the hierarchy ancestor product, isolated from the "all derived measures" multiplication.

If you also want to measure how derived-measure count amplifies the walk, add a second axis: `build_graduated_hierarchy_cube(depth, derived_count)` — but that's beyond the prompt's scope. Note the option in PERF.md §9 if relevant.

### E. Snapshot internals — what `cube.snapshot()` actually does

[`cube.rs::snapshot()`](../../crates/mc-core/src/cube.rs#L1001-L1009) is a 5-line function:

```rust
pub fn snapshot(&self, label: Option<&str>) -> Snapshot {
    Snapshot {
        cube: self.id,
        revision: self.revision,
        captured_at: 0,
        label: label.map(str::to_string),
        store: self.store.clone(),       // <- the entire cost
    }
}
```

`HashMapStore::clone()` is `AHashMap::clone()` over `(CellCoordinate, StoredCell)`. Each `CellCoordinate` is a fixed-arity SmallVec of ElementIds; each `StoredCell` is owned data with a SmallVec inside `Provenance::Consolidation`. Cloning is O(N) over store size with a per-entry constant of "memcpy a fixed struct."

For the cardinality sweep, use Phase 1B's existing setup helpers from `crates/mc-core/benches/`. A cube with N cells maps cleanly to bench labels:
- 0 cells: fresh `build_acme_cube()` (no inputs written; ~0 cells).
- 100 cells: fresh `build_acme_cube()` + a small loop writing 100 Spend cells.
- 2,520 cells: `build_acme_cube()` + `write_canonical_inputs()` (~2,520 inputs only).
- materialized (~25K cells): `build_acme_cube()` + `write_canonical_inputs()` + `materialize_all_dependencies()` (writes ancestor entries + derived caches).

Rollback should use a separate bench that takes a snapshot at state A, mutates to state B, and times `rollback_to(&snap_at_a)`. The rollback path also calls `dirty.clear_all()` and strips Rule-provenance cells [`cube.rs:1027`](../../crates/mc-core/src/cube.rs#L1027), so its cost is "clone + prune," not just "clone."

### F. The brief §11 ceilings the new benches should be compared against

| Phase 2A bench | Brief §11 row | 1A ceiling | 1B target |
|---|---|---|---|
| Cold consol. 3 leaves | `bench_consolidation_3_leaves` | < 50 µs | < 3 µs |
| Cold consol. 27 leaves Spend | `bench_consolidation_27_leaves` | < 1 ms | < 30 µs |
| Cold consol. 27 leaves Revenue | `bench_consolidation_revenue_27_leaves` | < 5 ms | < 200 µs |
| Cold consol. 27 leaves CPC (weighted avg) | `bench_consolidation_weighted_avg_27` | < 2 ms | < 100 µs |
| Cold consol. 420 leaves | `bench_consolidation_420_leaves` | < 20 ms | < 500 µs |
| Synthetic no-deps write | `bench_write_input_leaf_no_deps` | < 50 µs | < 2 µs |
| Snapshot clone (any) | (not in brief; new) | — | — |
| Hierarchy mark (any) | (not in brief; new) | — | — |

The synthetic-no-deps write is the row Phase 1B documented as a benchmark-scope mismatch. Phase 2A's new fixture lets the < 50 µs 1A ceiling be evaluated as the brief originally intended.

### G. The Phase 1B `Cargo.lock` pins are load-bearing — do not `cargo update`

PERF.md §5 documents that `criterion = "0.5"` works on Rust 1.78 only because three transitive deps are pinned to pre-edition2024 versions: `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Running `cargo update` (without `--precise`) on a fresh checkout will move them forward and break the build. Phase 2A should not run `cargo update` at all. If a NEW transitive dep needs holding back, follow the same `cargo update -p <name> --precise <version>` pattern Phase 1B used and document it in PERF.md §5.

---

## Pointers to existing files you will most likely touch

| Why you might touch it | File | Phase 2A action |
|---|---|---|
| Add cold variants alongside warm rows | [`crates/mc-core/benches/consolidated_read.rs`](../../crates/mc-core/benches/consolidated_read.rs) | extend; do not rewrite warm benches |
| Add `build_minimal_cube` and graduated builders | [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) | add public functions; do not modify existing ones |
| Bench files for new categories | `crates/mc-core/benches/snapshot_clone.rs`, `crates/mc-core/benches/hierarchy_mark.rs`, `crates/mc-core/benches/synthetic_no_deps.rs` (or merge synthetic + hierarchy into one file if they share fixtures) | new files |
| Register new `[[bench]]` entries | [`crates/mc-core/Cargo.toml`](../../crates/mc-core/Cargo.toml) | add entries; do not modify dependency lines |
| Append cold-path tables + interpretation | [`docs/PERF.md`](../PERF.md) | append §6.7–§6.10, update §7 / §8 / §9 / §10 |
| Update operating doc on Phase 2A closure | [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) | flip Deviation #6 (and reframe the §6.3 caveat) once 2A lands |

Files you should **NOT** touch unless a benchmark exposes a clear bug (per the prompt's hard rules):

- Anything in [`crates/mc-core/src/`](../../crates/mc-core/src/) — production behavior is locked.
- [`crates/mc-core/tests/*.rs`](../../crates/mc-core/tests/) — tests are contracts; don't loosen.
- The five Phase 1B bench files **as contracts**: extending is fine, rewriting/removing/renaming is not.
- The locked input documents [`docs/specs/engine-semantics.md`](../specs/engine-semantics.md), [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
- [`rust-toolchain.toml`](../../rust-toolchain.toml) (no toolchain bump).
- Workspace [`Cargo.toml`](../../Cargo.toml) (no dep-version changes; pins live in Cargo.lock).

If you need a new test fixture helper, prefer `mc-fixtures` (precedent established for both `materialize_all_dependencies` in Phase 1A and the criterion bench helpers used in Phase 1B).

---

## Reproducible commands you can rely on

These all exit 0 today on the inherited HEAD (Phase 1B baseline). They are the ground state your work must preserve.

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Phase 1B gate — must remain green throughout Phase 2A
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                 # 203 / 0
cargo run --release --bin mc -- demo   # matches brief §4.6

# Bench gate — pre-2A baseline (Phase 1B numbers)
cargo bench --workspace

# Quick smoke during 2A development (per-bench, sub-second per row)
cargo bench -p mc-core --bench <name> -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10
```

---

## Final checklist before you call Phase 2A done

- [ ] Cold consolidation rows added for all 5 §11.2 ceilings (3, 27 Spend, 27 weighted-avg CPC, 27 Revenue, 420).
- [ ] Each cold consolidation bench `assert!`s the target coord is dirty before timing the first read of the iteration.
- [ ] Golden-value match (brief §4.5.1) verified on the cold path before any timing is recorded.
- [ ] `build_minimal_cube` (or equivalent) lives in `mc-fixtures` with a unit test that asserts no hierarchies, no derived measures, single-cell write produces dirty-set delta == 1.
- [ ] Synthetic no-deps write bench reports a number measurable against the brief's 50 µs 1A ceiling.
- [ ] Snapshot clone bench reports rows for at least 0, 100, 2520, materialized cardinalities. Round-trip integrity test runs once before timing.
- [ ] Hierarchy mark microbench reports rows for graduated depth (0, 1, 2, 3 minimum). Marginal cost per ancestor is computable from the data.
- [ ] All 203 existing tests still pass.
- [ ] All 5 Phase 1B benches still produce numbers consistent with PERF.md §6.
- [ ] `cargo fmt --check --all` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] `docs/PERF.md` extended with §6.7–§6.10; §6.3 banner now points at §6.7 instead of "deferred to Phase 2"; §7 / §8 / §9 / §10 updated.
- [ ] `docs/CURRENT_STATE.md` Deviation #6 closed (or its closure noted with the new measurement).
- [ ] Completion report at [`../reports/phase-2a-completion-report.md`](../reports/phase-2a-completion-report.md), generated from [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md).
- [ ] Completion report posted in chat in the format the prompt specifies.
- [ ] No `crates/mc-core/src/` files modified.
- [ ] No `rust-toolchain.toml` change.
- [ ] **You did not start Phase 2B kernel optimization.**

If you are uncertain at any point, the resolution order is:

1. The Phase 2A prompt above.
2. [`../PERF.md`](../PERF.md) §6 / §7 / §8 / §9 (the Phase 1B baseline + interpretation).
3. [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) (Phase 1A audit).
4. [`../specs/engine-semantics.md`](../specs/engine-semantics.md) and [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
5. [`../../CLAUDE.md`](../../CLAUDE.md).
6. Anything else.

If those still don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.
