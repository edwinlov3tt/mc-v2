# CURRENT_STATE

> **What's live right now.** Update whenever a phase ships, a gate flips, or a deferral closes.

**Last updated:** 2026-05-02 (Phase 2C measurement landed; uncommitted, awaiting project-owner review per CLAUDE.md §"Executing actions with care")
**Last Phase 1A commit:** `bee2812` — *mc-core: update lib.rs doc-comment to point at docs/specs/* (Phase 1A kernel at `4aa674a`)
**Last Phase 1B + Phase 2A commit:** `48d52e9` — *bench: complete Phase 2A cold-path benchmark expansion* (Phase 1B and Phase 2A bundled into one commit; tag `phase-2a-cold-path-baseline` at this hash)
**Phase 2B commit / tag:** `6ea58ab` (tag `phase-2b-consolidation-fast-path`)
**Phase 2 housekeeping Q3 closure commit:** `9f7420c`
**Phase 2C commit / tag:** `789db15` (tag `phase-2c-workload-baseline`)
**Branch:** `main` (tracking `origin/main` at github.com/edwinlov3tt/mc-v2)

---

## What's shipping

- **Phase 1A — Rust kernel for the Acme demo.** Complete. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).
- **Phase 1B — Benchmark Baseline + PERF.md.** Complete 2026-05-01. Acceptance criterion 5 closed via Cargo.lock transitive pins (no toolchain bump). See [`PERF.md`](./PERF.md).
- **Phase 2A — Cold-Path Benchmark Expansion.** Complete 2026-05-01. Both Phase 1B measurement gaps closed: cold consolidation rows added against §11.2 ceilings (PERF.md §6.7); synthetic no-deps write fixture added against §11.1 50 µs ceiling (PERF.md §6.8). Two new diagnostic suites (snapshot clone PERF.md §6.9; hierarchy ancestor mark microbench PERF.md §6.10). **No `crates/mc-core/src/` files modified.** See [`reports/phase-2a-completion-report.md`](./reports/phase-2a-completion-report.md).
- **Phase 2B — Consolidation Fast Path.** Complete 2026-05-01, committed at `6ea58ab` (tag `phase-2b-consolidation-fast-path`). One targeted kernel change in [`cube.rs::read_consolidated`](../crates/mc-core/src/cube.rs) plus a `Vec<Arc<Hierarchy>>` shape change in [`dimension.rs`](../crates/mc-core/src/dimension.rs); replaces per-call `Vec<Dimension>` + `Vec<Hierarchy>` deep-clones with one `Arc::clone` + a `Vec<Arc<Hierarchy>>` collect (refcount-bumps). PERF.md §6.7 3-leaf cold consol drops 14.3 µs → **2.53 µs** (clears brief §11.2 1B target ≤ 3 µs); every other §6.7 row improves by ~12 µs absolute. New kernel unit test `consecutive_recompute_reads_match_phase_2b` (handoff item 3). One contract test rewritten (`t_consolidation_caches_value_within_revision`, semantic-not-timing) per ADR-0002 + the SPEC QUESTION round-trip approval. See [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md) and [`PERF.md`](./PERF.md) §6.11 + §9.4 + §10.
- **Phase 2C — Production-Shaped Workload Benchmarks.** **Pending project-owner review and commit (uncommitted at HEAD).** Measurement-only phase; **no `crates/mc-core/src/` change.** Adds internal `mc_fixtures::build_scaled_acme_cube(scale)` (`pub(crate)`) + three public wrappers `_10x` / `_50x` / `_100x` + 6 unit tests including the mandatory scale-1× equivalence test against brief §4.5.1 anchor goldens. Adds 27 new bench rows extending the existing five Phase 1B/2A bench files at 10× / 50× / 100×. Adds new [`combined_workflow.rs`](../crates/mc-core/benches/combined_workflow.rs) that simulates a 100-iteration planner session at 50× and 100× with stacked-snapshot hold (TM1 sandbox pattern per ADR-0003 Decision 6). PERF.md §6.12 / §6.13 / §6.14 written from the gate run. **Did not pick a Phase 2D winner** — §9 priority deliberately unspecified. See [`reports/phase-2c-completion-report.md`](./reports/phase-2c-completion-report.md).

## What's queued

- **Phase 2 housekeeping — Q3 (criterion baseline tracking).** **Closed retroactively 2026-05-01.** Workflow proven end-to-end at commit `9f7420c`. Both `phase-2a` and `phase-2b` baselines captured under [`reports/bench-data/`](./reports/bench-data/) (1.4 MB JSON; 45 rows × 2 phases × 4 files). Phase 2C onward must use `cargo bench -p mc-core --bench <name> -- --baseline phase-2b`. See [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md) §6.A.1 for the closure record. **Phase 2C extended this to a third baseline:** `phase-2c` saved under [`reports/bench-data/phase-2c/`](./reports/bench-data/phase-2c/) (post-2C scaled-row data captured at sample-size 10).
- **Phase 2 housekeeping — Q1 (workload sketch ADR).** **Accepted (provisional) 2026-05-01.** [`decisions/0003-workload-sketch.md`](./decisions/0003-workload-sketch.md) — sunset clause auto-flips status to "Needs revision" on first real planner usage data or 2026-11-01, whichever comes first. The workload curve (10× / 50× / 100× Acme) and 100 ms click-instant threshold from this ADR are what Phase 2C calibrates against. **Phase 2C produced the workload-shaped data ADR-0003 anchored to;** ADR-0003 stays Accepted — Provisional, no amendment yet.
- **Phase 2 housekeeping — Q2 (toolchain bump).** Deferred until any new runtime dep needs it (likely Phase 3A's parser dep choice).
- **Phase 2D — pick the §9 winner.** Reads from PERF.md §6.14 (scaling-shape table) to decide between §9.3 (bitset-backed dirty tracker), §9.2 (leaf-flag cache), or something else the data surfaces. Phase 2C confirmed §9.3's per-mark-cost-grows-with-set-size hypothesis is **not** strengthened *within a session* at 50× (the cost is flat at 434 / 430 / 439 ns at iters 1 / 50 / 100 of a 100-iteration session). Whether the cost grows *across scales* is read off §6.12.1; that is Phase 2D's call. Handoff TBD.

## Active ADRs

- [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) — Phase 1 scope: smallest kernel that runs the Acme demo. **Status:** Accepted.
- [`decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](./decisions/0002-perf-assertions-in-benchmarks-not-tests.md) — Performance assertions belong in criterion benchmarks, not in `cargo test`. **Status:** Accepted (Phase 2B). Authorizes the `t_consolidation_caches_value_within_revision` rewrite from a wall-clock ratio to semantic cache-state assertions.
- [`decisions/0003-workload-sketch.md`](./decisions/0003-workload-sketch.md) — Workload sketch & perception thresholds (Phase 2 housekeeping Q1). **Status:** Accepted — Provisional. Sunset clause: auto-flips to "Needs revision" on first real planner usage data, or 2026-11-01, whichever comes first. Defines the workload curve (10× / 50× / 100× Acme) and 100 ms click-instant threshold that Phase 2C calibrates against.

---

## Build / test / lint state (at HEAD)

| Gate | Command | Status |
|---|---|---|
| Build | `cargo build --release --workspace` | ✓ zero warnings |
| Format | `cargo fmt --check --all` | ✓ |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ |
| Tests | `cargo test --workspace` | ✓ 216 / 0 (was 210; +6 from Phase 2C `mc-fixtures` unit tests covering scaled fixtures + the mandatory scale-1× equivalence test) |
| Determinism (10×) | `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | ✓ 10 / 10 identical at 216 / 0 each run |
| CLI demo | `cargo run --release --bin mc -- demo` | ✓ matches brief §4.6 (kernel unchanged) |
| Benchmarks | `cargo bench --workspace` | ✓ Phase 1B baseline + Phase 2A cold-path expansion + Phase 2B fast path + **Phase 2C workload-shaped benches** all green. Numbers in [`PERF.md`](./PERF.md) §6 (Phase 1B), §6.7–§6.10 (Phase 2A), §6.11 (Phase 2B before/after), and **§6.12 / §6.13 / §6.14 (Phase 2C 10× / 50× / 100× rows + combined-workflow + scaling-shape summary)**. Phase 2C scaled rows compared against `--baseline phase-2b`; no Phase 1B/2A/2B regression beyond ±10% noise. **Phase 2C did not pick a Phase 2D winner** — §9 row priorities stay unspecified per the handoff hard rule. |

---

## Test count by target

| Target | Count |
|---:|---|
| `mc-core` unit tests | 84 |
| `tests/acme_demo.rs` | 20 |
| `tests/writeback.rs` | 11 |
| `tests/consolidation.rs` | 12 |
| `tests/trace.rs` | 9 |
| `tests/dependency.rs` | 7 |
| `tests/locks_permissions.rs` | 8 |
| `tests/correctness.rs` | 16 |
| `tests/hierarchy_cycle.rs` | 10 |
| `tests/duplicate_elements.rs` | 6 |
| `tests/coordinate_validity.rs` | 9 |
| `tests/value_nan.rs` | 8 |
| `mc-fixtures` unit tests (Phase 1A: 4 + Phase 2A: 6 + Phase 2C: 6) | 16 |
| **Total** | **216** |

`mc-core` unit tests are 84 (was 83) after Phase 2B added
`cube::tests::consecutive_recompute_reads_match_phase_2b` per the
Phase 2B handoff §3 mandate. `tests/consolidation.rs` is still 12 —
one test (`t_consolidation_caches_value_within_revision`) was
rewritten under ADR-0002 + the SPEC QUESTION approval but the count
is unchanged. Phase 2C added 6 tests in `mc-fixtures` (10 → 16):
mandatory scale-1× equivalence test + invariant tests at 10× / 50×
/ 100× + extra-leaf round-trip at 10× + scale-zero rejection.

---

## Toolchain + dependency state

- Rust 1.78 pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml).
- `mc-core` runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-core` dev deps: `mc-fixtures` (path), `criterion = "0.5"` (workspace, default-features=false). Added in Phase 1B.
- `mc-fixtures` and `mc-cli` depend on `mc-core` only.
- **Cargo.lock pins (Phase 1B):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. These pre-edition2024 versions keep criterion buildable on Rust 1.78. Documented in [`PERF.md`](./PERF.md) §5.
- **Still deferred:** `proptest` and `insta` declared at workspace level only; not pulled into `mc-core`. The toolchain blocker is no longer the reason — they're paired with §10.7 doctrines and snapshot tests that are Phase 2 work. See CLAUDE.md §1.1.

---

## Open deferrals (Phase 1A acceptance criteria)

None. Acceptance criterion 5 (`cargo bench --release` under §11 1A ceilings) closed 2026-05-01 in Phase 1B. See [`PERF.md`](./PERF.md) §6 for the table and [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §6 for the closure record. One known fixture-mismatch (`write_input_leaf_no_deps`) documented in PERF.md §7.3 as a non-regression for Phase 2 attention.

All Phase 1A criteria (1–10) now satisfied. Full table in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §5.

---

## Deviations from the brief that are still in effect

These are documented in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §3–§4 and remain active until a future spec or amendment supersedes them:

1. **`proptest` / `insta` still out of `mc-core` dev-deps.** `criterion` was restored in Phase 1B (2026-05-01) via Cargo.lock transitive pins (`clap` → 4.4.18, `clap_lex` → 0.6.0, `half` → 2.4.1) — the §11 bench gate is now active. `proptest` and `insta` remain deferred for a different reason: the §10.7 doctrines and snapshot-style tests that need them are Phase 2 work, not Phase 1B scope. Pulling the crates in without using them would just lengthen `cargo build`. See CLAUDE.md §1.1.
2. **§10.1 dirty-set assertions reframed as deltas** — the bound is preserved (215); the comparison frame changed because `write_canonical_inputs` legitimately accumulates marks across 2,520 input writes.
3. **§10.5 `t_dependency_graph_rejects_undeclared_dependency_in_test_mode`** asserts `RuleBodyTypeMismatch` (registration-time) rather than `UndeclaredDependency` (runtime). Strictly stronger guarantee.
4. **§10.7 `doctrine_no_mutation_of_frozen_dimensions`** asserts `dim.is_frozen()` post-build because no public mutation API exists in Phase 1; `EngineError::DimensionFrozen` variant retained for Phase 2.
5. **§10.7 `doctrine_atomicity_of_write` and `doctrine_causality`** are no-op stubs per §0.A; deterministic equivalents in `tests/acme_demo.rs`.
6. ~~**§11.1 `bench_write_input_leaf_no_deps`** measures ~165 µs (1A ceiling: 50 µs).~~ **Closed 2026-05-01 in Phase 2A.** The synthetic minimal-hierarchy fixture `mc_fixtures::build_minimal_cube` now lets the brief's "no-dependents" cost be measured directly — see PERF.md §6.8. The Acme `bench_write_input_leaf_no_deps` row remains in `leaf_read_write.rs` as a documented Acme-fixture path measurement; the new `synthetic_no_deps::write_input_leaf_no_deps_synthetic` row evaluates the brief's 50 µs 1A ceiling.

---

## Known Phase 2 follow-ups

Source-tagged hooks and surfaced findings. **Not scheduled.** Full lists in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §8 (Phase 1A) and [`PERF.md`](./PERF.md) §8 / §9 (Phase 1B + Phase 2A).

**Phase 2A closed Phase 1B's measurement gaps.** **Phase 2B closed PERF.md §9.4** (consolidation hierarchy clone). **Phase 2C produced the workload-shaped data ADR-0003 anchored to** but did *not* pick a Phase 2D winner — see PERF.md §6.14 for the scaling-shape table that the next phase reads from.

Optimization candidates surfaced from current data:

- ~~Hierarchy-clone hot-path in `cube.rs::read_consolidated`.~~ **Closed in Phase 2B** ([PERF.md](./PERF.md) §6.11 + §9.4).
- Per-dim leaf-flag caching to fast-path `is_consolidated_coord` ([PERF.md §9.2](./PERF.md)). **Phase 2C signal:** *opportunistic* — combined-workflow data shows per-mark cost is flat at 50× (no within-session blow-up); §9.2's payoff is the per-write fixed cost, not session-length growth.
- Hierarchy mark closure cost (lazy ancestor marks or bitset-backed dirty tracker — [PERF.md §9.3](./PERF.md)). **Phase 2C signal:** *suggestive but not conclusive at session level* — per-mark cost flat across the 50× session contradicts the strongest §9.3 hypothesis (the AHashSet insert cost grows with set size). Cross-scale 1× → 100× growth is what §6.12.1 measures; Phase 2D's pick reads from there.
- `Snapshot` copy-on-write at scale (Phase 1 ships deep-clone — [PERF.md §9.5](./PERF.md)). **Phase 2C signal:** *stays deferred* — TM1 stacked-sandbox pattern (10 live snapshots at 50×) shows linear scaling, no super-linear stacked-depth tax.
- `CellStore` trait introduction (Phase 1 ships concrete `HashMapStore`).
- Lock-acquisition capability check hardening.
- Toolchain bump → unlocks `proptest` / `insta` for the §10.7 doctrines and any insta-driven snapshot tests ([PERF.md §9.7](./PERF.md) housekeeping checklist).

---

## Repo layout (top level)

```
.
├── crates/
│   ├── mc-core/           kernel
│   ├── mc-fixtures/       Acme demo cube
│   └── mc-cli/            `mc demo` runner
├── docs/                  this folder
├── research/              raw reference PDFs (TM1 manuals, books, infra specs)
├── CLAUDE.md              operating manual
├── README.md              workspace README
├── Cargo.toml             workspace manifest
├── Cargo.lock
└── rust-toolchain.toml    pins Rust 1.78
```

---

## How to update this file

When a phase ships:

1. Update **Last updated**, **Last commit**, **Branch**.
2. Update **What's shipping / What's queued / Active ADRs**.
3. Update the build / test / lint state table.
4. Update the test count table if any tests were added.
5. Move closed deferrals out of the table; add closure dates to the relevant phase report.
6. Add an ADR if a new scope-level decision was made; link it in **Active ADRs**.

When a deferral closes (e.g. `cargo bench` becomes unblocked):

1. Move the row out of **Open deferrals**.
2. Update the relevant report's §6 to reflect closure.
3. If the closure required a decision (e.g. "we chose to bump Rust to 1.85"), write an ADR.
