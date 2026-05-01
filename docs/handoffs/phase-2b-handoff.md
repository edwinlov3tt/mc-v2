# Phase 2B Handoff — Consolidation Fast Path

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 2B.
> **You inherit a green Phase 2A.** Your job is **a single targeted
> kernel optimization driven by Phase 2A's data.** Read this whole file
> before touching code.
>
> **What this phase exists for.** PERF.md §6.7's 3-leaf cold
> consolidation benchmark measures 14.3 µs against the brief §11.2 1B
> target of 3 µs. The miss is a single, localized cause: every call to
> [`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs#L526)
> clones `self.dimensions` and each dim's default hierarchy (lines
> 576–582). Eliminating those clones is expected to collapse the
> ~14 µs fixed-cost floor into the sub-µs region and lift every
> higher-fan-out cold consolidation row by approximately the same
> constant. **One file change, one acceptance gate.**

---

## Where Phase 2A ended

- **Last commit:** `phase-2a-cold-path-baseline` (commit `48d52e9` — *bench: complete Phase 2A cold-path benchmark expansion*; backfill `4c11a3c`).
- **Test status:** 209 / 209 passing across all targets. 10/10 determinism gate runs identical.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build, fmt, clippy, test, demo, bench.
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval.**
- **Cargo.lock pins (Phase 1B, still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **The data justifying this phase:** [`docs/PERF.md`](../PERF.md) §6.7 (cold consolidation table) + §9.4 ("Consolidation hierarchy clone — now data-justified").

For the full Phase 2A audit read [`reports/phase-2a-completion-report.md`](../reports/phase-2a-completion-report.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md). Read sections 0, 1, 1.1, 2 (especially 2.6, 2.7, 2.12), 3, 5.1, 5.5, 6, 8, and 12 before writing any code.

The master roadmap is [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 2B is the second sub-phase of Phase 2; do not start Phase 2C+ work in this phase.

---

## Phase 2B prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2B: Consolidation Fast Path.
>
> **Context:**
> Phase 2A measured a 14.3 µs fixed-cost floor in the kernel's
> consolidated read path that misses the brief §11.2 3-leaf 1B target
> (3 µs) by ~5×. The cause is localized to two clones in
> `crates/mc-core/src/cube.rs::read_consolidated` (lines 576–582):
> `self.dimensions.clone()` + `dim.default_hierarchy().clone()` per
> dim, executed on every consolidated read regardless of cache state.
> See [`docs/PERF.md`](../../docs/PERF.md) §9.4 for the data + §7.4
> for the diagnosis.
>
> **Goal:**
> Eliminate the per-call hierarchy/dimension clone in
> `read_consolidated` so cold consolidation reads spend their time
> walking the consolidator tree, not preparing borrowed data.
>
> **Phase 2B scope:**
> 0. **(Phase 2 housekeeping Q3, ~30 min) Set up criterion baseline
>    tracking before any source change.** Run
>    `cargo bench --workspace -- --save-baseline phase-2a` against
>    the inherited HEAD. Copy `target/criterion/` JSON outputs to
>    `docs/reports/bench-data/phase-2a/` and `git add` them (small
>    commit, "bench: capture phase-2a baseline for run-to-run
>    diffs"). After the source change in step 1, run
>    `cargo bench --workspace -- --baseline phase-2a` to get a real
>    before/after diff for §6.11. This step blocks nothing in the
>    source change; do it first because Phase 2B's "we got faster"
>    claim is meaningless without it. See
>    [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md)
>    "Phase 2 housekeeping → Q3" for the rationale.
> 1. Modify `crates/mc-core/src/cube.rs::read_consolidated` so it does
>    not clone `self.dimensions` and does not clone any
>    `dim.default_hierarchy()` per call.
> 2. If the borrow checker resists (it will — that's why the original
>    code clones), wrap each dimension's `Vec<Hierarchy>` in `Arc` (or
>    each individual `Hierarchy` in `Arc`) so the per-call clone
>    becomes a refcount bump rather than a deep clone.
> 3. Add a kernel unit test confirming that two consecutive
>    consolidated reads at the same revision produce structurally
>    identical results before and after the change (the cache is
>    out-of-band; this test exercises the recompute path
>    specifically).
> 4. Re-run the full Phase 2A bench suite using
>    `--baseline phase-2a`. PERF.md must be updated to reflect the
>    new numbers; §6.11 should include criterion's reported
>    "improvement" % per row, not just before/after medians.
>
> **Hard rules:**
> - Source change is confined to `crates/mc-core/src/cube.rs` and
>   (if the chosen approach requires it) `crates/mc-core/src/dimension.rs`
>   and/or `crates/mc-core/src/hierarchy.rs`. No other source file may
>   change.
> - No new external dependency. `Arc` is in `std::sync` — that is fine.
> - No async / threads / rayon / tokio / serde / external storage.
> - No CellStore trait; no HashMapStore rewrite; no snapshot COW.
> - The `Cube` public API (the symbols re-exported from
>   `crates/mc-core/src/lib.rs`) MUST NOT change. Internal helper
>   signatures may. If `Hierarchy` or `Dimension` need to change,
>   keep their public field set or add an accessor that preserves
>   external readability.
> - All 209 existing tests must still pass.
> - All 5 + 4 = 9 existing benches (Phase 1B + Phase 2A) must still
>   build and run.
> - Do not bump `rust-toolchain.toml` without explicit approval.
> - Do not run `cargo update`. The Cargo.lock pins are load-bearing.
> - Do not touch `docs/specs/`. The brief and engine-semantics doc
>   are locked.
> - Do not add fields, methods, or behavior beyond what the
>   optimization strictly requires.
> - Do not start Phase 2C, Phase 3, or any other phase. The
>   deliverable is one targeted change + the bench data verifying
>   it.
>
> **Acceptance gate (the one thing that determines done):**
> The 3-leaf cold consolidation row in PERF.md §6.7
> (`consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)`) must
> measure ≤ 3 µs (the brief §11.2 1B target). If you achieve that,
> the same constant savings should also lift the 27-leaf and 420-leaf
> rows; record those numbers but they are not gating.
>
> **Validation gate before reporting done:**
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (must remain 209 / 0)
> - `cargo run --release --bin mc -- demo` (must match brief §4.6)
> - `cargo bench --workspace` (Phase 2A rows update; new 3-leaf cold
>   row ≤ 3 µs; full table re-recorded in PERF.md)
> - 10 consecutive `cargo test --workspace -q` (still deterministic)
>
> **PERF.md update requirements:**
> - Update §6.7's cold consolidation table with the new median +
>   range numbers. Mark the 3-leaf row's status as ✓ ✓ (1A and 1B
>   both pass) once it does.
> - Add a §6.11 "Phase 2B verification" subsection with a before/after
>   diff for every row that improved.
> - Update §9.4 from "data-justified" to "closed in Phase 2B
>   (commit `<hash>`)" once the gate passes.
> - Update §10's files-changed manifest to include the kernel source
>   files Phase 2B touched.
>
> **Completion report format:**
> ```
> DONE: Phase 2B Consolidation Fast Path
>
> Build:    [command] ✓/✗
> Format:   [command] ✓/✗
> Lint:     [command] ✓/✗
> Tests:    cargo test --workspace [N]/[N]
> Demo:     target/release/mc demo ✓/✗
> Bench:    cargo bench --workspace ✓/✗
>
> Approach chosen:
> - Option A (Arc<Hierarchy>) / Option B (signature refactor) / Option C (other) — describe
>
> Source changes:
> - list files + line counts; should be small
>
> Acceptance gate:
> - 3-leaf cold consolidation: <BEFORE> µs → <AFTER> µs (target ≤ 3 µs)
> - 27-leaf Spend cold:        <BEFORE> µs → <AFTER> µs
> - 27-leaf CPC cold:          <BEFORE> µs → <AFTER> µs
> - 27-leaf Revenue cold:      <BEFORE> µs → <AFTER> µs
> - 420-leaf Spend cold:       <BEFORE> µs → <AFTER> µs
>
> Other Phase 2A rows (drift check):
> - leaf read/write deltas
> - derived read deltas
> - dirty propagation deltas
> - demo path deltas
>
> Phase 2A regressions (must be empty):
> - list any benched row that got slower
>
> Deviations:
> - list any deviations from Phase 2B instructions
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are the landmarks the receiving instance will need that the user-facing prompt does not include.

