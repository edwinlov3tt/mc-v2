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

## What ships today (Phase 1A + Phase 1B)

- Rust 1.78 workspace, three crates: `mc-core`, `mc-fixtures`, `mc-cli`.
- 203 / 203 tests passing across §10.1–§10.8.
- 10 / 10 determinism gate runs identical.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --release`, `cargo test --workspace` all green.
- `cargo bench --workspace` (Phase 1B) green: 8 / 14 brief §11 1A ceilings directly comparable and pass; 6 §11.2 consolidation ceilings deferred to a Phase 2 cold-path measurement task (today's numbers are warm-cache hits, not cold consolidation cost); 1 §11.1 row (`write_input_leaf_no_deps`) over the 1A ceiling and accepted by Phase 1B as a benchmark-scope mismatch with the Acme fixture, awaiting a Phase 2 synthetic minimal-hierarchy fixture before treating the ceiling as either met or missed. See [`PERF.md`](./PERF.md) §6, §7.3, §7.4, §9.1, §9.3.
- `target/release/mc demo` matches brief §4.6.
- Allowed runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-core` dev deps: `mc-fixtures` + `criterion = "0.5"` (Phase 1B; workspace pin, default-features=false).
- No `unsafe`, no `async`, no threads, no `serde`. No kernel source modified between 1A and 1B.

Full audits: [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) (Phase 1A correctness gates + Phase 1B closure note in §6) and [`PERF.md`](./PERF.md) (Phase 1B benchmark baseline).

---

## What is queued

**Phase 2B — Consolidation Fast Path.** Handoff doc ready at [`handoffs/phase-2b-handoff.md`](./handoffs/phase-2b-handoff.md). Not started, not scheduled. One targeted kernel change: eliminate the per-call `self.dimensions.clone()` + `dim.default_hierarchy().clone()` in [`cube.rs::read_consolidated`](../crates/mc-core/src/cube.rs#L526) so the brief §11.2 3-leaf 1B target (≤ 3 µs cold) is met. Phase 2A measured the miss at 14.3 µs and localized the cause; Phase 2B closes it.

- **Recommended approach (Option A in the handoff):** wrap each dimension's hierarchies in `Arc` so the per-call clone is a refcount bump rather than a deep clone. Source change confined to `cube.rs` + at most `dimension.rs` + `hierarchy.rs`. No new dependency (`Arc` is in `std`). No public API change.
- **Acceptance gate:** PERF.md §6.7's `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)` ≤ 3 µs. Higher-fan-out rows should improve by approximately the same constant.
- **Out of scope:** §9.3 hierarchy mark closure work (next phase); any non-`cube.rs` / `dimension.rs` / `hierarchy.rs` source change; new dependencies; toolchain bump.

**Read the full Phase 2B handoff:** [`handoffs/phase-2b-handoff.md`](./handoffs/phase-2b-handoff.md). It contains the Phase 2B prompt verbatim plus eight "context-the-prompt-doesn't-spell-out" sections (the exact code to optimize, why the existing clones exist, three implementation options with the recommended path, the bench acceptance gate, the regression guard, the no-Arc-rule analysis, the tests that may need attention, the determinism gate).

**After Phase 2B:** the next sub-phase is TBD per [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md) "Phase 2 — Performance & Optimization". Likely candidates anchored in PERF.md §9: `§9.3` hierarchy mark closure (bitset-backed dirty tracker path), `§9.2` leaf-flag cache. **Snapshot COW (§9.5) is NOT data-justified** at Acme scale — defer.

**Phase 2 housekeeping (not gating; not optimization):** toolchain bump revisit — unlocks `proptest` (§10.7 doctrines) and `insta` (snapshot tests). Procedure in [`PERF.md`](./PERF.md) §9.7. CLAUDE.md §1.1 now treats `proptest`/`insta` as Phase-paired-work, not toolchain-blocked.

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
