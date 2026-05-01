# Phase 2A Completion Report

**Project:** MarketingCubes V2 — Rust kernel
**Phase contract:** [`docs/handoffs/phase-2a-handoff.md`](../handoffs/phase-2a-handoff.md)
**Inherited brief:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) (locked; not modified)
**Inherited semantics spec:** [`docs/specs/engine-semantics.md`](../specs/engine-semantics.md) (locked; not modified)
**Operating manual:** [`CLAUDE.md`](../../CLAUDE.md)
**Phase 1A initial commit:** `4aa674a` — *Initial commit: Phase 1 Rust kernel for MarketingCubes V2* (kernel unchanged from this commit)
**Phase 1B baseline tag:** _the `phase-1b-benchmark-baseline` tag (see [`CURRENT_STATE.md`](../CURRENT_STATE.md))_
**Phase 2A initial commit:** _to be tagged after this report is reviewed_
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml); **not bumped**)

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Validation gate | ✓ zero warnings |
| `cargo fmt --check --all` | Validation gate | ✓ no diffs |
| `cargo clippy --workspace --all-targets -- -D warnings` | Validation gate | ✓ exit 0 |
| `cargo test --workspace` | Validation gate (must still be 203/0 + new tests) | ✓ 209 / 0 (203 Phase 1A/1B contract tests + 6 new mc-fixtures unit tests) |
| `cargo run --release --bin mc -- demo` | Validation gate (matches brief §4.6) | ✓ demo output unchanged (kernel-identical to Phase 1A) |
| `cargo bench --workspace` | Validation gate (Phase 1B numbers consistent + Phase 2A new rows) | ✓ — Phase 1B rows within ~10% of [`PERF.md`](../PERF.md) §6 baseline; new rows in §6.7–§6.10 |
| `grep -rn "\.unwrap()\|\.expect(" crates/mc-core/src/` | CLAUDE.md §6.2 | ✓ no new matches in `mc-core/src/` (no files modified) |

The CLI demo run produced the §4.6 output verbatim (kernel
unchanged). The new mc-fixtures unit tests pass deterministically
across 10 consecutive runs (informal check).

---

## 2. Final test count

**Total: 209 tests passed / 0 failed.**

Per target:

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit tests (`cargo test -p mc-core --lib`) | 83 | unchanged from Phase 1A |
| `mc-core` integration `tests/acme_demo.rs` | 20 | unchanged (brief §10.1) |
| `mc-core` integration `tests/writeback.rs` | 11 | unchanged (brief §10.2) |
| `mc-core` integration `tests/consolidation.rs` | 12 | unchanged (brief §10.3) |
| `mc-core` integration `tests/trace.rs` | 9 | unchanged (brief §10.4) |
| `mc-core` integration `tests/dependency.rs` | 7 | unchanged (brief §10.5) |
| `mc-core` integration `tests/locks_permissions.rs` | 8 | unchanged (brief §10.6) |
| `mc-core` integration `tests/correctness.rs` | 16 | unchanged (brief §10.7 + §10.8) |
| `mc-core` integration `tests/hierarchy_cycle.rs` | 10 | unchanged |
| `mc-core` integration `tests/duplicate_elements.rs` | 6 | unchanged |
| `mc-core` integration `tests/coordinate_validity.rs` | 9 | unchanged |
| `mc-core` integration `tests/value_nan.rs` | 8 | unchanged |
| `mc-fixtures` unit tests (Phase 1A: 4 + **Phase 2A: 6**) | 10 | **+6 new** for synthetic fixtures |
| **Total** | **209** | (was 203 in Phase 1B) |

The 6 new mc-fixtures unit tests cover:

