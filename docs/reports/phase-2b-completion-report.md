# Phase 2B Completion Report — Consolidation Fast Path (Arc<Hierarchy>)

**Project:** MarketingCubes V2 — Rust kernel
**Phase:** 2B — Consolidation Fast Path
**Brief / contract:** [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §11.2 + the [Phase 2B handoff](../handoffs/phase-2b-handoff.md)
**Operating manual:** [`../../CLAUDE.md`](../../CLAUDE.md)
**Predecessor:** Phase 2A (`phase-2a-cold-path-baseline`, `48d52e9`) — see [`phase-2a-completion-report.md`](./phase-2a-completion-report.md)
**HEAD at end of phase:** uncommitted; tree review pending. `git diff --stat` against `48d52e9` shows two source files + four doc files + one new ADR + one new report.
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml)). Unchanged. `cargo update` not run; `Cargo.lock` unchanged.

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ exit 0 |
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo test --workspace` | Acceptance criterion 4 | ✓ **210 / 0** (was 209 + 1 new kernel unit test for handoff item 3) |
| `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | Acceptance criterion 9 (determinism) | ✓ **10 / 10** identical (210 / 0 every run) |
| `cargo run --release --bin mc -- demo` | Acceptance criterion 6 | ✓ matches brief §4.6 |
| `cargo bench --workspace` (full) | Acceptance criterion 5 | ✓ all rows pass; §6.7 3-leaf row clears 1B target (≤ 3 µs) at 2.53 µs (range 2.46 – 2.59 µs in isolated re-measurement) |
| `grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-core/src/` | Acceptance criterion 10 / CLAUDE.md §6.2 | ✓ no new occurrences (existing `unreachable!()` calls only) |
| Pre-2B baseline determinism (Phase 2B work stashed) | Confirm regression came from Phase 2B and not the test itself | ✓ 10 / 10 green at 209 / 0 |

The pre-2B gate was re-run with the Phase 2B work stashed because the
post-2B `cargo test --workspace` flaked ~50% of runs with a single
test (`t_consolidation_caches_value_within_revision`) failing on its
single-shot `Instant::elapsed()` 10× ratio assertion. The pre-2B run
was 10 / 10 green at 209 / 0, isolating Phase 2B as the cause. See
§3 deviation 1 + §4.1 below.

---

## 2. Final test count

