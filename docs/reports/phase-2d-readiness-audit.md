# Phase 2D Readiness Audit

Date: 2026-05-02
Repository: MarketingCubes / `mc-v2`
Auditor: Codex second engineering reviewer
Scope: checkpoint after Phase 2C and before Phase 2D implementation.

This audit inspected the repository state at `bc70ad2d9a5261083caef051a806c0ae084d796a`
on branch `main`. It did not change Rust source, tests, benchmarks, or kernel behavior.

## 1. Executive Summary

Current phase according to the repo: Phase 2D is proposed/scoped, not implemented. The latest
HEAD is `bc70ad2` (`docs: queue Phase 2D - Bitset-Backed Dirty Tracker (Section 9.3)`), which adds
`docs/handoffs/phase-2d-handoff.md` and updates roadmap/handoff docs after the Phase 2C tag.

What is complete:

- Phase 1A kernel slice is present and working: cube, dimensions, hierarchies, rules,
  consolidation, dependency graph, dirty tracking, snapshots, locks, permissions, demo, and tests.
- Phase 1B benchmarking harness is present in Criterion benches and `PERF.md`.
- Phase 2A cold-path benchmark expansion is present and tagged.
- Phase 2B consolidation fast path is implemented, tagged, tested, and documented.
- Phase 2C workload-shaped benchmark baseline is tagged, with fixture/benchmark/report artifacts.

What is next: Phase 2D should remain a scoped implementation phase targeting the dirty-tracker /
bulk-ingest hotspot, but the handoff and benchmark docs need cleanup before coding starts.

Readiness: the repo is ready to scope Phase 2D, and Phase 2D is already scoped at HEAD. It is not
clean enough to start implementation without a small documentation/measurement cleanup pass.

Phase 3 readiness: not ready to start Phase 3. Phase 2D is still open, Phase 3A still depends on a
parser/model-definition ADR, and the Phase 2 benchmark story should be made internally consistent
first.

Major red flags:

- Phase 2C combined-workflow docs report per-mark costs as about `422 ns`, but the benchmark output
  divides about `2110 us` by a `dirty_delta` of `5`, which is about `422,000 ns` or `422 us` per
  reported mark. The trend is still useful, but the unit/denominator claim is wrong or unclear.
- Several Phase 2C docs and bench-data READMEs still describe artifacts as uncommitted,
  prospective, TODO, or present when the current repo has tags and different files.
- The Phase 2D handoff has an API ambiguity around preserving `DirtyTracker::new()` while also
  saying the constructor becomes `DirtyTracker::with_shape(...)`.

## 2. Repo State

- Branch: `main`
- HEAD: `bc70ad2d9a5261083caef051a806c0ae084d796a`
- HEAD subject: `docs: queue Phase 2D - Bitset-Backed Dirty Tracker (Section 9.3)`
- Phase 2C tag target: `789db15c720f1e541f52a41576c83f9c800d92f8`
- Initial worktree state before this audit report: clean
- Files changed by this audit: `docs/reports/phase-2d-readiness-audit.md`

Tags present:

```text
phase-2a-cold-path-baseline
phase-2b-consolidation-fast-path
phase-2c-workload-baseline
```

Recent commits:

```text
bc70ad2 docs: queue Phase 2D - Bitset-Backed Dirty Tracker (Section 9.3)
96cca75 docs: backfill Phase 2C commit hash in CURRENT_STATE + roadmap tag column
789db15 bench: complete Phase 2C workload-shaped benchmark baseline
f73c168 docs(handoff): add Phase 2C - Production-Shaped Workload Benchmarks
31e355f docs: accept ADR-0003 with provisional status + sunset clause
d5f92b3 docs(PERF): add Section 9 Phase 2C provisional-priority note + Section 6.12 stub
b5141ca docs(phase-2b-report): append post-commit acceptance note above body
e642489 docs(CURRENT_STATE): finalize Phase 2 closure state
9f7420c bench: capture phase-2a + phase-2b criterion baselines (close Q3)
992be0a docs: backfill Phase 2B commit hash in CURRENT_STATE + roadmap tag column
6ea58ab bench: Phase 2B consolidation fast path (Arc<Hierarchy>)
0b27db1 docs: roadmap tweaks (status legend, perception thresholds, Phase 2 housekeeping)
30e1e84 docs: queue Phase 2B (consolidation fast path) handoff
4c11a3c docs: backfill Phase 2A commit hash in CURRENT_STATE + roadmap tag column
48d52e9 bench: complete Phase 2A cold-path benchmark expansion
```

