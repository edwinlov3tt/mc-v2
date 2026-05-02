# ADR-0003: Workload sketch & perception thresholds

**Status:** Accepted — Provisional workload assumptions (auto-flip to "Needs revision" on first real planner usage data, or 2026-11-01, whichever comes first)

> Provisional ADRs without sunset clauses become permanent by inertia. This one expires.

**Date:** 2026-05-01
**Deciders:** project owner
**Phase:** Phase 2 housekeeping → Q1 (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

---

## Context

The brief §11 sets bench ceilings calibrated to *hardware* ("M1/M2 Mac or equivalent x86-64 laptop"), not to *user perception*. Phase 1B and Phase 2A established that the kernel sits well within those ceilings on Acme; Phase 2B optimized the one row that missed a 1B target. But "the bench passes" is not the same as "the user feels it as instant," and the project is now at the point where the next optimization choice has to be answered by **what does a planner actually do, and what does that planner notice**.

The strategic question Phase 2C inherits is not "which §9 candidate has the biggest absolute speedup" — Phase 2A's data already says §9.3 (write-side hierarchy mark closure) does. The question is whether ingest latency or read latency is the *user-felt* budget. Without an answer, every Phase 2C optimization is the kernel chasing its own bench numbers instead of chasing the user's experience.

This ADR is the workload sketch: who uses MarketingCubes, what they do, what feels instant vs slow, what the typical cube looks like at production scale, and which §11 rows live on the user's critical path. It is the strategic gate for Phase 2C+ — and for several Phase 3+ decisions that depend on the same knowledge (UI grid sizing in Phase 6; LLM prompt structure in Phase 4; data-import batch sizing in Phase 5).

---

## Decisions needed

The six decisions below are listed roughly in dependency order — answering #1 informs #2, etc. Each has a question, my recommendation as a starting default, and the downstream effect of the answer.

### Decision 1: workflow archetypes — what does a planner *do*?

**Question:** what is the canonical sequence of operations a planner performs in one session?

**My default (refine to taste):**

1. **Open cube.** Load model definition, hydrate from store, render initial grid view of one slice (e.g. Q1 × all-channels × Florida).
2. **Edit input cell.** Type a number in a leaf input cell, press Tab/Enter, watch the consolidated row + dependent derived cells update.
3. **Drill in.** Click a consolidated cell to expand to its child leaves; click a derived measure to see its components (Spend → Clicks → Leads → ...).
4. **Snapshot.** Save current state with a label ("plan v3 before Q2 lift").
5. **Compare versions.** View two snapshots side by side, see deltas.
6. **Bulk import actuals.** Load real numbers from external source (CSV / API), reconciled against existing plan cells.
7. **Export.** Dump current state to spreadsheet / report.

**Downstream:** every other decision in this ADR depends on this list being approximately right. If your actual workflow is materially different (e.g. read-only consumption with no edits, or batch-only with no interactive editing), the priority shifts.

**Note (post-acceptance):** Read:write ratio remains unmeasured. This is the highest-leverage amendment trigger — see "What I can't decide for you" below. The auto-flip to "Needs revision" fires on the first planner-conversation amendment that quantifies this ratio.

### Decision 2: perception thresholds — what feels instant for **this** app?

**Question:** what wall-clock latencies are "instant," "responsive," and "needs progress UI" for the planner?

**My default (industry HCI norms, sharpened for spreadsheet-like apps):**

| Threshold | Latency | Applies to |
|---|---|---|
| Frame-budget instant | ≤ 16 ms | Per-keystroke response; cell-edit echo; scroll |
| Click-instant | ≤ 100 ms | Cell-edit → grid refresh; drill-in expand; tab switch |
| Responsive | ≤ 1 s | Snapshot/rollback; compare-versions diff; small bulk operations |
| Patience limit | ≤ 10 s | Bulk imports of typical-size datasets; full slice recomputes |
| Needs progress UI | > 10 s | Anything past the patience limit must show a progress bar / cancellable op |

**Tightening for spreadsheet-shaped apps:** Excel sets the bar high. Cell edits feel instant only when the recompute fits in the same frame as the keystroke echo. If MarketingCubes is positioned as "Excel-for-planning," the *click-instant* row is the gate that matters most — and it bounds the per-edit recompute slice cost (write + N visible-cell recomputes ≤ 100 ms wall clock).

**Downstream:** these thresholds are what map onto the §11 ceilings. They turn "this bench is 14 µs" into "this is instant for slice sizes up to ~7000 cells per recompute" — answerable rather than abstract.

**Note (post-acceptance):** The 100 ms spreadsheet-shaped click-instant gate is the load-bearing threshold this ADR is anchored to. If the product positioning shifts to batch-tool-shaped (loosen the click-instant gate to 1 s, treat anything sub-second as fine), this ADR needs an amendment, not a tweak — every downstream decision in this document changes magnitude when this gate moves.

### Decision 3: §11 row → archetype mapping

**Question:** which §11 / PERF.md §6 row gates which workflow archetype, and which 1B target follows from the perception threshold?

**My default mapping (anchor: 100 ms click-instant budget; visible slice typically ≤ 1000 cells):**

| §11 / §6 row | Workflow archetype | Per-cell cost (post-2B) | Cells per slice | Slice budget @100 ms | Status |
|---|---|---:|---:|---:|---|
| `read_input_leaf_warm` | Drill in (input cell) | 48 ns | up to ~2 M | trivially under | ✓✓ |
| `read_input_leaf_cold` | Open cube (input first read) | 825 ns | up to ~120 K | under | ✓ |
| `read_derived_leaf_warm` | Drill in (derived cell) | 58 ns | up to ~1.7 M | trivially under | ✓✓ |
| `read_derived_leaf_cold` | Open cube + post-edit derived recompute | 1.15–3.57 µs | up to ~28K (Clicks) → 28K (GP) | comfortable headroom | ✓ |
| `consolidation_cold` (3-leaf) | Drill in (small consolidated) | 2.53 µs | up to ~40 K | under | ✓✓ |
| `consolidation_cold` (27-leaf Spend) | Drill in (medium consolidated) | 4.53 µs | up to ~22 K | under | ✓✓ |
| `consolidation_cold` (27-leaf Revenue, rule chain) | Drill in (medium consolidated, derived) | 52.4 µs | up to ~1.9 K | **right at gate** | ⚠ |
| `consolidation_cold` (420-leaf) | Open cube (full FY × All_Channels × USA roll-up) | 31.8 µs | up to ~3 K | under | ✓ |
| `write_input_leaf` | Edit cell | 162 µs | 1 (single edit) | under | ✓✓ |
| `dirty_propagation/spend_at_anchor` | (subset of above) | 153 µs | 1 | under | ✓✓ |
| `load_canonical_inputs` (2,520 writes) | Bulk import (Acme-scale) | 240 ms total / 95 µs per write | 2,520 cells | **at responsive gate** | ⚠ |
| `snapshot/materialized` (~25K cells) | Snapshot | 55 µs | 1 op | trivially under | ✓✓ |
| `rollback/materialized` | Compare versions / undo | 173 µs | 1 op | under | ✓✓ |

**The two ⚠ rows are where the ADR's answer to Decision 5 (ingest vs read) bites:**

- **27-leaf Revenue cold-read at 52 µs/cell × 1.9 K cells = 100 ms.** A grid showing 2000 cells of derived Revenue post-edit is right at the click-instant gate today. If production cubes show 5000+ cells, this is over.
- **`load_canonical_inputs` at 240 ms for 2,520 cells.** Linear extrapolation: 100K-cell bulk import = ~10 seconds, which is at the *patience limit*. 1M-cell import (entire planning year for a multi-region business) = ~95 seconds, which **needs progress UI**.

**Downstream:** this table becomes the budget Phase 2C+ optimizes against. "We made bench X 3× faster" is replaced by "We made the post-edit derived-Revenue grid drop from 100 ms to 33 ms."

### Decision 4: production cube shape

**Question:** the Acme demo is a 6-dim / 11-measure shape that writes 2,520 canonical input cells (the `write_canonical_inputs` payload). What scale should Phase 2C calibrate against?

**Phase 2C calibrates against a curve, not a point.** Three reference cardinalities, each defined as a multiple of Acme's **populated input-cell count** (i.e. the number of canonical input cells written by the fixture's bulk-load — *not* the post-materialize total store length, which adds derived/consolidated cache entries):