### A. The exact code being optimized

[`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs#L565-L582), inside `Cube::read_consolidated`:

```rust
// Borrow the hierarchies upfront before we recurse into reads
// (we don't mutate Cube structurally during consolidation;
// dimensions/hierarchies stay constant). We DO mutate via the
// recursive `read_inner` calls in the closure, so we have to be
// careful. The trick: clone the hierarchies (they're Vec/HashMap
// backed; cloning is O(N) per hierarchy but happens once per
// consolidated read, which is acceptable for Phase 1A).
//
// Phase 2 optimization (deferred per §0.A bench gate): cache the
// hierarchy clones at cube-build time, or refactor to pass
// dimension positions instead of references.
let dims_clone: Vec<Dimension> = self.dimensions.clone();
let hierarchies_owned: Vec<Hierarchy> = self
    .dimensions
    .iter()
    .map(|d| d.default_hierarchy().clone())
    .collect();
let hierarchies_refs: Vec<&Hierarchy> = hierarchies_owned.iter().collect();
```

The Phase-2A measurement attributes ~14 µs to the work above. The downstream consumer is `Consolidator::read(coord, &dims_clone, &hierarchies_refs, ...)` (lines 600–608), which reads — but does not mutate — the dim/hierarchy data.

### B. Why those clones exist (the borrow conflict)

Look at lines 589–597:

```rust
let mut read_at_fn = |c: &CellCoordinate| -> Result<ScalarValue, EngineError> {
    let cv = self.read_inner(c, principal, request_trace)?;
    if request_trace { /* ... */ }
    Ok(cv.value)
};
```

`read_at_fn` is a `&mut self`-capturing closure that the consolidator calls recursively. If `Consolidator::read` were passed `&self.dimensions` and `&self.dimensions[i].default_hierarchy()`, those immutable borrows would coexist with `read_at_fn`'s mutable borrow of `self` — the borrow checker (correctly) refuses.

The 1A solution: clone the data so the dimension/hierarchy borrow is independent of `self`. The 2B fix needs to break the dependency without re-introducing the borrow conflict.

### C. Three implementation options

**Option A — wrap each `Dimension`'s hierarchies in `Arc`.**
- `Dimension::hierarchies: Vec<Hierarchy>` becomes `Vec<Arc<Hierarchy>>`.
- `Dimension::default_hierarchy()` returns `&Arc<Hierarchy>` (or `Arc<Hierarchy>` by clone — refcount bump only).
- `read_consolidated` constructs `Vec<Arc<Hierarchy>>` (per-dim refcount bumps; ~ns each) instead of `Vec<Hierarchy>` (per-dim deep clones; ~µs each).
- For dimensions, you still need owned data because of the borrow conflict (option C handles that), OR keep the existing `dims_clone` but recognize that with `Hierarchy` Arc-wrapped, cloning a `Dimension` is now O(elements + Arc-bump) instead of O(elements + hierarchy-edges).
- **Smallest source change.** Touches `dimension.rs`, `hierarchy.rs` (re-export), `cube.rs`. `mc-fixtures` doesn't change because `Hierarchy::builder().build()` can return an `Arc<Hierarchy>` from inside `DimensionBuilder::add_hierarchy` without changing any caller.

**Option B — refactor the consolidator to accept positions / closures, not references.**
- `Consolidator::read` takes a closure that resolves hierarchy data by `DimensionId` rather than holding direct references.
- Avoids the borrow conflict structurally; no Arc.
- Touches `cube.rs` + `consolidation.rs` + every caller of `Consolidator::read`. **Larger source change.** Worse blast radius for an optimization phase.

**Option C — store an `Arc<CubeShape>` alongside `self.dimensions`.**
- New field on `Cube` that holds dimension/hierarchy data in a structure that `read_consolidated` can borrow without holding `&self`.
- Cleanest long-term, but invents a new abstraction. **Out of scope for an optimization phase**; defer until a phase explicitly introduces a separation between cube structure and cube state.

**Recommended path: Option A.** Smallest source change, no new abstractions, keeps the public API intact, predicted to deliver the entire ~14 µs savings because hierarchy-cloning is the dominant cost (a `Dimension` without its hierarchies is mostly element/measure-meta — a relatively cheap clone).

### D. Acceptance gate — the bench row to watch

Look at PERF.md §6.7:

```
| consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves) | 14.3 µs | … | < 50 µs | < 3 µs | ✓ (1A) |
```

Phase 2B is done when that row reads:

```
| consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves) | ≤ 3 µs  | … | < 50 µs | < 3 µs | ✓ ✓     |
```

The other §6.7 rows should improve by approximately the same constant (~13 µs). They are not gating but their numbers are diagnostic — record them.

### E. Phase 2A regression guard

The Phase 2A handoff established that any kernel change must preserve all benched rows from §6.1–§6.10 (with run-to-run drift up to ~10%). After your `cube.rs` change:

- §6.3 warm consolidation rows (~67 ns) should not regress — a warm-cache hit doesn't go through the hierarchy clone path.
- §6.4 dirty propagation should be unchanged (different code path).
- §6.7 cold consolidation rows should improve.
- §6.8 synthetic no-deps write should be unchanged (no consolidation, no hierarchy).
- §6.10 hierarchy mark microbench should be unchanged (different code path).
- §6.5 demo path warm reads should be unchanged or marginally faster.

Any **regression** (a row gets slower beyond noise) on §6.1–§6.6, §6.8–§6.10 is a stop-the-line signal — investigate before recording.

### F. Phase 1's "no Arc" rule — does it still apply?

CLAUDE.md §3.1 lists `Box<dyn Trait>` in the forbidden patterns table. It does NOT forbid `Arc`. The brief §2.5 lists allowed runtime deps but `std::sync::Arc` is not a dependency — it's `std`. No deviation needed.

CLAUDE.md §2.7 ("adding traits 'for testability'") and §1.1 (storage trait deferred) are about traits, not about reference-counted pointers. Arc on `Hierarchy` does not introduce a trait or a dynamic dispatch. Clean.

### G. Tests that may need attention

Phase 2A added `tests/hierarchy_cycle.rs` and the existing `tests/consolidation.rs` (12 cases). Walk those before changing `Hierarchy` so you know which fields callers depend on. Pay particular attention to:

- [`crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs) — every consolidation case must still pass byte-for-byte.
- [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) `t_acme_read_consolidated_*` — Acme golden values must match.
- [`crates/mc-core/src/dimension.rs`](../../crates/mc-core/src/dimension.rs) `mod tests` — DimensionBuilder unit tests.
- [`crates/mc-core/src/hierarchy.rs`](../../crates/mc-core/src/hierarchy.rs) `mod tests` — Hierarchy unit tests.

