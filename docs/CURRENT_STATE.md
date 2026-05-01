# CURRENT_STATE

> **What's live right now.** Update this whenever a phase ships, a gate flips, or a deferred item closes.

**Last updated:** 2026-05-01
**Last commit:** `4aa674a` — *Initial commit: Phase 1 Rust kernel for MarketingCubes V2*

---

## What's shipping

- **Phase 1A — Rust kernel for the Acme demo.** Complete. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).

## What's queued

- **Phase 1B — Benchmark Baseline + PERF.md.** Not started. Handoff doc ready at [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md).

---

## Build / test / lint state

| Gate | Command | Status |
|---|---|---|
| Build | `cargo build --release --workspace` | ✓ zero warnings |
| Format | `cargo fmt --check --all` | ✓ |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ |
| Tests | `cargo test --workspace` | ✓ 203 / 0 |
| Determinism (10×) | `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | ✓ 10 / 10 identical |
| CLI demo | `cargo run --release --bin mc -- demo` | ✓ matches brief §4.6 |
| Benchmarks | `cargo bench` | **DEFERRED** — see Phase 1B handoff §A |

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
- **Deferred dev-deps:** `criterion`, `proptest`, `insta` declared at workspace level but **not** pulled into `mc-core` because of the Rust 1.78 / `clap_lex 1.1.0` / `edition2024` blocker. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §4.1 and CLAUDE.md §1.1. Closure conditions documented in the same place.

---

## Open deferrals (Phase 1A acceptance criteria)

| # | Criterion | State | Owner |
|---:|---|---|---|
| 5 | `cargo bench --release` under §11 1A ceilings | **DEFERRED** until criterion returns | Phase 1B |

All other Phase 1A criteria (1–4, 6–10) are satisfied. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §5.

---

## Deviations from the brief that are still in effect

These are surfaced in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §3–§4 and remain active until a future brief or amendment supersedes them:

1. **Toolchain-blocked dev-deps** — proptest doctrines and criterion benches are deferred per brief §0.A.
2. **§10.1 dirty-set assertions reframed as deltas** — the bound is preserved (215); the comparison frame changed because `write_canonical_inputs` legitimately accumulates marks across 2,520 input writes.
3. **§10.5 `t_dependency_graph_rejects_undeclared_dependency_in_test_mode`** asserts `RuleBodyTypeMismatch` (registration-time) rather than `UndeclaredDependency` (runtime). Strictly stronger guarantee.
4. **§10.7 `doctrine_no_mutation_of_frozen_dimensions`** asserts `dim.is_frozen()` post-build because no public mutation API exists in Phase 1; `EngineError::DimensionFrozen` variant retained for Phase 2.
5. **§10.7 `doctrine_atomicity_of_write` and `doctrine_causality`** are no-op stubs per §0.A; deterministic equivalents in `tests/acme_demo.rs`.

---

## Known Phase 2 follow-ups

These are explicit hooks in the source or surfaced during Phase 1A. **They are not scheduled.** See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §8 for the full list.

Highlights:

- Toolchain bump → criterion / proptest / insta unlock.
- `CellStore` trait introduction (Phase 1 ships concrete `HashMapStore`).
- `Snapshot` copy-on-write at scale (Phase 1 ships deep-clone).
- Hierarchy clone hot-path in `cube.rs::read_consolidated`.
- Lock-acquisition capability check hardening.

---

## How to update this file

When a phase ships:
1. Update the "What's shipping / What's queued" sections.
2. Update the "Last commit" line with the new HEAD.
3. Update the build / test / lint state table.
4. Update the test count table if any tests were added.
5. Move closed deferrals to the relevant phase report.
6. Add an entry to [`RESEARCH_JOURNAL.md`](./RESEARCH_JOURNAL.md) with the date, summary, and link to the new completion report.
