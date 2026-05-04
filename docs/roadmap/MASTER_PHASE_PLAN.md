# MASTER_PHASE_PLAN

> **The single source of truth for what phase the project is in and what comes next.**
>
> Read this before inventing a phase name or starting work that isn't already on the list. If a new phase is needed and it isn't here, add it here first (and link the ADR explaining the decision).

**Last updated:** 2026-05-03 (post-Phase 4A)
**Maintained by:** project lead. New sub-phases require an ADR in [`../decisions/`](../decisions/).

---

## Product vision

**Mosaic** (renamed from "MarketingCubes" on 2026-05-03 — see [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md)) is an **AI-powered Large Numbers Model (LNM) platform**: a TM1-inspired multidimensional kernel that holds structured numerical models — dimensions, hierarchies, formulas, assumptions, inputs, tests, traces, and (eventually) model-backed cells with uncertainty — across finance, marketing, prospecting, sports betting, sales forecasting, and analytics. The first proof domain is marketing/finance planning (the Acme demo); the substrate is general. The full marketing-to-revenue funnel — channel × market × time × scenario × version — is one schema family riding on the LNM engine; future schemas (sports-betting research, prospect scoring, FP&A, demand planning) install as separate model files on the same kernel. Later phases add LLM-assisted authoring, real-data integrations, a web UI with spreadsheet ergonomics, and customer-facing applications. The North Star is a tool an operator can use to author a numerical model, validate it, test it against goldens, load real data, see results consolidated correctly, edit them confidently, compare versions safely, trace every computed value back to its inputs, and (eventually) combine deterministic formulas with predictive model-backed cells.

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
| **3A** | Model definition layer — YAML + `mc-model` crate (per ADR-0004) | **complete** (report at [`../reports/phase-3a-completion-report.md`](../reports/phase-3a-completion-report.md)) | `phase-3a-model-definition-layer` (`603c537`) |
| **3B** | Model QA, Linter, and Diagnostics — `mc model {validate,inspect,lint,test}` + 10 lint rules + JSON diagnostics envelope (per ADR-0005) | **complete** (report at [`../reports/phase-3b-completion-report.md`](../reports/phase-3b-completion-report.md)) | `phase-3b-lint-and-diagnostics` (`f4f7fa8`) |
| **3C** | Model Test Fixtures and Input Sets — `canonical_inputs` + `test_fixtures`, sibling CSV + tabular inline YAML, 14 new validators (MC2012–MC2025), `mc model test --fixture <name>` (per ADR-0006) | **complete** (report at [`../reports/phase-3c-completion-report.md`](../reports/phase-3c-completion-report.md)) | `phase-3c-fixtures-and-inputs` (`8d2691a`) |
| **3D** | Friendly formula syntax — `Revenue = Customers * AOV` strings compile to `ParsedRuleBody`'s structured tree (per ADR-0007; originally ADR-0004's Phase 3C — renamed to 3D per ADR-0006 roadmap impact) | **complete** (report at [`../reports/phase-3d-completion-report.md`](../reports/phase-3d-completion-report.md)) | `phase-3d-friendly-formula-syntax` (`d5ab355`) |
| **3E–3N** | Further model layer extensions (TBD) | not started | — |
| **4A** | LLM-assisted authoring — Mosaic Claude Code plugin (skills + agents + commands + MCP server + marketing-mix domain schema) per ADR-0008 | **complete** (report at [`../reports/phase-4a-completion-report.md`](../reports/phase-4a-completion-report.md)) | `phase-4a-mosaic-plugin` (`36af56c`) |
| **4B** | Python reference adapters under `mosaic-plugin/examples/adapters/` (`anthropic-python/` + `openai-python/` ~150 lines each) | **complete** (report at [`../reports/phase-4b-completion-report.md`](../reports/phase-4b-completion-report.md); both adapters cleared best-of-3 gate — Anthropic 3/3, OpenAI 3/3) | `phase-4b-python-adapters` (`b5b6229`) |
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

### 3B — Model QA, Linter, and Diagnostics (complete)

