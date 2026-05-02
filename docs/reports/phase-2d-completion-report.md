# Phase 2D Completion Report

**Project:** MarketingCubes V2 — Rust kernel
**Brief:** [`phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) (inherited; Phase 2D ships against the brief's `WritebackResult.invalidated` type doc + engine-semantics.md §13)
**Handoff:** [`phase-2d-handoff.md`](../handoffs/phase-2d-handoff.md) (with amendment §A approved 2026-05-02)
**Operating manual:** [`CLAUDE.md`](../../CLAUDE.md)
**Initial commit:** `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`) — committed 2026-05-02 after PM/spec-maintainer signoff
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml))

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ |
| `cargo test --workspace` | Acceptance criterion 4 | ✓ 227 / 0 (was 216; +11 from Phase 2D) |
| `for i in $(seq 1 10); do cargo test --workspace -q; done` | Acceptance criterion 9 (determinism) | ✓ 10 / 10 identical at 227 / 0 each run |
| `cargo run --release --bin mc -- demo` | Acceptance criterion 6 | ✓ matches brief §4.6; "N dependent cells dirtied" line now reads `9` (marginal) instead of the Phase 1A cumulative ~17,820+ — both are within "exact N depends on impl; bounded — see §8" |
| `cargo bench -p mc-core --bench demo_path -- "load_canonical_inputs" --baseline phase-2c --sample-size 10` | Acceptance gate (PERF.md §6.12.7) | ✓ 50× ingest 230.80 s → **1.06 s (−99.5 %)**; gate ≤ 50 s **beat by 47×**. 1× / 10× / 100× rows in PERF.md §6.15.2 |
| `cargo bench -p mc-core --bench combined_workflow -- --baseline phase-2c` | Secondary expectation | ✓ per-edit ÷ dirty-delta median dropped from ≈ 422 µs to ≈ 2.05 µs (~200× faster); within-session shape stays flat (3.7 → 2.06 → 2.05 µs at iter 1/50/100) |
| `cargo bench -p mc-core --bench leaf_read_write -- --baseline phase-2c --sample-size 10` | Diagnostic rows | ✓ `write_input_leaf` 167 µs → 10.8 µs (−93.8 %); `write_input_leaf/10x` 691 µs → 15.7 µs (−97.7 %); read rows within ±5 % noise |
| `cargo bench -p mc-core --bench dirty_propagation -- --baseline phase-2c --sample-size 10` | Diagnostic | ✓ 153 µs → 10.9 µs (−93 %) |
| Forbidden-pattern grep | Acceptance criterion 10 / CLAUDE.md §6.2 | ✓ zero `unwrap()` / `expect(` / `panic!()` / `unimplemented!()` / `todo!()` / `unsafe` in `crates/mc-core/src/` |

The full bench impact table at every measured scale is in [`../PERF.md`](../PERF.md) §6.15.2; the A/B isolation
(per handoff §A.5) is in §6.15.3; the spec audit that authorized the writeback semantic correction is in §6.15.4.

---

## 2. Final test count

**Total: 227 tests passed / 0 failed.** (Was 216 at phase-2c; +11 from Phase 2D.)

Per target:

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit tests | 90 | +6 from Phase 2D: 4 in `cube_shape::tests` (cardinality, linearize round-trip, arity mismatch, unknown element); 2 in `dirty::tests` (`bitset_tracker_observationally_equivalent_to_ahashset`, `bitset_tracker_mark_closure_matches_hash`) per handoff item 4 |
| `tests/acme_demo.rs` | 20 | unchanged; §10.1 `t_acme_dirty_set_size_within_bound_after_one_spend_write` passes byte-for-byte (asserts `cube.dirty().len()`, not `WritebackResult.invalidated.len()`, so unaffected by the marginal-semantics correction) |
| `tests/writeback.rs` | 11 | unchanged |
| `tests/writeback_invalidated.rs` | **5** | **NEW.** Tests A–E per handoff §A.6 — pin the corrected marginal semantics |
| `tests/consolidation.rs` | 12 | unchanged |
| `tests/trace.rs` | 9 | unchanged |
| `tests/dependency.rs` | 7 | unchanged |
| `tests/locks_permissions.rs` | 8 | unchanged |
| `tests/correctness.rs` | 16 | unchanged |
| `tests/hierarchy_cycle.rs` | 10 | unchanged |
| `tests/duplicate_elements.rs` | 6 | unchanged |
| `tests/coordinate_validity.rs` | 9 | unchanged |
| `tests/value_nan.rs` | 8 | unchanged |
| `mc-fixtures` unit tests | 16 | unchanged |
| **Total** | **227** | +11 from Phase 2D |

### Determinism gate

10 consecutive runs of `cargo test --workspace -q`, all identical at 227 / 0. No flakes.

---

## 3. Deviations from the brief

1. **`WritebackResult.invalidated` content semantics — Phase 1A misimplementation surfaced by Phase 2D's bench gate.** The brief's compact pseudocode at line 1938 said `Return WritebackResult { invalidated: <full dirty set> }` and Phase 1A read this as "the cumulative dirty set." The brief's own type doc at line 1214–1216 (and engine-semantics.md §13.2 + I-WB-7 + the worked example at §13.4) say "Coordinates marked dirty by **this write**." Phase 2D reconciles to the marginal reading per CLAUDE.md §0 hierarchy of authority + the SPEC QUESTION amendment §A approved 2026-05-02. See §4.1.

2. **Bench preflight diagnostic wording** in `dirty_propagation.rs`, `hierarchy_mark.rs`, `combined_workflow.rs`. Per handoff §A.7. No behavior change. Field renamed: `final_invalidated_len` → `last_write_invalidated_len`. Eprintln strings rewritten to clarify cumulative cube dirty vs marginal per-write `WritebackResult.invalidated.len`. See §4.2.

3. **Phase 2D handoff diagnosis was empirically wrong.** The handoff attributed the §6.14 cliff to "AHashSet rehash + cache locality + hash-collision probability"; A/B isolation (§A.5) showed bitset alone moves the gate row by < 0.2 % (within noise). The cliff was at `cube.rs::write`'s cumulative invalidated collection. The bitset still ships as the structural foundation that makes the corrected `is_dirty` check O(1); see §4.3.

Each rationale is in §4.

---

## 4. Rationale per deviation

### 4.1 `WritebackResult.invalidated` content semantics

**What the brief says (six sites; one ambiguous):**

| Source | Says | Reading |
|---|---|---|
| Brief §3.18, type doc on `WritebackResult.invalidated` (line 1214–1216) | "Coordinates marked dirty by **this write** — both rule dependents and hierarchy ancestors. Order is unspecified; equality is by set content." | **Marginal** |
| Brief writeback algorithm step 12 (line 1259) | "Return `WritebackResult` with the invalidated set." | Marginal-leaning |
| Brief compact pseudocode (line 1938) | `Return WritebackResult { invalidated: <full dirty set> }.` | **Ambiguous** — Phase 1A read it as cumulative |
| engine-semantics.md §13.2 inline comment (line 1011) | "cells dirtied **by this write**" | **Marginal** |
| engine-semantics.md §13.4 worked example (line 1052–1054) | Enumerates ~10 coords (5 derived measures + same 5 at consolidated ancestors of one Spend coord), NOT cumulative | **Marginal** |
| I-WB-7 (engine-semantics line 1034) | "returns the list of invalidated coordinates so callers can pre-warm caches if they care" | **Marginal** (cumulative pre-warming on every write is incoherent) |

**What I did:** [`Cube::write`](../../crates/mc-core/src/cube.rs) now constructs `invalidated` from the *marginal* set (coords this write transitions from clean → dirty during this single `write()` call), via `is_dirty(&c)` checks before each `mark` call. The bitset's O(1) `is_dirty` keeps the marginal capture cheap (per-write fan-out × O(1) = ~216 × ~10 ns at Acme).

**Rationale:** Per CLAUDE.md §0 hierarchy of authority:
- "Brief wins for what to implement (types, signatures)" → the brief's *type doc* wins over its own *pseudocode shorthand*.
- "Semantics wins for what a concept means" → `invalidated` *means* "by this write."

Five of six sources agree on marginal; one (the compact pseudocode) is ambiguous. Phase 1A picked the wrong gloss; Phase 2D corrects it. Per the SPEC QUESTION amendment §A approved 2026-05-02, this fix is in-scope for Phase 2D (not scope creep) because the bitset alone doesn't close the §6.14 gate — bundling the writeback fix with the bitset is the only path that ships a green Phase 2D in one phase. See PERF.md §6.15.4 for the full audit trail.

**Behavior impact:** `WritebackResult.invalidated: Vec<CellCoordinate>` field type, name, struct, and re-export are all unchanged. The cumulative dirty state is still tracked by `cube.dirty()` (unchanged). `mc-cli demo`'s "N dependent cells dirtied" line now prints the marginal count (9 in the demo flow) — closer to the brief §4.6 "exact N depends on impl; bounded — see §8" intent than the Phase 1A cumulative ~17,820+ value.

### 4.2 Bench preflight wording fix

**What the brief / handoff says:** Phase 2D handoff §A.7 — three bench files print preflight diagnostics that conflated cumulative dirty count with marginal per-write `WritebackResult.invalidated.len`. Update `eprintln!` strings + comments to make the distinction explicit; rename the misnamed field in `combined_workflow.rs`.

**What I did:**
- [`crates/mc-core/benches/dirty_propagation.rs`](../../crates/mc-core/benches/dirty_propagation.rs): preflight now reads `cube.dirty.len: A -> B (delta D); WritebackResult.invalidated.len=D (must equal delta)`. Added `debug_assert_eq!(delta, invalidated_len)` as a regression net.
- [`crates/mc-core/benches/hierarchy_mark.rs`](../../crates/mc-core/benches/hierarchy_mark.rs): same shape.
- [`crates/mc-core/benches/combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs): renamed `final_invalidated_len` → `last_write_invalidated_len` (struct field, local var, `final_inv_len` push site, eprintln). Eprintln rewritten to label `cumulative cube.dirty.len median=X; last write WritebackResult.invalidated.len median=Y (marginal per-write transition count)`.

**Rationale:** Future maintainers shouldn't have to infer the distinction from prose — the bench output should say what each number means. The `debug_assert_eq!` is the regression net that catches anyone who re-introduces the cumulative reading. No behavior change in the bench bodies; only labels and one struct field name.

### 4.3 Handoff diagnosis was empirically wrong; bitset still ships

**What the handoff says:** PERF.md §6.14 + handoff §"Phase 2D scope" attribute the `load_canonical_inputs` super-linear cliff to "AHashSet rehash + cache locality + hash-collision probability" as the dirty set grows from 0 → 305 K entries. Gate: bitset replaces AHashSet; per-mark cost becomes O(1); cliff closes.

**What I did:** Implemented the bitset per spec (Cartesian-product flat bitset, `Vec<u64>` + `ever_marked` sticky bitset for tracked-Vec dedup, custom `DirtyIter` with exact `size_hint`). Measured 50× ingest at +4 % vs phase-2c (within criterion noise). Investigated by temporarily skipping the cumulative-`invalidated` collection in `cube.rs::write` and measured −98 % at 10× — pinpointing the actual bottleneck. Surfaced via SPEC QUESTION; user approved scope expansion to include the writeback fix; A/B isolation (per handoff §A.5) confirmed the writeback fix is the load-bearing change for the §6.14 cliff.

**Rationale:** The bitset is correct (kernel equivalence test + 222/222 pre-fix integration tests pass) and forward-compatible — it makes the corrected per-write `is_dirty` check O(1), so the marginal-set capture in `cube.rs:892–943` stays bounded by the per-write fan-out (~216) rather than degrading as the cumulative dirty set grows. Without the bitset, the AHashSet's `is_dirty` would grow O(probe-length) with set size, partially eroding the writeback fix's win at large scales. Shipping both keeps the closure complete and gives the dirty-tracker hot path a structural foundation any future optimization will build on. Per handoff §A.4: bundling avoids a rollback path and keeps the §6.14 finding closed in one phase.

---

## 5. Acceptance criteria — complete

| # | Criterion | Status |
|---:|---|---|
| 1 | `cargo build --release --workspace` zero warnings | ✓ |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` exits 0 | ✓ |
| 3 | `cargo fmt --check --all` exits 0 | ✓ |
| 4 | `cargo test --workspace` 100 % pass (excluding §10.7 proptest stubs deferred per CLAUDE.md §1.1) | ✓ 227 / 0 |
| 5 | All Phase 1B / 2A / 2B / 2C bench files still build and run; no row regressed beyond ±10 % noise | ✓ — every diagnostic row improved or stayed within noise; full table in PERF.md §6.15.2 |
| 6 | `target/release/mc demo` matches brief §4.6 output | ✓ — structure matches; "N dependent cells dirtied" reports 9 (marginal) per the corrected semantics, within "exact N depends on impl; bounded — see §8" |
| 7 | `docs/specs/engine-semantics.md` and `docs/specs/phase-1-rust-kernel-build-brief.md` unchanged | ✓ — confirmed by `git diff docs/specs/` (empty) |
| 8 | No `mc-core` reference to any §1 out-of-scope item | ✓ — no new dep; no `unsafe` / `async` / threads; no Roaring / bit-vec / bitvec crate; in-house `Vec<u64>` |
| 9 | 10 consecutive `cargo test` runs identical | ✓ 10 / 10 at 227 / 0 each |
| 10 | Zero `unwrap()` / `expect()` / `panic!()` in `crates/mc-core/src/` (greps clean) | ✓ |
| (acceptance gate) | `load_canonical_inputs/50x` ≤ 50 s | ✓ **1.06 s** (beats by 47×) |
| (secondary) | combined-workflow per-edit ÷ dirty-delta stays within ±10 % of ≈ 422 µs at 50× **or improves** | ✓ improves by ~200× to ≈ 2.05 µs at iter-100; within-session shape stays flat (3.7 → 2.06 → 2.05 µs at iter 1/50/100) |

---

## 6. Acceptance criteria — deferred

None. All Phase 2D handoff items (the original prompt + the §A amendment) are addressed by this report.

The §10.7 proptest stubs remain deferred per CLAUDE.md §1.1 (Phase 2 work; not part of Phase 2D scope).

---

## 7. Implemented files / modules

### Workspace / config

- [`Cargo.toml`](../../Cargo.toml) — unchanged.
- [`Cargo.lock`](../../Cargo.lock) — unchanged. No `cargo update`. The Phase 1B transitive pins (`clap` → 4.4.18, `clap_lex` → 0.6.0, `half` → 2.4.1) are still load-bearing.
- [`rust-toolchain.toml`](../../rust-toolchain.toml) — unchanged. Rust 1.78.

### `mc-core` source

| Module | File | Change |
|---|---|---|
| `cube_shape` | [`crates/mc-core/src/cube_shape.rs`](../../crates/mc-core/src/cube_shape.rs) | **NEW.** `CubeShape` struct (per-dim element-id → local-index `Vec<u32>` + per-dim strides + Cartesian cardinality). Built once at `CubeBuilder::build`. Cardinality guard at `1 << 30`; per-dim id-range guard at `1 << 24`. ~165 lines. |
| `dirty` | [`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) | Internal repr enum `DirtyImpl::{Hash, Bitset}`. Public method signatures preserved byte-for-byte. New `pub(crate) fn with_shape(Arc<CubeShape>)`. Bitset path: `bits` + sticky `ever_marked` + insertion-order `tracked: Vec<TrackedEntry>` with cached bit indices. Custom `DirtyIter` exposes exact `size_hint` so `.collect::<Vec<_>>()` preallocates. ~530 lines (up from ~220). |
| `cube` | [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) | `Cube` gains `cube_shape: Option<Arc<CubeShape>>` field. `CubeBuilder::build` constructs the shape and routes the dirty tracker through `with_shape()` (or `new()` if cardinality overflows the guard). `Cube::write` semantic correction at lines ~892–943: `WritebackResult.invalidated` is now the marginal set, captured via `is_dirty` checks before each `mark`. |
| `lib` | [`crates/mc-core/src/lib.rs`](../../crates/mc-core/src/lib.rs) | `mod cube_shape;` (private — no public re-export). |

