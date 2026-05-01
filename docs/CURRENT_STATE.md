# CURRENT_STATE

> **What's live right now.** Update whenever a phase ships, a gate flips, or a deferral closes.

**Last updated:** 2026-05-01
**Last commit:** `5ebc7bc` — *docs+research: reorganize into research-heavy filing system* (Phase 1A kernel at `4aa674a`)
**Branch:** `main` (tracking `origin/main` at github.com/edwinlov3tt/mc-v2)

---

## What's shipping

- **Phase 1A — Rust kernel for the Acme demo.** Complete. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).

## What's queued

- **Phase 1B — Benchmark Baseline + PERF.md.** Not started. Handoff doc ready at [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md). Closes Phase 1A acceptance criterion 5.

## Active ADRs

- [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) — Phase 1 scope: smallest kernel that runs the Acme demo. **Status:** Accepted.

---

## Build / test / lint state (at HEAD)

| Gate | Command | Status |
|---|---|---|
| Build | `cargo build --release --workspace` | ✓ zero warnings |
| Format | `cargo fmt --check --all` | ✓ |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ |
| Tests | `cargo test --workspace` | ✓ 203 / 0 |
| Determinism (10×) | `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | ✓ 10 / 10 identical |
| CLI demo | `cargo run --release --bin mc -- demo` | ✓ matches brief §4.6 |
| Benchmarks | `cargo bench` | **DEFERRED** — see Phase 1B handoff |

---

## Test count by target

| Target | Count |
|---:|---|
| `mc-core` unit tests | 83 |
| `tests/acme_demo.rs` | 20 |
| `tests/writeback.rs` | 11 |
| `tests/consolidation.rs` | 12 |
| `tests/trace.rs` | 9 |
| `tests/dependency.rs` | 7 |
| `tests/locks_permissions.rs` | 8 |
| `tests/correctness.rs` | 16 |
| `tests/hierarchy_cycle.rs` | 10 |
| `tests/duplicate_elements.rs` | 6 |
| `tests/coordinate_validity.rs` | 9 |
| `tests/value_nan.rs` | 8 |
| `mc-fixtures` unit tests | 4 |
| **Total** | **203** |

---

## Toolchain + dependency state

- Rust 1.78 pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml).
- `mc-core` runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-fixtures` and `mc-cli` depend on `mc-core` only.
- **Deferred dev-deps:** `criterion`, `proptest`, `insta` declared at workspace level but **not** pulled into `mc-core` because of the Rust 1.78 / `clap_lex 1.1.0` / `edition2024` blocker. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §4.1 and CLAUDE.md §1.1. Closure conditions documented there.

---

## Open deferrals (Phase 1A acceptance criteria)

| # | Criterion | State | Owner |
|---:|---|---|---|
| 5 | `cargo bench --release` under §11 1A ceilings | **DEFERRED** until criterion returns | Phase 1B |

All other Phase 1A criteria (1–4, 6–10) satisfied. Full table in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §5.

---

## Deviations from the brief that are still in effect

These are documented in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §3–§4 and remain active until a future spec or amendment supersedes them:

1. **Toolchain-blocked dev-deps** — proptest doctrines and criterion benches deferred per brief §0.A.
2. **§10.1 dirty-set assertions reframed as deltas** — the bound is preserved (215); the comparison frame changed because `write_canonical_inputs` legitimately accumulates marks across 2,520 input writes.
3. **§10.5 `t_dependency_graph_rejects_undeclared_dependency_in_test_mode`** asserts `RuleBodyTypeMismatch` (registration-time) rather than `UndeclaredDependency` (runtime). Strictly stronger guarantee.
4. **§10.7 `doctrine_no_mutation_of_frozen_dimensions`** asserts `dim.is_frozen()` post-build because no public mutation API exists in Phase 1; `EngineError::DimensionFrozen` variant retained for Phase 2.
5. **§10.7 `doctrine_atomicity_of_write` and `doctrine_causality`** are no-op stubs per §0.A; deterministic equivalents in `tests/acme_demo.rs`.

---

## Known Phase 2 follow-ups

Source-tagged hooks and surfaced findings from Phase 1A. **Not scheduled.** Full list in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §8.

Highlights:

- Toolchain bump → criterion / proptest / insta unlock.
- `CellStore` trait introduction (Phase 1 ships concrete `HashMapStore`).
- `Snapshot` copy-on-write at scale (Phase 1 ships deep-clone).
- Hierarchy-clone hot-path in `cube.rs::read_consolidated`.
- Lock-acquisition capability check hardening.

---

## Repo layout (top level)

```
.
├── crates/
│   ├── mc-core/           kernel
│   ├── mc-fixtures/       Acme demo cube
│   └── mc-cli/            `mc demo` runner
├── docs/                  this folder
├── research/              raw reference PDFs (TM1 manuals, books, infra specs)
├── CLAUDE.md              operating manual
├── README.md              workspace README
├── Cargo.toml             workspace manifest
├── Cargo.lock
└── rust-toolchain.toml    pins Rust 1.78
```

---

## How to update this file

When a phase ships:

1. Update **Last updated**, **Last commit**, **Branch**.
2. Update **What's shipping / What's queued / Active ADRs**.
3. Update the build / test / lint state table.
4. Update the test count table if any tests were added.
5. Move closed deferrals out of the table; add closure dates to the relevant phase report.
6. Add an ADR if a new scope-level decision was made; link it in **Active ADRs**.

When a deferral closes (e.g. `cargo bench` becomes unblocked):

1. Move the row out of **Open deferrals**.
2. Update the relevant report's §6 to reflect closure.
3. If the closure required a decision (e.g. "we chose to bump Rust to 1.85"), write an ADR.
