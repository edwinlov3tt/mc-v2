# MASTER_PHASE_PLAN

> **The single source of truth for what phase the project is in and what comes next.**
>
> Read this before inventing a phase name or starting work that isn't already on the list. If a new phase is needed and it isn't here, add it here first (and link the ADR explaining the decision).

**Last updated:** 2026-05-01 (post-Phase 2A)
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
4. **Performance is within the brief's §11 1B targets** on a representative production-sized cube (≥ 50K cells, ≥ 8 dimensions, ≥ 3 hierarchies per dim).
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
| **2B** | Consolidation Fast Path (hierarchy clone) | **proposed** | — |
| **2C–2N** | Further optimization rounds (TBD) | not started | — |
| **3A** | Model definition layer — declarative format + parser | **proposed** | — |
| **3B–3N** | Model layer extensions (TBD) | not started | — |
| **4** | LLM-assisted model authoring | not started | — |
| **5** | Data integration & actuals | not started | — |
| **6** | UI & internal app proofs | not started | — |
| **7** | Productization (incl. Media Partner App) | not started | — |

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

### 2B — Consolidation Fast Path (proposed)

- **Status:** proposed; handoff doc to land at `docs/handoffs/phase-2b-handoff.md`.
- **Purpose:** Eliminate the per-call hierarchy clone in [`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs#L526) — the ~14 µs fixed-cost floor that today causes the brief §11.2 3-leaf 1B target (3 µs) to miss by ~5×.
- **What it proves:** Whether the kernel's consolidation algorithm hits the 1B targets once a single localized over-cloning is removed.
- **Deliverables (planned, high-level):** a kernel change confined to `cube.rs` (and possibly `dimension.rs`) that replaces `self.dimensions.clone()` + `dim.default_hierarchy().clone()` with `&[Dimension]` + `Arc<Hierarchy>` per dim or equivalent; a fresh PERF.md §6.7 re-run; a brief Phase 2B completion report.
- **Acceptance gates (planned):** all gates from 2A still pass; brief §11.2 3-leaf 1B target met (≤ 3 µs); every higher-fan-out cold consolidation row improves by approximately the same fixed amount; no semantics change.
- **Out of scope (explicit):** §9.3 hierarchy mark closure changes; any new dependency; any new public API; any work beyond `cube.rs` / `dimension.rs` source files.

### 2C, 2D, … (TBD)

Sub-phases beyond 2B are intentionally not pre-named. Open a new sub-phase only when a measured 1B miss in a fresh PERF.md justifies it. The candidate list (in rough priority order, anchored in PERF.md §9):

- **§9.3 Hierarchy mark closure cost.** Acme writes spend ~712 ns/mark vs ~98 ns/mark on the synthetic; the gap is per-mark CellCoordinate allocation + AHashSet insert. Likely path: bitset-backed dirty tracker keyed by per-dim element index (PERF.md §9.3 path b). Lazy ancestor marks (path a) is a behavior shift and would require a §10.1 invariant audit; deprioritized.
- **§9.2 leaf-flag cache** on `Element` (`is_leaf_in_default_hierarchy: bool`). Trivial; opportunistic.
- **§9.5 Snapshot COW.** NOT data-justified at Acme scale. Defer until a workflow takes many snapshots per turn.
- **§9.6 Recursive rule eval.** Leave alone; well within 1B targets.

**Phase 2 exits** when no remaining 1B miss in `PERF.md` is unaddressed and unexplained.

---

## Phase 3 — Model Definition Layer

> Today, cubes are authored by writing Rust against `mc-core`'s builder API (see `mc-fixtures::build_acme_cube`). That doesn't scale to a UI or LLM-assisted authoring. Phase 3 introduces a declarative format that compiles to the existing builder API. **No kernel semantics change** — this is a translation layer.

### 3A — Declarative model format + parser (proposed)

- **Status:** proposed (placeholder; needs an ADR before formal scoping).
- **Purpose:** Define a config format (likely TOML or a small custom DSL — design decision pending) that describes cube models and produces a `Cube` via the existing `CubeBuilder` / `Dimension::builder` / `Hierarchy::builder` / `Rule { … }` constructors.
- **What it proves:** A round-trip from `model.toml` to `cargo run --release --bin mc -- demo --model model.toml` produces an identical cube to `build_acme_cube()` (same coordinates, same dirty propagation, same consolidation results).
- **Deliverables (planned, high-level):** a new crate (`mc-model` or similar); a parser; a schema validator with structured error messages; the Acme cube re-expressed as a declarative file alongside the existing Rust builder; a round-trip test asserting parsed cubes produce byte-identical Acme demo output.
- **Acceptance gates (planned):** parsing the Acme model file produces the same `Cube` (per a structural diff helper); all 209 kernel tests still pass; CLI `mc demo --model <path>` produces brief §4.6 output verbatim.
- **Out of scope (explicit):** LLM authoring (Phase 4); UI (Phase 6); any kernel source change beyond what's required to make the existing builder API consumable from a parser; `serde` if it brings in async-runtime transitives — pick a parser that keeps the dep set tight.

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

## Phase 6 — UI & Internal App Proofs

> A web UI (or internal tool) that lets a planner view, drill, edit, snapshot, and compare cubes without touching the CLI.

- **Status:** not started.
- **Purpose:** Make the kernel + model layer usable by a non-engineer. This is the smallest step that transforms the project from "library" to "application."
- **What it proves:** An internal operator can complete a real planning task end-to-end (load a model, ingest actuals, edit a forecast, take a snapshot, compare versions, export results) without engineering help.
- **Deliverables (anticipated, high-level):** a web UI (framework TBD); navigation surface for dimensions / hierarchies / measures; drill-down + grid editing for input cells; visible plan-vs-actual variance; snapshot + version compare; at least one internal proof-of-value scenario shipped end-to-end (not the Media Partner App — see Phase 7).
- **Acceptance gates (anticipated):** at least one internal team member can use the UI to complete a planning task without instruction; performance against a representative production-sized cube (≥ 50K cells) hits brief §11 1B targets; auth + audit trail in place for the internal team.
- **Out of scope (explicit):** customer-facing UX; multi-tenancy; the Media Partner App (Phase 7); any change to the kernel's `Cube` public API beyond what the UI strictly requires.

**Why "internal app proofs" first, not the Media Partner App.** The Media Partner App is a customer-facing artifact with its own scoping, scaling, and security requirements. Shipping it before an internal proof-of-value is high-risk; shipping the internal proof first lets us validate the model + UI loop on a captive audience before any external user sees it.

---

## Phase 7 — Productization (incl. Media Partner App)

> Turn the validated internal product into something a paying customer can use. The Media Partner App is the proof-of-value use case driving this phase, but the phase covers everything that "productization" implies — not the app alone.

- **Status:** not started.
- **Purpose:** Convert the internal app into a production-grade, multi-tenant offering that a customer (an external media partner) can use independently.
- **What it proves:** The kernel + authoring layer + UI scale to multi-tenant, multi-user, customer-facing use without sacrificing the determinism and correctness that Phases 1–6 established.
- **Deliverables (anticipated, high-level):** multi-tenancy (data-isolated per partner); a Media Partner App (the first customer-facing surface, per the user's note); production-grade auth + audit + observability + backup; a documented onboarding path for new partners; SLAs / on-call rota / incident playbooks.
- **Acceptance gates (anticipated):** at least one external partner is using the system in production for one full planning cycle without engineering escalation; security review passed; data export / portability story documented.
- **Out of scope (explicit):** anything that violates the Phase 1–6 invariants (no behavior changes that break the §10 contract tests; no `serde` / `tokio` / `rayon` in `mc-core` unless an ADR explicitly retires that constraint).

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