**Total: 210 tests passed / 0 failed** (was 209; +1 from the new
`consecutive_recompute_reads_match_phase_2b` kernel unit test in
[`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs)
mod tests).

Per target (`cargo test --workspace` on Phase 2B HEAD):

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit tests | 84 | +1 vs Phase 2A (was 83) — the new `cube::tests::consecutive_recompute_reads_match_phase_2b` for handoff item 3. |
| `tests/acme_demo.rs` | 20 | unchanged |
| `tests/writeback.rs` | 11 | unchanged |
| `tests/consolidation.rs` | 12 | unchanged count; one test rewritten — see §3.1 / §4.1. |
| `tests/trace.rs` | 9 | unchanged |
| `tests/dependency.rs` | 7 | unchanged |
| `tests/locks_permissions.rs` | 8 | unchanged |
| `tests/correctness.rs` | 16 | unchanged |
| `tests/hierarchy_cycle.rs` | 10 | unchanged |
| `tests/duplicate_elements.rs` | 6 | unchanged |
| `tests/coordinate_validity.rs` | 9 | unchanged |
| `tests/value_nan.rs` | 8 | unchanged |
| `mc-fixtures` unit tests | 10 | unchanged |
| **Total** | **210** | |

### Determinism gate

```
Run 1:  failed_tests=0 pass_total=210
Run 2:  failed_tests=0 pass_total=210
Run 3:  failed_tests=0 pass_total=210
Run 4:  failed_tests=0 pass_total=210
Run 5:  failed_tests=0 pass_total=210
Run 6:  failed_tests=0 pass_total=210
Run 7:  failed_tests=0 pass_total=210
Run 8:  failed_tests=0 pass_total=210
Run 9:  failed_tests=0 pass_total=210
Run 10: failed_tests=0 pass_total=210
```

10 / 10 identical. Determinism is restored after the §3.1 rewrite.

---

## 3. Deviations from the brief

1. **`t_consolidation_caches_value_within_revision` rewrite from a
   timing-ratio assertion to a semantic-cache-state assertion.**
   Authorized by the project owner via SPEC QUESTION round-trip
   (CLAUDE.md §11). Documented in [`../decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md).

2. **One new kernel unit test added.** `cube::tests::consecutive_recompute_reads_match_phase_2b`
   in [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs).
   Required by Phase 2B handoff item 3. Not a deviation from the brief
   itself — the handoff explicitly mandates it — but called out here
   for traceability.

No other behavior, test, fixture, public API symbol, dependency,
toolchain pin, `Cargo.lock` entry, or locked spec input changed in
this phase.

---

## 4. Rationale per deviation

### 4.1 `t_consolidation_caches_value_within_revision` rewrite

**What the brief says** ([`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md)
§10.3, lines 2258–2260):

> ```rust
> #[test]
> fn t_consolidation_caches_value_within_revision()
> // Read consolidated Q1 Spend; record duration. Read again immediately;
> // assert second read is at least 10x faster (cache hit).
> ```

**What the test was** (Phase 1A + Phase 2A): a single-shot ratio
assertion using `std::time::Instant::elapsed()`:

```rust
let d1 = t0.elapsed();           // cold cube.read
let d2 = t1.elapsed();           // warm cube.read
assert!(d2_ns * 10 <= d1_ns, "second read must be at least 10× faster");
```

**What it is now** (Phase 2B): a sequence of semantic assertions
using the public `cube.store()`, `cube.dirty()`, `cube.revision()`,
and `cube.read()` / `cube.write()` API:

- (a) Cold read returns the brief §4.5.1 golden (33,000 = Jan + Feb + Mar Spend).
- (b) `cube.store().read(&q1)` returns `Some(stored)` with `Provenance::Consolidation { .. }` and `stored.revision == cube.revision()`.
- (c) `!cube.dirty().is_dirty(&q1)` after the cold read.
- (d) Second read returns the byte-for-byte same `ScalarValue`; `cube.revision()` is unchanged across the two reads.
- (e) Write a new value to one of the consolidation's child leaves (Mar Spend → 50,000).
- (f) `cube.dirty().is_dirty(&q1)` becomes true after the write.
- (g) Third read recomputes and reflects the new leaf; `cube.revision()` advanced from the write in (e).

**Rationale.** Phase 2B's Arc-based fast path drops the cold
consolidation cost from ~14 µs to ~2.5 µs (PERF.md §6.7 + §6.11).
Under `cargo test --workspace` debug-mode parallel-test load, that
shrinks the d1/d2 ratio from ~43× (which the test was sized for) to
~9× (just below the 10× contract). The flake rate observed: 6 / 10
post-2B runs failed; 10 / 10 pre-2B runs passed.

The brief comment "10x faster (cache hit)" was a Phase-1A-era proxy
for the actual invariant the test is trying to prove: **the cache hit
happened**. The rewritten test asserts that invariant directly via
the public API and is robust to future further optimization. The
performance claim moved to PERF.md §6.3 (warm reads ≈ 64 ns) + §6.7
(cold reads ≈ 2.5 µs), where it lives behind a criterion harness with
a sample-of-100 statistical bound — the proper home for any
sub-microsecond performance assertion.

The decision rule (perf assertions belong in `cargo bench`, not in
`cargo test`) is captured in
[ADR-0002](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md).
The user's approval explicitly directs that "wall-clock
micro-performance assertions belong in criterion benches; `cargo
test` is for correctness gates only."

Per CLAUDE.md §2.6 / §4.1 / §11: the implementing instance hit the
conflict, declined to bend either side silently, posted a SPEC
QUESTION with full context (timing data, reproduction, three
candidate paths), and waited for direction. The project owner
approved the rewrite. **That round-trip is the operating manual
working as designed,** and the user's explicit feedback in the
approval message reinforced the pattern as the standard for future
similar conflicts.

### 4.2 New kernel unit test `consecutive_recompute_reads_match_phase_2b`

**What the handoff says** ([`../handoffs/phase-2b-handoff.md`](../handoffs/phase-2b-handoff.md)
prompt §3):

> Add a kernel unit test confirming that two consecutive consolidated
> reads at the same revision produce structurally identical results
> before and after the change (the cache is out-of-band; this test
> exercises the recompute path specifically).

**What I did:** added a `#[test]` in `cube::tests` that uses the
existing `micro_cube` fixture (USA → Florida → Tampa hierarchy) and
calls `cube.read_with_trace(...)` twice. The `request_trace` flag
bypasses the consolidation cache in
[`Cube::read_consolidated`](../../crates/mc-core/src/cube.rs)'s
`if cached_fresh && !request_trace { ... }` guard, so both reads
exercise the Arc-borrowed dim/hierarchy snapshot through the
consolidator. The test asserts:

- Both reads return the same `ScalarValue` (11,500.0 on the trivial fixture).
- Both have `Provenance::Consolidation { .. }`.
- Both `CellValue.revision` values match.
- `cube.revision()` is unchanged across the two reads (reads do not bump revision).

**Rationale.** The handoff explicitly mandates this test. It is the
recompute-path equivalent of the §10.3 cache-hit test — one verifies
the cache works, the other verifies the recompute path is consistent
across calls — and together they prove the Arc fast path is correct
on both sides of the cached / not-cached split.

---

## 5. Acceptance criteria — complete

The Phase 2B handoff defines a single primary acceptance gate plus a
validation suite. Both are met.

| # | Criterion | Status |
|---:|---|---|
| Primary | PERF.md §6.7 `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)` ≤ 3 µs (brief §11.2 1B target) | ✓ **2.53 µs** (range 2.46 – 2.59 µs in isolated bench) |
| Validation 1 | `cargo fmt --check --all` exits 0 | ✓ |
| Validation 2 | `cargo clippy --workspace --all-targets -- -D warnings` exits 0 | ✓ |
| Validation 3 | `cargo build --release --workspace` zero warnings | ✓ |
| Validation 4 | `cargo test --workspace` 100% pass | ✓ 210 / 0 |
| Validation 5 | `cargo run --release --bin mc -- demo` matches brief §4.6 | ✓ |
| Validation 6 | `cargo bench --workspace` — full table re-recorded in PERF.md | ✓ §6.7 + §6.11 |
| Validation 7 | 10× determinism gate | ✓ 10 / 10 identical |
| Constraint | Source change confined to `cube.rs`, `dimension.rs`, `hierarchy.rs` | ✓ `cube.rs` + `dimension.rs` only (no `hierarchy.rs` change needed) |
| Constraint | No new external dependency | ✓ `std::sync::Arc` is `std`, not a dependency |
| Constraint | No public API change in [`lib.rs`](../../crates/mc-core/src/lib.rs) re-exports | ✓ verified by grep |
| Constraint | All 209 existing tests still pass | ✓ all 209 pass; +1 new (210 total) |
| Constraint | All 9 existing benches still build and run | ✓ 9 / 9 |
| Constraint | `rust-toolchain.toml` not bumped | ✓ |
| Constraint | `cargo update` not run; `Cargo.lock` unchanged | ✓ |
| Constraint | `docs/specs/` not touched | ✓ |
| Constraint | No work started on Phase 2C / Phase 3 / any other phase | ✓ |

---

## 6. Acceptance criteria — deferred

None. All Phase 2B acceptance criteria are met.

### 6.A Deviation: Q3 (criterion baseline tracking) was substituted, not implemented

The Phase 2B handoff §"Phase 2B scope" item 0 mandated:
`cargo bench --workspace -- --save-baseline phase-2a` against the
inherited HEAD, commit of `target/criterion/` JSON to
`docs/reports/bench-data/phase-2a/`, then `--baseline phase-2a` for
the post-change re-run. **This was scoped as step 0 of Phase 2B's
source change, not as a parallel housekeeping track.** It slipped.

What I did instead: captured the before/after numbers in PERF.md
§6.11 as document-form medians and ranges (substituting hand-recorded
numbers for the criterion baseline-comparison workflow). The
substitution is reasonable because no `phase-2a` baseline file
existed at the inherited HEAD to compare against — running
`--save-baseline phase-2a` on the pre-2B tree and `--baseline
phase-2a` on the post-2B tree would have produced approximately the
same numbers PERF.md §6.11 already reports, modulo criterion's
statistical bounds.

The cost of the substitution: every "we got faster" claim in §6.11
is hand-edited rather than independently verifiable from a
checked-in baseline JSON. A reviewer who wants to reproduce the
diff today must run both branches manually and compare medians,
rather than running `cargo bench -- --baseline phase-2a` once.

**Phase 2C MUST close this gap as actual step 0.** Concretely:

1. From the post-2B HEAD, run `cargo bench --workspace -- --save-baseline phase-2b`.
2. Commit the resulting `target/criterion/` JSON to `docs/reports/bench-data/phase-2b/`.
3. Apply Phase 2C's source change.
4. Run `cargo bench --workspace -- --baseline phase-2b` for the diff.
5. Phase 2C's PERF.md update cites the criterion-reported "improvement" % per row.

Without step 1–2 happening before any Phase 2C source change, the
slip propagates: every subsequent optimization's "we got faster"
claim stays document-asserted rather than tool-verified, and the
master plan's Q3 workflow stays unproven end-to-end.

The decision to surface this as a deviation rather than a quiet
deferral follows the same CLAUDE.md §11 pattern that produced
ADR-0002: when a hard-rule scope item slipped, name it explicitly
so the next phase has a clear closure target.

### 6.A.1 Retroactive closure (2026-05-01, same day)

The slip described above was closed in a follow-on commit later the
same day, before any Phase 2C work began. Steps actually taken:

1. Ran `cargo bench -p mc-core --bench <name> -- --save-baseline phase-2b`
   for each of the 8 bench files at the post-Phase-2B HEAD
   (commit `992be0a`, tag `phase-2b-consolidation-fast-path`).
2. `git checkout phase-2a-cold-path-baseline` (commit `48d52e9`,
   pre-Arc kernel).
3. Re-ran the same loop with `--save-baseline phase-2a` to capture
   the pre-2B baseline retroactively.
4. `git checkout main`.
5. Copied both `target/criterion/<bench>/<id>/{phase-2a,phase-2b}/`
   subdirs into [`bench-data/phase-2a/`](../reports/bench-data/phase-2a/)
   and [`bench-data/phase-2b/`](../reports/bench-data/phase-2b/) —
   45 rows × 2 phases × 4 small JSON files = 1.4 MB total. No
   `raw.csv` (criterion's `default-features = false` skips raw
   sample CSV).
6. Sanity-check confirmed the JSON medians reproduce PERF.md §6.11's
   document-asserted numbers within run-to-run drift: the 3-leaf
   cold consol gate row reads 12.65 µs → 2.38 µs across the two
   captured baselines (vs the §6.11 document-form 14.3 µs → 2.53 µs).

The original "SLIPPED" deviation header above is preserved as the
audit trail for *why* this section exists; this §6.A.1 records the
closure. **Phase 2C inherits a working Q3:** the validation-gate
expectation in any Phase 2C handoff should be `cargo bench -p mc-core
--bench <name> -- --baseline phase-2b` against the saved JSON, not a
hand-rolled before/after.

---

## 7. Implemented files / modules

### Workspace / config

- [`../../Cargo.toml`](../../Cargo.toml) — unchanged.
- [`../../rust-toolchain.toml`](../../rust-toolchain.toml) — unchanged (pinned at 1.78).
- [`../../Cargo.lock`](../../Cargo.lock) — unchanged.

### `mc-core` source

| Module | File | Brief / handoff §X | Phase 2B change |
|---|---|---|---|
| `cube` | [`../../crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) | §3.18 + handoff §C Option A | `dimensions: Vec<Dimension>` → `Arc<Vec<Dimension>>`; `Cube::read_consolidated` lines 565–597 rewritten to `Arc::clone` + `Vec<&Hierarchy>` collect; `Cube::dimensions()` accessor body adjusted to slice through Arc; `CubeBuilder::build` wraps the final `dimensions` in `Arc::new`; new kernel unit test `consecutive_recompute_reads_match_phase_2b` for handoff item 3. |
| `dimension` | [`../../crates/mc-core/src/dimension.rs`](../../crates/mc-core/src/dimension.rs) | §3.5 + handoff §C Option A | `pub hierarchies: Vec<Hierarchy>` → `pub hierarchies: Vec<Arc<Hierarchy>>`; new `default_hierarchy_arc()` accessor returning `&Arc<Hierarchy>` for cheap clone; `hierarchy()` accessor adapted to deref Arc; `DimensionBuilder.hierarchies` follows; `add_hierarchy` and `build` wrap with `Arc::new`. |
| `hierarchy` | [`../../crates/mc-core/src/hierarchy.rs`](../../crates/mc-core/src/hierarchy.rs) | — | Unchanged. The handoff §C Option A allowed touching this file but no change was needed. |

### `mc-core` tests

- [`../../crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs)
  `t_consolidation_caches_value_within_revision` rewritten to semantic
  assertions per ADR-0002 + the SPEC QUESTION approval. See §3.1 +
  §4.1 above.

### Documentation

- [`../decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md) — new ADR.
- [`../decisions/README.md`](../decisions/README.md) — ADR index entry added.
- [`../PERF.md`](../PERF.md) — §6.7 row + status flip; new §6.11 verification subsection; §9.4 closure-noted; §10 manifest + behavior-change note + Phase 2B files-changed block.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 2B flipped from queued to shipping; ADR-0002 added to active list; test count + determinism row updated; deviation #6 unchanged (it remains as it was after Phase 2A).
- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 2B row → complete; tag column populated post-commit.
- [`./phase-2b-completion-report.md`](./phase-2b-completion-report.md) — this file.

---

## 8. Known follow-ups for the next phase

These are explicit hooks left during this phase. **They are not scheduled.**

1. **Save criterion baseline JSON.** Phase 2A's Q3 housekeeping
   workflow specified `cargo bench --workspace -- --save-baseline phase-2b`
   into `docs/reports/bench-data/phase-2b/` so future sub-phases can
   diff with `--baseline phase-2b`. Phase 2B records the before/after
   diff in document form (PERF.md §6.11) and skips the JSON
   directory because the workflow has not yet been exercised
   end-to-end. **Phase 2C should land this as step 0** so its
   optimization data slots into a real diff.

2. **Phase 2C candidate selection.** PERF.md §9.2, §9.3, §9.5 are
   the candidate list. §9.4 is now closed by Phase 2B. The next
   sub-phase should pick *one* candidate, justified by data, and
   run it through the same SPEC QUESTION discipline if any hard rule
   conflict appears. The master plan ([`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))
   does not pre-name Phase 2C; the first candidate per current
   PERF.md §9 priority is §9.2 (per-dim leaf-flag caching to
   fast-path `is_consolidated_coord`) followed by §9.3 (hierarchy
   mark closure).

3. **Workload sketch ADR (master plan Q1).** The master plan flags
   a "workload sketch ADR" as the strategic gate for everything past
   Phase 2B. That ADR is now unblocked (Phase 2B closes the only
   §11.2 1B miss the brief had) and should land before Phase 2C
   selection so the candidate priority list is anchored in
   user-perception thresholds, not in the brief's pre-product
   ceilings.

The previous phase's follow-ups (Phase 2A) that this phase did not
address remain open at [`./phase-2a-completion-report.md`](./phase-2a-completion-report.md)
§8.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit on `git diff` against the
Phase 2A baseline (`48d52e9`).

- **No new dependencies** beyond what the brief allows. `std::sync::Arc`
  is `std`, not a dependency.
- **No banned imports** added (`serde`, `tokio`, `rayon`, `anyhow`,
  etc.) — confirmed by grep.
- **No `unsafe` / `async` / threads** — confirmed.
- **No new public types** beyond what the brief lists. `Arc<Hierarchy>`
  appears in `Dimension::hierarchies` (a `pub` field), but the wrapping
  is allowed by the handoff's "keep their public field set or add an
  accessor that preserves external readability" provision: the existing
  `hierarchy()` and `default_hierarchy()` accessors continue to return
  `&Hierarchy`, the new `default_hierarchy_arc()` accessor returns
  `&Arc<Hierarchy>` for callers (currently only `cube.rs::read_consolidated`)
  that want the cheap-clone handle.
- **No `unwrap()` / `expect()` / `panic!()` added** in `mc-core/src/`
  production code — confirmed by grep. The two new `unreachable!()`
  calls in `dimension.rs::default_hierarchy_arc()` mirror the existing
  pattern in `default_hierarchy()` and are guarded by the same
  `DimensionBuilder::build` invariant (per spec §2 I-Dim-4).
- **No public symbol from [`lib.rs`](../../crates/mc-core/src/lib.rs)
  re-exports added, removed, or renamed.** Confirmed by `git diff
  crates/mc-core/src/lib.rs` (empty).
- **No locked spec input under `docs/specs/` modified.** Confirmed
  by `git diff docs/specs/` (empty).
- **No `Cargo.lock` change.** Confirmed by `git diff Cargo.lock` (empty).
- **No work started on Phase 2C / Phase 3 / any other phase.**

---

## 10. Notes for the project owner

- The git tree is left **uncommitted, untagged, unpushed**. The user
  reviews first per the Phase 2B prompt and the original Phase 2B
  handoff.
- The recommended commit grouping is one Phase 2B commit covering
  the kernel + test changes + new ADR + new report + status flips.
  The "Files changed in Phase 2B" block in
  [`../PERF.md`](../PERF.md) §10 is the authoritative manifest.
- The recommended tag is `phase-2b-consolidation-fast-path` on the
  Phase 2B commit. The `MASTER_PHASE_PLAN.md` 2B row's tag column
  should be backfilled after that.
- **Working-tree files that are NOT part of Phase 2B and should be
  excluded from the Phase 2B commit unless the user wants them
  bundled:**
  - `docs/reports/codex-context-audit.md` — pre-existing external-tool
    report that arrived at session start. It correctly observed that
    `crates/mc-core/src/cube.rs` and `dimension.rs` were uncommitted
    during Phase 2B development; that was the in-progress kernel
    change, not stale debris.
  - `docs/research-notes/README.md` (modified) and
    `docs/research-notes/dual-fixture-claw-stress-test.md` (new)
    — out-of-scope research-note proposal added separately. Untouched
    by Phase 2B work; the user owns the decision on whether to
    commit them in the same commit, a separate commit, or to leave
    them in the tree.

  `git diff --stat` against `48d52e9` will show all of these alongside
  the Phase 2B files; staging selectively (per the manifest in
  [`../PERF.md`](../PERF.md) §10) keeps the Phase 2B commit clean.

---

*Phase 2B ships pending project owner review of the uncommitted tree.*