| Cardinality | Populated input cells (target) | Approx. derived/consolidated cache headroom (post-materialize) |
|---|---:|---|
| 1× Acme (calibration check) | 2,520 | ~25 K total |
| 10× Acme | ~25 K | ~250 K total |
| 50× Acme | ~125 K | ~1.25 M total |
| 100× Acme | ~250 K | ~2.5 M total |

Each isolated bench reports all three of 10× / 50× / 100×. The combined-workflow bench (see Phase 2C handoff §3) reports at 50× as the default and at 100× as a stress point.

**What 100× Acme is *not*:** real production. TM1-shaped planning workloads in industry routinely run 50–100 dimensions, hierarchy depths 5–8, and tens of millions of populated cells at typical sparsity of 0.1%–1%. 100× Acme is calibration, not a production analog. The first time real partner data lands, this decision gets amended in place — not retired.

**Why a curve and not a point.** A single data point tells you cost; a curve tells you *scaling shape*. Linear-in-N is annoying; super-linear is alarming; cliff-at-N is architectural. The curve is what determines whether §9.3 is "nice-to-have" or "load-bearing for any cube past size X."

**Why "populated input cells written by the fixture," not "total store length":** Phase 2A's §6.10 counted the dirty-set delta after a single Spend write at ~215 marks on Acme — that's a function of dim-count × hierarchy-depth × derived-measure count, not of total store length. Defining the scale by canonical input cells keeps the per-write work proportional to a well-understood number. Total store length swells with derived-leaf and consolidated cache entries that materialization populates after the bulk-load; including those in the headline number would conflate ingest cost with cache state.

