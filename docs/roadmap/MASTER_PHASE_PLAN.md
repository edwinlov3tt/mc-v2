# MASTER_PHASE_PLAN

> **The single source of truth for what phase the project is in and what comes next.**
>
> Read this before inventing a phase name or starting work that isn't already on the list. If a new phase is needed and it isn't here, add it here first (and link the ADR explaining the decision).

**Last updated:** 2026-05-02 (post-Phase 2C)
**Maintained by:** project lead. New sub-phases require an ADR in [`../decisions/`](../decisions/).

---

## Product vision

**MarketingCubes** is a TM1-inspired multidimensional planning kernel and authoring layer for marketing finance and media planning. It models the full marketing-to-revenue funnel — channel × market × time × scenario × version — with declarative rules, sparse storage, fast consolidation, snapshot-based what-if analysis, and a deterministic recompute pipeline. Later phases add a model-definition layer (so cubes can be authored without writing Rust), an LLM-assisted authoring path, real-data integrations (actuals from media platforms), an internal UI, and customer-facing applications. The North Star is a planning tool a media-business operator can use to author a forecast, see it consolidated correctly, edit it confidently, compare versions safely, and ground it in real data.

---

## What "done" means for the first usable product

The first usable product (target: end of Phase 6) is the smallest version one operator can use end-to-end without engineering help. Concretely:

1. An internal user can **author a cube model** (dimensions, hierarchies, measures, rules) without writing Rust — through a config format or schema-validated authoring layer.
2. The cube can be **loaded with real actuals** from at least one external source (e.g. a media platform's reporting API or a CSV export with a documented schema).
3. A **web UI presents the cube** as a navigable planning grid with drill-down, edit, snapshot/rollback, and a version comparison view.
4. **Performance meets phase-specific targets on a representative production-sized cube** — initially calibrated against ≥ 50K populated cells, ≥ 8 dimensions, and realistic hierarchy depth. The exact shape (per-dim hierarchy depth, derived-measure count, scenario fan-out) is a Phase 2 housekeeping deliverable (see "Phase 2 housekeeping" below); the brief §11 1B targets are the starting calibration but will be re-anchored to user-perception thresholds (sub-100 ms = instant, sub-1 s = responsive, multi-second = needs progress UI) once that sketch lands.
5. The system has **authentication, an audit trail, and multi-user concurrency** sufficient for an internal team of 5–10 planners.
6. There is **at least one shipped proof-of-value internal use case** demonstrating that the system produces a correct plan a human operator trusts.

Productization beyond the first usable product (multi-tenancy, customer-facing apps, billing, scaling) is Phase 7 and explicitly out of scope for "first usable product."

---

## Phase status overview

| Phase | Name | Status | Tag |
|---|---|---|---|
| **1A** | Rust kernel for the Acme demo | **complete** | `4aa674a` (initial) |
| **1B** | Benchmark baseline + PERF.md | **complete** | bundled into `phase-2a-cold-path-baseline` (`48d52e9`) — they shipped in the same commit; no standalone `phase-1b` tag was cut |
| **2A** | Cold-path benchmark expansion | **complete** | `phase-2a-cold-path-baseline` (`48d52e9`) |
| **2B** | Consolidation Fast Path (hierarchy clone) | **complete** | `phase-2b-consolidation-fast-path` (`6ea58ab`) |
| **2C** | Production-Shaped Workload Benchmarks | **complete** | `phase-2c-workload-baseline` (`789db15`) |
| **2D** | Bitset-Backed Dirty Tracker + WritebackResult.invalidated semantic correction (§9.3 closure) | **complete** | `phase-2d-bitset-and-invalidated-fix` (`0678a98`) |
| **2E–2N** | Further optimization rounds (TBD) | not started | — |
| **3A** | Model definition layer — YAML + `mc-model` crate (per ADR-0004) | **complete** (report at [`../reports/phase-3a-completion-report.md`](../reports/phase-3a-completion-report.md)) | — *(uncommitted; user reviews + tags)* |
| **3B–3N** | Model layer extensions (TBD) | not started | — |
| **4** | LLM-assisted model authoring | not started | — |
| **5** | Data integration & actuals | not started | — |
| **6** | UI & internal app proofs (incl. internal Media Partner model proof) | not started | — |
| **7** | Productization (customer-facing Media Partner App + multi-tenancy) | not started | — |

**Status legend.**
- **complete** — shipped and tagged.
- **proposed** — handoff doc exists; next to start. **At most one row at a time.**
- **planned** — committed to but not yet promoted to `proposed`; flips when the phase ahead of it ships.
- **not started** — no scoping yet.

The "How to use" section below treats `proposed` as the next-to-start row; `planned` rows are sequenced but not yet active. This avoids two `proposed` rows leading the queue.

**Numbering rule.** Major phases (1, 2, 3, …) are pillars of capability. Sub-phases (2A, 2B, 3A, …) are concrete shippable increments inside a pillar. Don't invent a sub-phase name without first adding it here. If a sub-phase needs to be split, append a new letter (2A → 2A.1 / 2A.2) or open a new sub-phase (2B → 2B.1 / 2B.2). Don't reuse retired letters.

---

## Phase 1 — Kernel Foundation

> Build the smallest correct, deterministic, single-threaded Rust kernel that runs the Acme demo end-to-end. Establish the spec and the build contract; nothing about UI or data integration belongs here.

### 1A — Rust kernel for the Acme demo (complete)

- **Status:** complete (2026-05-01).
- **Purpose:** Implement the brief's §3 types, §10 tests, and §4.6 demo. Establish the spec hierarchy (engine-semantics > brief > CLAUDE.md > intuition).
- **What it proves:** The dimensional model — coordinates, hierarchies, rules, dirty propagation, consolidation with weighted average, snapshots — works correctly on a 6-dim, 11-measure, 5-rule, 2,520-input-cell fixture with deterministic results.
- **Deliverables:** [`reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md). Three crates (`mc-core`, `mc-fixtures`, `mc-cli`); 203 tests passing across §10.1–§10.8; `target/release/mc demo` matches brief §4.6.
- **Acceptance gates (all met):** zero clippy warnings; zero `unwrap()` in `mc-core/src/`; 10/10 deterministic test runs identical; allowed deps only (`smallvec`, `ahash`, `thiserror`, `once_cell`).
- **Out of scope (explicit):** SIMD, threads, async, `serde`, `CellStore` trait, snapshot COW, model authoring, UI.

### 1B — Benchmark baseline + PERF.md (complete)

- **Status:** complete (2026-05-01).
- **Purpose:** Close acceptance criterion 5 (`cargo bench` under §11 1A ceilings) and produce a measurement baseline before any optimization decision.
- **What it proves:** Phase 1A's "obviously-naive-but-not-pathological" implementation is in fact obviously-not-pathological — the kernel ships well within design constraints on a representative machine.
- **Deliverables:** [`PERF.md`](../PERF.md) §1–§6.5; five criterion bench files in `crates/mc-core/benches/`; criterion 0.5 working on Rust 1.78 via three Cargo.lock transitive pins.
- **Acceptance gates (all met):** all gates from 1A still pass; eight directly-comparable §11 1A ceilings cleared.
- **Out of scope (explicit):** any kernel optimization; any Rust toolchain bump; cold-path measurements (deferred to 2A).

---

## Phase 2 — Performance & Optimization

> Drive the kernel toward the brief's §11 1B targets, **measure-first then optimize**. Each sub-phase pairs measurement with at most one source change. Phase 2 ends when there is no remaining 1B target whose miss is justified by data and unaddressed.

### 2A — Cold-path benchmark expansion (complete)

- **Status:** complete (2026-05-01).
- **Purpose:** Close the two measurement gaps Phase 1B left (cold consolidation; synthetic no-deps write) and add two adjacent diagnostic suites (snapshot clone; hierarchy ancestor mark microbench) so 2B+ can prioritize from data, not guesswork.
- **What it proves:** The kernel's true consolidation cost (cold) is well under §11.2 1A ceilings; the brief §11.1 `bench_write_input_leaf_no_deps` ceiling is measurable on a synthetic minimal-hierarchy fixture and clears by ~200×; the dominant Acme write cost is per-mark CellCoordinate allocation + AHashSet insert, not hierarchy traversal.
- **Deliverables:** [`reports/phase-2a-completion-report.md`](../reports/phase-2a-completion-report.md); [`PERF.md`](../PERF.md) §6.6–§6.10 + updated §7–§10; three new bench files; new `mc-fixtures::build_minimal_cube` + `build_graduated_hierarchy_cube`.
- **Acceptance gates (all met):** all 1A/1B gates still pass; cold-state verification (`assert!(cube.dirty().is_dirty(...))`) runs before every cold timing; goldens verified pre-timing; 209/209 tests pass.
- **Out of scope (explicit):** any kernel source change; any Phase 2B optimization work.

### 2B — Consolidation Fast Path (complete)

- **Status:** complete (2026-05-01). Tag `phase-2b-consolidation-fast-path` at `6ea58ab`.
- **Purpose:** Eliminated the per-call hierarchy/dimension clone in [`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs) — the ~14 µs fixed-cost floor that caused the brief §11.2 3-leaf 1B target (3 µs) to miss by ~5×.
- **What it proves:** The kernel's consolidation algorithm hits every brief §11.2 1B target once the single localized over-cloning is removed. The 3-leaf row drops 14.3 µs → 2.53 µs (clears ≤ 3 µs); every higher-fan-out cold row improves by ~12 µs absolute. Warm rows + every adjacent benched row hold within ±10% noise.
- **Deliverables (shipped):** kernel change in [`cube.rs`](../../crates/mc-core/src/cube.rs) + [`dimension.rs`](../../crates/mc-core/src/dimension.rs) (Option A — `Arc<Vec<Dimension>>` + `Vec<Arc<Hierarchy>>`); new kernel unit test `consecutive_recompute_reads_match_phase_2b` (handoff §3); rewrite of `t_consolidation_caches_value_within_revision` from a single-shot wall-clock ratio to semantic cache-state assertions per [ADR-0002](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md); [PERF.md §6.7 + §6.11 + §9.4 + §10](../PERF.md); [`reports/phase-2b-completion-report.md`](../reports/phase-2b-completion-report.md).
- **Acceptance gates (all met):** brief §11.2 3-leaf 1B target ≤ 3 µs cleared at 2.53 µs (every other §6.7 row also clears 1B); 210 / 0 tests pass (was 209 + 1 new); 10 / 10 deterministic; release demo matches brief §4.6; no clippy warnings; no public API change; no new dependency; no `Cargo.lock` change; no toolchain bump.
- **Out of scope (held):** §9.3 hierarchy mark closure changes; any new dependency; any public API change; any work beyond `cube.rs` / `dimension.rs` source files (`hierarchy.rs` was authorized but no change was needed).

### 2C — Production-Shaped Workload Benchmarks (complete)

- **Status:** complete (2026-05-02). Tag `phase-2c-workload-baseline` at `789db15`.
- **Purpose:** Calibrate the kernel against ADR-0003's 10× / 50× / 100× Acme curve and produce the workload-shaped data Phase 2D needs to pick between PERF.md §9.3 (bitset-backed dirty tracker), §9.2 (leaf-flag cache), or something else the data surfaces. **Measurement only — no `crates/mc-core/src/` change.**
- **What it proves:** The kernel's per-edit and per-read cost shape across a 100× cube-size range is (a) tractable for measurement at criterion's minimum sample size of 10 (sample-of-100 is prohibitive at 100× because per-iteration setup includes a 252K-write bulk-load), (b) bounded by ADR-0003's 100 ms click-instant budget at 50× combined-workflow scale (per-edit p99 ≈ 2.5 ms within a 100-iteration session), and (c) **flat per-edit-amortized cost across a session at 50×** (≈ 434 → 430 → 439 µs at iters 1 / 50 / 100, computed as `edit_time ÷ dirty_delta`; see PERF.md §6.13.2 unit caveat — the bench labels the divisor unit "ns" but the result magnitude is µs, not ns). Within-session flatness is *consistent with* §9.3 once the dirty set is saturated, but does not isolate the AHashSet-insert component on its own; the load-bearing §9.3 evidence is the cross-scale `load_canonical_inputs` cliff in §6.12.7.
- **Deliverables (shipped):** internal `mc_fixtures::build_scaled_acme_cube(scale)` (`pub(crate)`) + 3 public wrappers `_10x` / `_50x` / `_100x` + 6 unit tests including the mandatory scale-1× equivalence test against brief §4.5.1 anchor goldens; 27 new bench rows extending the existing five Phase 1B/2A bench files at 10× / 50× / 100×; new [`combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs) at 50× and 100× (TM1 stacked-sandbox pattern per ADR-0003 Decision 6); [PERF.md](../PERF.md) §6.12 / §6.13 / §6.14 (new) plus updated §7 / §8 / §9 (priorities deliberately unspecified per the handoff hard rule); [`reports/phase-2c-completion-report.md`](../reports/phase-2c-completion-report.md); `bench-data/phase-2c/` populated.
- **Acceptance gates (all met):** 216 / 0 tests pass (was 210; +6 net additions); 10 / 10 deterministic; release demo matches brief §4.6 (kernel unchanged); fmt / clippy / build green; **no `crates/mc-core/src/` or `crates/mc-core/tests/` modification**; no new dependency; no `Cargo.lock` change; no `rust-toolchain.toml` change. **Did not pick a Phase 2D winner** — §9 row priorities deliberately unspecified.
- **Out of scope (held):** any kernel source change; any §9.3 or §9.2 implementation work; any new dependency.

### 2D — Bitset-Backed Dirty Tracker + WritebackResult.invalidated semantic correction (complete; pending review)

Phase 2D opened on **Branch A — §9.3** (bitset-backed dirty tracker) per PERF.md §6.14's `load_canonical_inputs` super-linear cliff hypothesis. **The handoff diagnosis was wrong:** measurement showed the bitset alone moves 50× ingest by **+4 % (within criterion noise)** — see PERF.md §6.15.3 A/B isolation. The actual cause of the §6.14 cliff was at [`cube.rs::write`](../../crates/mc-core/src/cube.rs)'s construction of `WritebackResult.invalidated`, which Phase 1A implemented as the *cumulative* dirty set (`self.dirty.iter().cloned().collect()`) in disagreement with the brief's own type doc + engine-semantics.md §13. Per the [Phase 2D handoff §A](../handoffs/phase-2d-handoff.md) amendment approved 2026-05-02 (SPEC QUESTION round-trip), Phase 2D scope expanded to include the writeback semantic correction. Result: 50× ingest **230.80 s → 1.06 s (−99.5 %)**, beats the ≤ 50 s gate by ~47×.

- **Status:** complete (2026-05-02). Tag `phase-2d-bitset-and-invalidated-fix` at `0678a98`. Completion report at [`../reports/phase-2d-completion-report.md`](../reports/phase-2d-completion-report.md).
- **Approach (shipped):** (1) Cartesian-product flat bitset (`Vec<u64>` + sticky `ever_marked` bitset + insertion-order `tracked` Vec with cached bit indices) behind `Arc<CubeShape>`; `DirtyTracker` public method signatures preserved byte-for-byte; new `pub(crate) fn with_shape(Arc<CubeShape>)` constructor used by `CubeBuilder::build`. (2) `WritebackResult.invalidated` semantic correction in `Cube::write`: the field's *contents* are now the marginal per-write transition set (coords this write transitioned clean → dirty), not the cumulative dirty state — same field name + type + re-export, no public API surface change. The bitset is the foundation that makes the corrected per-write `is_dirty` check O(1) so the marginal capture is bounded by per-write fan-out (~216 at Acme, §10.1) rather than the cumulative set size.
- **Acceptance gate:** PERF.md §6.12.7 `load_canonical_inputs/50x` ≤ 50 s — **met by 47×** (1.06 s). Secondary expectation (combined-workflow per-edit-amortized stays within ±10 % of ≈ 422 µs) **met and exceeded** — new median ≈ 2.05 µs at iter-100 (~200× improvement, side-effect of the writeback correction); within-session shape stays flat (3.7 → 2.06 → 2.05 µs at iter 1 / 50 / 100).
- **Source touched:** `crates/mc-core/src/cube_shape.rs` (NEW), `crates/mc-core/src/dirty.rs`, `crates/mc-core/src/cube.rs`, `crates/mc-core/src/lib.rs` (one `mod` line). Tests: `crates/mc-core/tests/writeback_invalidated.rs` (NEW; 5 tests A–E pinning the marginal semantics). Bench preflight wording fixes per handoff §A.7 in `dirty_propagation.rs` + `hierarchy_mark.rs` + `combined_workflow.rs` (no behavior change).
- **A/B isolation (handoff §A.5):** the bitset alone moves 10× and 50× ingest by < 0.2 % (within noise) — the writeback semantic correction is the load-bearing change for the §6.14 cliff. The bitset still ships as the structural foundation for any future dirty-tracker optimization and for the marginal capture's O(1) `is_dirty`.

### 2E, 2F, … (TBD)

Sub-phases beyond 2D are intentionally not pre-named. Whether 2E exists depends on what Phase 2D's bench data + the Phase 2C 50× / 100× env-gated rows reveal once they're opted in. Likely candidates if needed (priority order is **what 2E decides**, not pre-pinned):

- **§9.2 leaf-flag cache** on `Element` (`is_leaf_in_default_hierarchy: bool`). Trivial; opportunistic; payoff is per-write fixed cost.
- **§9.5 Snapshot COW.** Phase 2C signal: stays deferred. TM1 stacked-sandbox-of-10 at 50× shows linear snapshot scaling, no super-linear stacked-depth tax.
- **§9.6 Recursive rule eval.** Leave alone; still well within 1B targets at scale.

If Phase 2D succeeds and §9.2 / §9.5 / §9.6 all stay opportunistic, **Phase 2 exits** and Phase 3A becomes proposed.

**Phase 2D shipped** at `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`); §9.3 closed. **§9.2 / §9.5 / §9.6 all remain opportunistic** per the post-2D §6.15 numbers (the writeback semantic correction made the combined-workflow per-edit cost ~200× faster as a side-effect — §9.2's payoff window narrowed substantially). **Pending the format/parser ADR landing for Phase 3A, Phase 2 exits and Phase 3A flips to `proposed`.**

**Phase 2 exits** when Phase 2D's source change ships AND no remaining 1B miss in `PERF.md` is unaddressed and unexplained AND the three Phase 2 housekeeping items below are complete.

### Phase 2 housekeeping (cross-cutting; sequenced around the optimization sub-phases)

Three small but load-bearing tasks that are not optimizations but condition every optimization decision. Treat the sequence below as the actual run order — Q3 first because it makes the rest measurable; Q1 next because it strategically gates everything past Phase 2B; Q2 last because its urgency depends on Q1's Phase 3A scoping.

**Q3 — Criterion baseline tracking (≈ 30 min; precedes 2B).**
Run `cargo bench --workspace -- --save-baseline phase-2a` once at the `phase-2a-cold-path-baseline` tag. Copy `target/criterion/` JSON outputs into `docs/reports/bench-data/phase-2a/` (small, committable). From Phase 2B onward, every optimization sub-phase runs `cargo bench --workspace -- --baseline phase-2a` (or the appropriate prior baseline) to produce a real before/after diff instead of a hand-edited PERF.md table. **Phase 2B's handoff explicitly folds this in as step 0.** No ADR required; document the workflow in PERF.md once.

**Phase 2B status (2026-05-01):** initially SLIPPED, then **closed retroactively** later the same day. The Phase 2B source change shipped without first capturing the `phase-2a` baseline (substituting document-form medians in PERF.md §6.11). The gap was closed in a follow-on commit by capturing both baselines back-to-back: `phase-2b` from the post-2B HEAD, then `phase-2a` from a checkout of the `phase-2a-cold-path-baseline` tag. Both `target/criterion/` snapshots live under [`../reports/bench-data/phase-2a/`](../reports/bench-data/phase-2a/) and [`../reports/bench-data/phase-2b/`](../reports/bench-data/phase-2b/) (1.4 MB total, JSON only — no `raw.csv` because criterion is `default-features = false`). Sanity check on the 3-leaf cold consol gate row reproduces PERF.md §6.11: 12.65 µs (phase-2a) → 2.38 µs (phase-2b), within drift of the document-asserted 14.3 → 2.53 µs. The Phase 2B completion report §6.A retains the original slip as the audit trail.

**Phase 2C onward:** every optimization sub-phase runs `cargo bench -p mc-core --bench <name> -- --baseline phase-2b` against the post-Phase-2B baseline (or `--baseline phase-2c`, etc., once that lands) — never re-asserting medians by hand again. See [`../reports/bench-data/README.md`](../reports/bench-data/README.md) for the workflow.

**Q1 — Workload sketch ADR (after Phase 2B).**
Write a short ADR in `docs/decisions/` titled "Workload sketch & perception thresholds" that:

- Enumerates the planner workflow archetypes (open cube, edit cell, recompute slice, snapshot, compare versions, fork-and-merge — adjust to fit observed reality).
- Assigns each archetype a perception threshold (sub-100 ms = instant, sub-1 s = responsive, multi-second = needs progress UI).
- Maps each brief §11 row onto the archetype it gates so future optimization choices read as "we made post-edit recompute drop from N ms to M ms" rather than "we improved bench X by Y×."
- Documents fixture assumptions (per-dim hierarchy depth, derived-measure count, scenario fan-out) — Acme is one cube shape, not THE cube shape, and §6.10's per-mark Cartesian-product blowup depends on those assumptions.
- States explicitly whether ingest latency or read latency is the gating user-felt budget — this answers whether Phase 2C should be §9.3 (write-side) or something read-side.

This ADR is the strategic gate for everything past Phase 2B.

**Q2 — Toolchain bump revisit (deferred until needed).**
Rule: bump `rust-toolchain.toml` past 1.78 **before any new runtime dep lands that requires it**, not on phase boundaries. PERF.md §9.7 has the procedure. The trigger is most likely Phase 3A's parser dep choice (e.g. if 3A picks `serde` + `toml` and a transitive of those needs Rust > 1.78). Q1's ADR + 3A's parser-dep ADR together determine when Q2 fires; until then it stays deferred. CLAUDE.md §1.1 already treats `proptest`/`insta` as Phase-paired-work, not toolchain-blocked, so Q2 unblocks nothing on its own today.

---

## Phase 3 — Model Definition Layer

> Today, cubes are authored by writing Rust against `mc-core`'s builder API (see `mc-fixtures::build_acme_cube`). That doesn't scale to a UI or LLM-assisted authoring. Phase 3 introduces a declarative format that compiles to the existing builder API. **No kernel semantics change** — this is a translation layer.

### 3A — Declarative model format + parser (proposed)

- **Status:** **complete (2026-05-02).** Acceptance gate cleared: `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` produces empty output. New `mc-model` crate ships the three-stage pipeline (YAML → ParsedModel → ValidatedModel → Cube) per ADR-0004 Decision 9. Tests: 252 / 0 (was 227; +25 from Phase 3A — 6 parse unit tests, 3 smoke tests, 1 structural-equivalence, 14 validator negative tests, 1 golden-runner). 10/10 deterministic. `mc-core` deps unchanged. `mc-fixtures` byte-for-byte unchanged. Toolchain stayed at Rust 1.78; `serde_yaml 0.9.34`'s transitive `indexmap 2.14.0` pinned to `2.7.0` per Decision 3 escape hatch (Phase 1B precedent). Phase 2 housekeeping Q2 (toolchain bump) **stays deferred** — Phase 3A did not trigger ADR-0005. Handoff was at [`../handoffs/phase-3a-handoff.md`](../handoffs/phase-3a-handoff.md); completion report at [`../reports/phase-3a-completion-report.md`](../reports/phase-3a-completion-report.md).
- **Purpose:** Ship the `mc-model` crate that loads YAML cube definitions into `mc_core::Cube` instances, with the Acme cube as the round-trip proof. No kernel change.
- **What it proves:** A round-trip from `crates/mc-model/examples/acme.yaml` to `cargo run --release --bin mc -- demo --model <path>` produces brief §4.6 output **byte-for-byte identical** to the existing `cargo run --release --bin mc -- demo` (which uses `build_acme_cube()`). The structural-equivalence check between the two paths is a kernel test (lives in `mc-model` with `mc-fixtures` as a dev-dep).
- **Deliverables (planned, high-level):** new `mc-model` crate; YAML parser configured for the safe subset; `ParsedModel` + `ValidatedModel` intermediate types per ADR-0004 Decision 9; validator covering ADR-0004 Decision 6's table; `acme.yaml` example with inline goldens covering brief §4.5.1 anchors; `mc demo --model <path>` CLI flag; structural-equivalence test against `build_acme_cube()`; per-validator unit tests proving each error path triggers correctly.
- **Acceptance gates (planned):** see ADR-0004 success-criteria section (8 items). Headline: byte-for-byte demo equivalence; zero new `mc-core` deps; ≥ 227 / 0 tests; 10/10 deterministic.
- **Out of scope (explicit):** see ADR-0004 "Out of scope" table (UI, LLM authoring, DuckDB, actuals, auth, permissions, multi-cube, cross-cube rules, custom formula parser, format migration, bidirectional round-trip — each named with its real future Phase). **Dep-discipline rule:** `serde` and any other parser dep must NOT be added to `mc-core` — this is enforced by Decision 3 of ADR-0004, not by intuition.

### 3B, 3C, … (TBD)

Likely follow-ons (placeholders, do not pre-name without an ADR):

- Round-trip *write* (cube → declarative file). Needed for Phase 6 UI editors.
- Multi-cube / cube-of-cubes composition. Needed once the first model file outgrows a single document.
- Schema versioning + migration semantics. Needed once a real user has authored a cube the format authors can't reflexively rewrite.

---

## Phase 4 — LLM-Assisted Model Authoring

> A user describes a planning model in natural language; the system produces a model file (Phase 3 format) that the kernel accepts. Strictly post-Phase-3.

- **Status:** not started.
- **Purpose:** Lower the authoring bar from "write Rust" / "write a config file" to "describe what you want."
- **What it proves:** A non-engineer operator can express a planning intent and the system produces a kernel-loadable cube without hand-editing config.
- **Deliverables (anticipated, high-level):** a prompting layer that maps free-text descriptions to Phase 3 model files; a validation loop that surfaces schema errors back to the LLM (or user) in plain language; a curated test set of "planning intent → expected cube shape" pairs.
- **Acceptance gates (anticipated):** the test set's intent → cube shape pairs round-trip with ≥ N% accuracy; no LLM output bypasses the Phase 3 parser; schema errors are surfaced with file/line context.
- **Out of scope (explicit):** any tool that lets the LLM write directly to `mc-core/src/`; any tool that bypasses the Phase 3 schema validator; cost/latency optimization of the LLM path (defer to Phase 7 productization).

**Why this comes after Phase 3.** Without Phase 3, the LLM has nothing concrete to emit. Phase 3's schema is the LLM's grounding rail.

---

## Phase 5 — Data Integration & Actuals

> Connect cubes to real-world data (actuals from external systems, e.g. ad platform reporting APIs, CRM exports) so plans can be compared to reality.

- **Status:** not started.
- **Purpose:** Distinguish *plan* (forecasted Spend, Revenue, etc.) from *actual* (what the platform reported) inside the cube model. Today every cell is a single value; planning is the difference between a plan and an actual.
- **What it proves:** The kernel can hold both plan and actual on the same coordinate (likely via a Scenario-dimension axis or a parallel cube), and that loading actuals from a real external source produces a usable variance report.
- **Deliverables (anticipated, high-level):** an "actuals" semantic somewhere in the cube model (Scenario axis values like `Plan`, `Actual`, `Forecast`, plus a documented variance pattern); at least one external-source adapter (likely a CSV importer first, then one platform API); a CLI command that loads actuals and prints a variance summary.
- **Acceptance gates (anticipated):** at least one real production dataset can be loaded and produces a variance report a human operator approves; the kernel's deterministic-recompute invariant holds in the face of partial / late-arriving actuals.
- **Out of scope (explicit):** UI (Phase 6); multi-source reconciliation (Phase 7); any work that requires the LLM (Phase 4 first or in parallel, but not gated by it).

**Phase 4 vs Phase 5 ordering.** These are independent and can be sequenced either way depending on which user need is more pressing. Default: Phase 4 first because it widens the authoring funnel; an early Phase 4 model can be sharpened by Phase 5 actuals later.

---

## Phase 6 — UI & Internal App Proofs (incl. internal Media Partner model proof)

> A web UI (or internal tool) that lets a planner view, drill, edit, snapshot, and compare cubes without touching the CLI. Phase 6's proof-of-value scenarios may include an **internal-only Media Partner model proof** — building a Media Partner cube (rate cards, tactics, partner views, order math) as one of the proof scenarios. Phase 7 takes the same model external; Phase 6 keeps it inside.

- **Status:** not started.
- **Purpose:** Make the kernel + model layer usable by a non-engineer. This is the smallest step that transforms the project from "library" to "application."
- **What it proves:** An internal operator can complete a real planning task end-to-end (load a model, ingest actuals, edit a forecast, take a snapshot, compare versions, export results) without engineering help. If the proof scenario is the internal Media Partner model, it also proves that the model layer (Phase 3) can express the partner-side concepts (rate cards, tactics, partner-scoped views) before any of that surface ships externally.
- **Deliverables (anticipated, high-level):** a web UI (framework TBD); navigation surface for dimensions / hierarchies / measures; drill-down + grid editing for input cells; visible plan-vs-actual variance; snapshot + version compare; **at least one internal proof-of-value scenario shipped end-to-end** — candidates include an internal-only Media Partner model, a finance-team plan/actual variance review, or a marketing-team campaign-level forecast. Pick one (or stage in this order) once Q1 (workload sketch) settles which is the right first proof.
- **Acceptance gates (anticipated):** at least one internal team member can use the UI to complete a planning task without instruction; performance against a representative production-sized cube hits the perception thresholds set in Q1; auth + audit trail in place for the internal team.
- **Out of scope (explicit):** customer-facing UX; multi-tenancy; the customer-facing Media Partner App (Phase 7); any change to the kernel's `Cube` public API beyond what the UI strictly requires.

**Why "internal app proofs" first, not the customer-facing Media Partner App.** Customer-facing has its own scoping, scaling, and security requirements. The internal Media Partner model proof in Phase 6 lets us validate the model + UI loop with a captive audience before any external user sees it; the same kernel + model + UI then carries forward into Phase 7 with the multi-tenant / auth / billing concerns added.

---

## Phase 7 — Productization (customer-facing Media Partner App + multi-tenancy)

> Turn the validated internal product into something a paying customer can use. The **customer-facing Media Partner App** is the proof-of-value use case driving this phase (distinct from Phase 6's *internal* Media Partner model proof). The phase covers everything that "productization" implies — multi-tenancy, auth, audit, scaling, support — not the app alone.

- **Status:** not started.
- **Purpose:** Convert the internal app into a production-grade, multi-tenant offering that a customer (an external media partner) can use independently.
- **What it proves:** The kernel + authoring layer + UI scale to multi-tenant, multi-user, customer-facing use without sacrificing the determinism and correctness that Phases 1–6 established.
- **Deliverables (anticipated, high-level):** multi-tenancy (data-isolated per partner); the customer-facing Media Partner App (the first external surface, building on Phase 6's internal Media Partner model proof); production-grade auth + audit + observability + backup; a documented onboarding path for new partners; SLAs / on-call rota / incident playbooks.
- **Acceptance gates (anticipated):** at least one external partner is using the system in production for one full planning cycle without engineering escalation; security review passed; data export / portability story documented.
- **Out of scope (explicit):** anything that violates the Phase 1–6 invariants (no behavior changes that break the §10 contract tests; no `serde` / `tokio` / `rayon` in `mc-core` unless an ADR explicitly retires that constraint — note this is an `mc-core`-specific rule, see Phase 3A for the surrounding-crate exception).

---

## How to use this document

- **Starting work?** Look at the Phase status overview table. Find the first row whose status is `proposed`; check whether its handoff doc exists; if yes, that is your next task.
- **Finishing work?** Update the Phase status overview row from `proposed` → `complete` and link the completion report. Add a tag entry. Move the next sub-phase to `proposed`.
- **Want to add a sub-phase?** Append it to the Phase status overview AND its parent's "TBD" sub-section. Open an ADR describing why this sub-phase is justified by data (or by a concrete user need). Don't ship work whose phase isn't named here.
- **Want to skip a phase?** Open an ADR. Phases 1–7 are sequenced for a reason; skipping requires a deliberate decision.

---

## Update procedure

When a phase ships:

1. Flip its row in the Phase status overview from `proposed` → `complete`.
2. Add the tag (e.g. `phase-2b-consolidation-fast-path`) to that row.
3. Move the next sub-phase from "TBD" or `not started` to `proposed`.
4. Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) so the live state reflects the same shipping/queued split.
5. If the work made a non-trivial design choice, write an ADR in [`../decisions/`](../decisions/) and link it from the relevant phase section here.
6. If a phase reveals a new sub-phase need, add it under "TBD" with a one-line description; do NOT scope it in detail here. Detailed scoping lives in `docs/handoffs/<phase>-handoff.md`.

---

## Cross-links

- [`../HANDOFF.md`](../HANDOFF.md) — 5-minute orientation; points at this file as the master plan.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — current build / test / gate state; current shipping/queued phases.
- [`../specs/`](../specs/) — locked input contracts (engine-semantics + the Phase 1 brief). The brief governs Phase 1; future phases will produce their own briefs if they need one.
- [`../handoffs/`](../handoffs/) — per-phase handoff docs; one per shipping sub-phase.
- [`../reports/`](../reports/) — per-phase completion reports.
- [`../decisions/`](../decisions/) — ADRs.
- [`../PERF.md`](../PERF.md) — performance baseline + Phase 2 optimization candidates.