- **Status:** **complete (2026-05-03).** Shipped at `f4f7fa8`, tagged `phase-3b-lint-and-diagnostics`. All 15 ADR-0005 Decision 8 acceptance items closed; report at [`../reports/phase-3b-completion-report.md`](../reports/phase-3b-completion-report.md); handoff was at [`../handoffs/phase-3b-handoff.md`](../handoffs/phase-3b-handoff.md). [ADR-0005](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) Accepted 2026-05-02 with 15 project-owner acceptance amendments (10 from GPT + 5 from Claude Desktop).
- **Purpose:** Add a **read-only quality and diagnostics layer** over `mc-model` that makes authoring (human, and later LLM) safer *before* Phase 3C friendly formulas, Phase 4 LLM authoring, Phase 5 actuals, or Phase 6 UI work begins. Closes four gaps in Phase 3A's surface: (1) no way to inspect a model at-a-glance; (2) no quality signal beyond "is it buildable?"; (3) no stable diagnostic vocabulary for Phase 4 LLM consumption; (4) no CLI surface for any of the above.
- **What it proves:** Adding diagnostics + lint over `mc-model` is a small, reversible, leverage move that unblocks every later phase (each consumes Phase 3B's stable diagnostic codes + JSON envelope). The Acme YAML lints cleanly with zero documented warnings; intentionally-flawed fixtures trigger each rule; `mc demo --model` does NOT run goldens (separation of concerns); `mc model test` is the dedicated golden runner.
- **Deliverables (planned, high-level):** four new CLI subcommands (`mc model validate / inspect / lint / test`) plus a `--format text|json` modifier; 10 starting lint rules (MC3001–MC3007 + MC3009–MC3011, with MC3008 permanently retired and promoted to validation as MC2011); structured `Diagnostic { code, severity, path, message, suggestion }` shape; JSON output wrapped in `{ "schema_version": "1.0", "diagnostics": [...] }` envelope; deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)`; one negative test fixture per lint rule under `crates/mc-model/tests/lint_fixtures/`; hand-rolled snapshot fixture comparison (no `insta` unless proven on Rust 1.78); MC3008-retirement assertion; `mc demo --model` doesn't-run-goldens integration test.
- **Acceptance gates (planned):** see ADR-0005 Decision 8 (15 items). Headline: Acme lints clean with zero warnings; ≥ 252 tests still pass; `mc-core` and `mc-fixtures` untouched; deterministic 10/10; JSON envelope schema_version assertion; demo-without-goldens integration test passes.
- **Out of scope (explicit):** see ADR-0005 Decision 6 — no formula strings (Phase 3C), no LLM authoring (Phase 4), no UI (Phase 6), no actuals (Phase 5), no DuckDB, no multi-cube, no `mc-core` changes, no auto-fix, no snapshot diff. **Dep-discipline rule:** parser/serde deps stay in `mc-model` only (per ADR-0004 Decision 3, inherited).

### 3C — Model Test Fixtures and Input Sets (complete)

- **Status:** **complete (2026-05-03).** Shipped at `8d2691a`, tagged `phase-3c-fixtures-and-inputs`. Report at [`../reports/phase-3c-completion-report.md`](../reports/phase-3c-completion-report.md). [ADR-0006](../decisions/0006-phase-3c-model-test-fixtures.md) Accepted 2026-05-03 with 13 project-owner acceptance amendments (9 from GPT + 4 from Claude Desktop, including a wording-tightening note on `--fixture` semantics).
- **Purpose:** Close the visible scaffolding hack `mc model test` left in `mc-cli/src/main.rs:253` (the `metadata.name == "Acme_MarketingFinance"` branch). Add model-owned `canonical_inputs:` and `test_fixtures:` schema, sibling CSV + tabular inline YAML data forms, and 14 new validators (MC2012–MC2025) so generic models work with `mc model test` without Acme-specific CLI logic.
- **What it proves:** A YAML+CSV-authored model produces byte-identical store state to the Rust-fixture path on Acme across all 2,520 canonical input coordinates and all 9 inline goldens. The equivalence test uses ONLY existing public APIs from `mc-core` + `mc-fixtures` — no new APIs added to either crate (per ADR-0006 acceptance amendments #15 + (c)).
- **Architecture clarification (project-owner-pinned):** `validate()` stays filesystem-free. A new named stage `mc_model::resolve_inputs(&ValidatedModel, Option<&Path>)` reads CSVs, canonicalizes paths, and emits MC2012–MC2025 as `ValidationError` variants. `mc_model::load(path)` runs the four-stage pipeline (parse → validate → resolve_inputs → compile) but does NOT apply inputs to the cube; the returned `Cube` is empty of input data. `mc model test` is the only consumer that calls `apply_canonical_inputs` / `apply_fixture`. See completion report §4.1.
- **Delivered:** schema additions to `ParsedModel`/`ValidatedModel` (additive, backwards-compatible); hand-rolled strict CSV parser (`crates/mc-model/src/csv.rs`, no `csv` crate dep); 14 new validators with one negative-test fixture each; `mc model test --fixture <name>` filter flag (filter-only semantic); `acme.yaml` + `acme.inputs.csv` cleanup; removal of the `metadata.name` Acme special case from `mc-cli`; `Cube::snapshot` + `Cube::rollback_to` used for between-goldens reset.
- **Headline gates (achieved):** `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` = 0; equivalence test passes (2,520 coords bit-equal + 9 goldens within 1e-9); `mc model test acme.yaml` runs in **32 ms** (under both 500 ms gate and 200 ms stretch); 328 tests pass / 0 fail (was 293); 10/10 deterministic; `mc-core` and `mc-fixtures` untouched. All 17 ADR-0006 Decision 9 success-gate items closed.
- **Out of scope (explicit):** see ADR-0006 Decision 8 — no actuals import (Phase 5), no DuckDB, no API loading, no formula strings (Phase 3D), no LLM authoring (Phase 4), no UI (Phase 6), no `mc-core`/`mc-fixtures` changes, no multi-cube, no auto-fix, no Cube → YAML round-trip.

### 3D — Friendly formula syntax (complete)

- **Status:** **complete (2026-05-03).** Shipped at `d5ab355`, tagged `phase-3d-friendly-formula-syntax`. Originally named "Phase 3C" in [ADR-0004 Decision 4](../decisions/0004-phase-3a-model-definition-format.md); renamed to Phase 3D per ADR-0006 roadmap impact. **First phase shipped under the new "handoff-first parallel flow"** — see [`../process-notes.md`](../process-notes.md) §1.
- **Purpose:** Compile `Revenue = Customers * AOV`-style formula strings down to `ParsedRuleBody`'s structured tree. No kernel change; new parser sits in `mc-model` alongside the structured-tree path.
- **Delivered:** new module [`crates/mc-model/src/formula.rs`](../../crates/mc-model/src/formula.rs) (~250-line hand-rolled recursive-descent parser + minimal-paren serializer; no `pest` / `nom` / `lalrpop`); `ParsedRuleBodyForm { Formula(String), Structured(ParsedRuleBody) }` enum wrapping `ParsedRule.body` (serde untagged, String-first dispatch); `ValidatedRule` with flat `body: ParsedRuleBody` on `ValidatedModel.rules` (downstream stages have ZERO awareness of the wrapper per amendment #23); 4 new diagnostic codes MC1003–MC1006 in `ParseError` (per amendment #25, MC1004 covers both unexpected tokens AND unknown function calls; MC1007 NOT introduced); `mc model inspect` renders all rules in formula form regardless of authoring (amendment #24); Acme's 5 rules in [`crates/mc-model/examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) migrated to formula form (`Gross_Profit` uses `body: "Revenue * (1 - COGS_Rate)"`). Backwards compat mandatory and verified — `_acme_with_bad_golden.yaml` structured-form fixture still loads identically.
- **Headline gates (achieved):** all 5 Acme rules use `body: "<formula>"`; round-trip stability passes for the explicit risky-case list (sub/div associativity, `Mul(a, Div(b, c))` parens, `(a + b) * (c - d)`, unary minus canonical form, all 5 Acme formulas); demo-equivalence diff stays empty; lint zero warnings; goldens 9/9; equivalence test still byte-identical; 396 tests pass / 0 fail (was 328); 10/10 deterministic; `mc-core` and `mc-fixtures` zero-line diff vs `phase-3c-fixtures-and-inputs`. All 14 acceptance-gate items closed.
- **API adjustment (project-owner-approved):** `validate()` return type widened from `Result<_, Vec<ValidationError>>` to `Result<_, Vec<Error>>` so MC1003–MC1006 (parse-stage codes) and MC2xxx (validate-stage codes) coexist in the unified error pile. `Diagnostic` struct shape unchanged; JSON envelope `schema_version` stays `"1.0"`. See [`../reports/phase-3d-completion-report.md`](../reports/phase-3d-completion-report.md) §3–§4 for the rationale.
- **Out of scope (explicit):** no kernel changes (`mc-core` locked); no fixture changes (`mc-fixtures` locked); no new dependencies; no toolchain bump; no `Cargo.lock` pin drift; no new `ParsedRuleBody` AST variants (formulas compile DOWN to the existing 7 variants); no `Diagnostic` struct shape change; MC3008 stays permanently retired; no LLM authoring (Phase 4); no actuals import (Phase 5); no UI (Phase 6).

