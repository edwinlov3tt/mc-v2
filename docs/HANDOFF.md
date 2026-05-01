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

**Phase 2A — Cold-path benchmark expansion.** Handoff doc ready at [`handoffs/phase-2a-handoff.md`](./handoffs/phase-2a-handoff.md). Not started, not scheduled. **Measure first; do not optimize the kernel until Phase 2A's data is in.** Phase 1B established that warm reads are cheap and writes are expensive on Acme, but two things are currently hidden:

- The consolidation cache hides the cold-path cost (today's ~67 ns numbers are cache hits, not cost-of-walk).
- The Acme hierarchy fan-out hides the true "no-dependents" write cost (`write_input_leaf_no_deps` and `write_input_leaf` measure the same thing on Acme, ~165 µs both).

Phase 2A's job is to close those two holes before any kernel optimization decision is made. Concretely:

1. **Cold consolidation benchmarks.** Per-iteration setup that issues a write to invalidate the cache before each timed read, mirroring the cold/warm split in `derived_read.rs`. Until this lands, brief §11.2's 1A ceilings (50 µs / 1 ms / 20 ms / 5 ms / 2 ms) are uncomparable. (PERF.md §9.1)
2. **Synthetic minimal-hierarchy fixture for `no_deps` writes.** A second fixture with no Time/Channel/Market hierarchies, so a Spend write actually has zero ancestors to mark. Resolves the §7.3 benchmark-scope mismatch and gives the brief §11.1 50 µs ceiling something to measure against. (PERF.md §7.3, §9.3)
3. **Snapshot clone benchmark.** `Snapshot::take` at scale. Phase 1 ships deep-clone; not exercised by the current bench suite. (PERF.md §8.3, §9.5)
4. **Hierarchy ancestor marking microbench.** Isolate the dominant write cost from other write fixed costs (permission/lock/type checks, store insert, revision bump). (PERF.md §8.1, §9.3)

**Phase 2B — Optimization.** Only after 2A's data justifies the work. Candidates (do not start until 2A points at them with numbers):

- `is_consolidated_coord` fast-path (cache `is_leaf_in_default_hierarchy` on `Element`). (PERF.md §8.5, §9.2)
- Hierarchy mark-closure: lazy ancestor marks vs bitset-backed dirty tracker. (PERF.md §9.3)
- `read_consolidated` hierarchy-clone hot path. (PERF.md §8.2, §9.4; Phase 1A follow-up #9)
- `Snapshot` COW. (PERF.md §9.5; Phase 1A follow-up #3)
- `CellStore` trait introduction. (Phase 1A follow-up; not justified by Phase 1B data — defer until a second store impl is genuinely needed.)

**Phase 2 housekeeping (not gating; not optimization).**

- Toolchain bump revisit — unlocks `proptest` (§10.7 doctrines) and `insta` (snapshot tests). Procedure in [`PERF.md`](./PERF.md) §9.7. CLAUDE.md §1.1 now treats `proptest`/`insta` as Phase-2-paired-work, not toolchain-blocked.

**Read the full Phase 2A handoff:** [`handoffs/phase-2a-handoff.md`](./handoffs/phase-2a-handoff.md). It contains the Phase 2A prompt verbatim plus seven "context-the-prompt-doesn't-spell-out" sections (consolidation cache mechanics, mc-fixtures extension, iter_batched_ref pattern, hierarchy ancestor walk isolation, snapshot internals, brief §11 ceiling map, Cargo.lock pin protection) and the touch/don't-touch file list.

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