Transient file assessment:

- No uncommitted transient files were present before this report was created.
- `docs/reports/phase-2d-handoff-scaffold.md` was deleted after the Phase 2C tag and replaced by
  `docs/handoffs/phase-2d-handoff.md`; docs still pointing at the deleted scaffold need updates.
- Bench-data directories are committed artifacts, not transient files, but their README metadata is
  stale and should be corrected.

## 3. Phase Status Verification

### Phase 1A

Status in roadmap/current state: complete.

Expected deliverables:

- Rust workspace with core kernel crate, CLI demo crate, and fixtures crate.
- Cube model, dimensions, hierarchies, elements, rules, consolidation, dependency tracking,
  dirty tracking, snapshots, locks, permissions, and demo path.
- Correctness tests covering Acme semantics and edge cases.

Actual files present:

- `crates/mc-core/src/lib.rs`
- `crates/mc-core/src/cube.rs`
- `crates/mc-core/src/dimension.rs`
- `crates/mc-core/src/hierarchy.rs`
- `crates/mc-core/src/cell.rs`
- `crates/mc-core/src/coordinate.rs`
- `crates/mc-core/src/consolidation.rs`
- `crates/mc-core/src/dependency.rs`
- `crates/mc-core/src/dirty.rs`
- `crates/mc-core/src/store.rs`
- `crates/mc-core/src/snapshot.rs`
- `crates/mc-core/src/permission.rs`
- `crates/mc-core/src/lock.rs`
- `crates/mc-fixtures/src/lib.rs`
- `crates/mc-cli/src/main.rs`
- `crates/mc-core/tests/*.rs`

Test/bench evidence:

- `cargo test --workspace` passed with 216 tests.
- `cargo run --release --bin mc -- demo` passed and produced the expected Acme demo behavior,
  including derived reads, consolidated reads, dependency invalidation after writeback, and rejected
  derived/consolidated writes.

Deviations still active:

- `crates/mc-core/src/lib.rs` has stale module-level wording saying Phase 1 deferred modules include
  rule, dependency, dirty, consolidation, cube, slice, permission, lock, and snapshot. Those modules
  now exist and are exported.

Stale or contradictory docs:

- Minor: the lib crate doc comment is stale relative to actual implementation.

### Phase 1B

Status in roadmap/current state: complete.

Expected deliverables:

- Criterion benchmark harness for Phase 1 behavior.
- Baseline performance docs in `docs/PERF.md`.
- Performance assertions kept in benches/docs rather than tests, per ADR-0002.

Actual files present:

- `crates/mc-core/benches/leaf_read_write.rs`
- `crates/mc-core/benches/derived_read.rs`
- `crates/mc-core/benches/consolidated_read.rs`
- `crates/mc-core/benches/dirty_propagation.rs`
- `crates/mc-core/benches/demo_path.rs`
- Bench registrations in `crates/mc-core/Cargo.toml`
- `docs/PERF.md`
- `docs/decisions/0002-perf-assertions-in-benchmarks-not-tests.md`
- `docs/handoffs/phase-1b-handoff.md`

Test/bench evidence:

- Criterion bench files exist and compile through clippy/build gates.
- Phase 1B results are documented in `PERF.md`.

Deviations still active:

- There is no standalone Phase 1B tag in `git tag --list`; later docs treat Phase 1B as bundled
  into subsequent baseline history. This is not blocking, but it should remain explicit.

Stale or contradictory docs:

- `PERF.md` still carries older baseline metadata in places and should be treated as a cumulative
  benchmark journal rather than a single current-state source.

### Phase 2A

Status in roadmap/current state: complete.

Expected deliverables:

- Cold-path benchmark expansion.
- Synthetic no-dependency write bench.
- Snapshot clone bench.
- Hierarchy mark bench.
- Fixture support for minimal and graduated hierarchy cubes.
- Bench-data artifact folder and completion report.

Actual files present:

