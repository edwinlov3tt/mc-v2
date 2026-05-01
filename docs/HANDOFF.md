# HANDOFF

> **5-minute orientation for a new session.** Read this first, then [`CURRENT_STATE.md`](./CURRENT_STATE.md), then the active handoff document linked below.

---

## What this project is

**MarketingCubes V2** — a Rust-based, TM1-inspired multidimensional planning kernel. The Phase 1 deliverable is a kernel that executes the **Acme demo** (6 dimensions, 11 measures, 5 rules, 2,520 input cells) end-to-end.

The project is **spec-driven**. Two files in [`specs/`](./specs/) are the contract:

- [`specs/engine-semantics.md`](./specs/engine-semantics.md) — what the kernel *means*.
- [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) — what to *build* in Phase 1.

The **operating manual** ([`../CLAUDE.md`](../CLAUDE.md)) is the rules of engagement. Read its sections 0, 1, 1.1, 2, 3, 5, and 6 before touching code. The brief overrides the semantics doc; CLAUDE.md overrides nothing but tells you how to behave.

The **scope decision** that produced both contracts is recorded in [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md).

---

## What ships today (Phase 1A)

- Rust 1.78 workspace, three crates: `mc-core`, `mc-fixtures`, `mc-cli`.
- 203 / 203 tests passing across §10.1–§10.8.
- 10 / 10 determinism gate runs identical.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --release`, `cargo test --workspace` all green.
- `target/release/mc demo` matches brief §4.6.
- Allowed runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- No `unsafe`, no `async`, no threads, no `serde`.

Full audit: [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).

---

## What is queued (active handoff)

**Phase 1B — Benchmark Baseline + PERF.md.** Close the deferred benchmark gate from Phase 1A (acceptance criterion 5) and produce a trustworthy performance baseline before Phase 2 begins.

**Read the full handoff:** [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md).

It contains the Phase 1B prompt verbatim plus six "context-the-prompt-doesn't-spell-out" sections (toolchain blocker, fixture surface area, caching layers, lazy dep graph, hot spots, brief §11 ceilings) and the touch/don't-touch file list.

**Hard rules for Phase 1B (verbatim from the handoff):**

- No model cells, no DuckDB, no WASM, no PyO3.
- No async, threads, rayon, tokio, serde, external storage.
- No `CellStore` trait yet.
- No `HashMapStore` rewrite.
- No optimization before first measuring.
- No loosening or removing existing tests; all 203 must still pass.
- If a benchmark dep requires bumping Rust, **stop and report options before changing `rust-toolchain.toml`.**

---

## Where to look in this folder

| Question | File |
|---|---|
| What's live right now? | [`CURRENT_STATE.md`](./CURRENT_STATE.md) |
| What was the last phase's audit? | [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) |
| What is the next phase doing? | [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md) |
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
