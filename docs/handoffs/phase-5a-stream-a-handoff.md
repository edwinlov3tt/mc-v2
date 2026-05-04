# Phase 5A Stream A Handoff — WriteBatch Kernel Performance

> **Audience:** the Claude Code instance running in a git worktree at
> `../mc-v2-stream-a` on branch `phase-5a/stream-a-write-batch`.
> **You inherit Phase 4B** (commit `027f772` on `main`, tag
> `phase-4b-python-adapters` at `b5b6229`, 416/0 tests passing,
> Rust 1.78 toolchain).
>
> **This stream ships `WriteBatch` — the bulk-write performance unlock
> for `mc-core`.** It is the highest-stakes stream in Phase 5A because
> it unlocks the kernel for the first time since Phase 2D. Everything
> else in Phase 5A (recipe format, source drivers, orchestrator) depends
> on this stream's interface being stable and fast.
>
> **Read [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
> BEFORE this handoff.** ADR-0010 is the Accepted strategic gate for
> Phase 5. Focus on Decision 3 (WriteBatch API), Decision 6 (performance
> targets), Decision 11 (mc-core locked-surfaces amendment), and
> Appendix A (Stream A interface contract). Those sections are the
> binding contract for everything in this handoff.
>
> **Hard rule:** Stream A touches `crates/mc-core/` ONLY — and within
> that crate, ONLY the files permitted by ADR-0010 Decision 11. No other
> crate, no other directory. The verification gate is:
> `git diff phase-4b-python-adapters -- crates/mc-core/src/` must show
> changes ONLY in the permitted files.

---

## The one paragraph you must internalize before writing code

**Performance is the moat.** The current `Cube::write` path does a
revision bump, dirty-set update, hierarchy ancestor walk, and listener
fire PER CELL. For 1M cells, that is 1M revision bumps + 1M dirty-set
updates + 1M ancestor walks. `WriteBatch::commit()` amortizes all of
that into ONE revision bump + ONE cumulative dirty-set scan + ONE
listener fire for the entire batch. The amortization IS the speedup.
Connectors are commodity; this bulk-write architecture is proprietary IP.
The target is 50-100x over per-cell writes at 1M+ scale. At that speed,
a quarterly data refresh runs while the planner waits — no overnight
batch job.

---

## ADR-0010 amendments quick-reference (4 affect Stream A)

| # | Amendment | How it shows up in Stream A |
|---|---|---|
| **3** | **Rename `CommitInfo` to `CommitResult`; rename `invalidated_count` to `dirty_count_after` + `newly_dirtied_count`.** Avoids reopening the Phase 2D cumulative-vs-marginal semantic bug. | Your `CommitResult` struct uses `dirty_count_after` (cumulative) and `newly_dirtied_count` (marginal). Never use the word "invalidated" in the new API. |
| **5** | **Tighten WriteBatch atomicity: snapshot captured at `commit()` step 2 (immediately before mutation), NOT at `new()`.** Saves snapshot-clone cost on batches that are staged but never committed (dry-run flows). | `WriteBatch::new()` does NOT clone the store. The `pre_snapshot` field is populated lazily inside `commit()` step 2. |
| **6** | **Parallel-stream Cargo governance: PM/integration branch owns root `Cargo.toml` + `Cargo.lock`.** Streams cannot independently add unapproved deps. | You do NOT touch `Cargo.toml` at the workspace root. You do NOT add any new dependency to `crates/mc-core/Cargo.toml`. |
| **12** | **Stream A must commit measured baselines to PERF.md as FIRST commit before any WriteBatch code exists.** Prevents measuring after-the-fact with churned code. | Your FIRST deliverable (first commit on this branch) is per-cell baselines at 1K/10K/100K/1M. No `batch.rs` until baselines are committed. |

---

## Where Phase 4B ended

- **Commit on main:** `027f772` — *docs(plugin): Phase 4A.1 fix severity-case mismatch*.
- **Phase 4B tag:** `phase-4b-python-adapters` at commit `b5b6229`.
- **Test status:** 416 / 0 passing across all workspace targets. 10/10 deterministic runs.
- **Toolchain:** Rust 1.78 pinned in `rust-toolchain.toml`. Cargo.lock pins from Phase 1B (`clap` 4.4.18, `clap_lex` 0.6.0, `half` 2.4.1) + Phase 3A (`indexmap` 2.7.0, `hashbrown` 0.15.5).
- **mc-core deps (unchanged since Phase 1A):** `smallvec`, `ahash`, `thiserror`, `once_cell` (runtime); `criterion` 0.5 (dev, bench only); `mc-fixtures` (dev).
- **Current per-cell write cost on Acme:** ~160-165 us per `Cube::write()` call (PERF.md section 6.1, row `write_input_leaf`). This includes validation + revision bump + store write + dirty propagation (rule dependents + hierarchy ancestors for 5 derived measures across 3 hierarchical dims).
- **Phase 2D write-cost insight:** the per-write cost on Acme is dominated by the hierarchy ancestor mark walk (~216 cells marked dirty per write), NOT by rule fan-out. The `write_input_leaf_no_deps` row equals `write_input_leaf` on Acme because the hierarchy walk cost dwarfs the rule-dependent cost. On a synthetic minimal-hierarchy fixture, per-write cost drops to ~246 ns.

---

## Phase 5A Stream A prompt (verbatim — this is your contract)

> We are starting Phase 5A Stream A: the WriteBatch kernel performance work in `mc-core`.
>
> **Context.** The Mosaic kernel (`mc-core`) has been locked since Phase 2D. The current `Cube::write()` path costs ~165 us per cell on the Acme fixture (PERF.md section 6.1). At that rate, writing 1M cells would take ~165 seconds. Phase 5A's Tessera ingestion engine needs bulk-write performance to be viable. `WriteBatch` amortizes the per-cell costs (revision bump, dirty-set update, hierarchy ancestor walk) into a single commit operation. ADR-0010 (Accepted 2026-05-04) is the strategic gate; Appendix A is the binding interface contract.
>
> **FIRST DELIVERABLE (Amendment #12 — baselines-first gate):**
>
> Before writing ANY `WriteBatch` code, measure and commit per-cell write baselines at four scale points. This is your FIRST commit on this branch.
>
> 1. Create a new benchmark file `crates/mc-core/benches/baseline_writebatch.rs` that measures the cost of N sequential `Cube::write()` calls on the Acme fixture at scale points: 1K, 10K, 100K, 1M cells. Use the existing `mc_fixtures::build_acme_cube()` + `mc_fixtures::write_canonical_inputs()` to get a materialized cube, then write to distinct leaf coordinates in a loop.
> 2. Run `cargo bench -p mc-core -- baseline_writebatch` and record mean + p99 + variance for each scale point.
> 3. Add a new section to `docs/PERF.md` — "section 6.X Tessera bulk-write baselines (per-cell, pre-WriteBatch)" — documenting the measured numbers with hardware spec.
> 4. Commit this baseline measurement BEFORE `crates/mc-core/src/batch.rs` exists on the branch.
>
> The extrapolated baselines from ADR-0010 Decision 6 (1K ~ 165 ms, 10K ~ 1.65 s, 100K ~ 16.5 s, 1M ~ 165 s) may be wrong — Phase 2D's writeback semantic correction changed the per-write cost profile. Measured numbers override extrapolations.
>
> **SECOND DELIVERABLE (WriteBatch implementation):**
>
> After baselines are committed, implement the WriteBatch API per ADR-0010 Appendix A:
>
> - **New file `crates/mc-core/src/batch.rs`** containing:
>   - `pub struct WritebackContext` — source identification + audit metadata for a bulk import.
>     ```rust
>     #[derive(Debug, Clone)]
>     pub struct WritebackContext {
>         pub source_name: String,
>         pub import_id: String,
>         pub principal: PrincipalId,
>     }
>     ```
>   - `pub struct WriteBatch<'cube>` — stages writes for atomic batch commit. Lifetime-borrows `&'cube mut Cube`.
>     ```rust
>     pub struct WriteBatch<'cube> {
>         cube: &'cube mut Cube,
>         context: WritebackContext,
>         staged: Vec<(CellCoordinate, ScalarValue)>,
>     }
>     ```
>   - `pub struct CommitResult` — summary of a committed batch.
>     ```rust
>     #[derive(Debug)]
>     pub struct CommitResult {
>         pub rows_written: usize,
>         pub rows_failed: usize,
>         pub revision_before: Revision,
>         pub revision_after: Revision,
>         pub dirty_count_after: usize,
>         pub newly_dirtied_count: usize,
>         pub snapshot_id: String,
>     }
>     ```
>   - `impl<'cube> WriteBatch<'cube>` with methods:
>     - `pub fn new(cube: &'cube mut Cube, context: WritebackContext) -> Self`
>     - `pub fn push(&mut self, coord: CellCoordinate, value: ScalarValue) -> Result<(), EngineError>`
>     - `pub fn push_batch(&mut self, cells: &[(CellCoordinate, ScalarValue)]) -> Result<(), EngineError>`
>     - `pub fn staged_count(&self) -> usize`
>     - `pub fn commit(self) -> Result<CommitResult, EngineError>`
>
> - **Modify `crates/mc-core/src/lib.rs`** — add `pub mod batch;` and `pub use batch::{WriteBatch, WritebackContext, CommitResult};`
>
> - **Modify `crates/mc-core/src/cube.rs`** — add `pub(crate)` helper methods that `WriteBatch::commit()` calls for the validated-batch fast path. These are internal methods that bypass per-cell overhead:
>   - A method to write a cell directly to the store (skipping per-cell revision bump, validation already done in batch).
>   - A method to compute the dirty set for all written coordinates in aggregate (one pass over the combined set of affected ancestors, deduplicating).
>   - A method to fire listeners once for the entire batch.
>
> - **New file `crates/mc-core/benches/tessera_writeback.rs`** — benchmark `WriteBatch::commit()` at 1K/10K/100K/1M scale points. Compare against the baselines from the first deliverable.
>
> **Performance gates (ADR-0010 Decision 6):**
>
> | Scale | Target | Baseline (extrapolated) |
> |---|---|---|
> | `write_batch/commit/1K` | <= 10 ms | ~165 ms |
> | `write_batch/commit/10K` | <= 100 ms | ~1.65 s |
> | `write_batch/commit/100K` | <= 1 s | ~16.5 s |
> | `write_batch/commit/1M` | <= 5 s | ~165 s |
>
> **Correctness tests (add to `crates/mc-core/tests/` or inline in `batch.rs`):**
>
> 1. **Snapshot equivalence:** `WriteBatch` commit at N cells produces IDENTICAL cube state (same store contents, same revision) as N individual `Cube::write()` calls for the same data. Test at N = 100 and N = 2520 (full Acme canonical inputs).
> 2. **Rollback correctness:** a `WriteBatch::commit()` that fails mid-apply (simulate by staging a write to a Derived cell after valid writes) leaves cube state UNCHANGED — auto-rollback to pre-commit snapshot.
> 3. **Drop safety:** dropping a `WriteBatch` before calling `commit()` has NO side effects on the cube.
> 4. **Atomicity — validation failure:** if any staged write fails validation (e.g., type mismatch, derived cell), the ENTIRE batch fails — no partial commit. The cube is unchanged.
> 5. **dirty_count_after vs newly_dirtied_count:** after a batch commit, `dirty_count_after` equals the total dirty-set size; `newly_dirtied_count` equals cells that were NOT dirty before this commit but are dirty after.
> 6. **All 416 existing tests pass unchanged.**
>
> **Tier 1 optimization strategy (the amortization insight):**
>
> The current `Cube::write` does these steps PER CELL:
> 1. Permission check
> 2. Cube-id / arity check
> 3. Consolidated-coord rejection
> 4. Derived-measure rejection
> 5. Version-state check
> 6. Lock check
> 7. Intent resolution (Set/Clear/Increment)
> 8. Type check
> 9. NaN/Inf rejection
> 10. Optimistic concurrency (expected_revision)
> 11. **Revision bump** (one per cell)
> 12. **Store write**
> 13. **Dirty propagation** (rule dependents + hierarchy ancestors)
> 14. Soft-lock advisory collection
>
> `WriteBatch::commit()` amortizes steps 11-13:
> - **Single revision bump** for the entire batch (step 11 runs once).
> - **Batched store writes** (step 12 runs N times but without interleaving other work).
> - **Deduplicating dirty propagation** (step 13 computes the UNION of all ancestors for ALL written coords in one pass, avoiding redundant marks).
>
> Steps 1-10 still run per cell during the validation phase (commit step 1). The Tier 1 speedup comes entirely from amortizing 11-13.
>
> **Tier 2 optimization (if time permits after Tier 1 gates pass):**
> - Sort staged writes by coordinate before commit (improves cache locality during store writes).
> - Pre-compute the ancestor set per hierarchical dimension once, then intersect with written coords (avoids redundant hierarchy walks).
> - SIMD-amenable validation pass for bulk type checking.
>
> **NO changes to existing public API signatures.**
> **NO new mc-core dependencies.**
> **NO unsafe (unless SIMD Tier 2 with justification).**
> **NO async, no rayon, no threads.**
> **All 416 existing tests pass unchanged.**

---

## Hard rules — permitted and prohibited files

**Per ADR-0010 Decision 11 — exhaustive.**

### Permitted additions

| File | Action | Notes |
|---|---|---|
| `crates/mc-core/src/batch.rs` | NEW | `WriteBatch`, `WritebackContext`, `CommitResult` types + impl |
| `crates/mc-core/src/lib.rs` | MODIFY | Add `pub mod batch;` + `pub use batch::{WriteBatch, WritebackContext, CommitResult};` |
| `crates/mc-core/src/cube.rs` | MODIFY | Add `pub(crate)` helper methods for the validated-batch fast path |
| `crates/mc-core/benches/baseline_writebatch.rs` | NEW | Per-cell baselines at 1K/10K/100K/1M (first commit) |
| `crates/mc-core/benches/tessera_writeback.rs` | NEW | WriteBatch benchmarks at 1K/10K/100K/1M |
| `crates/mc-core/Cargo.toml` | MODIFY | Add `[[bench]]` entries for the two new bench files ONLY |
| `crates/mc-core/tests/batch_*.rs` | NEW | Correctness tests for WriteBatch |
| `docs/PERF.md` | MODIFY | Add baselines section + WriteBatch results section |

### Prohibited changes (violation = gate failure)

| Prohibition | Why |
|---|---|
| No modifications to `pub fn write()` signature or behavior | Existing API unchanged |
| No modifications to existing public type signatures (`WritebackRequest`, `WritebackResult`, `WriteIntent`, `Cube`, `CubeBuilder`, etc.) | Contract stability |
| No modifications to any existing `#[test]` function | Regression safety |
| No new runtime dependencies in `crates/mc-core/Cargo.toml` | The 4 existing deps are sufficient |
| No `unsafe` without explicit justification in the completion report | Safety contract |
| No `async`, no `rayon`, no threads | Phase 5A is single-threaded |
| No changes to `crates/mc-fixtures/` | Locked |
| No changes to `crates/mc-model/` | Locked |
| No changes to `crates/mc-cli/` | Not Stream A's scope |
| No changes to root `Cargo.toml` or `Cargo.lock` (beyond what `cargo` auto-updates for the new bench entries) | Cargo governance (Amendment #6) |
| No changes to `docs/specs/` | Locked specs |
| No changes to `mosaic-plugin/` | Not Stream A's scope |

### Verification gate

```bash
git diff phase-4b-python-adapters -- crates/mc-core/src/ | grep "^diff --git" | sort
# Must show ONLY:
#   diff --git a/crates/mc-core/src/batch.rs b/crates/mc-core/src/batch.rs
#   diff --git a/crates/mc-core/src/cube.rs b/crates/mc-core/src/cube.rs
#   diff --git a/crates/mc-core/src/lib.rs b/crates/mc-core/src/lib.rs
```

---

## SPEC QUESTION triggers

Open a SPEC QUESTION (per CLAUDE.md section 11) before continuing if any of these surface:

1. **Performance target miss.** If Tier 1 implementation does not meet the 100K <= 1s gate after reasonable optimization, STOP. Document the measured numbers, the bottleneck, and whether Tier 2 or Tier 3 (rayon) would close the gap. Do not add rayon without ADR-0012.

2. **Unsafe needed for SIMD.** If Tier 2 SIMD intrinsics require `unsafe`, document: which intrinsic, why no safe alternative exists, the minimal unsafe surface area, and the `#[cfg]` feature flag gating it. Do not proceed without approval.

3. **Interface contract change needed.** If implementing `WriteBatch` reveals that the Appendix A type signatures need modification (e.g., `commit()` needs an additional parameter, or `CommitResult` needs an additional field), STOP and surface. The interface is frozen; any change requires an ADR-0010 amendment.

4. **Existing test failure.** If ANY of the 416 existing tests fail after your changes, the implementation is wrong. Do not modify existing tests. Debug the implementation.

5. **Rayon temptation.** If you find yourself thinking "this would be 4x faster with rayon for the validation pass" — that is ADR-0012 territory. Phase 5A Stream A is single-threaded. Document the parallelism opportunity in the completion report; do not implement it.

6. **Snapshot clone cost at 1M cells.** If the `store.clone()` at commit step 2 dominates the benchmark budget (e.g., snapshot alone takes > 3s for 1M cells), surface as a performance finding. The mitigation might be copy-on-write, but that is a design decision, not an implementation detail.

7. **The `push()` validation question.** ADR-0010 Appendix A shows `push()` returning `Result<(), EngineError>`. If you determine that `push()` should do NO validation (defer everything to `commit()`), or that it should do FULL validation (expensive per-push), surface the tradeoff. The recommended path: `push()` does coordinate-arity validation only (cheap, catches obvious errors early); `commit()` step 1 does full validation (type check, derived-cell rejection, lock/permission checks).

---

## Context: the current write path's per-cell costs

The existing `Cube::write()` method (at `crates/mc-core/src/cube.rs` line 735) performs these steps for EVERY SINGLE CELL:

1. **Permission check** — `self.permissions.check(...)` — O(grants)
2. **Cube-id / arity check** — two comparisons
3. **Consolidated-coord rejection** — `self.is_consolidated_coord(...)` — walks hierarchy edges
4. **Derived-measure rejection** — `self.measure_at_coord(...)` — hash lookup
5. **Version-state check** — find version dim, look up element state
6. **Lock check** — `self.locks.check_write(...)` — walks lock table
7. **Intent resolution** — match on WriteIntent (Set/Clear/Increment)
8. **Type check** — `measure_meta.dtype.matches(...)` — pattern match
9. **NaN/Inf rejection** — `validate_finite_f64()` — one branch
10. **Optimistic concurrency** — compare expected vs current revision
11. **Revision bump** — `self.revision = self.revision.next()` (AMORTIZABLE)
12. **Store write** — `self.store.write(coord, StoredCell{...})` (fast, but interleaved with other work)
13. **Dirty propagation** — `self.deps.closure_of_dependents(&coord)` + `self.compute_dirty_ancestors(&coord, measure_id)` — THE DOMINANT COST. On Acme, this marks ~216 cells dirty per write (hierarchy ancestors across 3 dims x all derived measures). (AMORTIZABLE via deduplication)
14. **Soft-lock advisory** — `self.locks.soft_locks_covering(...)` — walks lock table again

Steps 11 + 13 are where `WriteBatch` wins. Step 11 goes from N bumps to 1 bump. Step 13 goes from N independent ancestor walks (with massive overlap between nearby coords) to ONE deduplicating pass over the UNION of all affected ancestors.

### The Phase 2D dirty-set insight (marginal vs cumulative)

Phase 2D (commit `0678a98`) fixed a major semantic bug where `WritebackResult.invalidated` was conflated between "cumulative dirty set" and "marginal per-write dirtied cells." The fix pinned `invalidated` to mean MARGINAL (cells dirtied by THIS write only). The pre-2D implementation had O(|cumulative dirty|) cost per write — a super-linear cliff that made 50x writes take 230 seconds.

Phase 2D introduced a bitset-backed `DirtyTracker` with O(1) `is_dirty` checks. The `invalidated` vec now contains only cells that transition clean-to-dirty in THIS write (~216 on Acme), not the entire cumulative dirty set.

`CommitResult` follows this precedent: `newly_dirtied_count` is the marginal set (cells that went clean-to-dirty during this batch); `dirty_count_after` is the total dirty-set size post-commit. Two distinct fields, no ambiguity.

### Tier 1 to Tier 2 optimization progression

| Tier | Technique | Expected speedup | Ships in |
|---|---|---|---|
| **Tier 1** | Single revision bump, batched dirty tracking (deduplicating ancestor union), deferred post-commit bookkeeping | 10-30x over per-cell | Phase 5A Stream A (required) |
| **Tier 2** | Sorted-by-coordinate insertion, pre-computed ancestor sets, SIMD validation | Additional 2-5x on Tier 1 | Phase 5A Stream A (if time permits) |
| **Tier 3** | Bounded parallelism with `rayon` for parse/validate phases, single-writer commit | Additional 4-8x on multi-core | Gated behind ADR-0012 (NOT this stream) |

Start with Tier 1. Measure. If 100K <= 1s and 1M <= 5s pass on Tier 1 alone, Tier 2 is optional polish. If they do not pass, Tier 2 is required before shipping. Tier 3 is NEVER in scope for this stream.

---

## Pointers to existing files

| File | Why you need it |
|---|---|
| `crates/mc-core/src/cube.rs` (line 735) | The existing `write()` method you are amortizing |
| `crates/mc-core/src/cube.rs` (line 966) | `compute_dirty_ancestors()` — the dominant per-cell cost |
| `crates/mc-core/src/dirty.rs` | `DirtyTracker` — bitset-backed, O(1) mark/check |
| `crates/mc-core/src/store.rs` | `HashMapStore` — the concrete storage |
| `crates/mc-core/src/snapshot.rs` | `Snapshot` — holds a `HashMapStore` by value; taking = `store.clone()` |
| `crates/mc-core/src/error.rs` | `EngineError` — all error variants you will return |
| `crates/mc-core/src/lib.rs` | Current public API surface (re-exports) |
| `crates/mc-core/src/coordinate.rs` | `CellCoordinate` — positional slots, `[Scenario, Version, Time, Channel, Market, Measure]` |
| `crates/mc-core/src/value.rs` | `ScalarValue` — the value type you stage |
| `crates/mc-core/src/id.rs` | `Revision`, `PrincipalId`, `CubeId`, `ElementId` |
| `crates/mc-core/benches/writeback.rs` | Existing write benchmarks — see how they set up the Acme cube |
| `crates/mc-fixtures/src/lib.rs` | `build_acme_cube()`, `write_canonical_inputs()`, the Acme fixture |
| `docs/PERF.md` | Current benchmark numbers and measurement discipline |
| `docs/decisions/0010-phase-5-tessera-architecture.md` | The ADR — Appendix A is your interface contract |
| `CLAUDE.md` | Operating manual — especially sections 2.3 (unwrap), 3.1 (forbidden patterns), 6 (self-check) |

---

## Reproducible commands

```bash
cd /Users/edwinlovettiii/Projects/mc-v2-stream-a

source $HOME/.cargo/env

# Pre-work gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                    # 416 / 0
cargo bench -p mc-core                                    # existing benches pass

# Baseline measurement (FIRST deliverable)
cargo bench -p mc-core -- baseline_writebatch

# WriteBatch benchmarks (SECOND deliverable)
cargo bench -p mc-core -- tessera_writeback

# Correctness tests
cargo test -p mc-core -- batch

# Full workspace gate (after all changes)
cargo test --workspace --all-features                     # >= 416 + new tests
cargo clippy --workspace --all-targets -- -D warnings     # zero warnings
cargo fmt --check --all                                   # no diffs

# Forbidden pattern check
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
# Must be zero matches

grep -rn "unsafe" crates/mc-core/src/
# Must be zero matches (unless SIMD Tier 2 with justification)

grep -rn "use serde\|use tokio\|use rayon\|use anyhow" crates/
# Must be zero matches

grep -rn "println!\|eprintln!\|dbg!" crates/mc-core/src/
# Must be zero matches

# Verification gate (Decision 11)
git diff phase-4b-python-adapters -- crates/mc-core/src/ | grep "^diff --git" | sort
# Must show ONLY batch.rs, cube.rs, lib.rs

# Determinism gate
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Locked surfaces
git diff phase-4b-python-adapters -- crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/
# Must be zero output
```

---

## Final checklist before declaring Stream A done

- [ ] Measured per-cell baselines at 1K/10K/100K/1M committed to PERF.md as the FIRST commit on this branch (Amendment #12).
- [ ] `crates/mc-core/src/batch.rs` exists with `WriteBatch`, `WritebackContext`, `CommitResult`.
- [ ] `crates/mc-core/src/lib.rs` has `pub mod batch;` + re-exports.
- [ ] `crates/mc-core/src/cube.rs` has `pub(crate)` helpers for the fast path.
- [ ] `crates/mc-core/benches/baseline_writebatch.rs` exists and runs.
- [ ] `crates/mc-core/benches/tessera_writeback.rs` exists and runs.
- [ ] `Cargo.toml` in `mc-core` has `[[bench]]` entries for both new bench files.
- [ ] Performance gate: `write_batch/commit/100K` <= 1 second.
- [ ] Performance gate: `write_batch/commit/1M` <= 5 seconds.
- [ ] Snapshot equivalence test passes (WriteBatch commit = N individual writes).
- [ ] Rollback correctness test passes (mid-apply failure auto-restores).
- [ ] Drop safety test passes (drop before commit = no side effects).
- [ ] Atomicity test passes (validation failure = no partial commit).
- [ ] `dirty_count_after` + `newly_dirtied_count` semantics test passes.
- [ ] All 416 existing tests pass unchanged.
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] Zero `unwrap()` / `expect()` / `panic!()` in `crates/mc-core/src/`.
- [ ] Zero `unsafe` in `crates/mc-core/src/` (unless SIMD Tier 2 with justification).
- [ ] No new runtime deps in `crates/mc-core/Cargo.toml`.
- [ ] `git diff phase-4b-python-adapters -- crates/mc-core/src/` shows only `batch.rs`, `cube.rs`, `lib.rs`.
- [ ] `git diff phase-4b-python-adapters -- crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/` is empty.
- [ ] PERF.md updated with both baselines AND WriteBatch results.
- [ ] No `async` / `tokio` / `rayon` / threads anywhere in changes.
- [ ] You did NOT modify any existing public API signature.
- [ ] You did NOT modify any existing `#[test]` function.
- [ ] You did NOT modify root `Cargo.toml`.
- [ ] You did NOT start Stream B, C, or D work.

---

## Operating principles

**Baselines first.** Amendment #12 exists because performance claims without a before-measurement are unverifiable. The extrapolated baselines in ADR-0010 may be wrong. Measure the real per-cell cost at scale BEFORE writing optimization code. Your first commit is measurement only.

**Amortization is the insight.** Do not get distracted by micro-optimizations to individual steps (faster hash function, smaller StoredCell, etc.). The 10-30x speedup comes from doing N things once instead of once-per-cell. Single revision bump. Deduplicated ancestor set. One listener fire. That is Tier 1. If Tier 1 passes the gates, ship it. Tier 2 polish is optional.

**Do not fight the borrow checker with unsafe.** `WriteBatch<'cube>` borrows `&'cube mut Cube`. This means you cannot have two `WriteBatch` instances against the same cube simultaneously — that is the CORRECT semantic (exclusive write access). If the borrow checker complains about something inside `commit()`, restructure the code flow rather than reaching for `unsafe`. The single-threaded, single-writer model means the borrow checker is your ally, not your enemy.

**Performance claims require measured evidence.** Every optimization must show a before/after benchmark diff in the same PR. "I believe this is faster because..." is not evidence. `cargo bench` output is evidence. Document hardware, mean, p99, variance.

**The existing write path is your test oracle.** The gold standard for correctness is: "does WriteBatch produce the same cube state as N individual Cube::write() calls?" If the states diverge, WriteBatch is wrong. This is the snapshot equivalence test — it is the most important correctness test in Stream A.

**Do not modify what you do not own.** You own `batch.rs` (new), two bench files (new), and minimal additions to `lib.rs` and `cube.rs`. Everything else in `mc-core/src/` is read-only reference material. If you find a bug in the existing code, surface it — do not fix it in this stream.

---

*Phase 5A Stream A handoff drafted 2026-05-04 immediately after [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) was Accepted (same day, 12 amendments after parallel GPT + Claude Desktop reviews). Stream A is the critical path: Streams B, C, and D all depend on `WriteBatch` being stable and fast. This is the first mc-core unlock since Phase 2D.*
