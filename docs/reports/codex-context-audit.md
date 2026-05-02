# Codex Context Audit

**Date:** 2026-05-01  
**Repo:** MarketingCubes V2  
**Auditor:** Codex, second engineering reviewer  
**Scope:** Audit only. No Rust source edits, no behavior changes, no refactors.

## 1. Executive Summary

Current phase according to the live docs:

- `docs/CURRENT_STATE.md` says Phase 1A, Phase 1B, and Phase 2A are complete.
- `docs/roadmap/MASTER_PHASE_PLAN.md` says Phase 2B is proposed and Phase 3A is also proposed.
- `docs/HANDOFF.md` correctly queues Phase 2B, but its "What ships today" section is stale and still describes the pre-Phase-2A Phase 1B state.

What is complete:

- Phase 1A kernel exists and is exercised by 203 original contract tests.
- Phase 1B Criterion baseline exists.
- Phase 2A cold-path benchmark expansion exists: cold consolidation rows, synthetic no-deps write bench, snapshot clone bench, hierarchy mark bench, and synthetic fixture builders.
- Current validation gates are green on this checkout: fmt, clippy, release build, workspace tests, and demo.

What is next:

- Phase 2B Consolidation Fast Path is the only next implementation task with a concrete handoff.
- Phase 3A is marked proposed in the roadmap, but the roadmap itself says it needs an ADR before formal scoping.

Major red flags:

- Against clean HEAD at the start of the audit, no build, test, or demo red flags were found.
- During final verification, uncommitted Phase-2B-looking Rust source edits appeared in `crates/mc-core/src/cube.rs` and `crates/mc-core/src/dimension.rs`. I did not make or validate those edits. The current worktree fails `cargo check --workspace`.
- Important documentation drift exists. The biggest items are stale Phase 1B text in `docs/HANDOFF.md`, stale commit/tree metadata in `docs/PERF.md`, a stale `mc-core/src/lib.rs` crate-level module-status comment, and a contradiction between `dependency.rs` comments and dependency tests about hierarchy edges.
- The Phase 2B handoff says "5 + 4 = 9 existing benches", but `crates/mc-core/Cargo.toml` registers 8 bench binaries. Phase 2A added three new bench files plus cold rows inside the existing `consolidated_read` bench.

## 2. Repo Map

Top-level folders:

- `crates/`: Rust workspace crates.
- `docs/`: roadmap, specs, reports, handoffs, ADRs, research notes, and product docs.
- `research/`: raw reference material.
- `target/`: Cargo build output, ignored by git.

Crates:

- `crates/mc-core`: kernel crate. Owns `Cube`, dimensions, hierarchies, coordinates, cells, rules, dependency tracking, dirty tracking, consolidation, permissions, locks, snapshots, slices, and the concrete `HashMapStore`.
- `crates/mc-fixtures`: test and benchmark fixtures. Builds the Acme cube, writes canonical inputs, materializes dependencies, and provides Phase 2A synthetic fixtures.
- `crates/mc-cli`: small CLI. `cargo run --release --bin mc -- demo` runs the Acme demo.

Important docs:

- `docs/roadmap/MASTER_PHASE_PLAN.md`: intended single source of truth for phase order and status.
- `docs/CURRENT_STATE.md`: live build, test, dependency, and phase state.
- `docs/HANDOFF.md`: 5-minute orientation. Currently partly stale.
- `docs/PERF.md`: Phase 1B and Phase 2A benchmark record and interpretation.
- `docs/reports/phase-1-completion-report.md`: Phase 1A and Phase 1B closure narrative.
- `docs/reports/phase-2a-completion-report.md`: Phase 2A measurement report.
- `docs/specs/engine-semantics.md`: semantic definitions and invariants.
- `docs/specs/phase-1-rust-kernel-build-brief.md`: Phase 1 build contract. Parts of its active-deviation text are stale after Phase 1B.
- `CLAUDE.md`: implementation operating manual and process constraints.
- `docs/handoffs/phase-2b-handoff.md`: concrete next implementation prompt.

## 3. Phase Verification

### Phase 1A Claims vs Code and Tests

Verified in code:

- Workspace has three crates: `mc-core`, `mc-fixtures`, `mc-cli`.
- Runtime deps match the narrow set in `mc-core`: `smallvec`, `ahash`, `thiserror`, `once_cell`.
- `build_acme_cube()` exists in `crates/mc-fixtures/src/lib.rs` and builds 6 dimensions, 11 measures, and 5 deterministic rules.
- `write_canonical_inputs()` writes 2,520 input cells.
- `Cube::write` rejects derived cells, consolidated cells, bad types, NaN/Inf, stale revisions, unauthorized principals, and approved versions.
- `Cube::read` dispatches to leaf or consolidated paths, evaluates derived rules lazily, and caches derived/consolidated values by revision.
- `DirtyTracker::mark_closure` explicitly excludes the freshly written root cell.
- `Snapshot` is a deep clone of `HashMapStore`, as claimed.

Verified by tests:

- `cargo test --workspace` passed 209 tests total.
- The original Phase 1A/1B contract count is still represented as 203 tests plus 6 Phase 2A fixture tests.
- Two proptest-backed doctrine tests are still no-op stubs: `doctrine_atomicity_of_write` and `doctrine_causality`.

Mismatch:

- `crates/mc-core/src/lib.rs` still says `rule`, `dependency`, `dirty`, `consolidation`, `cube`, `slice`, `permission`, `lock`, and `snapshot` are deferred, even though those modules are implemented and re-exported.

### Phase 1B Claims vs Bench Files and PERF.md

Verified:

- `criterion.workspace = true` is present in `crates/mc-core/Cargo.toml`.
- Phase 1B bench targets are present: `leaf_read_write`, `derived_read`, `consolidated_read`, `dirty_propagation`, `demo_path`.
- `docs/PERF.md` contains the Phase 1B baseline tables and explains the two caveats that Phase 2A later closed.

Mismatch:

- `docs/HANDOFF.md` still describes the Phase 1B caveats as current in its "What ships today" section.
- The Phase 1 brief still has an "active" benchmark deferral section saying the bench directory is not created and `cargo bench` is inert. That is no longer true for Criterion.

### Phase 2A Claims vs Bench Files and PERF.md

Verified:

- Phase 2A bench files exist: `synthetic_no_deps.rs`, `snapshot_clone.rs`, `hierarchy_mark.rs`.
- `consolidated_read.rs` includes cold consolidation variants with dirty-state assertions and cold golden checks.
- `mc-fixtures` includes `build_minimal_cube()` and `build_graduated_hierarchy_cube()`.
- `git diff --name-only bee2812..48d52e9` shows no `crates/mc-core/src/*.rs` files changed for Phase 2A; the kernel source was not modified by the measurement phase.
- Targeted cold consolidation bench run reproduced the current shape: median 14.704 us for `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)`.

Mismatch:

- `docs/reports/phase-2a-completion-report.md` says there is a `phase-1b-benchmark-baseline` tag and that the Phase 2A initial commit is still "to be tagged". Current repo has only `phase-2a-cold-path-baseline` at `48d52e9`.
- `docs/PERF.md` still records `bee2812` as "HEAD at bench time" and says the Phase 1B + Phase 2A wiring was uncommitted at bench time. That is historical but now confusing in a document that is otherwise used as the live performance baseline.

## 4. Current Test, Build, and Bench State

Commands run against clean HEAD before the uncommitted Rust source drift was observed:

- `git status --short`: clean before this audit document was created.
- `git log --oneline -n 10`: 7 commits, latest `30e1e84 docs: queue Phase 2B (consolidation fast path) handoff`.
- `cargo fmt --check --all`: passed, no output.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --release --workspace`: passed, zero warnings observed.
- `cargo test --workspace`: passed.
- `cargo run --release --bin mc -- demo`: passed and printed the Acme demo.
- Full `cargo bench --workspace`: not run because the full Criterion suite is intentionally long. Exact bench commands are listed below. A targeted Phase 2A cold consolidation bench was run.

Additional final-verification command after uncommitted Rust source edits appeared:

- `cargo check --workspace`: failed. Errors were in `crates/mc-core/src/cube.rs` after a partial `Arc`-based source change: missing `dims_clone`, iterating `&Arc<Vec<Dimension>>`, and assigning `Vec<Dimension>` where `Arc<Vec<Dimension>>` was expected.

Current worktree caveat:

- The green build/test/demo/bench observations above describe the clean HEAD state inspected for this audit.
- The current uncommitted worktree is not green until the source edits in `cube.rs` / `dimension.rs` are either completed and verified or removed by the owner of those edits.

Test count:

- `mc-core` unit tests: 83.
- `mc-core` integration tests: 116.
- `mc-fixtures` unit tests: 10.
- Doc tests: 0.
- Total: 209 passed, 0 failed.

Bench commands available:

```bash
cargo bench --workspace
cargo bench -p mc-core --bench leaf_read_write
cargo bench -p mc-core --bench derived_read
cargo bench -p mc-core --bench consolidated_read
cargo bench -p mc-core --bench dirty_propagation
cargo bench -p mc-core --bench demo_path
cargo bench -p mc-core --bench synthetic_no_deps
cargo bench -p mc-core --bench snapshot_clone
cargo bench -p mc-core --bench hierarchy_mark
```

Targeted bench run:

```bash
cargo bench -p mc-core --bench consolidated_read -- "consolidation_cold/Q1_PaidSearch_Tampa/Spend" --warm-up-time 1 --measurement-time 1 --sample-size 10
```

Result:

- `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)`: 14.371 us to 15.146 us, median 14.704 us.
- Criterion warned it could not complete 10 samples in the requested 1 second and extended collection to about 2.484 seconds.
- No statistically significant performance change was detected.

## 5. Kernel Architecture Summary

`Cube`:

- Implemented in `crates/mc-core/src/cube.rs`.
- Owns cube id/name, dimensions, rules, locks, permissions, store, revision, dependency graph, and dirty tracker.
- `read` checks permissions and coordinate shape, then dispatches to leaf or consolidated reads.
- `write` performs validation, bumps revision, writes input provenance, marks rule dependents and hierarchy ancestors dirty, and returns `WritebackResult`.

`Dimension`:

- Implemented in `dimension.rs`.
- Ordered element catalog with kind (`Standard`, `Measure`, `Scenario`, `Version`), element indexes, hierarchies, default hierarchy, and a freeze flag.
- If no hierarchy is supplied, the builder synthesizes a flat default hierarchy with no edges.

`Hierarchy`:

- Implemented in `hierarchy.rs`.
- Single-parent forest over elements. Stores edges, roots, leaves, consolidated elements, parent pointers, and children maps.
- Builder rejects non-finite weights, duplicate edges, multiple parents, and cycles.
- Provides `descendants()` for consolidation walks and `ancestors()` for dirty ancestor marking.

`Element`:

- Implemented in `element.rs`.
- Named dimension member with optional metadata depending on dimension kind.
- Measure elements carry dtype, role (`Input` or `Derived`), and aggregation rule.
- Version elements carry `VersionState`; scenario elements carry `ScenarioMeta`.

`CellCoordinate`:

- Implemented in `coordinate.rs`.
- Cube id plus ordered `SmallVec` of element ids.
- Builder validates dimension membership and element membership against a cube's dimensions.
- Direct `from_parts` constructor is low-level and does not validate ordering or membership.

`CellValue`:

- Implemented in `cell.rs` and `value.rs`.
- `CellValue` is the read result: scalar value, dtype, provenance, optional uncertainty, optional trace, and revision.
- `StoredCell` is the lighter store representation.
- `ScalarValue::Null` is distinct from zero. NaN and infinity are rejected at write/API boundaries.

`Rule`:

- Implemented in `rule.rs`.
- Phase 1 supports expression trees with constants, same-coordinate measure refs, arithmetic, division, and `IfNull`.
- `RuleSet::add` checks declared dependencies, duplicate rule targets, and measure-level dependency cycles.
- Cube-aware validation happens in `CubeBuilder::add_rule`.

`Consolidation`:

- Implemented in `consolidation.rs` and called from `Cube::read_consolidated`.
- `Consolidator::read` expands consolidated dimensions into leaf combinations, reads every leaf through a caller-supplied closure, and combines values by `Sum`, `WeightedAverage`, `Min`, or `Max`.
- Consolidated values are cached in the store at the current revision.
- Current hot path still clones `self.dimensions` and every default hierarchy before calling `Consolidator::read`.

`DirtyTracker`:

- Implemented in `dirty.rs`.
- Stores an `AHashSet<CellCoordinate>`.
- Rule invalidation uses dependency graph reverse closure and excludes the freshly written root cell.
- Hierarchy ancestor dirtying is computed separately in `Cube::compute_dirty_ancestors`.

`DependencyGraph`:

- Implemented in `dependency.rs`.
- Stores forward edges (`cell -> cells it reads`) and reverse edges (`cell -> cells that read it`).
- Rule edges are materialized lazily when derived cells are read.
- Current tests assert the graph is empty immediately after cube build. Despite a module comment saying hierarchy edges are added at build time, this implementation does not fold hierarchy edges into `DependencyGraph`.

`Snapshot`:

- Implemented in `snapshot.rs` and `Cube::snapshot` / `Cube::rollback_to`.
- Snapshot is a deep clone of `HashMapStore` plus cube id, revision, timestamp placeholder, and optional label.
- Rollback clones the snapshot store back, bumps live revision, clears dirty state, and removes rule-provenance cells.

Permissions and locks:

- `permission.rs` provides grant-based capability checks over `ScopePattern`.
- Root principal has full access.
- Non-root principals need grants matching the coordinate and capability bit.
- `lock.rs` supports hard and soft scoped locks with mandatory expiration.
- Hard locks block writes by other principals; soft locks allow writes but return advisory notes.

## 6. Benchmark Interpretation

What is fast:

- Warm leaf reads and warm derived reads are tens of nanoseconds in `PERF.md`.
- Warm consolidation cache hits are about 67 ns in `PERF.md`.
- Synthetic no-deps writes are about 246 ns in `PERF.md`.
- Snapshot and rollback are not currently gating at Acme scale.

What is slow:

- Acme writes are about 150 to 165 us because dirty propagation creates many `CellCoordinate` values and inserts them into an `AHashSet`.
- The 3-leaf cold consolidation path is about 14 to 15 us, even though it only walks 3 leaves. This is the fixed-cost floor Phase 2B targets.

What Phase 2A proved:

- Phase 1B's warm consolidation numbers were not valid evidence for cold consolidation ceilings. Phase 2A added real cold measurements.
- All 1A consolidation ceilings pass on cold reads.
- The brief's no-deps write ceiling passes on the synthetic fixture; the Acme row is measuring hierarchy/derived dirty-mark cost, not a no-deps write.
- The hierarchy traversal itself is cheaper than Acme's full dirty-mark cost; allocation and hash insertion dominate.

Is Phase 2B justified:

- Yes. The current code still contains the `read_consolidated` dimension/hierarchy clone path, and the observed 3-leaf cold benchmark still misses the 3 us 1B target by about 5x.
- The fix is localized and data-justified.
- Risk is real because changing dimension/hierarchy ownership can affect public field shapes or borrow behavior, but the intended scope is narrow.

## 7. Roadmap Review

Does `MASTER_PHASE_PLAN.md` match `CURRENT_STATE.md` and `HANDOFF.md`?

- Mostly for Phase 2B: all three identify Phase 2B as the next concrete optimization.
- Not fully for current status: `HANDOFF.md` still says "What ships today (Phase 1A + Phase 1B)" and repeats Phase 1B caveats that Phase 2A closed.

Is there only one obvious next phase?

- For implementation, yes: Phase 2B has the concrete handoff.
- For status scanning, no: the roadmap marks both Phase 2B and Phase 3A as `proposed`. The "Starting work?" instructions say to pick the first proposed row with a handoff, which points to Phase 2B, but the table still makes Phase 3A look concurrently ready.

Are any phase statuses confusing?

- Yes. Phase 3A is `proposed` but also says it needs an ADR before formal scoping.
- Phase 2B is described as `proposed` and "not scheduled" in current state. That is acceptable, but Phase 3A should probably be "placeholder" or "blocked pending ADR" until Phase 2 exits.

Are any future acceptance gates unrealistic or underspecified?

- Phase 2B's <= 3 us gate is precise, but the expected "same constant savings" on higher-fan-out rows is diagnostic, not rigorously specified.
- Phase 3A's "byte-identical Acme demo output" is precise for the CLI, but "structural diff helper" is not yet designed.
- Phase 4's ">= N% accuracy" is intentionally underspecified.
- Phase 6's "an internal team member can use the UI without instruction" is product-meaningful but needs a concrete test protocol later.
- Phase 7's "one full planning cycle without engineering escalation" is reasonable as product evidence, but not an engineering acceptance test.

Does Phase 3A depend on an ADR that does not exist yet?

- Yes. The roadmap says Phase 3A needs an ADR before formal scoping. `docs/decisions/` currently only contains ADR-0001 for Phase 1 scope.

## 8. Drift and Contradiction List

1. `crates/mc-core/src/cube.rs` and `crates/mc-core/src/dimension.rs`
   - What it says/does: uncommitted source edits introduce `Arc<Vec<Dimension>>` and `Arc<Hierarchy>` as a partial Phase 2B fast-path implementation.
   - Why wrong/confusing: this audit was requested as read-only, and the current worktree does not compile. `cargo check --workspace` fails with missing `dims_clone`, invalid iteration over `&Arc<Vec<Dimension>>`, and `Vec<Dimension>` vs `Arc<Vec<Dimension>>` mismatch.
   - Severity: blocker.
   - Suggested fix: pause implementation work and decide whether these edits are intended Phase 2B work. If yes, complete them in a Phase 2B implementation pass with full gates. If no, restore clean HEAD before continuing. This audit did not revert them.

2. `docs/HANDOFF.md`
   - What it says: "What ships today (Phase 1A + Phase 1B)", 203 tests, warm-only consolidation caveat, and synthetic no-deps caveat still pending.
   - Why wrong/confusing: Phase 2A is complete, tests are 209, cold consolidation and synthetic no-deps measurement gaps are closed.
   - Severity: important.
   - Suggested fix: Update the section to "Phase 1A + Phase 1B + Phase 2A", use 209 tests, and summarize Phase 2A closures. Keep Phase 2B as queued.

3. `docs/reports/phase-2a-completion-report.md`
   - What it says: Phase 1B baseline tag is `phase-1b-benchmark-baseline`; Phase 2A initial commit is "to be tagged".
   - Why wrong/confusing: Current state and roadmap say no standalone Phase 1B tag was cut. The only repo tag is `phase-2a-cold-path-baseline` at `48d52e9`.
   - Severity: important.
   - Suggested fix: Replace the stale tag text with `phase-2a-cold-path-baseline` at `48d52e9` and note Phase 1B was bundled.

4. `docs/PERF.md`
   - What it says: "HEAD at bench time" is `bee2812`; Phase 1B + Phase 2A wiring was uncommitted and would be tagged after review.
   - Why wrong/confusing: The benchmark baseline is now committed and tagged at `48d52e9`, while current HEAD is `30e1e84`.
   - Severity: important.
   - Suggested fix: Split historical bench metadata from current baseline metadata, or add a "Current baseline commit" row.

5. `crates/mc-core/src/lib.rs`
   - What it says: `trace` is "types only" and `rule`, `dependency`, `dirty`, `consolidation`, `cube`, `slice`, `permission`, `lock`, and `snapshot` are deferred.
   - Why wrong/confusing: All listed modules are implemented and publicly re-exported.
   - Severity: important.
   - Suggested fix: Update the crate-level doc comment only. No code behavior change needed.

6. `crates/mc-core/src/dependency.rs`
   - What it says: hierarchy edges are added by the cube builder and show up in `forward` / `reverse` from cube-build time.
   - Why wrong/confusing: `tests/dependency.rs` explicitly says hierarchy edges are not folded into `DependencyGraph`, and `t_dependency_graph_is_empty_immediately_after_cube_build` asserts the graph is empty after build.
   - Severity: important.
   - Suggested fix: Update the module comment to match the implementation: rule edges live in `DependencyGraph`; hierarchy walks happen directly from per-dimension hierarchies.

7. `docs/handoffs/phase-2b-handoff.md`
   - What it says: "All 5 + 4 = 9 existing benches".
   - Why wrong/confusing: `crates/mc-core/Cargo.toml` registers 8 bench binaries. Phase 2A added 3 new bench binaries plus cold rows inside `consolidated_read`.
   - Severity: minor.
   - Suggested fix: Say "all 8 existing bench binaries" or "5 Phase 1B bench binaries plus 3 new Phase 2A bench binaries, with cold rows inside `consolidated_read`".

8. `docs/specs/phase-1-rust-kernel-build-brief.md`
   - What it says: Criterion/benches are inert, `crates/mc-core/benches/` is not created, and `cargo bench` is not part of the gate.
   - Why wrong/confusing: Criterion was restored in Phase 1B. The bench directory exists and is used.
   - Severity: important.
   - Suggested fix: Because this is a locked historical brief, either add a clear supersession note at the top pointing to `PERF.md` and `CURRENT_STATE.md`, or move the active-deviation section into historical context.

9. `crates/mc-core/tests/correctness.rs`
   - What it says: proptest is unavailable because criterion's `clap_lex` requires Rust edition 2024.
   - Why wrong/confusing: `CLAUDE.md` now says criterion was restored and proptest/insta are deferred as Phase-paired work, not because Criterion is still blocked.
   - Severity: minor.
   - Suggested fix: Update comments on the two no-op doctrine tests to reference the current deferral reason.

10. `docs/roadmap/MASTER_PHASE_PLAN.md`
   - What it says: Phase 2B is proposed and Phase 3A is also proposed, while Phase 3A needs an ADR before formal scoping.
   - Why wrong/confusing: A status-only scan can make two phases look ready at once.
   - Severity: minor.
   - Suggested fix: Mark Phase 3A as "blocked pending ADR" or "placeholder" until Phase 2 exits.

11. `docs/reports/phase-1-completion-report.md`
    - What it says: final note says two benchmark sub-ceilings remain measurement gaps queued for Phase 2A.
    - Why wrong/confusing: This is historically true for Phase 1, but readers using the report as current state can miss that Phase 2A closed both gaps.
    - Severity: minor.
    - Suggested fix: Add a short "closed later in Phase 2A" note near the final paragraph or rely on `CURRENT_STATE.md` as the live correction.

## 9. Recommended Next Action

Pause for cleanup before proceeding.

The current uncommitted worktree has incomplete Rust source edits and fails `cargo check --workspace`. Resolve that first:

- If the source edits are intended Phase 2B work, move into a Phase 2B implementation pass and complete/verify them with the full gate.
- If they are not intended, restore the clean Phase 2A/Phase 2B-handoff state before continuing.

After the worktree is green again, proceed to Phase 2B. No ADR is needed before Phase 2B. Do not start Phase 3A until its ADR exists.

Also do a small documentation cleanup before or during Phase 2B prep:

- Fix `docs/HANDOFF.md` so a new assistant does not inherit stale Phase 1B caveats.
- Fix the Phase 2A report tag metadata.
- Fix the Phase 2B handoff bench count.

## 10. Appendix

### Command Output Summary

`git status --short`

```text
<no output; clean before this audit file was added>
```

Final `git status --short` after audit writing and after uncommitted source drift was observed:

```text
 M crates/mc-core/src/cube.rs
 M crates/mc-core/src/dimension.rs