- `crates/mc-core/benches/synthetic_no_deps.rs`
- `crates/mc-core/benches/snapshot_clone.rs`
- `crates/mc-core/benches/hierarchy_mark.rs`
- Fixture builders in `crates/mc-fixtures/src/lib.rs`
- `docs/reports/bench-data/phase-2a/`
- `docs/reports/phase-2a-completion-report.md`
- Tag `phase-2a-cold-path-baseline`

Test/bench evidence:

- Smoke bench `synthetic_no_deps` ran successfully in this audit.
- `cargo test --workspace` passed.

Deviations still active:

- None active in code.

Stale or contradictory docs:

- Some Phase 2A report language is historical and predates tag backfills. It is not blocking if
  current-state docs remain accurate.

### Phase 2B

Status in roadmap/current state: complete.

Expected deliverables:

- Consolidation fast path using shared hierarchy references instead of repeated deep clones.
- No semantic behavior change.
- Tests remain green.
- Bench-data and completion report.

Actual files present:

- `crates/mc-core/src/cube.rs`
- `crates/mc-core/src/dimension.rs`
- `docs/reports/bench-data/phase-2b/`
- `docs/reports/phase-2b-completion-report.md`
- `docs/handoffs/phase-2b-handoff.md`
- Tag `phase-2b-consolidation-fast-path`

Test/bench evidence:

- `cargo test --workspace` passed.
- Phase 2B bench-data exists and `PERF.md` documents the consolidation cold-path improvement.

Deviations still active:

- None active in code.

Stale or contradictory docs:

- Some roadmap text still mentions the Phase 2B acceptance snapshot as uncommitted. That is stale
  after the tag.

### Phase 2C

Status in roadmap/current state: complete/tagged, with Phase 2D now proposed at HEAD.

Expected deliverables:

- Production-shaped scaled Acme fixtures.
- Combined workflow benchmark.
- Isolated scaled workload benches.
- Bench-data capture for Phase 2C.
- ADR-0003 workload sketch accepted provisionally.
- No core kernel source or test behavior changes during the measurement phase.

Actual files present:

- `crates/mc-fixtures/src/lib.rs`
- `crates/mc-core/benches/combined_workflow.rs`
- Existing isolated bench files extended for scaled fixture rows.
- `tools/bench/phase-2c/`
- `docs/reports/bench-data/phase-2c/`
- `docs/reports/phase-2c-completion-report.md`
- `docs/handoffs/phase-2c-handoff.md`
- Tag `phase-2c-workload-baseline`

Test/bench evidence:

- `cargo test --workspace` passed with 216 tests.
- 1x fixture equivalence tests are present and passing.
- Smoke `synthetic_no_deps` bench passed.
- Targeted `combined_workflow` bench passed for 50x and skipped 100x by environment gate.

Deviations still active:

- 100x combined workflow remains env-gated and was not captured by this audit.
- Bench-data includes partial scaled cold-consolidation rows that `PERF.md` describes as deferred.
- Phase 2C documentation overstates some captured rows and still contains uncommitted/prospective
  metadata.

Stale or contradictory docs:

- Several Phase 2C docs need cleanup before implementation starts; details are listed in Section 6.

## 4. Build/Test Gate

Commands run:

```text
git status --short
git log --oneline -n 15
git tag --list
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
cargo test --workspace
cargo run --release --bin mc -- demo
grep -rn "unsafe" crates/mc-core/src/
grep -rn "use serde\|use tokio\|use rayon\|use anyhow" crates/
grep -rn "println!\|eprintln!\|dbg!" crates/mc-core/src/
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
```

Results:

- Format: passed.
- Clippy: passed with `-D warnings`.
- Release build: passed.
- Tests: passed, 216 passed / 0 failed.
- CLI demo: passed.
- `unsafe` grep in `crates/mc-core/src/`: no matches.
- banned dependency grep for `serde`, `tokio`, `rayon`, `anyhow` in `crates/`: no matches.
- debug print grep in `crates/mc-core/src/`: no matches.
- unwrap/expect/panic/todo grep in `crates/mc-core/src/`: 139 matches. Most are test-only. Production
  non-test `expect` calls remain in `cube.rs` around internal invariants; clippy accepts them and
  this audit did not treat them as Phase 2C drift.

Test count by binary:

```text
mc-cli unit tests: 0
mc-core unit tests: 84
acme_demo: 20
consolidation: 12
coordinate_validity: 9
correctness: 16
dependency: 7
duplicate_elements: 6
hierarchy_cycle: 10
locks_permissions: 8
trace: 9
value_nan: 8
writeback: 11
mc-fixtures unit tests: 16
doc tests: 0
total: 216
```

CLI demo evidence:

- Built Acme cube with 6 dimensions, 3 hierarchies, 11 measures, and 5 rules.
- Loaded 2520 input cells.
- Read expected input, derived, and consolidated sample values.
- Recomputed derived values after a leaf write.
- Rejected derived and consolidated writes.

## 5. Benchmark Artifact Audit

Bench files registered in `crates/mc-core/Cargo.toml`:

```text
leaf_read_write
derived_read
consolidated_read
dirty_propagation
demo_path
synthetic_no_deps
snapshot_clone
hierarchy_mark
combined_workflow
```

Bench-data folders:

- `docs/reports/bench-data/phase-2a/` exists.
- `docs/reports/bench-data/phase-2b/` exists.
- `docs/reports/bench-data/phase-2c/` exists.

Smoke benchmark run:

```text
cargo bench -p mc-core --bench synthetic_no_deps -- --warm-up-time 1 --measurement-time 1 --sample-size 10
```

Result:

```text
[synthetic_no_deps preflight] dirty_set len after write: 0; invalidated.len: 0
write_input_leaf_no_deps_synthetic
time: [205.86 ns 218.17 ns 224.15 ns]
Performance has improved versus prior local baseline.
```

Targeted workload benchmark run:

```text
cargo bench -p mc-core --bench combined_workflow -- --warm-up-time 1 --measurement-time 1 --sample-size 10
```

Result:

```text
[combined_workflow x50] session median over 3 samples: 473.20 ms
[combined_workflow x50] edit median(p50): 2117.8 us
[combined_workflow x50] slice_read median(p50): 4927.8 us
[combined_workflow x50] snapshot median(p50): 10.22 ms
[combined_workflow x50] final dirty_set median: 305039
[combined_workflow x100] SKIPPED - set MC_BENCH_COMBINED_WORKFLOW_100X=1 to run
```

Interpretation:

- Fast: no-dependency leaf writes are sub-microsecond; Phase 2B cold consolidation fast path remains
  materially improved; 50x combined edit/read/snapshot slices are in low millisecond ranges.
- Slow: bulk canonical input loading at 50x is documented around 230.8 seconds; dirty-set growth in
  combined workflow reaches about 305K final dirty coordinates.
- Phase 2C proved that production-shaped workload costs are not dominated by the Phase 2B
  consolidation path. It pointed to write/load and dirty-tracking overhead as the better Phase 2D
  target.
- Phase 2C correctly avoided claiming a planner-level production truth, but the current Phase 2D
  handoff does pick Branch A from suggestive benchmark evidence.

Artifact issues:

- `docs/reports/bench-data/phase-2c/README.md` claims all scaled variants and combined 50/100 were
  captured. The repo does not show 100x combined data, and some 50/100 isolated rows are absent.
- `PERF.md` says scaled cold-consolidation rows were deferred, but the Phase 2C bench-data folder
  contains partial 10x and 50x cold-consolidation data for `Q1_PaidMedia_Florida_Spend`.
- Combined-workflow per-mark units are inconsistent across bench output and docs. The actual audit
  output shows about `422,166.8 ns` per reported dirty mark, not about `422 ns`.

## 6. Documentation Drift Audit