Phase 2A's mc-fixtures unit tests for the synthetic + graduated cubes must also still pass.

### H. Determinism gate

After the source change, run 10 consecutive `cargo test --workspace -q` to confirm the change did not introduce any nondeterminism (e.g. iteration order over an `Arc`-wrapped collection that was previously cloned).

---

## Pointers to existing files you will most likely touch

| Why you might touch it | File | Phase 2B action |
|---|---|---|
| The optimization site | [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) | replace lines 576–582's clones with the chosen approach |
| `Hierarchy` Arc-wrapping (Option A) | [`crates/mc-core/src/hierarchy.rs`](../../crates/mc-core/src/hierarchy.rs) | optionally re-export `pub type ArcHierarchy = Arc<Hierarchy>;` if it improves readability |
| `Dimension::hierarchies` Arc-wrapping (Option A) | [`crates/mc-core/src/dimension.rs`](../../crates/mc-core/src/dimension.rs) | change the field type and `default_hierarchy()` accessor |
| Update bench numbers + §6.11 verification subsection | [`docs/PERF.md`](../PERF.md) | append §6.11 + update §6.7 + update §9.4 + update §10 |
| Phase 2B completion report | `docs/reports/phase-2b-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flip in master plan + state | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 2B from `proposed` → `complete`, append the tag |

Files you should **NOT** touch:

- `crates/mc-core/src/` — any file other than `cube.rs`, `dimension.rs`, `hierarchy.rs`. Touching anything else is a signal that the scope has crept.
- `crates/mc-core/tests/` — the contract test suite is locked.
- `crates/mc-core/benches/` — extending PERF.md does not require new bench code; the existing files are the regression guard.
- `crates/mc-fixtures/src/lib.rs` — the public fixtures are a shared contract with benches and tests.
- `docs/specs/` — locked.
- `rust-toolchain.toml` — pinned.
- workspace `Cargo.toml` — pinned.

---

## Reproducible commands you can rely on

These all exit 0 today on the inherited HEAD (`phase-2a-cold-path-baseline`).

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-2B gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 209 / 0
cargo run --release --bin mc -- demo    # matches brief §4.6
cargo bench --workspace                 # establishes pre-2B baseline

# Quick smoke during 2B development (per-bench, sub-second per row)
cargo bench -p mc-core --bench consolidated_read -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10
```