?? docs/reports/codex-context-audit.md
```

`git log --oneline -n 10`

```text
30e1e84 docs: queue Phase 2B (consolidation fast path) handoff
4c11a3c docs: backfill Phase 2A commit hash in CURRENT_STATE + roadmap tag column
48d52e9 bench: complete Phase 2A cold-path benchmark expansion
bee2812 mc-core: update lib.rs doc-comment to point at docs/specs/
5adcd0c docs: reorganize as a spec-driven Rust systems project
5ebc7bc docs+research: reorganize into research-heavy filing system
4aa674a Initial commit: Phase 1 Rust kernel for MarketingCubes V2
```

`cargo fmt --check --all`

```text
<no output; exit 0>
```

`cargo clippy --workspace --all-targets -- -D warnings`

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
```

`cargo build --release --workspace`

```text
Finished `release` profile [optimized] target(s) in 0.04s
```

`cargo test --workspace`

```text
209 passed; 0 failed across workspace tests.
mc-core unit tests: 83
mc-core integration tests: 116
mc-fixtures unit tests: 10
doc tests: 0
```

`cargo run --release --bin mc -- demo`

```text
Loaded 2520 input cells.
Sample Revenue before write: 3_066.67
Q1 Paid_Search Tampa Spend: 33_000.00
Q1 Paid_Media Florida Spend: 329_400.00
Q1 Paid_Search Florida CPC: 1.5202381
Write Spend revision: 2520 -> 2521
Dependent cells dirtied: 19919
Revenue after write: 13_333.33
Derived and consolidated write rejections printed as expected.
```

Targeted cold consolidation bench:

```text
consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)
time: [14.371 us 14.704 us 15.146 us]
No change in performance detected.
```

Final `cargo check --workspace` after uncommitted source drift:

```text
error[E0425]: cannot find value `dims_clone` in this scope
error[E0277]: `&Arc<Vec<Dimension>>` is not an iterator
error[E0308]: mismatched types: expected `Arc<Vec<Dimension>>`, found `Vec<Dimension>`
error: could not compile `mc-core` (lib) due to 3 previous errors
```

### Relevant Commits and Tags

- Current HEAD: `30e1e84aaacdaa129055fcc0122b8ef32dad6c72`.
- Phase 2A tag: `phase-2a-cold-path-baseline` -> `48d52e9f46e23e345ec75f1ae4b0e102d12a673f`.
- Phase 1A initial commit: `4aa674a`.
- No tag points at current HEAD.

### Files Changed by This Audit

- `docs/reports/codex-context-audit.md`

No Rust source files were intentionally edited by this audit. Final verification observed uncommitted Rust source edits in `crates/mc-core/src/cube.rs` and `crates/mc-core/src/dimension.rs`; those edits were not part of the audit deliverable and currently fail `cargo check --workspace`.