| Severity | File path | Stale or incorrect text | Why it matters | Recommended fix |
| --- | --- | --- | --- | --- |
| important | `docs/CURRENT_STATE.md` | Last-updated line still says Phase 2C measurement landed, uncommitted, awaiting review. | HEAD is clean, Phase 2C is tagged, and Phase 2D handoff is queued. This makes the current state ambiguous. | Update the header to reflect `bc70ad2` and Phase 2D proposed/scoped. |
| important | `docs/PERF.md` | Phase 2C summary points at `docs/reports/phase-2d-handoff-scaffold.md`. | That file was deleted at HEAD and replaced by `docs/handoffs/phase-2d-handoff.md`. | Replace the link and add a note that the scaffold was promoted after the Phase 2C tag. |
| important | `docs/reports/phase-2c-completion-report.md` | Report still says ready for review, uncommitted, prospective tag, and lists the handoff scaffold as a new file. | The tag exists and the scaffold was removed/promoted later. Historical report needs a post-commit note to avoid misleading future readers. | Add a top note with tag `phase-2c-workload-baseline`, commit `789db15`, and follow-on `bc70ad2`. |
| important | `docs/reports/bench-data/README.md` | Calls Phase 2C tag prospective and says all 10/50/100 scaled variants plus combined 50/100 are captured. | Actual artifacts do not include complete 100x combined data. | Backfill tag/hash and describe exact captured/missing/env-gated rows. |
| important | `docs/reports/bench-data/phase-2c/README.md` | `tag: TODO`, uncommitted metadata, and overbroad capture claims. | Benchmark provenance is hard to audit and overstates coverage. | Backfill metadata and correct row inventory. |
| important | `docs/PERF.md`, `docs/reports/phase-2c-completion-report.md`, `docs/handoffs/phase-2d-handoff.md` | Combined-workflow per-mark values are reported around `422 ns`. | Audit output shows the printed attribution value is around `422,000 ns` per reported mark. Phase 2D justification should not rely on the wrong unit. | Correct the unit or redefine the denominator; keep the flatness conclusion separate from the absolute cost. |
| important | `docs/handoffs/phase-2d-handoff.md` | Says public `DirtyTracker` methods/signatures stay verbatim, but also says `DirtyTracker::new()` becomes `DirtyTracker::with_shape(...)`. | `DirtyTracker` is re-exported publicly. This is an API compatibility ambiguity before implementation. | Specify whether `new()` remains, whether `with_shape()` is additive, and whether an API change is accepted. |
| important | `docs/handoffs/phase-2d-handoff.md` | Bitset memory/cardinality estimate appears too low for full coordinate-space bitsets. | Incorrect sizing can lead to wrong acceptance assumptions. | Recalculate from actual dimension cardinalities and clarify whether the bitset covers all cells or only writable/storable cells. |
| minor | `docs/HANDOFF.md` | Says benchmark history spans four tags but lists three tags. | Small trust issue in the main handoff. | Change "four" to "three" or add the missing tag if one exists. |
| minor | `docs/HANDOFF.md` | "Where to look" table omits Phase 2B and Phase 2C reports/handoffs. | New reviewers may miss the latest evidence. | Add Phase 2B/2C report and handoff links. |
| minor | `docs/roadmap/MASTER_PHASE_PLAN.md` | Last-updated wording is post-Phase 2C, while HEAD has Phase 2D queued. | Roadmap status is mostly correct but the header lags. | Update header wording to Phase 2D proposed. |
| minor | `crates/mc-core/src/lib.rs` | Module doc comment still describes now-implemented modules as deferred. | Public crate docs lag the code. | Update crate-level docs in a future documentation-only pass. |

## 7. Code Drift Audit

Phase 2C source-lock verification:

- `git diff --name-only phase-2b-consolidation-fast-path..phase-2c-workload-baseline -- crates/mc-core/src crates/mc-core/tests` returned no files.
- `git diff --name-only phase-2c-workload-baseline..HEAD -- crates/mc-core/src crates/mc-core/tests crates/mc-fixtures/src crates/mc-core/benches crates/mc-core/Cargo.toml Cargo.toml Cargo.lock rust-toolchain.toml` returned no files.

Conclusion: `crates/mc-core/src` and `crates/mc-core/tests` were unchanged during Phase 2C, and no
kernel source/test behavior was changed after the Phase 2C tag.

Public API drift:

- No unexpected code drift was found after the Phase 2C tag.
- Existing public API is re-exported from `crates/mc-core/src/lib.rs`, including `DirtyTracker`.
- Phase 2D handoff must account for this public API surface before changing constructors.

Banned dependencies and constructs:

- No `unsafe` found in `crates/mc-core/src/`.
- No `serde`, `tokio`, `rayon`, or `anyhow` imports found in `crates/`.
- No `println!`, `eprintln!`, or `dbg!` found in `crates/mc-core/src/`.
- Existing production `expect` calls in `cube.rs` are invariant checks and are not new Phase 2C work.

Accidental behavior work:

- No evidence of behavior work mixed into Phase 2C measurement changes.
- Phase 2C changes are fixtures, benches, scripts, docs, and committed bench artifacts.