1. `build_minimal_cube_has_no_hierarchies_and_no_derived` — invariant audit on the new minimal fixture.
2. `build_minimal_cube_single_write_produces_zero_dirty_delta` — confirms no ancestors, no rev-edges, no derived dirty marks.
3. `build_graduated_hierarchy_cube_zero_depth_matches_minimal_invariants` — depth-0 graduated equals minimal-shape semantics.
4. `build_graduated_hierarchy_cube_depth_three_chain_is_linear` — 3 ancestors visible from the leaf via `Hierarchy::ancestors`.
5. `build_graduated_hierarchy_cube_depth_one_write_dirty_delta_matches_depth` — dirty-set delta = depth at depth=1.
6. `build_graduated_hierarchy_cube_rejects_excessive_depth` — depth > 3 returns `EngineError::Internal`.

---

## 3. Phase 2A bench results

### 3.1 Cold consolidation reads — closes Phase 1B caveat #1

[`PERF.md`](../PERF.md) §6.7 / §7.4. All five brief §11.2 1A
ceilings now pass on real cold reads (cache miss after a
revision-bumping idempotent write, cold-state verified by
`assert!(cube.dirty().is_dirty(&target))` before each timed read):

| Bench | Median | 1A ceiling | 1B target | Status |
|---|---:|---:|---:|:---:|
| `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)` | **14.3 µs** | < 50 µs | < 3 µs | ✓ (1A); ✗ (1B by ~5×) |
| `consolidation_cold/Q1_PaidMedia_Florida/Spend (27 leaves)` | **16.2 µs** | < 1 ms | < 30 µs | ✓ (1A); ✓ (1B) |
| `consolidation_cold/Q1_PaidMedia_Florida/CPC (27 leaves, weighted avg)` | **18.1 µs** | < 2 ms | < 100 µs | ✓ (1A); ✓ (1B) |
| `consolidation_cold/Q1_PaidMedia_Florida/Revenue (27 leaves, rule chain)` | **67.6 µs** | < 5 ms | < 200 µs | ✓ (1A); ✓ (1B) |
| `consolidation_cold/FY_AllChannels_USA/Spend (420 leaves)` | **42.8 µs** | < 20 ms | < 500 µs | ✓ (1A); ✓ (1B) |

The single 1B miss (3-leaf at 14.3 µs vs 3 µs target) is a fixed-cost
floor in `Cube::read_consolidated` — see PERF.md §9.4 for the Phase
2B fix candidate (replace per-call hierarchy clones with borrows /
`Arc`).

### 3.2 Synthetic no-deps write — closes Phase 1B caveat #2

[`PERF.md`](../PERF.md) §6.8 / §7.3. The brief §11.1
`bench_write_input_leaf_no_deps < 50 µs` ceiling is now measurable
on the new `mc_fixtures::build_minimal_cube` fixture:

| Bench | Median | 1A ceiling | 1B target | Status |
|---|---:|---:|---:|:---:|
| `write_input_leaf_no_deps_synthetic` | **246 ns** | < 50 µs | < 2 µs | ✓ ✓ (1A by ~200×; 1B by ~8×) |

The Acme `write_input_leaf_no_deps` row in §6.1 (165 µs) is
reframed: not a kernel ceiling miss, but the cost of the Acme
fixture's Cartesian-product mark walk (215 marks × ~712 ns/mark).
See PERF.md §7.3 + §8.1 for the full decomposition.

### 3.3 Snapshot clone — new diagnostic suite

[`PERF.md`](../PERF.md) §6.9 / §8.3 / §9.5. Round-trip integrity
verified once before timing. Snapshot is sub-linear in cardinality
at Acme scale; rollback is ~3× more expensive than snapshot due to
the prune walk + revision bump + dirty.clear_all combination:

| Bench | Median | Per-cell |
|---|---:|---:|
| `snapshot/0_cells_fresh` | 7.59 ns | (~empty struct constructor) |
| `snapshot/100_cells` | 1.13 µs | ~11 ns/cell |
| `snapshot/2520_cells_loaded` | 29.5 µs | ~12 ns/cell |
| `snapshot/materialized` (~25K cells) | 55.1 µs | ~2.2 ns/cell |
| `rollback/0_cells_fresh` | 370 ns | (per-iter setup mutates one cell) |
| `rollback/100_cells` | 5.49 µs | ~55 ns/cell |
| `rollback/2520_cells_loaded` | 73.7 µs | ~29 ns/cell |
| `rollback/materialized` | 173 µs | ~7 ns/cell |

### 3.4 Hierarchy mark cost — new diagnostic microbench

[`PERF.md`](../PERF.md) §6.10 / §8.1 / §9.3. Marginal cost per
hierarchy ancestor on the synthetic graduated-depth fixture (2 dim,
1 input measure, no derived):

| Bench | Median | dirty_set_delta | Marginal vs prev |
|---|---:|---:|---:|
| `hierarchy_mark/depth_0` | 253 ns | 0 | (baseline) |
| `hierarchy_mark/depth_1` | 438 ns | 1 | +185 ns |
| `hierarchy_mark/depth_2` | 514 ns | 2 | +76 ns |
| `hierarchy_mark/depth_3` | 548 ns | 3 | +34 ns |

**Average marginal cost per ancestor: ~98 ns** on the synthetic
fixture. The Acme per-mark cost is ~712 ns (153 µs ÷ 215 marks);
the ~7× delta is dominated by 6-element `CellCoordinate` SmallVec
allocation + AHashSet insert, **not** by the hierarchy traversal
itself. This refines PERF.md §9.3's optimization options — see §3.5
below.

### 3.5 Phase 2B candidates this data justifies (with magnitudes)

From [`PERF.md`](../PERF.md) §9 (now data-quantified, not guesswork):

1. **§9.4 Consolidation hierarchy-clone hot path** — the 3-leaf cold
   row (14.3 µs vs 1B target 3 µs) localizes a ~14 µs fixed cost in
   `read_consolidated`. Replace per-call dimension/hierarchy clones
   with `&[Dimension]` + `Arc<Hierarchy>`. Trivial source change;
   would unlock the 3-leaf 1B target and improve every higher-leaf
   row by the same ~14 µs constant.
2. **§9.3 Hierarchy mark closure cost reduction** — per-mark
   allocation + AHashSet insert dominates Acme writes (~712 ns/mark
   vs ~98 ns/mark on synthetic). Two paths: (a) lazy ancestor marks
   (behavior shift, needs §10.1 invariant audit), (b) bitset-backed
   dirty tracker keyed by per-dim element index (pure perf change).
   Magnitudes per §6.10 + §8.1.
3. **§9.5 Snapshot COW** — **not justified yet** at Acme scale
   (≤173 µs at 25K cells). Defer until a workflow takes many
   snapshots per turn.
4. **§9.2 `is_consolidated_coord` leaf-flag cache** — still listed
   from Phase 1A; not exercised by Phase 2A's new benches (would
   require a microbench targeting just `is_consolidated_coord`).
5. **§9.6 Recursive rule eval** — still leave alone. The 27-leaf
   Revenue cold row at 67.6 µs (vs 1B target 200 µs) confirms the
   rule chain depth 5 stays well under 1B even on cold reads.

### 3.6 Phase 1B re-runs — drift check

