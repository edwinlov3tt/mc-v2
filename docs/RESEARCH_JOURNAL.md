# RESEARCH_JOURNAL

> Chronological log of what we tried, what shipped, what failed. **Append, never rewrite.** New entries go at the top.

---

## 2026-05-01 — Documentation reorganization (post-Phase 1A)

**Status:** complete.

After Phase 1A shipped, the `docs/` and `research/` folders were flat — eight markdown files mixed together (PRD, transfer inventory, two GPT responses, two Claude responses, the brief, the semantics spec, the completion report, the Phase 1B handoff) and seven PDFs in a single directory.

Reorganized to mirror the claw-core convention so this project can carry research and context heavy enough for many sessions:

- `docs/` now has subfolders for `planning/`, `external-research/`, `reports/`, `handoffs/`, `concepts/`, `experiments/`, `hypotheses/`, `dead-ends/`, `audits/`, `archive/`, and `templates/`.
- `research/` now has subfolders for `tm1/`, `books/`, and `architecture/`.
- Top-level navigation files added at `docs/`: `README.md`, `HANDOFF.md`, `CURRENT_STATE.md`, `RESEARCH_JOURNAL.md`.
- Templates added for concept, experiment, hypothesis, dead-end, handoff, and phase completion report.
- Locked input documents (`engine-semantics.md` and `phase-1-rust-kernel-build-brief.md`) **kept at `docs/` root** because source comments reference them by name and CLAUDE.md §6.6 acceptance criterion 7 requires they remain unchanged.

Internal markdown links inside the moved Phase 1A files were updated. Build / clippy / test gates re-verified after the reorganization.

---

## 2026-05-01 — Phase 1B handoff drafted

**Status:** queued for next session.
**File:** [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md).

Goal of Phase 1B: close the deferred benchmark gate from Phase 1A (acceptance criterion 5 — `cargo bench`) and produce `docs/PERF.md` with results, environment, commands, observations, and recommended Phase 2 optimizations.

Hard rules: no model cells, no DuckDB, no WASM, no PyO3, no async / threads / rayon / tokio / serde / external storage, no `CellStore` trait yet, no `HashMapStore` rewrite, no optimization before measurement, no test removal, all 203 tests must still pass, no Rust toolchain bump without explicit approval.

The handoff includes six "context-not-spelled-out" sections covering: the toolchain blocker rationale (Rust 1.78 / `clap_lex 1.1.0` / `edition2024`), the fixture surface area benchmarks should use, the two caching layers cold/warm reads must contend with, the lazy dep graph and `materialize_all_dependencies`, the hot-spots already identified during Phase 1A, and the brief §11 ceilings as calibration not pass/fail.

---

## 2026-05-01 — Phase 1A completion report drafted + 10/10 determinism gate

**Status:** complete.
**File:** [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).

Wrote the full Phase 1A audit covering:
- Commands run + outputs.
- Final test count (203 / 0).
- Five deviations from the brief, each with rationale.
- Acceptance criteria status (9 of 10 satisfied; criterion 5 deferred per brief §0.A).
- Implemented files / modules table.
- Known Phase 2 follow-ups.
- Confirmation no out-of-scope features were added.

Ran the full `for i in $(seq 1 10); do cargo test --workspace -q ...; done` determinism gate. All 10 runs reported `203 passed / 0 failed` identical — CLAUDE.md §6.1 step 5 satisfied.

---

## 2026-05-01 — Phase 1A initial commit

**Status:** complete.
**Commit:** `4aa674a` — *Initial commit: Phase 1 Rust kernel for MarketingCubes V2*.

Phase 1A is shipped: the Rust kernel implements the brief end-to-end. Full audit in the completion report (entry above).

Key implementation milestones:
- Built engine layers in §15 order (id, value, error, element, dimension, hierarchy, coordinate, cell, store, trace, rule, dependency, dirty, consolidation, permission, lock, snapshot, cube, slice).
- Built the Acme fixture with 6 dimensions, 11 measures, 5 rules, 2,520 input cells per the canonical formulas in brief §4.5.
- Wrote integration tests for §10.1–§10.8: `acme_demo.rs`, `writeback.rs`, `consolidation.rs`, `trace.rs`, `dependency.rs`, `locks_permissions.rs`, `correctness.rs`. Step-3 deliverable tests retained: `hierarchy_cycle.rs`, `duplicate_elements.rs`, `coordinate_validity.rs`, `value_nan.rs`.
- Wrote the `mc demo` runner per brief §4.6.

Notable design decisions made during build (each surfaced in chat at the time):
- Phase 1 ships concrete `HashMapStore` per brief §3.9; `CellStore` trait deferred to Phase 2.
- Consolidation results cache in `cube.rs::read_consolidated` — required to satisfy `t_consolidation_caches_value_within_revision`.
- `mark_closure` excludes the freshly-written root coord (it is by definition clean post-write).
- `compute_dirty_ancestors` marks every (ancestor × measure-to-mark) coord including same-leaf-different-measure cells; only the exact-written (coord, measure) is excluded.
- Dirty-set tests reframed as delta assertions (the brief's 215 bound preserved; the comparison frame changed) because `write_canonical_inputs` legitimately accumulates marks across 2,520 writes.

---

## 2026-04-30 — Brief + semantics finalized

**Status:** complete.
**Files:** [`engine-semantics.md`](./engine-semantics.md), [`phase-1-rust-kernel-build-brief.md`](./phase-1-rust-kernel-build-brief.md), [`planning/MC-PRD.md`](./planning/MC-PRD.md), [`planning/transfer-inventory.md`](./planning/transfer-inventory.md), [`external-research/`](./external-research/).

Two contractual documents produced after a back-and-forth between Claude and GPT-5 critiquing the original PRD + transfer inventory. The exchange is preserved in [`external-research/`](./external-research/):

- [`chat-gpt-response-1.md`](./external-research/chat-gpt-response-1.md) — GPT-5's critique of the PRD and inventory ("strong foundation, not ready to execute as-is").
- [`claude-response-2.md`](./external-research/claude-response-2.md) — Claude's response, conceding most points.
- [`chat-gpt-response-2.md`](./external-research/chat-gpt-response-2.md) — GPT-5's reply.
- [`claude-xgboost.md`](./external-research/claude-xgboost.md) — Claude analyzing XGBoost experiments (relevant to model-cell discussions, **not in Phase 1 scope**).

Net effect: the brief gained §0.A (active-deviations index), §4.5.1 (golden-input table including the anchor-cell check at Mar/Paid_Search/Tampa), and per-test invariants in §10. CLAUDE.md was added as the operating manual.

---

## How to write entries

- New entries at the **top** of the file.
- Include date (ISO), one-line summary, status (`complete | active | open | superseded`).
- Link to the detail file (experiment / hypothesis / dead-end / report / handoff).
- Don't summarize what's in the linked file — that's the file's job. Summarize **why this happened in this session** and **what to do next**.
- Preserve old entries verbatim. If an entry needs correction, append a `**Correction (YYYY-MM-DD):**` paragraph below it; don't edit the original.