---

## Final checklist before you call Phase 2B done

- [ ] Single chosen approach implemented (Option A / B / C from §C above), with the choice documented in §4 of the completion report.
- [ ] Source change is confined to `cube.rs` (+ at most `dimension.rs`, `hierarchy.rs`).
- [ ] No public symbol from `crates/mc-core/src/lib.rs` removed or renamed.
- [ ] No new external dependency.
- [ ] All 209 tests still pass.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] **Acceptance gate met:** the 3-leaf cold consolidation row measures ≤ 3 µs.
- [ ] No Phase 2A bench row regressed beyond run-to-run noise (~10%).
- [ ] PERF.md §6.7 updated with new numbers; §6.11 verification subsection added; §9.4 closure-noted; §10 manifest updated.
- [ ] Completion report at `docs/reports/phase-2b-completion-report.md`.
- [ ] Completion report posted in chat in the format the prompt specifies.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 2B from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 2C / 3 / any later phase.**

If you are uncertain at any point, the resolution order is:

1. The Phase 2B prompt above.
2. [`../PERF.md`](../PERF.md) §6.7 + §9.4 (the data justifying this phase).
3. [`../reports/phase-2a-completion-report.md`](../reports/phase-2a-completion-report.md).
4. [`../specs/engine-semantics.md`](../specs/engine-semantics.md), [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
5. [`../../CLAUDE.md`](../../CLAUDE.md).
6. [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md).
7. Anything else.

If those still don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.