Per the Phase 2A handoff hard rule "All 5 Phase 1B benches must still
run and produce numbers in the same shape as PERF.md §6 (small drift
is fine; substantial drift means you've changed something you
shouldn't have)":

- All Phase 1B rows re-run at sub-10% drift from the §6.1–§6.5
  baseline.
- Notable: `write_input_leaf` ticked 163 µs → 153 µs (−6%, criterion
  flagged "Performance has improved"). This is run-to-run variance
  on Acme; no kernel change. The same mc-core build is in use.
- Warm consolidation rows still ~64–67 ns. ✓
- `dirty_propagation/spend_at_anchor` still 153 µs. ✓
- Demo path rows (build_only, full_demo_reads, full_revenue_slice_warm,
  load_canonical_inputs) within ~1% of baseline. ✓

---

## 4. Deviations from the Phase 2A handoff

**One deviation, surfaced inline as required by CLAUDE.md §11.**

### 4.1 Synthetic no-deps write `dirty-set delta` is **0**, not the handoff's stated **1**

**What the handoff says:** "After a write, the dirty set delta is
exactly 1 (just the written coord; no ancestors, no rev-edges)."
([`docs/handoffs/phase-2a-handoff.md`](../handoffs/phase-2a-handoff.md)
"Sanity checks before timing (per category)" → "Synthetic no_deps
write".)

**What I did:** The mc-fixtures unit test
`build_minimal_cube_single_write_produces_zero_dirty_delta` and the
`synthetic_no_deps.rs::preflight()` both assert `dirty_set delta ==
0` and `WritebackResult.invalidated.is_empty()`. The bench passes
with this stricter invariant.

**Rationale:** The "freshly-written coord is dirty" mental model
disagrees with the kernel's current behavior in two places:

1. [`dirty.rs::mark_closure`](../../crates/mc-core/src/dirty.rs#L42)
   explicitly excludes `root` from the closure: "Does NOT include
   `root` itself — `root` is the freshly-written cell; the values
   that need recompute are the ones that read from it."
2. [`cube.rs::compute_dirty_ancestors`](../../crates/mc-core/src/cube.rs#L912)
   skips the `(leaf, written_measure)` cell at the pure-leaf
   indices: "The cell that was just written — fresh, not dirty."

So with no hierarchies, no rev-edges, and no derived measures, the
dirty-set delta after a write is zero. The handoff's stated "delta
== 1" was an approximation; the engineering-correct invariant is
"no ancestor coords are marked, no rev-edges are marked, no derived
measures are marked, and the freshly-written coord itself remains
clean by §16 I-Dirty-1's interpretation."

Per CLAUDE.md §2.6 (test-fudging) and §11 (communication protocol),
the test asserts the actual invariant, the deviation is surfaced
here, and no kernel code was touched to "make the test pass" —
neither side of the assertion was bent. Phase 2B can revisit the
mental model if a workflow surfaces a need for the written coord
to appear dirty (it currently doesn't — every test in Phase 1B and
Phase 2A passes with the current invariant).

---

## 5. Validation gate — complete

| Command | Required | Status |
|---|---|:---:|
| `cargo fmt --check --all` | exit 0 | ✓ |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | ✓ |
| `cargo build --release --workspace` | zero warnings | ✓ |
| `cargo test --workspace` | 203 existing tests pass + new tests pass | ✓ 209 / 0 |
| `cargo run --release --bin mc -- demo` | matches brief §4.6 | ✓ (kernel-identical) |
| `cargo bench --workspace` | Phase 1B rows still consistent; Phase 2A rows produce data | ✓ (see §3 + PERF.md §6.6–§6.10) |

---

## 6. Phase 2A handoff "Final checklist" — complete

Per [`docs/handoffs/phase-2a-handoff.md`](../handoffs/phase-2a-handoff.md)
"Final checklist before you call Phase 2A done":

- [x] Cold consolidation rows added for all 5 §11.2 ceilings (3, 27 Spend, 27 weighted-avg CPC, 27 Revenue, 420). ✓ §6.7
- [x] Each cold consolidation bench `assert!`s the target coord is dirty before timing. ✓ `force_cold` in [`consolidated_read.rs`](../../crates/mc-core/benches/consolidated_read.rs).
- [x] Golden-value match (brief §4.5.1) verified on the cold path before any timing. ✓ `assert_cold_golden` runs once per cold variant.
- [x] `build_minimal_cube` lives in `mc-fixtures` with a unit test that asserts no hierarchies, no derived measures, single-cell write produces dirty-set delta == 0 (note §4.1 deviation: not 1). ✓ 2 dedicated unit tests.
- [x] Synthetic no-deps write bench reports a number measurable against the brief's 50 µs 1A ceiling. ✓ 246 ns (~200× under).
- [x] Snapshot clone bench reports rows for at least 0, 100, 2520, materialized cardinalities. Round-trip integrity verified once before timing. ✓ 4 snapshot rows + 4 rollback rows.
- [x] Hierarchy mark microbench reports rows for graduated depth (0, 1, 2, 3 minimum). Marginal cost per ancestor computable from data. ✓ ~98 ns/ancestor on synthetic fixture.
- [x] All 203 existing tests still pass. ✓ 203/0 (plus 6 new).
- [x] All 5 Phase 1B benches still produce numbers consistent with PERF.md §6. ✓ ≤10% drift.
- [x] `cargo fmt --check --all` clean. ✓
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean. ✓
- [x] `cargo run --release --bin mc -- demo` still matches §4.6. ✓
- [x] `docs/PERF.md` extended with §6.7–§6.10; §6.3 banner now points at §6.7 instead of "deferred to Phase 2"; §7 / §8 / §9 / §10 updated. ✓
- [x] `docs/CURRENT_STATE.md` Deviation #6 closed (with the new measurement). ✓
- [x] Completion report at [`../reports/phase-2a-completion-report.md`](./phase-2a-completion-report.md), generated from [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md). ✓ (this file)
- [x] Completion report posted in chat in the format the handoff specifies (separate from this file's longer form). ✓ (see chat reply)
- [x] No `crates/mc-core/src/` files modified. ✓ (`git status` confirms)
- [x] No `rust-toolchain.toml` change. ✓
- [x] No new `Cargo.lock` entries beyond Phase 1B's criterion transitive deps. The three Phase 1B pins (`clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`) are intact; no `cargo update` sweep was run. ✓
- [x] **Phase 2B kernel optimization NOT started.** ✓ — the deliverable is the data, not the fix.

---

## 7. Files added / modified in Phase 2A

### Added (new files)

- [`crates/mc-core/benches/synthetic_no_deps.rs`](../../crates/mc-core/benches/synthetic_no_deps.rs) — 1 bench row + preflight invariants.
- [`crates/mc-core/benches/snapshot_clone.rs`](../../crates/mc-core/benches/snapshot_clone.rs) — 8 bench rows (4 snapshot + 4 rollback) + roundtrip integrity check.
- [`crates/mc-core/benches/hierarchy_mark.rs`](../../crates/mc-core/benches/hierarchy_mark.rs) — 4 bench rows + per-depth preflight assertions.
- [`docs/reports/phase-2a-completion-report.md`](./phase-2a-completion-report.md) — *this file*.

### Modified

- [`crates/mc-core/Cargo.toml`](../../crates/mc-core/Cargo.toml) — added 3 new `[[bench]]` entries (`synthetic_no_deps`, `snapshot_clone`, `hierarchy_mark`). No dependency-line changes.
- [`crates/mc-core/benches/consolidated_read.rs`](../../crates/mc-core/benches/consolidated_read.rs) — extended with 5 cold-state variants, cold-state verification helpers, cold-path golden-value preflight, and 3 closed-form golden generators. Warm rows from Phase 1B preserved verbatim.
- [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) — added `build_minimal_cube`, `build_graduated_hierarchy_cube`, `MinimalRefs`, `GraduatedRefs`, `minimal_coord`, `graduated_leaf_coord` + 6 unit tests. No existing function modified.
- [`docs/PERF.md`](../PERF.md) — updated top-of-doc banner (caveats now closure notes), §6.3 banner (cold rows pointer), added §6.6 (drift report), §6.7–§6.10 (4 new tables + interpretation), updated §7.3 (closed in §6.8), §7.4 (warm + cold consolidation), §8.1–§8.3 (data-quantified hot spots), §9.1 (closed), §9.4 (data-justified), §9.5 (data-justified — not yet warranted), §10 (Phase 2A files-changed manifest + behavior-change statement).
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 2A added to "What's shipping"; Deviation #6 closure-noted; bench gate row + test counts updated; Phase 2 follow-ups section trimmed (Phase 2A's measurement work is now done; only Phase 2B optimization candidates remain).

### Not modified (verified)

- `crates/mc-core/src/*.rs` — **no changes.**
- `crates/mc-core/tests/*.rs` — **no changes.**
- `docs/specs/*.md` — **no changes** (locked spec inputs).
- `rust-toolchain.toml` — **not bumped.**
- `Cargo.toml` (workspace) — **no dependency-version changes.**
- `Cargo.lock` — **not regenerated** (no `cargo update`; load-bearing
  pre-edition2024 pins for `clap`, `clap_lex`, `half` preserved).

---

## 8. Known follow-ups for the next phase (Phase 2B)

Phase 2A produced the data. Phase 2B picks the optimizations,
prioritizing from data, not from a wish list.

The candidates surveyed in [`PERF.md`](../PERF.md) §9 are now
data-quantified. In rough priority by expected impact-per-effort:

1. **Consolidation hierarchy-clone hot path** (§9.4) — ~14 µs fixed
   floor in `read_consolidated`, blocks 3-leaf 1B target. Trivial
   source change (`&[Dimension]` borrow + `Arc<Hierarchy>` per dim).
2. **Hierarchy mark closure cost reduction** (§9.3) — ~165 µs per
   write on Acme dominated by per-mark `CellCoordinate` allocation.
   Either lazy ancestor marks (behavior shift; needs §10.1 audit) or
   bitset-backed dirty tracker. Both unlock the Acme `_with_deps`
   1B target (currently 153 µs vs 10 µs target).
3. **Per-dim leaf-flag cache** (§9.2) — minor; would help future
   high-cardinality cubes but Acme isn't bottlenecked here.
4. **Snapshot COW** (§9.5) — defer; not yet justified by data at
   Acme scale.
5. **Toolchain bump revisit** (§9.7) — paired housekeeping; unlocks
   the Phase 2 §10.7 proptest doctrines and insta-driven snapshot
   tests.

Phase 1A follow-ups not addressed by Phase 2A remain open at
[`phase-1-completion-report.md`](./phase-1-completion-report.md) §8;
Phase 1B PERF.md §9 is the more current data-quantified list.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit:

- **No new dependencies.** `mc-core` runtime deps unchanged
  (`smallvec`, `ahash`, `thiserror`, `once_cell`). `mc-core`
  dev-deps unchanged (`mc-fixtures`, `criterion`). No new
  workspace dependencies.
- **No banned imports** (`serde`, `tokio`, `rayon`, `anyhow`) —
  confirmed by `grep -rn 'use serde\|use tokio\|use rayon\|use anyhow' crates/`
  returning zero matches.
- **No `unsafe`** anywhere.
- **No `async fn` / `.await` / threads** introduced.
- **No `Box<dyn Trait>` for storage** — `HashMapStore` still
  concrete; no `CellStore` trait.
- **No `unwrap()` / `expect()` / `panic!()`** in `mc-core/src/`
  (because no `mc-core/src/` file was modified).
- **Locked input contracts unchanged** (`docs/specs/*` not
  modified).
- **Cargo.lock unchanged** (no `cargo update`).
- **`rust-toolchain.toml` unchanged.**

---

*Phase 2A ships as a measurement-only deliverable. Both Phase 1B
caveat banners are now closure notes; every brief §11 1A ceiling is
either passed on the row the brief was describing (cold §11.2;
synthetic §11.1) or already passing on Phase 1B's row. Phase 2B is
unblocked and has data to prioritize from.*
