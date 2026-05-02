# MASTER_PHASE_PLAN

> **The single source of truth for what phase the project is in and what comes next.**
>
> Read this before inventing a phase name or starting work that isn't already on the list. If a new phase is needed and it isn't here, add it here first (and link the ADR explaining the decision).

**Last updated:** 2026-05-01 (post-Phase 2B)
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
| **2C–2N** | Further optimization rounds (TBD) | not started | — |
| **3A** | Model definition layer — declarative format + parser | **planned** (flips to `proposed` when Phase 2 exits) | — |
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

- **Status:** complete (2026-05-01, uncommitted at the time of this update; awaiting project-owner review).
- **Purpose:** Eliminated the per-call hierarchy/dimension clone in [`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs) — the ~14 µs fixed-cost floor that caused the brief §11.2 3-leaf 1B target (3 µs) to miss by ~5×.
- **What it proves:** The kernel's consolidation algorithm hits every brief §11.2 1B target once the single localized over-cloning is removed. The 3-leaf row drops 14.3 µs → 2.53 µs (clears ≤ 3 µs); every higher-fan-out cold row improves by ~12 µs absolute. Warm rows + every adjacent benched row hold within ±10% noise.
- **Deliverables (shipped):** kernel change in [`cube.rs`](../../crates/mc-core/src/cube.rs) + [`dimension.rs`](../../crates/mc-core/src/dimension.rs) (Option A — `Arc<Vec<Dimension>>` + `Vec<Arc<Hierarchy>>`); new kernel unit test `consecutive_recompute_reads_match_phase_2b` (handoff §3); rewrite of `t_consolidation_caches_value_within_revision` from a single-shot wall-clock ratio to semantic cache-state assertions per [ADR-0002](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md); [PERF.md §6.7 + §6.11 + §9.4 + §10](../PERF.md); [`reports/phase-2b-completion-report.md`](../reports/phase-2b-completion-report.md).
- **Acceptance gates (all met):** brief §11.2 3-leaf 1B target ≤ 3 µs cleared at 2.53 µs (every other §6.7 row also clears 1B); 210 / 0 tests pass (was 209 + 1 new); 10 / 10 deterministic; release demo matches brief §4.6; no clippy warnings; no public API change; no new dependency; no `Cargo.lock` change; no toolchain bump.
- **Out of scope (held):** §9.3 hierarchy mark closure changes; any new dependency; any public API change; any work beyond `cube.rs` / `dimension.rs` source files (`hierarchy.rs` was authorized but no change was needed).

### 2C, 2D, … (TBD)

Sub-phases beyond 2B are intentionally not pre-named. Open a new sub-phase only when a measured 1B miss in a fresh PERF.md justifies it. The candidate list (in rough priority order, anchored in PERF.md §9):

- **§9.3 Hierarchy mark closure cost.** Acme writes spend ~712 ns/mark vs ~98 ns/mark on the synthetic; the gap is per-mark CellCoordinate allocation + AHashSet insert. Likely path: bitset-backed dirty tracker keyed by per-dim element index (PERF.md §9.3 path b). Lazy ancestor marks (path a) is a behavior shift and would require a §10.1 invariant audit; deprioritized.
- **§9.2 leaf-flag cache** on `Element` (`is_leaf_in_default_hierarchy: bool`). Trivial; opportunistic.
- **§9.5 Snapshot COW.** NOT data-justified at Acme scale. Defer until a workflow takes many snapshots per turn.
- **§9.6 Recursive rule eval.** Leave alone; well within 1B targets.

**Phase 2 exits** when no remaining 1B miss in `PERF.md` is unaddressed and unexplained AND the three Phase 2 housekeeping items below are complete.

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

### 3A — Declarative model format + parser (planned)

- **Status:** planned. Flips to `proposed` when Phase 2 exits and the format/parser ADR lands. The choice between TOML+serde, a custom parser, or another option is itself an ADR-pending decision.
- **Purpose:** Define a config format (likely TOML or a small custom DSL — design decision pending) that describes cube models and produces a `Cube` via the existing `CubeBuilder` / `Dimension::builder` / `Hierarchy::builder` / `Rule { … }` constructors.
- **What it proves:** A round-trip from `model.toml` to `cargo run --release --bin mc -- demo --model model.toml` produces an identical cube to `build_acme_cube()` (same coordinates, same dirty propagation, same consolidation results).
- **Deliverables (planned, high-level):** a new crate (`mc-model` or similar); a parser; a schema validator with structured error messages; the Acme cube re-expressed as a declarative file alongside the existing Rust builder; a round-trip test asserting parsed cubes produce byte-identical Acme demo output.
- **Acceptance gates (planned):** parsing the Acme model file produces the same `Cube` (per a structural diff helper); all 209 kernel tests still pass; CLI `mc demo --model <path>` produces brief §4.6 output verbatim.
- **Out of scope (explicit):** LLM authoring (Phase 4); UI (Phase 6); any kernel source change beyond what's required to make the existing builder API consumable from a parser. **Dep-discipline rule:** `serde` and any other parser dep must NOT be added to `mc-core`. A new parser crate (e.g. `mc-model`) MAY use `serde` / `toml` / similar, gated by an ADR that records the parser-dep choice, the keep-out-of-`mc-core` invariant, and any toolchain implications. If the parser dep needs Rust > 1.78, see the Phase 2 housekeeping toolchain item before scoping 3A.

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