### 3E, 3F, … (TBD)

Likely follow-ons (placeholders, do not pre-name without an ADR):

- Round-trip *write* (cube → declarative file). Needed for Phase 6 UI editors.
- Multi-cube / cube-of-cubes composition. Needed once the first model file outgrows a single document.
- Schema versioning + migration semantics. Needed once a real user has authored a cube the format authors can't reflexively rewrite.

---

## Phase 4 — LLM-Assisted Authoring + Mosaic Plugin Ecosystem

> A user describes a planning model in natural language; the system produces a Mosaic YAML file (per the Phase 3A schema, Phase 3D formula syntax) that passes `mc model validate / lint / test`. Per [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md), the centerpiece is the **Mosaic plugin** — a portable knowledge package (skills + agents + commands + MCP server + hooks + examples) that any AI agent can consume. The plugin is institutional knowledge in agent-framework-agnostic form. **Decomposed into Phase 4A + Phase 4B; Phase 4C dissolved per "no vague TBD buckets" rule.**

### 4A — Mosaic Claude Code plugin (complete)

- **Status:** **complete** 2026-05-03, committed at `36af56c` (tag `phase-4a-mosaic-plugin`). Report at [`../reports/phase-4a-completion-report.md`](../reports/phase-4a-completion-report.md). Handoff at [`../handoffs/phase-4a-handoff.md`](../handoffs/phase-4a-handoff.md). [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) Accepted 2026-05-03 with 9 acceptance amendments.
- **Purpose:** Ship the Mosaic Claude Code plugin so that any Claude Code instance with the plugin installed can author a Mosaic YAML model from a natural-language prompt. The plugin is the source-of-truth knowledge package; future SDK adapters (Phase 4B) consume the same content.
- **What it proved:** the in-session end-to-end proof produced `MyCo_Marketing_Q1_2026` (a 3-channel × 3-market × Q1 marketing-mix model materially different from Acme) from plugin content alone — converged in two iterations to validate-clean, lint-clean (zero warnings), test-pass (3/3 goldens). Full transcript + YAML at [`../reports/phase-4a-proof/`](../reports/phase-4a-proof/). Real fresh-instance verification deferred to user post-review (the in-session limitation: a separate Claude Code session must install the plugin to fully close the headline gate). The plugin's structured knowledge (skills + agents + commands + MCP server) demonstrably suffices to embue an LLM with Mosaic-authoring competence.
- **Deliverables shipped:** `mosaic-plugin/` directory at workspace root with manifest at `.claude-plugin/plugin.json` (canonical Claude Code shape per cached vercel/0.40.1 + superpowers/5.0.7 references) + skills/ (authoring, debugging, schema-design, formulas, testing, marketing-mix domain — 6 total) + agents/ (mosaic-architect, mosaic-author, mosaic-debugger, mosaic-validator — 4 total) + commands/ (mosaic-init, validate, inspect, lint, test, author — 6 total; **/mosaic-explain deferred to Phase 4A.2** — needs `mc model trace` CLI verb that doesn't exist yet) + .mcp.json + hooks/ (placeholder per Phase 4A.1) + examples/models/ (Acme YAML + CSV byte-identical to `crates/mc-model/examples/`) + examples/adapters/ (Phase 4B placeholder). Single Rust addition: `mc-cli` gains `mc mcp` subcommand at `crates/mc-cli/src/mcp.rs` (318-line hand-rolled JSON-RPC parser body + 66-line emitter; over the 250 trigger #10 budget, user-authorized as scope-specific decision; reuses Phase 3B's `diagnostics_to_json` envelope verbatim; no new deps).
- **Acceptance gates met:** all 13 acceptance items in [`../reports/phase-4a-completion-report.md`](../reports/phase-4a-completion-report.md) §5. Locked surfaces: `git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns 0 lines (Phase 4A added zero changes to the kernel/fixtures/model layer; the inherited 55-line diff vs `phase-3d-friendly-formula-syntax` is entirely the rename commit's Cargo.toml/lib.rs description updates). Test count: 396 → **416** (+20 from Phase 4A). 10/10 deterministic. Toolchain unchanged at Rust 1.78. Cargo.lock pins (`clap`, `clap_lex`, `half`, `indexmap`, `hashbrown`) all unchanged. JSON envelope `schema_version` stays `"1.0"`. Headline (in-session best-effort): a fresh-reader LLM produced a working marketing-mix YAML from the plugin's content alone; full real-environment verification = user post-review step.
- **Out of scope (held):** Python adapters (Phase 4B); additional domain schemas beyond marketing-mix (future demand-driven phases); a Rust LLM client (`mc-author`); SDK deps in the Rust workspace; tokio / async / reqwest; toolchain bump; UI; actuals; model-backed cells.
- **Phase 4A.1 candidate (small amendment):** ship the two hooks (`pre-commit-lint.json`, `post-edit-validate.json`) once the canonical Claude Code hook-spec format is verified against a live install. Plus skill-example sweep for any remaining mismatched-shape examples.
- **Phase 4A.2 candidate (small amendment):** add `mc model trace <coord>` CLI verb + the `/mosaic-explain` slash command that consumes it (the kernel has rule-chain trace per PERF.md §6.4; surfacing as CLI requires touching `mc-model` which Phase 4A's locked-surfaces rule blocked).

### 4B — Python reference adapters (complete 2026-05-03)

- **Status:** **complete** at `b5b6229` (tag `phase-4b-python-adapters`). All deliverables shipped + best-of-3 gate cleared. Committed at `b5b6229` (tag `phase-4b-python-adapters`).
- **Purpose:** Demonstrate that the Mosaic plugin's content is portable across LLM environments by shipping working Python reference adapters that consume the same plugin and produce equivalent results.
- **Deliverables shipped:** two Python adapters under `mosaic-plugin/examples/adapters/`:
  - `anthropic-python/` — 267-line reference iteration loop using the official Anthropic Python SDK; default provider per ADR-0008 amendment D; uses `claude-opus-4-7`.
  - `openai-python/` — 263-line reference iteration loop using the official OpenAI Python SDK; cross-provider proof per amendment G; uses `gpt-5.5` via `responses.create`.

  Each reads the plugin's `skills/`, `agents/`, `commands/`, and `examples/models/acme-marketing.yaml`; concatenates them into a single 138K-char system prompt with the binding ```yaml-fence response-format instruction; calls the provider's API; runs the iteration loop against `mc model {validate,lint,test} --format json` (subprocess; not MCP) up to 5 iterations.