## 8. Fixture and Benchmark Shape Audit

Fixture review:

- `build_acme_cube` remains the canonical Acme fixture.
- `write_canonical_inputs` still writes the canonical 2520 input cells.
- `build_scaled_acme_cube_10x`, `build_scaled_acme_cube_50x`, and
  `build_scaled_acme_cube_100x` are public wrappers.
- The generic scaled builder is `pub(crate)`, which keeps the benchmark scale surface controlled.
- `write_canonical_inputs_scaled` is present and writes `2520 * scale` inputs.
- Scaled builders are additive: they widen the Market dimension by adding deterministic extra city
  leaves under existing state parents. They do not alter the base Acme dimensions, measures, rules,
  or hierarchy semantics.
- Scale 1 equivalence tests are present and passing.

Benchmark shape review:

- `combined_workflow` models build, load, materialization, repeated writes, reads, snapshots, and
  dirty-set growth.
- 100x combined workflow is explicitly env-gated with `MC_BENCH_COMBINED_WORKFLOW_100X=1`.
- Isolated scaled benches exist for cold/warm input reads, derived reads, writes, snapshots,
  rollback, demo load, and selected consolidation rows.
- Benchmark labels are mostly clear, but the `combined_workflow/50x_marker` Criterion timing is a
  marker/no-op timing. The useful data is printed in benchmark stderr and documented in `PERF.md`,
  not in the Criterion estimate alone.
- Env-gated and abandoned rows are partially documented, but the bench-data README currently
  overstates what was captured.

Fixture conclusion:

- The scaled builders preserve Acme semantics and are suitable for Phase 2C evidence.
- The benchmark artifact documentation needs exact row inventory cleanup before Phase 2D starts.

## 9. ADR Readiness

ADR-0003 status:

- `docs/decisions/0003-workload-sketch.md` is `Accepted-Provisional`.
- It has a sunset/amendment trigger tied to real planner or modeling evidence.
- The six decisions are clear enough for synthetic workload design.

Does Phase 2C require an amendment?

- No formal amendment is required to preserve Phase 2C as a synthetic benchmark baseline.
- A short addendum would help: Phase 2C data supports using the workload sketch as a prioritization
  proxy, but it does not prove planner-level production behavior.

Is ADR-0003 strong enough for Phase 2D scoping?

- Yes, for Phase 2D scoping only.
- No, for Phase 3 model/parser semantics. Phase 3A still needs a parser/model-definition ADR before
  implementation starts.

What still needs real planner input:

- Real-world model shapes and dimension cardinalities.
- Rule/dependency graph distributions.
- Snapshot lifecycle expectations.
- Planner/editor edit patterns.
- Whether the dirty tracker should optimize for all coordinate space, storable leaf inputs, or
  dependent computed cells.

## 10. Phase 2D Recommendation

Recommendation: proceed with Phase 2D scoping around the dirty tracker / bulk canonical ingest path,
after documentation cleanup.

Likely target area:

- Dirty-tracker representation and write/load invalidation behavior, corresponding to roadmap
  Branch A / Section 9.3.

Evidence:

- Phase 2B already improved the consolidation clone path.
- Phase 2C documents 50x canonical load as the largest observed cost.
- Combined workflow shows low-millisecond edit costs but a large final dirty set around 305K.
- The latest handoff already proposes a bitset-backed dirty tracker.

Risks:

- The per-mark unit mismatch must be corrected before setting acceptance targets.
- The full coordinate-space cardinality estimate in the Phase 2D handoff appears too low.
- `DirtyTracker` is a public re-export; constructor/API compatibility must be decided.
- Optimizing dirty tracking can silently change invalidation semantics if coverage tests are not
  preserved.

Phase 2D handoff must specify:

- Exact public API compatibility plan for `DirtyTracker`.
- Exact bitset shape and coordinate indexing domain.
- Whether bitsets cover all coordinates, only writeable leaf coordinates, or dirtyable computed
  coordinates.
- Acceptance gates using Phase 2C baselines, especially canonical load and combined workflow.
- Required no-regression checks for demo semantics, dirty closure, rollback, snapshots, locks, and
  permissions.
- Whether memory growth is acceptable at 1x/10x/50x/100x.

