# CURRENT_STATE

> **What's live right now.** Update whenever a phase ships, a gate flips, or a deferral closes.

**Last updated:** 2026-05-02 (Phase 3A shipped at `603c537`, tagged `phase-3a-model-definition-layer`. ADR-0005 Accepted later same day with 15 project-owner acceptance amendments (10 from GPT + 5 from Claude Desktop); Phase 3B flipped from `not started` → `proposed`; Phase 3B handoff drafted at `docs/handoffs/phase-3b-handoff.md`.)
**Last Phase 1A commit:** `bee2812` — *mc-core: update lib.rs doc-comment to point at docs/specs/* (Phase 1A kernel at `4aa674a`)
**Last Phase 1B + Phase 2A commit:** `48d52e9` — *bench: complete Phase 2A cold-path benchmark expansion* (Phase 1B and Phase 2A bundled into one commit; tag `phase-2a-cold-path-baseline` at this hash)
**Phase 2B commit / tag:** `6ea58ab` (tag `phase-2b-consolidation-fast-path`)
**Phase 2 housekeeping Q3 closure commit:** `9f7420c`
**Phase 2C commit / tag:** `789db15` (tag `phase-2c-workload-baseline`)
**Phase 2D commit / tag:** `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`)
**Phase 3A commit / tag:** `603c537` (tag `phase-3a-model-definition-layer`)
**Branch:** `main` (tracking `origin/main` at github.com/edwinlov3tt/mc-v2)

---

## What's shipping

- **Phase 1A — Rust kernel for the Acme demo.** Complete. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).
- **Phase 1B — Benchmark Baseline + PERF.md.** Complete 2026-05-01. Acceptance criterion 5 closed via Cargo.lock transitive pins (no toolchain bump). See [`PERF.md`](./PERF.md).
- **Phase 2A — Cold-Path Benchmark Expansion.** Complete 2026-05-01. Both Phase 1B measurement gaps closed: cold consolidation rows added against §11.2 ceilings (PERF.md §6.7); synthetic no-deps write fixture added against §11.1 50 µs ceiling (PERF.md §6.8). Two new diagnostic suites (snapshot clone PERF.md §6.9; hierarchy ancestor mark microbench PERF.md §6.10). **No `crates/mc-core/src/` files modified.** See [`reports/phase-2a-completion-report.md`](./reports/phase-2a-completion-report.md).
- **Phase 2B — Consolidation Fast Path.** Complete 2026-05-01, committed at `6ea58ab` (tag `phase-2b-consolidation-fast-path`). One targeted kernel change in [`cube.rs::read_consolidated`](../crates/mc-core/src/cube.rs) plus a `Vec<Arc<Hierarchy>>` shape change in [`dimension.rs`](../crates/mc-core/src/dimension.rs); replaces per-call `Vec<Dimension>` + `Vec<Hierarchy>` deep-clones with one `Arc::clone` + a `Vec<Arc<Hierarchy>>` collect (refcount-bumps). PERF.md §6.7 3-leaf cold consol drops 14.3 µs → **2.53 µs** (clears brief §11.2 1B target ≤ 3 µs); every other §6.7 row improves by ~12 µs absolute. New kernel unit test `consecutive_recompute_reads_match_phase_2b` (handoff item 3). One contract test rewritten (`t_consolidation_caches_value_within_revision`, semantic-not-timing) per ADR-0002 + the SPEC QUESTION round-trip approval. See [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md) and [`PERF.md`](./PERF.md) §6.11 + §9.4 + §10.
- **Phase 2C — Production-Shaped Workload Benchmarks.** Complete 2026-05-02, committed at `789db15` (tag `phase-2c-workload-baseline`). Measurement-only phase; **no `crates/mc-core/src/` change.** Adds internal `mc_fixtures::build_scaled_acme_cube(scale)` (`pub(crate)`) + three public wrappers `_10x` / `_50x` / `_100x` + 6 unit tests including the mandatory scale-1× equivalence test against brief §4.5.1 anchor goldens. Adds 27 new bench rows extending the existing five Phase 1B/2A bench files at 10× / 50× / 100×. Adds new [`combined_workflow.rs`](../crates/mc-core/benches/combined_workflow.rs) that simulates a 100-iteration planner session at 50× (100× attempted then abandoned) with stacked-snapshot hold (TM1 sandbox pattern per ADR-0003 Decision 6). PERF.md §6.12 / §6.13 / §6.14 written from the gate run. Headline finding: `load_canonical_inputs` super-linear cliff between 10× (4.33×/write) and 50× (19.7×/write) — points at §9.3 as the Phase 2D candidate. **Did not pick a Phase 2D winner** in §9; the pick is in [`handoffs/phase-2d-handoff.md`](./handoffs/phase-2d-handoff.md). See [`reports/phase-2c-completion-report.md`](./reports/phase-2c-completion-report.md).
- **Phase 3A — Model Definition Layer (`mc-model` crate).** **Complete 2026-05-02, committed at `603c537` (tag `phase-3a-model-definition-layer`).** Ships a new `crates/mc-model/` crate that translates a human-authored YAML cube definition into an `mc_core::Cube` via the three-stage pipeline per [ADR-0004](./decisions/0004-phase-3a-model-definition-format.md) Decision 9: YAML bytes → `ParsedModel` → `ValidatedModel` → `Cube`. Each stage has its own error type (`ParseError` / `ValidationError` / `EngineError`) so blame is unambiguous (Phase 4's LLM-feedback loop and Phase 6's UI editor consume these). The Acme cube is re-expressed as [`crates/mc-model/examples/acme.yaml`](../crates/mc-model/examples/acme.yaml) (264 lines, 9 inline goldens covering brief §4.5.1 anchor values + 1 consolidation rollup). `mc-cli` gains a `--model <path>` flag that routes through `mc_model::load`. **Acceptance gate cleared:** `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` produces empty output (byte-for-byte stdout equality between Rust and YAML paths). 14 validator negative tests cover ADR-0004 Decision 6's 10-row table (one extra split for the 10th row's structural vs value sides). Structural-equivalence test diffs YAML-loaded Acme against `build_acme_cube()` on dim count, element names, hierarchy edges, measure metadata, weight-measure targets, and rule body shapes. **`mc-core` not modified** — same 4 runtime deps as Phase 2D. **`mc-fixtures` not modified** — `build_acme_cube()` byte-for-byte unchanged. **Toolchain stayed at Rust 1.78** — `serde_yaml 0.9.34`'s transitive `indexmap 2.14.0` pinned to `2.7.0` per [ADR-0004](./decisions/0004-phase-3a-model-definition-format.md) Decision 3 escape hatch (Phase 1B precedent); ADR-0005 was *not* opened. See [`reports/phase-3a-completion-report.md`](./reports/phase-3a-completion-report.md).
- **Phase 2D — Bitset-Backed Dirty Tracker + WritebackResult.invalidated semantic correction.** Complete 2026-05-02, committed at `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`). Acceptance gate cleared by ~47×: `load_canonical_inputs/50x` drops from 230.80 s → **1.06 s (−99.5 %)**; 100× ingest (abandoned at >38 min in phase-2c) now runs in **2.13 s**. Two changes shipped per [Phase 2D handoff §A](./handoffs/phase-2d-handoff.md): (1) `DirtyTracker` internal repr replaced with a Cartesian-product flat bitset behind `Arc<CubeShape>` (foundation), and (2) `WritebackResult.invalidated` semantic correction in `cube.rs::write` from cumulative-dirty (Phase 1A reading of brief line-1938 pseudocode shorthand) to marginal-per-write (brief type-doc + engine-semantics.md §13 + I-WB-7 reading). A/B isolation confirmed the writeback semantic correction is the load-bearing change for the §6.14 cliff; the bitset is enabling (makes `is_dirty` O(1) so the marginal capture is bounded by per-write fan-out, not cumulative set size) but moves the cliff by < 0.2 % in isolation. New test file [`tests/writeback_invalidated.rs`](../crates/mc-core/tests/writeback_invalidated.rs) with five tests pinning the marginal semantics. Public API surface unchanged; the brief's `WritebackResult.invalidated: Vec<CellCoordinate>` field name + type + re-export are byte-for-byte identical — only the *contents* differ per the spec audit in [PERF.md §6.15](./PERF.md). See [`reports/phase-2d-completion-report.md`](./reports/phase-2d-completion-report.md).

## What's queued

- **Phase 2 housekeeping — Q3 (criterion baseline tracking).** **Closed retroactively 2026-05-01.** Workflow proven end-to-end at commit `9f7420c`. Both `phase-2a` and `phase-2b` baselines captured under [`reports/bench-data/`](./reports/bench-data/) (1.4 MB JSON; 45 rows × 2 phases × 4 files). Phase 2C onward must use `cargo bench -p mc-core --bench <name> -- --baseline phase-2b`. See [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md) §6.A.1 for the closure record. **Phase 2C extended this to a third baseline:** `phase-2c` saved under [`reports/bench-data/phase-2c/`](./reports/bench-data/phase-2c/). **Phase 2D extended this to a fourth baseline:** `phase-2d` saved under [`reports/bench-data/phase-2d/`](./reports/bench-data/phase-2d/) (post-2D corrected-semantics + bitset baseline at sample-size 10).
- **Phase 2 housekeeping — Q1 (workload sketch ADR).** **Accepted (provisional) 2026-05-01.** [`decisions/0003-workload-sketch.md`](./decisions/0003-workload-sketch.md) — sunset clause auto-flips status to "Needs revision" on first real planner usage data or 2026-11-01, whichever comes first. The workload curve (10× / 50× / 100× Acme) and 100 ms click-instant threshold from this ADR are what Phase 2C calibrates against. **Phase 2C produced the workload-shaped data ADR-0003 anchored to;** ADR-0003 stays Accepted — Provisional, no amendment yet. **Phase 2D's measured 50× ingest at 1.06 s is well within ADR-0003's 10 s patience-limit gate** (the metric was Phase 2D's acceptance contract).
- **Phase 2 housekeeping — Q2 (toolchain bump).** Deferred until any new runtime dep needs it (likely Phase 3A's parser dep choice).
- **Phase 3B — Model QA, Linter, and Diagnostics — proposed.** [ADR-0005](./decisions/0005-phase-3b-model-qa-linter-diagnostics.md) Accepted 2026-05-02 with 15 acceptance amendments. Phase 3B adds a read-only quality + diagnostics layer over `mc-model`: four CLI subcommands (`mc model validate / inspect / lint / test`); 10 starting lint rules (MC3001–MC3007 + MC3009–MC3011, with MC3008 permanently retired and promoted to validation as MC2011); structured `Diagnostic` shape with stable codes and JSON envelope `{schema_version: "1.0", diagnostics: [...]}` for Phase 4 LLM consumption + Phase 6 UI consumption; deterministic emission order. **Acceptance gate:** Acme lints clean with zero warnings; intentionally-flawed fixtures trigger each rule; `mc demo --model` does NOT run goldens (separation of concerns); ≥ 252 tests still pass; `mc-core` and `mc-fixtures` untouched. **Handoff at [`handoffs/phase-3b-handoff.md`](./handoffs/phase-3b-handoff.md)** is the implementation contract.
- **Phase 3C — Friendly formula syntax (TBD).** Not started. `Revenue = Customers * AOV` strings compile down to `ParsedRuleBody`'s structured tree per ADR-0004 Decision 4. Phase 3C runs after Phase 3B per ADR-0005's "What this unlocks" rationale (diagnostics first, formulas second). No handoff yet.

## Active ADRs

- [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) — Phase 1 scope: smallest kernel that runs the Acme demo. **Status:** Accepted.
- [`decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](./decisions/0002-perf-assertions-in-benchmarks-not-tests.md) — Performance assertions belong in criterion benchmarks, not in `cargo test`. **Status:** Accepted (Phase 2B). Authorizes the `t_consolidation_caches_value_within_revision` rewrite from a wall-clock ratio to semantic cache-state assertions.
- [`decisions/0003-workload-sketch.md`](./decisions/0003-workload-sketch.md) — Workload sketch & perception thresholds (Phase 2 housekeeping Q1). **Status:** Accepted — Provisional. Sunset clause: auto-flips to "Needs revision" on first real planner usage data, or 2026-11-01, whichever comes first. Defines the workload curve (10× / 50× / 100× Acme) and 100 ms click-instant threshold that Phase 2C calibrates against.
- [`decisions/0004-phase-3a-model-definition-format.md`](./decisions/0004-phase-3a-model-definition-format.md) — Phase 3A model-definition format & parser scope. **Status:** Accepted (with project-owner acceptance amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: YAML safe subset; `mc-model` crate; no parser deps in `mc-core`; structured expression trees for rules (formula strings deferred to Phase 3C); one cube per file; exhaustive blocking validation; inline golden tests; LLM authoring is Phase 4 not 3A; mandatory three-stage `YAML → ParsedModel → ValidatedModel → Cube` pipeline. Phase 3A handoff at [`handoffs/phase-3a-handoff.md`](./handoffs/phase-3a-handoff.md).
- [`decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](./decisions/0005-phase-3b-model-qa-linter-diagnostics.md) — Phase 3B Model QA, Linter, and Diagnostics. **Status:** Accepted (with 15 project-owner acceptance amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: four error layers (parse < validation < golden < lint, with golden exclusively `mc model test`'s responsibility, not `mc demo`); lint advisory by default with `mc_model::load()` ignoring lint output unconditionally; four CLI subcommands (`mc model validate / inspect / lint / test` plus `--format text|json`); `inspect` summary covering 11 fields; 10 starting lint rules (MC3001–MC3007 + MC3009–MC3011, MC3008 permanently retired and promoted to MC2011 in validation); strict out-of-scope (no formula strings, LLM, UI, actuals, mc-core changes, auto-fix); JSON envelope `{schema_version: "1.0", diagnostics: [...]}` with deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)`; 15-item success gate including Acme lints clean with zero documented warnings; no Rust toolchain bump (hand-rolled snapshot fixtures preferred over `insta`). Phase 3B handoff at [`handoffs/phase-3b-handoff.md`](./handoffs/phase-3b-handoff.md).

---

## Build / test / lint state (at HEAD)

| Gate | Command | Status |
|---|---|---|
| Build | `cargo build --release --workspace` | ✓ zero warnings |
| Format | `cargo fmt --check --all` | ✓ |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ |
| Tests | `cargo test --workspace` | ✓ **252 / 0** (was 227; +25 from Phase 3A: 6 mc-model unit tests + 3 parse_validate_smoke + 1 structural_equivalence + 14 validators + 1 golden_acme) |
| Determinism (10×) | `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | ✓ 10 / 10 identical at 252 / 0 each run |
| CLI demo (Rust path) | `./target/release/mc demo` | ✓ matches brief §4.6 |
| CLI demo (YAML path) | `./target/release/mc demo --model crates/mc-model/examples/acme.yaml` | ✓ matches brief §4.6 |
| **Acceptance gate** | `diff <(./target/release/mc demo) <(./target/release/mc demo --model ...)` | **✓ empty output** (Phase 3A headline) |
| Benchmarks | `cargo bench --workspace` | ✓ Phase 1B baseline + Phase 2A cold-path expansion + Phase 2B fast path + **Phase 2C workload-shaped benches** all green. Numbers in [`PERF.md`](./PERF.md) §6 (Phase 1B), §6.7–§6.10 (Phase 2A), §6.11 (Phase 2B before/after), and **§6.12 / §6.13 / §6.14 (Phase 2C 10× / 50× / 100× rows + combined-workflow + scaling-shape summary)**. Phase 2C scaled rows compared against `--baseline phase-2b`; no Phase 1B/2A/2B regression beyond ±10% noise. **Phase 2C did not pick a Phase 2D winner** — §9 row priorities stay unspecified per the handoff hard rule. |

---

## Test count by target

| Target | Count |
|---:|---|
| `mc-core` unit tests | 90 |
| `tests/acme_demo.rs` | 20 |
| `tests/writeback.rs` | 11 |
| `tests/writeback_invalidated.rs` (Phase 2D) | 5 |
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
| `mc-model` unit tests (Phase 3A — `src/parse/tests`) | 6 |
| `mc-model` `tests/parse_validate_smoke.rs` (Phase 3A) | 3 |
| `mc-model` `tests/structural_equivalence.rs` (Phase 3A) | 1 |
| `mc-model` `tests/validators.rs` (Phase 3A — one negative test per ADR-0004 Decision 6 row) | 14 |
| `mc-model` `tests/golden_acme.rs` (Phase 3A — runs the 9 inline goldens from acme.yaml) | 1 |
| **Total** | **252** |

`mc-core` unit tests are 90 (was 84) after Phase 2D added 4
`cube_shape::tests` (cardinality + linearize round-trip + arity
mismatch + unknown element) and 2 `dirty::tests` (bitset
equivalence under a long mixed mark/clear/clear_all script + bitset
mark_closure parity with the AHashSet path) per Phase 2D handoff
item 4 + §A.6. New file [`tests/writeback_invalidated.rs`](../crates/mc-core/tests/writeback_invalidated.rs)
adds 5 tests (A–E) pinning the corrected marginal semantics of
`WritebackResult.invalidated` — Test D ("bulk-ingest preserves the
§10.1 per-write bound") is the regression net that, had it
existed, would have caught the Phase 1A bug originally. Phase 2C
added 6 tests in `mc-fixtures` (10 → 16): mandatory scale-1×
equivalence test + invariant tests at 10× / 50× / 100× +
extra-leaf round-trip at 10× + scale-zero rejection.

---

## Toolchain + dependency state

- Rust 1.78 pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml).
- `mc-core` runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-core` dev deps: `mc-fixtures` (path), `criterion = "0.5"` (workspace, default-features=false). Added in Phase 1B.
- `mc-fixtures` and `mc-cli` depend on `mc-core` only.
- **Cargo.lock pins (Phase 1B):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. These pre-edition2024 versions keep criterion buildable on Rust 1.78. Documented in [`PERF.md`](./PERF.md) §5.
- **Cargo.lock pins (Phase 3A):** `indexmap → 2.7.0`, `hashbrown → 0.15.5`. Pre-edition2024 versions keep `serde_yaml 0.9.34` buildable on Rust 1.78. Per ADR-0004 Decision 3 escape hatch (Phase 1B precedent reused). ADR-0005 was *not* opened — toolchain stays at 1.78.
- **`mc-model` (new in Phase 3A) runtime deps:** `serde 1` (derive), `serde_yaml 0.9.34`, `thiserror`. Dev-deps: `mc-fixtures` (path).
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

**Phase 2A closed Phase 1B's measurement gaps.** **Phase 2B closed PERF.md §9.4** (consolidation hierarchy clone). **Phase 2C produced the workload-shaped data ADR-0003 anchored to** but did *not* pick a Phase 2D winner — see PERF.md §6.14 for the scaling-shape table that the next phase reads from. **Phase 2D closed PERF.md §9.3** by shipping the bitset and (per the SPEC QUESTION amendment §A) correcting `WritebackResult.invalidated` from cumulative to marginal semantics; A/B isolation in [PERF.md §6.15](./PERF.md) shows the writeback semantic correction is the load-bearing change for the §6.14 cliff and the bitset is enabling foundation rather than the closer.

Optimization candidates surfaced from current data:

- ~~Hierarchy-clone hot-path in `cube.rs::read_consolidated`.~~ **Closed in Phase 2B** ([PERF.md](./PERF.md) §6.11 + §9.4).
- Per-dim leaf-flag caching to fast-path `is_consolidated_coord` ([PERF.md §9.2](./PERF.md)). **Phase 2C signal:** *opportunistic* — combined-workflow data shows per-edit total cost is flat at 50× across the session (≈ 422 µs amortized over `dirty_delta`; no within-session blow-up); §9.2's payoff is the per-write fixed cost, not session-length growth. **Phase 2D update:** combined-workflow per-edit cost dropped to ~11 µs at 50× (was ~2.4 ms; ~200× faster) as a side-effect of the writeback semantic correction; §9.2's payoff window is much smaller now.
- ~~Hierarchy mark closure cost.~~ **Closed in Phase 2D** ([PERF.md §6.15](./PERF.md) + §9.3 closure note). The §6.14 cliff was attributable to the cumulative-`invalidated`-collection bug, not to the AHashSet hash cost the Phase 2C handoff framing assumed; the bitset shipped as the structural foundation but moves the cliff by < 0.2 % in isolation.
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
│   ├── mc-model/          model definition layer (Phase 3A) — YAML → mc_core::Cube
│   └── mc-cli/            `mc demo` runner (Phase 3A: + `--model <path>` flag)
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
