# ADR-0001: Phase 1 scope — smallest kernel that runs the Acme demo

**Status:** Accepted
**Date:** 2026-04-30
**Deciders:** Project owner + implementing instance
**Phase:** 1A

---

## Context

MarketingCubes V2 is a TM1-inspired multidimensional planning kernel. The original product framing (see [`../product/MC-PRD.md`](../product/MC-PRD.md)) is broad: planning, finance, marketing-attribution, model-backed cells, distributional outputs, possibly DuckDB-backed storage at scale, possibly WASM/PyO3 bindings, possibly federated reads. All of these are eventually-plausible but not all of them are necessary to **prove the kernel** end-to-end.

The risk pattern for an engine project at this stage is well-understood: ambition collides with under-specified semantics, and the project either ships a thin sliver that doesn't validate the kernel or a wide slice that has too many concurrent unknowns to debug. The external review captured in [`../external-conversations/chat-gpt-response-1.md`](../external-conversations/chat-gpt-response-1.md) makes this argument directly: "very strong foundation, but it is not ready to execute as-is." The advisable correction was to draw an unambiguously narrow scope and turn the PRD into an executable engine specification.

The two contractual documents that came out of that — [`../specs/engine-semantics.md`](../specs/engine-semantics.md) and [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) — are the contract. This ADR records the scope decision those documents implement, separate from the documents themselves, so the *reasoning* doesn't get lost when the briefs are eventually superseded.

## Decision

**Phase 1 ships the smallest Rust kernel that can execute the Acme demo end-to-end and pass every test in brief §10.** Nothing more.

**What we are doing:**

- Build a Rust workspace with three crates: `mc-core`, `mc-fixtures`, `mc-cli`.
- Implement the kernel modules listed in brief §3 (id, value, error, element, dimension, hierarchy, coordinate, cell, store, trace, rule, dependency, dirty, consolidation, permission, lock, snapshot, cube, slice). Names character-exact.
- Build the Acme cube fixture per brief §4: 6 dimensions, 11 measures, 5 rules, 2,520 input cells loaded by closed-form formulas.
- Pass every test in brief §10.1–§10.8.
- Ship a `mc demo` CLI runner per brief §4.6.
- Pin Rust toolchain at 1.78 in `rust-toolchain.toml`.
- Allowed runtime deps in `mc-core`: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.

**What we are explicitly NOT doing:**

- **No model cells.** Distributional output, regression weights, calibration layers — all out of scope.
- **No DuckDB / external storage.** `HashMapStore` is the only store. No trait abstraction yet.
- **No WASM, no PyO3, no language bindings.**
- **No `async`, no threads, no `rayon`, no `tokio`.** The kernel is single-threaded.
- **No `serde`, no `anyhow`.** No serialization layer in this phase.
- **No `unsafe`.**
- **No `CellStore` trait.** Concrete `HashMapStore` ships per brief §3.9.
- **No `MeasureRole::Both`** — semantics doc allows it, brief excludes it.
- **No multiple hierarchies per dimension** — semantics allows, brief restricts to one default per dim.
- **No optimizations beyond what the brief asks for.** Phase 1A is naive-correct; Phase 1B and Phase 2 will benchmark and optimize where measurement justifies.

## Consequences

**Positive:**

- The contract is small enough to pass cleanly. Phase 1A shipped 203 / 203 tests with a 10-run determinism gate, all gates green ([`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md)).
- Every concept that lands later (model cells, federated storage, language bindings) lands on top of a kernel whose correctness is already locked in.
- The contract documents (semantics + brief) are short enough to read in one sitting, which means the operating manual ([`../../CLAUDE.md`](../../CLAUDE.md)) can stay short too.

**Negative / accepted trade-offs:**

- Phase 1A produces a kernel that, by design, doesn't yet do anything a planning-finance customer would pay for. The PRD's vision-level features are deferred to Phase 2+.
- Some implementation choices ship in their naive form (e.g. consolidated reads clone hierarchies on every call, snapshots are deep-clones of the store) and will need re-measurement / optimization in Phase 1B and beyond. These are tagged in source.
- Re-adding criterion / proptest / insta is blocked by the Rust 1.78 toolchain pin (see [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §4.1). Phase 1A acceptance criterion 5 (`cargo bench`) is deferred. Phase 1B closes this gate.

**Reversal cost:**

Cheap to reverse a *narrowing* (adding back a feature later is normal). Expensive to reverse a *broadening* (relaxing the hard rules above mid-phase invalidates the gate). The hard rules are therefore one-way during the phase: change the rules → start a new phase.

## Alternatives considered

1. **Build the broader vision in one phase.** Rejected. Without a passing kernel to anchor on, every concurrent unknown (storage shape, model integration, async) compounds debugging time. The external review is explicit about this.

2. **Build a thinner sliver that doesn't include consolidation, dependency tracking, locks, or permissions.** Rejected. The test contract in brief §10.1–§10.8 specifically exercises these features against the Acme cube. Cutting them produces a kernel that doesn't validate the engine's core invariants.

3. **Allow `serde` for cross-crate type sharing.** Rejected. `serde` would mask where serialization decisions are made and would add a decision surface (which fields are public, which are skipped, what version) we don't have evidence to make yet. Phase 2+ revisits this when there's a concrete consumer.

4. **Allow async / threads up front.** Rejected. The brief is single-threaded for a reason: most engine bugs in this class come from concurrency, and Phase 1 cannot afford concurrency bugs while it's still proving correctness. CLAUDE.md §2.12 and §1 enforce this.

5. **Use a `CellStore` trait so storage backends can be swapped.** Rejected. Brief §3.9 / CLAUDE.md §2.7 are explicit: concrete `HashMapStore` only. The trait introduction is a Phase 2 concern, after the concrete shape has been exercised against real test load.

## Cross-links

- Specs that implement this scope: [`../specs/engine-semantics.md`](../specs/engine-semantics.md), [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
- Operating manual encoding the rules: [`../../CLAUDE.md`](../../CLAUDE.md).
- The external conversation that surfaced the scope-discipline argument: [`../external-conversations/chat-gpt-response-1.md`](../external-conversations/chat-gpt-response-1.md), [`../external-conversations/claude-response-2.md`](../external-conversations/claude-response-2.md), [`../external-conversations/chat-gpt-response-2.md`](../external-conversations/chat-gpt-response-2.md).
- The product framing this scope cuts down from: [`../product/MC-PRD.md`](../product/MC-PRD.md), [`../product/transfer-inventory.md`](../product/transfer-inventory.md).
- Phase 1A audit of how this scope was executed: [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md).
- Source code where the scope shows up: [`../../crates/mc-core/`](../../crates/mc-core/), [`../../crates/mc-fixtures/`](../../crates/mc-fixtures/), [`../../crates/mc-cli/`](../../crates/mc-cli/).

## Notes

The Phase 1A acceptance criterion 5 deferral (criterion benchmarks blocked by Rust 1.78 / `clap_lex` / `edition2024`) is **not** a scope change — the brief authorizes deferral in §0.A. Phase 1B closes that gate. The scope decision in this ADR survives the deferral; it is a deliberate, documented postponement of one acceptance gate, not a relaxation of the kernel scope.
