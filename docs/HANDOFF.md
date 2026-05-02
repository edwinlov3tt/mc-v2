# HANDOFF

> **5-minute orientation for a new session.** Read this first, then [`CURRENT_STATE.md`](./CURRENT_STATE.md), then [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md), then the active handoff document linked below.

---

## What this project is

**MarketingCubes V2** — a Rust-based, TM1-inspired multidimensional planning kernel. The Phase 1 deliverable is a kernel that executes the **Acme demo** (6 dimensions, 11 measures, 5 rules, 2,520 input cells) end-to-end.

The project is **spec-driven**. Two files in [`specs/`](./specs/) are the contract:

- [`specs/engine-semantics.md`](./specs/engine-semantics.md) — what the kernel *means*.
- [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) — what to *build* in Phase 1.

The **operating manual** ([`../CLAUDE.md`](../CLAUDE.md)) is the rules of engagement. Read its sections 0, 1, 1.1, 2, 3, 5, and 6 before touching code. The brief overrides the semantics doc; CLAUDE.md overrides nothing but tells you how to behave.

The **scope decision** that produced both contracts is recorded in [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md).

---

## What ships today (Phase 1A → Phase 2C)

- Rust 1.78 workspace, three crates: `mc-core`, `mc-fixtures`, `mc-cli`.
- **216 / 216 tests passing** across §10.1–§10.8 + new fixture tests (was 210; +6 from Phase 2C scaled-Acme builders incl. the mandatory scale-1× equivalence test).
- 10 / 10 determinism gate runs identical.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --release`, `cargo test --workspace` all green.
- `cargo bench --workspace` baselined across four tags: `phase-2a-cold-path-baseline` (`48d52e9`), `phase-2b-consolidation-fast-path` (`6ea58ab`), `phase-2c-workload-baseline` (`789db15`). Per-tag JSON saved under [`reports/bench-data/`](./reports/bench-data/).
- `target/release/mc demo` matches brief §4.6.
- Allowed runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-core` dev deps: `mc-fixtures` + `criterion = "0.5"` (Phase 1B; workspace pin, default-features=false).
- Kernel source touched only in Phase 2B (consolidation fast path: `cube.rs` + `dimension.rs`). Phase 2A and Phase 2C were source-locked.
- No `unsafe`, no `async`, no threads, no `serde`. No `cargo update` since Phase 1B's three transitive pins.

Full audits: completion reports for [Phase 1A](./reports/phase-1-completion-report.md), [Phase 2A](./reports/phase-2a-completion-report.md), [Phase 2B](./reports/phase-2b-completion-report.md), [Phase 2C](./reports/phase-2c-completion-report.md). Performance baseline + per-phase verification at [`PERF.md`](./PERF.md). Master plan at [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md).

---

## What is queued

**Phase 2D — Bitset-Backed Dirty Tracker (§9.3).** Handoff doc ready at [`handoffs/phase-2d-handoff.md`](./handoffs/phase-2d-handoff.md). Branch picked from PERF.md §6.14: **Branch A — §9.3** (Cartesian-product flat bitset). Phase 2C's headline finding is a super-linear cliff in `load_canonical_inputs` between 10× (4.33×/write) and 50× (19.7×/write); §6.14 attributes it to AHashSet rehash + cache-locality cost as the dirty set grows from 0 → 1.5 M entries during bulk ingest. Replacing with a flat bitset keyed by linearized coord-index makes mark/check O(1), independent of set size.

- **Acceptance gate:** PERF.md §6.12.7 `load_canonical_inputs/50x` drops from 230.84 s → ≤ 50 s.
- **Source confined to:** `crates/mc-core/src/dirty.rs` + `cube.rs` + (optional) new `cube_shape.rs` + (optional) `coordinate.rs` linearize helper. The rare phase that touches the kernel.
- **Validation:** kernel unit test `bitset_tracker_observationally_equivalent_to_ahashset` proves the new representation reproduces AHashSet's exact dirty-set membership across an arbitrary mark/clear sequence. The §10.1 `t_acme_dirty_set_size_within_bound_after_one_spend_write` MUST pass byte-for-byte.
- **Rollback paths if scope explodes:** Roaring Bitmap (Option B; new dep + ADR) or hashed-CellCoordinate (Option C; smaller win). Either is a Phase 2D.1 amendment.

**Read the full Phase 2D handoff:** [`handoffs/phase-2d-handoff.md`](./handoffs/phase-2d-handoff.md). Contains the Phase 2D prompt verbatim plus seven "context-the-prompt-doesn't-spell-out" sections (the exact code being optimized, why a flat bitset is the right shape, the §10.1 invariant proof requirement, iter() ordering semantics, shape vs snapshot lifetime, Phase 2C regression guard, Phase 2E forecast).

**After Phase 2D:** Phase 2E may not need to exist. If 2D succeeds and the §6.14 50× / 100× env-gated rows (opt-in via `MC_BENCH_CONSOL_SCALED=1`) don't surface another super-linear curve, **Phase 2 exits** and Phase 3A becomes proposed. Likely-not-needed §9.2 / §9.5 / §9.6 stay opportunistic.

**Phase 2 housekeeping:**
- Q1 (workload sketch ADR): **complete** — [ADR-0003](./decisions/0003-workload-sketch.md) Accepted — Provisional, sunset 2026-11-01.
- Q2 (toolchain bump): deferred until Phase 3A's parser-dep choice forces it.
- Q3 (criterion baseline tracking): closed; three baselines on disk per [`reports/bench-data/README.md`](./reports/bench-data/README.md).

---

## Where to look in this folder

| Question | File |
|---|---|
| What's live right now? | [`CURRENT_STATE.md`](./CURRENT_STATE.md) |
| What's the master phase plan (Phase 1 → 7)? | [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md) |
| What was the last phase's audit? | [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md), [`reports/phase-2a-completion-report.md`](./reports/phase-2a-completion-report.md) |
| What did Phase 1B/2A's benchmarks show? | [`PERF.md`](./PERF.md) |
| What was the previous handoff? | [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md), [`handoffs/phase-2a-handoff.md`](./handoffs/phase-2a-handoff.md) (both delivered) |
| What is the contract? | [`specs/engine-semantics.md`](./specs/engine-semantics.md) and [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) |
| Why was this scope chosen? | [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) |
| What rules govern how I work? | [`../CLAUDE.md`](../CLAUDE.md) |
| Where are the templates? | [`templates/`](./templates/) |
| Where are the reference PDFs? | [`../research/`](../research/) |

---

## How to think about this project

**It is an engine, not a model.** The kernel is single-threaded, allocates conservatively, returns `Result` everywhere, and has no hidden global state. Performance and correctness are gated by the brief; do not "improve" things unless the brief asks. CLAUDE.md §2 lists the recurring traps; re-read it.

**Drift is the enemy.** Every public type and test name is a contract. Renaming for "clarity" is a contract violation. CLAUDE.md §2.2 spells this out.

**Decisions get ADRs.** When you make a non-trivial choice during implementation, write an ADR in [`decisions/`](./decisions/). Future-you (or the next instance) will need to know the alternatives you considered and why you rejected them. The first ADR ([0001](./decisions/0001-phase-1-scope.md)) is the model.

**Reports describe what shipped.** When a phase ends, write a completion report in [`reports/`](./reports/) that answers the template's nine sections honestly — including deviations and out-of-scope-not-added confirmation.