- **Acceptance gates cleared:** `python examples/adapters/anthropic-python/author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"` and the matching OpenAI invocation each ran 3 times. **Anthropic 3/3 ✓** (post-fix runs at 2 / 1 / 4 iter); **OpenAI 3/3 ✓** (1 / 1 / 1 iter). Both canonical YAMLs pass `mc model validate / lint / test` with 10/10 goldens. Both adapters use the same plugin content with no provider-specific tags. See [`reports/phase-4b-completion-report.md`](../reports/phase-4b-completion-report.md) + [`reports/phase-4b-proof/`](../reports/phase-4b-proof/) for the audit trail.
- **In-flight bug fixes (Phase 4B-internal):** initial gate-run produced Anthropic 1/3 because of two real adapter bugs (case-insensitive severity filter mismatch + truncation-tolerant YAML extraction). Both fixed; pre-fix Anthropic artifacts archived. One plugin-doc inconsistency (envelope `severity` PascalCase in `mc-cli` vs lowercase in `skills/debugging/SKILL.md`) surfaced as **Phase 4A.1 follow-up candidate** (NOT folded into 4B per SPEC QUESTION trigger #2).
- **Locked surfaces (vs `phase-4a-mosaic-plugin`):** `crates/` 0-line diff; `mosaic-plugin/skills/` / `agents/` / `commands/` / `.claude-plugin/` / `.mcp.json` / `examples/models/` / `hooks/` 0-line diff. Toolchain still Rust 1.78. `cargo test --workspace` still 416/0.
- **Out of scope (held):** TypeScript adapters; Codex / Gemini / Mistral / Ollama adapters; cost tracking; prompt hardening; production-quality polish (rate limit handling, network failures, partial-completion resumption); schema marketplace. All deferred to demand-driven future phases.

**Why this comes after Phase 3.** Without Phase 3 (3A schema, 3B diagnostics, 3C fixtures, 3D formula syntax), the LLM has nothing concrete to emit. Phase 3's schema + diagnostic codes (MC1xxx–MC4xxx) are the LLM's grounding rails.

**No Phase 4C.** Per [`../process-notes.md`](../process-notes.md) "no vague TBD buckets" rule + ADR-0008 Decision 7. After 4B ships, next phase is Phase 5 (actuals). Future schemas / providers / production polish / schema marketplace are demand-driven phases (named when a real customer or proof requires them).

---

## Phase 5 — Data Integration & Actuals (Tessera)

> Connect cubes to real-world data (actuals from external systems — CSV / SQL / REST) via the **Tessera** ingestion engine. The TM1 TurboIntegrator replacement: declarative YAML recipes (not scripting), schema-validated, LLM-authorable, blazing-fast bulk-write through `WriteBatch`.

- **Status:** in progress. Sub-phase decomposition per [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) Decision 9.

### Phase 5 sub-phase status (per ADR-0010 Decision 9)

| Sub-phase | Deliverable | Status |
|---|---|---|
| **5A — Tessera Core Engine** | `WriteBatch` (mc-core); recipe format (mc-recipe); 6 source drivers (mc-drivers); Tessera orchestrator (mc-tessera) + 5 CLI verbs; Acme CSV equivalence test; 100K-row perf gate | **Streams A+B+C merged** at `6c9950d` (502/0 tests). Stream D orchestrator + CLI verbs in-flight per [`../handoffs/phase-5a-stream-d-handoff.md`](../handoffs/phase-5a-stream-d-handoff.md). |
| **5A.1 — Long-Format Recipe Support** | Schema extension (`format: long` + `long_format:`); Acme equivalence-test switch from generated-wide to actual `acme.inputs.csv`; MC5019–MC5022 codes | Filed per [ADR-0010 Amendment 2](../decisions/0010-amendment-2-long-format-recipe-support.md); pending implementation after 5A Stream D ships. |
| **5B — LLM-Assisted Recipe Authoring** | Plugin skills for import mapping (csv / sql / api); `mosaic-importer` agent; `/mosaic-import` command; Phase 4B adapter `--mode propose-recipe` | **Complete pending user review** on branch `phase-5b/llm-recipe-authoring`. Best-of-3 gate: Anthropic 3/3 ✓, OpenAI 3/3 ✓. See [`../reports/phase-5b-completion-report.md`](../reports/phase-5b-completion-report.md). NOT committed. |
| **5B.1 — `mc tessera propose` CLI verb** | Native CLI verb that wraps the LLM authoring loop (Rust-side); requires `mc-tessera` (Stream D's deliverable) | Deferred until Stream D ships. Phase 5B confined to plugin + adapter (no Rust crate modifications). |
| **5C — Driver Expansion** | MySQL native, D1 REST, Snowflake/BigQuery via ODBC; cron scheduling; incremental loads; element auto-creation | Future ADR(s) after 5B. Demand-driven; each driver independent. |
| **5D — Document/OCR Ingestion** | Document ingestion (open-weight OCR + vision-language models + LLM-assisted field mapping) | Placeholder. Full scope in a future ADR. |
| **5E — Grout Proper** | Full secrets layer (vault, rotation, audit log, external secret-manager integrations) | Placeholder. Phase 5A ships the `SecretResolver` trait + `EnvVarSecretResolver` only. |


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