**For the §6.10 Acme finding:** the per-mark cost on Acme is ~712 ns (CellCoordinate alloc + AHashSet insert dominated). Cartesian product per write scales with dim/hierarchy structure, *not* with input-cell count — but the *number* of writes during bulk ingest scales linearly with input-cell count. So 100× Acme means 100× more writes, each paying approximately the same per-mark cost as Acme (modulo allocator/cache behavior at scale, which is exactly what the curve measures).

**Downstream:** this is what determines whether §9.3 (write-side, attacks per-mark cost) is the right Phase 2D target. The shape of the curve, not its absolute values, is the deciding signal — see the Phase 2C handoff for how that signal is captured.

### Decision 5: **ingest latency vs read latency — the strategic question**

**Question:** which is the gating user-felt budget — write-side (edits, bulk imports) or read-side (grid renders, drill-in)?

**My recommendation: ingest, with a caveat.** Reasoning:

- At Acme scale, both are sub-100 ms and neither is felt. The choice is moot.
- At production scale (Decision 4 guesses), ingest hits the patience limit first. Bulk imports of 100 K cells = 10 s today; 1 M cells = 95 s. Progress UI is unavoidable.
- Single-edit recompute at production scale could also bite (per Decision 4: 35 ms/write at 100× Acme), but that's still inside the click-instant budget *if* the planner edits one cell at a time.
- Where ingest dominates: the bulk-import workflow (Decision 1 archetype #6) and the per-edit recompute *if* visible slice + dirty closure exceed click-instant budget.
- Where read dominates: the open-cube / drill-in workflow if the slice has many cold derived cells. The 27-leaf Revenue cold row is the canary here — if production grids commonly show 2K+ derived cells, the read side bites first.

**Caveat:** the read side becomes urgent only if production grids are **derived-heavy**. If most cells in a typical view are inputs (Spend, CPC) with derived Revenue/Profit shown only as totals, read latency stays sub-millisecond at any reasonable scale. If grids commonly show full derived chains (Revenue and Gross_Profit per leaf), 27-leaf Revenue × 5K cells × cold = 250 ms — over the click-instant gate.

**Downstream (now Phase 2D, after Phase 2C measurement lands):**

- If **ingest**: Phase 2D candidate is §9.3 (hierarchy mark closure / bitset-backed dirty tracker). Per-mark cost reduction directly multiplies through the ingest path.
- If **read**: Phase 2D candidate is *something past §9.4* on the read path — possibly §9.6 (recursive rule eval flattening, attacks the 27-leaf Revenue cold cost), possibly §9.2 (leaf-flag cache, opportunistic).
- If **both equally** (or the data points elsewhere): Phase 2D's §9 priority pick reads from PERF.md §6.14 (scaling shape), not from this ADR.

**Note (post-acceptance):** Phase 2C's combined-workflow bench measures both ingest and read paths in the same simulated planner session — that's the data that confirms or rejects this recommendation. If per-edit p99 grows superlinearly across the session, write-side data structures are on the critical path and §9.3 is right; if p99 stays flat across the session, the per-write fixed cost dominates and §9.2 may win. Phase 2C reports the answer; this ADR's Decision 5 is provisional until then.

### Decision 6: snapshot rate

**Question:** how often does a planner snapshot? (This determines whether `Cube::snapshot` cost matters per-session or not at all.)

**Default:** rarely — once per session, manually triggered. PERF.md §6.9 shows snapshot at 55 µs / 25K cells; rollback at 173 µs. Both trivially under any threshold for occasional use.

**Conclusion:** Snapshot COW (§9.5) stays **deferred until workflow data justifies it.** The §6.9 numbers (55 µs at 25K cells, sub-millisecond extrapolated to 100K) are linear extrapolations from a single allocator regime; they do not capture cache-wall behavior at scale or the cumulative cost of nested snapshots held simultaneously.

**The TM1 pattern that could change this calculus:** stacked sandbox/scenario workflows. Planners commonly hold 2–4 nested snapshots open at once during what-if analysis (compare current plan vs aggressive vs conservative side-by-side, edit one without disturbing the others, merge selected branches back). Phase 2C's combined-workflow bench includes a "snapshot every 10 edits" pattern with all snapshots held live until the session ends; if per-snapshot cost grows super-linearly with session depth, §9.5 reopens.

**Do not invent COW unless the data forces it.** Keep this row in §9 as a tracked candidate, not a planned phase. Phase 2C's combined-workflow bench is the test; if it passes, §9.5 stays deferred indefinitely. If it shows a cliff or super-linear growth, §9.5 jumps in priority for Phase 2D consideration.

---

## Prior information that should inform the sketch

These data points already exist; the ADR's job is to interpret them through the workflow lens.

### From PERF.md §6 (Phase 1B + Phase 2A + Phase 2B baseline)

- **Warm reads ≈ 50 ns** regardless of input/derived/consolidation. This is the "cache hit" cost — what dominates the second-and-subsequent grid renders.
- **Cold derived reads ≈ 1–3.5 µs** scaling linearly with rule chain depth (~600 ns/level × 5 levels for Gross_Profit).
- **Cold consolidations ≈ 2.5 µs (3 leaves) → 52 µs (27-leaf Revenue) → 32 µs (420-leaf Spend)** post-Phase-2B. The Revenue row is the outlier; it dominates because each of 27 leaves runs the 5-deep rule chain on a cold read.
- **Writes ≈ 162 µs each on materialized Acme**, dominated by hierarchy ancestor mark walk (PERF.md §6.10: 712 ns/mark × 215 marks = 153 µs of the 162 µs).
- **Snapshot/rollback ≈ 55–173 µs** at full materialized size.
- **Bulk import ≈ 240 ms for 2,520 cells** (95 µs/write).

### From PERF.md §6.10 (hierarchy mark microbench)

The Acme per-mark cost (~712 ns) is **7× higher than the synthetic per-mark cost (~98 ns)** — the gap is 6-dim CellCoordinate allocation + AHashSet insert, *not* hierarchy traversal. This is the strongest single data point arguing that §9.3 (bitset-backed dirty tracker keyed by per-dim element index) is the right write-side optimization: it attacks the dominant cost, not the wrong one.

### From the brief

- §11 ceilings (1A loose, 1B tight) — what the spec considers acceptable.
- §4.5.1 golden values — what "correct" looks like.
- §10.3 — the consolidation cache contract (now expressed semantically per ADR-0002).

### From CLAUDE.md

- §2.12: "Premature SIMD / parallelism / arena allocation" — Phase 1A/B/2A/2B all stayed naive. Any Phase 2C choice past "remove a clone" needs to be data-justified, not aesthetic.

### From the master phase plan

- Phase 6's "first usable product" target: ≥ 50K populated cells, ≥ 8 dimensions, realistic hierarchy depth. This is the Decision 4 anchor — production cubes are at least 2× Acme on cells and 1.3× on dims.
- Phase 5: "actuals from at least one external source" — implies bulk import is a routine flow, not a one-time setup.

### What I can't decide for you (data not yet measured)

These are the open questions whose answers would change one or more of the six decisions above. **The first planner-conversation amendment that fills any of them is an explicit auto-flip trigger** for the ADR's sunset clause.

- **Read patterns of an actual planner.** The Acme demo's read sequence (6 leaf reads + 5 consolidated + 1 trace) is a smoke test, not a workflow sample.
- **Edit cadence.** How many cells does a planner change in one session? (Decision 5's read:write ratio depends on this.)
- **Grid size in real UIs.** A grid of 100 cells vs 10K cells changes every per-cell cost calculation by 100×. Decision 3's ⚠ rows are calibrated against ~1K visible cells per slice; if the actual planning grid shows 10K+ cells routinely, the ⚠ count grows.
- **Read:write ratio.** Decision 5's "ingest matters more" recommendation is anchored on a guessed ratio. If real usage is read-heavy (planner edits one cell per minute, scrolls/drills constantly), Decision 5 flips and Phase 2D's pick changes.
- **Snapshot cadence and depth.** Decision 6's "rare, single snapshot" assumption is the load-bearing input for §9.5 staying deferred. TM1-shaped stacked sandboxes are the test Phase 2C runs; planner-side observation could pre-empt that test.
- **Product positioning shift.** Decision 2's spreadsheet-shaped 100 ms gate assumes "Excel-comparable" framing. A shift to "batch-tool-shaped" loosens the gate to 1 s and flips multiple downstream decisions — that's an amendment, not a tweak.

These gaps are why this ADR ships as **Accepted — Provisional**. Phase 2C measurement work narrows two of them empirically (combined-workflow bench → ingest vs read; stacked-snapshot bench → snapshot cadence shape). The others wait on real planner data.

---

## My recommendations as defaults — TL;DR

Accepted defaults (subject to the sunset clause):

1. **Workflow archetypes:** the 7-item list above (Open / Edit / Drill / Snapshot / Compare / Import / Export). Refine via amendment when planner-conversation data lands.
2. **Perception thresholds:** 16 ms / 100 ms / 1 s / 10 s / >10 s with the "spreadsheet-shaped" tightening on the 100 ms click-instant gate. **Load-bearing threshold** — moving it requires a full amendment, not a tweak.
3. **§11 row → archetype mapping:** the table above. Two ⚠ rows are at-the-gate today: 27-leaf Revenue cold (read-side, dominated by 5-deep recursion × 27 leaves) and `load_canonical_inputs` (write-side, dominated by per-mark hierarchy walk).
4. **Production cube shape:** Phase 2C calibrates against the **10× / 50× / 100× Acme curve** (populated input cells: ~25K / ~125K / ~250K) — see Decision 4 for the curve rationale. 100× Acme is calibration, not a production analog; real partner data triggers an in-place amendment.
5. **Ingest vs read:** **ingest, provisional.** Phase 2C's combined-workflow bench is the test that confirms or rejects this — see Decision 5 amendment note.
6. **Snapshot rate:** rarely (once per session) by default. §9.5 stays deferred. Phase 2C's "snapshot every 10 edits, all held live" pattern is the test that could reopen it.

**Strongest opinion (downgraded post-acceptance):** the original draft argued Phase 2C should be §9.3 directly. That conflated *measurement* with *optimization*. Phase 2C is measurement, not optimization. The §9.3 vs §9.2 priority call happens in Phase 2D after workload-shaped data lands. The §6.10 finding remains the strongest *evidence* for §9.3, but suggestive evidence is not a basis for kernel changes — see Phase 2C handoff for the data that turns suggestion into decision.

---

## Implications for Phase 2C / Phase 2D

**Phase 2C is measurement.** It does not optimize; it produces the data that lets Phase 2D pick. The implications cascade in two stages:

### Phase 2C scope (forced by this ADR)

- Builds 10× / 50× / 100× scaled-Acme calibration fixtures (Decision 4 curve).
- Runs isolated-operation benches at all three scales against `--baseline phase-2b`.
- Runs one combined-workflow bench at 50× (default) and 100× (stress), holding stacked snapshots live across the session (Decision 6 stress test).
- Reports scaling shape per operation in PERF.md §6.14.
- Does **not** pick a Phase 2D winner. See [`../handoffs/phase-2c-handoff.md`](../handoffs/phase-2c-handoff.md).

### Phase 2D pick (driven by Phase 2C's data, not by this ADR)

| If Phase 2C scaling-shape data says | Phase 2D candidate |
|---|---|
| Per-edit p99 grows super-linearly with session length (write-side data structure on critical path) | §9.3 hierarchy mark closure / bitset-backed dirty tracker. |
| Per-edit p99 stays flat across session, but per-write fixed cost dominates at scale | §9.2 leaf-flag cache (attacks per-write fixed work). |
| Cold consolidation grows super-linearly with cube size (read-side cliff) | §9.6 recursive rule eval flattening, or a cold-consolidation read fast path past §9.4. |
| Snapshot cost grows non-linearly with stacked-snapshot depth | §9.5 (Snapshot COW) reopens; otherwise stays deferred. |
| Multiple curves bend together | Phase 2D handoff sequences them; this ADR doesn't. |

Phase 2D and beyond chain off **PERF.md §6.14**, not this ADR. Every subsequent sub-phase's purpose is to either close another archetype's perception threshold or rule out an over-optimization that doesn't actually help.

---

## Consequences

**Positive:**

- Future "we made X 3× faster" claims become "we made the open-cube grid drop from 80 ms to 30 ms" — answerable in product terms, not bench terms.
- Phase 2C's prioritization is a 1-paragraph derivation from this ADR + PERF.md, not a fresh argument every time.
- Phase 6's UI sizing (how many cells per grid view, when to paginate, when to show a spinner) inherits the same threshold table.
- Phase 4's LLM-authored model definitions can be sanity-checked: "this model would produce a 5M-cell cube; the workload sketch sets the patience-limit at 1M for bulk imports, so this needs a smaller scope or chunked authoring."

**Accepted trade-offs:**

- The numbers in Decisions 4 and 5 are guesses until a real production cube is loaded. The ADR is explicit about this — re-anchor when first real data lands. The risk is over-optimizing for a guessed shape; mitigation is the data-driven Phase 2C+ rule (PERF.md justifies every choice).
- Tightening the 100 ms click-instant gate to 16 ms (Excel-shaped) is a stricter target than HCI norms suggest. If the product is positioned as "batch tool" rather than "Excel-shaped," loosening to 1 s is fine and frees up Phase 2C scope.

**Reversal cost:**

Cheap. This ADR is a target document, not an implementation. Re-anchor any decision when better data lands; that re-anchoring is itself an ADR (`0003-amendment-N`).

---

## Alternatives considered

1. **Skip the workload sketch; pick Phase 2C from PERF.md numbers alone.** Rejected. PERF.md gives "this row costs N µs"; the ADR's job is to translate that into "the user feels it in workflow X." Without the translation, every "should we optimize this?" is a fresh argument.
2. **Defer the workload sketch to Phase 6 (UI proof).** Rejected. Phase 2C+ optimization decisions need the answer *now*; deferring to Phase 6 means optimizing without it for the entire Phase 2 cycle.
3. **Write a prescriptive workload spec instead of a sketch.** Rejected. We don't have the data for prescription. The "sketch" framing keeps the decisions explicit and re-anchorable.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) "Phase 2 housekeeping → Q1" — defines this ADR as the strategic gate for everything past Phase 2B.
- [`../PERF.md`](../PERF.md) §6 (bench data), §8 (hot spots), §9 (Phase 2 candidates), §6.10 (per-mark cost analysis).
- [`../reports/phase-2b-completion-report.md`](../reports/phase-2b-completion-report.md) §6.A.1 — Phase 2B closure and the criterion baseline-tracking workflow this ADR uses to verify future optimization claims.
- [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §11 — bench ceiling source.
- [`../../CLAUDE.md`](../../CLAUDE.md) §2.12 — premature optimization rule.
- [`0001-phase-1-scope.md`](0001-phase-1-scope.md), [`0002-perf-assertions-in-benchmarks-not-tests.md`](0002-perf-assertions-in-benchmarks-not-tests.md) — prior ADRs.

## Notes

This ADR will likely produce a follow-up ADR each time a real production cube lands or a new workflow archetype emerges. That's expected — the workload sketch is a *snapshot of current understanding*, not a permanent contract. The contract lives in the consequences (the §11 row → archetype mapping table is what gates each Phase 2C+ choice).