What not to touch:

- Rule semantics.
- Consolidation semantics.
- Snapshot semantics beyond preserving dirty state behavior.
- Parser/model-definition work.
- New dependencies unless explicitly justified.

Suggested acceptance gates:

- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --release --workspace`
- `cargo test --workspace`
- `cargo run --release --bin mc -- demo`
- Targeted Phase 2C benchmark comparison for canonical 50x load.
- Combined workflow 50x no semantic/performance regression.
- No new `unsafe`, async runtime, serde, rayon, or anyhow in `mc-core`.

The data points to Section 9.3. It does not prove Section 9.3 is the only production answer.

## 11. Phase 3 Readiness

Is the repo ready to start Phase 3 now?

- No.

What must happen first:

- Finish or explicitly defer Phase 2D.
- Clean up Phase 2C/Phase 2D benchmark documentation drift.
- Decide and document the `DirtyTracker` public API plan.
- Backfill current-state and handoff metadata so the next assistant does not have to infer status
  from tags and commit history.

Documents that should exist before Phase 3A:

- Parser/model-definition ADR for Phase 3A.
- Updated `CURRENT_STATE.md` after Phase 2D disposition.
- Updated `HANDOFF.md` after Phase 2D disposition.
- Phase 2D completion report or explicit deferral report.
- Updated `PERF.md` with corrected Phase 2C/2D benchmark interpretation.

Does Phase 3A need an ADR?

- Yes. The roadmap already indicates Phase 3A depends on parser/model definition choices. The ADR
  should define accepted syntax scope, semantic mapping to the kernel, unsupported features,
  migration assumptions, and test fixtures.

Phase 2 work to close before model definition starts:

- Dirty-tracker performance decision.
- Benchmark artifact metadata cleanup.
- Handoff/current-state alignment.
- Any Phase 2D public API compatibility decision.

## 12. Cleanup Checklist

Required before Phase 2D implementation:

- Fix combined-workflow per-mark unit/denominator language in `docs/PERF.md`,
  `docs/reports/phase-2c-completion-report.md`, and `docs/handoffs/phase-2d-handoff.md`.
- Clarify the `DirtyTracker::new()` versus `DirtyTracker::with_shape(...)` API plan in the Phase 2D
  handoff.
- Recalculate Phase 2D bitset cardinality/memory estimates from actual dimensions and document the
  indexing domain.
- Update `docs/CURRENT_STATE.md` to reflect clean tagged Phase 2C plus queued Phase 2D handoff.
- Correct `docs/reports/bench-data/phase-2c/README.md` and
  `docs/reports/bench-data/README.md` to show actual tag/hash/captured rows/env-gated rows.
- Replace deleted scaffold links with `docs/handoffs/phase-2d-handoff.md`.

Nice before Phase 2D:

- Add a post-commit note to `docs/reports/phase-2c-completion-report.md`.
- Update `docs/HANDOFF.md` to say three benchmark tags, or add the missing fourth tag if intended.
- Expand the main handoff "Where to look" table to include Phase 2B and Phase 2C reports/handoffs.
- Update `docs/roadmap/MASTER_PHASE_PLAN.md` header/status wording to say Phase 2D proposed.
- Decide whether partial scaled cold-consolidation rows should be documented in `PERF.md` or removed
  from accepted Phase 2C artifacts.

Defer until Phase 3:

- Refresh `crates/mc-core/src/lib.rs` crate-level module documentation.
- Write the Phase 3A parser/model-definition ADR.
- Revisit ADR-0003 with real planner/model evidence.

## 13. Final Verdict

READY AFTER CLEANUP

- Build, format, lint, tests, demo, and smoke benchmarks are green.
- Phase 2C source-lock claims are verified: no `mc-core/src` or `mc-core/tests` changes landed
  during Phase 2C or after its tag.
- Phase 2D is scoped in docs, but the handoff contains API and sizing ambiguities that should be
  fixed before code changes.
- Benchmark docs contain stale metadata and one important unit/denominator mismatch.
- Phase 3 should wait until Phase 2D is completed or explicitly deferred and a Phase 3A parser ADR
  exists.

Recommended next action: run a documentation-only cleanup pass for Phase 2C/Phase 2D benchmark
metadata and handoff ambiguities, then start Phase 2D from the corrected handoff.