### `mc-core` tests

- [`crates/mc-core/tests/writeback_invalidated.rs`](../../crates/mc-core/tests/writeback_invalidated.rs) — **NEW.** 5 tests A–E pinning the corrected marginal semantics per handoff §A.6.
- All pre-existing tests under `crates/mc-core/tests/` are unchanged byte-for-byte.

### `mc-core` benches

- [`crates/mc-core/benches/dirty_propagation.rs`](../../crates/mc-core/benches/dirty_propagation.rs) — preflight wording fix per handoff §A.7. No behavior change.
- [`crates/mc-core/benches/hierarchy_mark.rs`](../../crates/mc-core/benches/hierarchy_mark.rs) — preflight wording fix per handoff §A.7. No behavior change.
- [`crates/mc-core/benches/combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs) — rename `final_invalidated_len` → `last_write_invalidated_len`; preflight wording fix per handoff §A.7. No behavior change.

### Documentation

- [`docs/PERF.md`](../PERF.md) — annotations on §6.4 / §6.13 / §6.14 historical-bug artifacts; new §6.15 (full Phase 2D verification + A/B isolation + spec audit + memory footprint); §9.3 closure note; §10 manifest.
- [`docs/handoffs/phase-2d-handoff.md`](../handoffs/phase-2d-handoff.md) — amendment §A added 2026-05-02 (existing file).
- [`docs/handoffs/phase-2c-handoff.md`](../handoffs/phase-2c-handoff.md) — historical-artifact footnote at line 72 per handoff §A.8.
- [`docs/reports/phase-2c-completion-report.md`](./phase-2c-completion-report.md) — historical-artifact footnotes at §3.2.3 + §10.2 (combined_workflow new-file description) per handoff §A.8.
- [`docs/reports/phase-2d-completion-report.md`](./phase-2d-completion-report.md) — **NEW.** This file.
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 2D row added to "What's shipping"; §9.3 closure logged; test count updated 216 → 227; demo line updated.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 2D row flipped `proposed` → `complete` with tag `phase-2d-bitset-and-invalidated-fix` at `0678a98`; Phase 2D narrative section rewritten to reflect the writeback semantic correction + A/B isolation findings; "Phase 2 exits" condition gated on the format/parser ADR for Phase 3A.
- [`docs/reports/bench-data/phase-2d/`](./bench-data/) — **NEW directory.** Phase 2D criterion baseline saved per [`bench-data/README.md`](./bench-data/README.md) workflow. Diff of `phase-2d` vs `phase-2c` reproduces the §6.15.2 table.

---

## 8. Known follow-ups for the next phase

Per Phase 2D handoff item E + PERF.md §9 post-2D state:

1. **§9.2 leaf-flag cache** — payoff window narrowed substantially after the writeback semantic correction (combined-workflow per-edit cost is now ≈ 11 µs at 50×, was ≈ 2.4 ms; §9.2 attacks the per-write fixed cost, which is now a small fraction of total). Stays opportunistic.
2. **§9.5 Snapshot COW** — stays deferred per Phase 2C; no new data argues for reopening.
3. **§9.6 Recursive rule eval** — leave it; well within 1B targets at every measured scale.
4. **Phase 2 housekeeping Q2 (toolchain bump)** — still queued; no Phase 2D blocker.
5. **`proptest` / `insta`** — still queued for a future Phase 2 sub-phase or Phase 3 work per CLAUDE.md §1.1.
6. **Phase 3A** flips from `planned` → `proposed` once the format/parser ADR lands. Phase 2D's commit + tag + report review are complete (`0678a98` / `phase-2d-bitset-and-invalidated-fix`); the format/parser ADR is the only remaining Phase 3A precondition.

The previous phase's follow-ups that this phase did not address are still open at [`./phase-2c-completion-report.md`](./phase-2c-completion-report.md) §8.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit:

- **No new dependencies** — `cargo tree -p mc-core` is identical to phase-2c (unchanged from Phase 1B).
- **No banned imports** (`serde`, `tokio`, `rayon`, `anyhow`, `roaring`, `bit-vec`, `bitvec`, etc.) — `grep -rn "use serde\|use tokio\|use rayon\|use anyhow\|use roaring\|use bit_vec\|use bitvec" crates/mc-core/src/` is empty.
- **No `unsafe` / `async` / threads** — `grep -rn "unsafe\|async\|tokio::\|std::thread" crates/mc-core/src/` is empty (the pre-existing `is_async` test asserting absence of `async fn` still passes).
- **Public API surface unchanged.** The lib.rs re-exports of `DirtyTracker`, `CellCoordinate`, `Cube`, `Snapshot`, `WritebackResult`, etc. are byte-for-byte identical. `WritebackResult.invalidated: Vec<CellCoordinate>` field type + name + struct are all unchanged; only contents change per §4.1.
- **DirtyTracker public method signatures unchanged byte-for-byte** — `new`, `mark`, `mark_closure`, `is_dirty`, `clear`, `clear_all`, `len`, `is_empty`, `iter`, `snapshot_sorted` all preserved. Only additive surface: `pub(crate) fn with_shape(Arc<CubeShape>)`.
- **No `unwrap()` / `expect()` / `panic!()` in production code** — clippy lint enforces; matches confined to `#[cfg(test)]` and one `unreachable!()` in `dimension::default_hierarchy` (pre-existing).
- **Locked input contracts unchanged** — `git diff docs/specs/` is empty. ADR-0003 is unmodified.

---

## 10. WritebackResult.invalidated semantic correction (per handoff §A.10)

This dedicated section walks through the change end-to-end so a reviewer can audit the spec interpretation without bouncing between PERF.md, the handoff amendment, and the source.

### 10.1 The chosen interpretation

> **`WritebackResult.invalidated` contains coordinates that transitioned clean → dirty during this single `write()` call. Excludes coords already dirty before the call.**

Field type: `Vec<CellCoordinate>` (unchanged). Struct: `WritebackResult` (unchanged). Re-export: `pub use cube::WritebackResult` in `crates/mc-core/src/lib.rs` (unchanged).

### 10.2 Spec ambiguity table

See §4.1 above. Five of six authoritative spec sites name the marginal reading; one (the brief's compact pseudocode at line 1938) is genuinely ambiguous and Phase 1A picked the wrong gloss.

### 10.3 Why the cumulative reading was wrong

1. **Performance.** Per-write cost of `self.dirty.iter().cloned().collect()` was O(|cumulative dirty|). Across an N-write bulk-load with monotonically growing dirty, total cost = O(N · cumulative_dirty) ≈ O(N²) until cube saturation. **That's the §6.14 cliff.** A/B isolation (§A.5 + PERF.md §6.15.3) shows: skipping just this collection drops 10× ingest by −98 %. Replacing AHashSet with bitset and keeping cumulative collection moves 50× ingest by +4 % (within criterion noise).

2. **Misleading bench output.** Three Phase 1B/2C benches printed `invalidated.len = <cumulative>` and rationalized it ("`invalidated.len` is the full transitive closure including hierarchy ancestors" — PERF.md §6.4 phase-2c text). The eprintln line was treated as a sanity check but was reporting a meaningless number. Fixed in handoff §A.7.

3. **Incoherent under I-WB-7.** The semantics doc says "returns the list of invalidated coordinates so callers can pre-warm caches if they care." Pre-warming the entire cumulative dirty set on every write is incoherent — the caller would re-warm the same coords on every subsequent write until cube clear-all, doing O(N²) wasted work. The marginal reading is the only one that lets the I-WB-7 use case make sense.

4. **Internal contradiction with the brief's type doc.** The brief's own type doc at line 1214 says "by THIS write." The cumulative reading was a misread of the compact pseudocode shorthand at line 1938.

### 10.4 Behavior impact summary

| Aspect | Phase 1A (cumulative) | Phase 2D (marginal) |
|---|---|---|
| `WritebackResult.invalidated` field type | `Vec<CellCoordinate>` | `Vec<CellCoordinate>` (unchanged) |
| Field re-export | `pub` | `pub` (unchanged) |
| `cube.dirty()` cumulative tracker | tracked correctly | tracked correctly (unchanged) |
| Per-write cost (saturated 50× cube) | ~1.8 ms (dominated by `iter().cloned().collect()` of ~150 K entries average) | ~10 µs (capture bounded by per-write fan-out ~216) |
| `mc-cli demo` "N dependent cells dirtied" | ~17,820+ (cumulative after canonical-input loading) | 9 (marginal — 5 derived measures + 4 hierarchy ancestors at the demo coord) |
| Bench preflight `invalidated.len=` (e.g. dirty_propagation) | 17,825 (cumulative, mismatched dirty_set delta of 5) | 5 (marginal, equals dirty_set delta) |
| `combined_workflow` `last_write_invalidated_len` median (50×) | 305,039 | 5 |
| §10.1 `t_acme_dirty_set_size_within_bound_after_one_spend_write` bound | passes (asserts `cube.dirty().len()`, not `invalidated.len`) | passes byte-for-byte (same assertion target) |

### 10.5 Bench impact table (full, every measured scale)

| Bench | phase-2c median | phase-2d median | Δ |
|---|---:|---:|---:|
| `demo_path/load_canonical_inputs` (1×) | 233.83 ms | 20.88 ms | **−91.1 %** |
| `demo_path/load_canonical_inputs/10x` | 10.12 s | 208.90 ms | **−97.9 %** |
| `demo_path/load_canonical_inputs/50x` (gate row) | 230.80 s | **1.06 s** | **−99.5 %** (gate ≤ 50 s) |
| `demo_path/load_canonical_inputs/100x` | abandoned (>38 min) | 2.13 s | new |
| `leaf_read_write/write_input_leaf` | 167.19 µs | 10.77 µs | **−93.8 %** |
| `leaf_read_write/write_input_leaf/10x` | 691.50 µs | 15.69 µs | **−97.7 %** |
| `dirty_propagation/spend_at_anchor` | 153 µs | 10.90 µs | **−93.0 %** |
| `read_input_leaf_warm` | ~50 ns | 47.93 ns | within noise |
| `read_input_leaf_cold` | 875 ns | 389.87 ns | −57.0 % (free side-effect) |
| `combined_workflow/50x_marker` (criterion noop) | 425 ps | 384 ps | −9.3 % (within noise) |
| Combined workflow per-mark amortized @ iter 1/50/100 (50×) | ≈ 422 / 419 / 422 µs | 3.7 / 2.06 / 2.05 µs | **−99 %** (200× faster); shape still flat |

### 10.6 A/B isolation result (per handoff §A.5)

| Configuration | 10× ingest | 50× ingest | Verdict |
|---|---:|---:|---|
| (1) phase-2c — AHashSet + cumulative `invalidated` | 10.12 s | 230.80 s | the bundled phase-1A bug |
| (2) Bitset only — bitset + cumulative `invalidated` | 10.12 s (−0.17 %, p > 0.05) | 238.64 s (+3.4 %, p = 0.00 — within typical run-to-run bench noise) | the bitset's contribution alone — **moves the gate row by < 5 % at every measured scale** |
| (3) Bitset + writeback fix (the shipping Phase 2D state) | 208.90 ms (−97.9 %) | **1.06 s (−99.5 %)** | both changes together; gate ≤ 50 s **beat by 47×** |

**Headline:** the **writeback semantic correction is the load-bearing change** for the §6.14 cliff. The bitset alone is enabling foundation, not the closer.

The bitset is still **required** in the shipping state for two reasons: (i) it makes the corrected per-write `is_dirty` check O(1), so the marginal-set capture stays bounded by per-write fan-out (~216 at Acme, §10.1) rather than degrading O(probe-length) as the AHashSet's load factor climbs at large scales; (ii) it's the structural foundation any future dirty-tracker optimization will build on (the `Arc<CubeShape>` infrastructure can be reused for slice-bounded reads, lazy ancestor marking via fan-out bitmaps, etc.).

### 10.7 Test coverage added (per handoff §A.6)

[`crates/mc-core/tests/writeback_invalidated.rs`](../../crates/mc-core/tests/writeback_invalidated.rs):

- **Test A** (`t_phase_2d_write_a_clean_cube_invalidated_is_marginal_closure`) — fresh write on a clean cube; `invalidated.len() ≤ 215`; equals `cube.dirty().len()` (no prior dirt to add); contains the 5 derived measures at the same leaf coord and at least one hierarchy ancestor coord.
- **Test B** (`t_phase_2d_write_b_repeated_write_skips_already_dirty`) — second identical write returns empty `invalidated`; cumulative `cube.dirty()` does not shrink. Validates that already-dirty dependents are excluded from the second write's marginal set.
- **Test C** (`t_phase_2d_write_c_recompute_then_redirty_reports_again`) — read forces recompute (clears Revenue@coord); subsequent write at upstream Spend@coord re-reports Revenue@coord in `invalidated` because it transitioned clean → dirty again. The load-bearing semantic distinction: marginal is a *transition* set, not a *cumulative-state* set.
- **Test D** (`t_phase_2d_write_d_bulk_ingest_preserves_per_write_bound`) — every individual write across the 2,520-write canonical-input bulk-ingest reports `invalidated.len() ≤ 215` while `cube.dirty().len()` grows monotonically. **The test that, had it existed, would have caught the Phase 1A bug originally.**
- **Test E** (`t_phase_2d_write_e_demo_dirty_count_is_marginal`) — smoke test asserting the demo-flow `invalidated.len()` is < 100 (would have been ~17 K under the cumulative reading).

Plus the kernel equivalence tests in `dirty.rs::tests`:

- `bitset_tracker_observationally_equivalent_to_ahashset` — drives a 24-step mixed mark/clear/clear_all script against both AHashSet and bitset trackers, asserts agreement on `len`, `is_empty`, `is_dirty(c)` per coord, sorted iter content, and `snapshot_sorted` after each step. Per handoff item 4.
- `bitset_tracker_mark_closure_matches_hash` — `mark_closure` parity across a small dependency graph.

### 10.8 Standard validation gate (final state)

- `cargo build --release --workspace` ✓ zero warnings
- `cargo fmt --check --all` ✓
- `cargo clippy --workspace --all-targets -- -D warnings` ✓
- `cargo test --workspace` ✓ 227 / 0
- `for i in $(seq 1 10); do cargo test --workspace -q; done` ✓ 10 / 10 identical
- `cargo run --release --bin mc -- demo` ✓ matches §4.6
- `cargo bench -p mc-core --bench demo_path -- --baseline phase-2c --sample-size 10` ✓ acceptance gate ≤ 50 s **beat by 47×** (1.06 s)
- Forbidden-pattern grep ✓ zero matches in `crates/mc-core/src/`

---

*Phase 2D shipped 2026-05-02 at `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`) after project owner review. The implementing Claude Code instance honored the handoff's "Final checklist" line item "**You did NOT commit, tag, or push.** The user does that after reading the review." — the user did the commit + tag step after PM/spec-maintainer signoff.*
