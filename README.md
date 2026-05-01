# MarketingCubes

A Rust-based, TM1-inspired multidimensional planning kernel for marketing
and finance.

**Status:** Phase 1 — first deliverable in progress.

## Documentation

The two contractual specs live in [docs/](./docs/):

- [`engine-semantics.md`](./docs/engine-semantics.md) — the durable semantic
  model. Every concept (Cube, Dimension, Element, Cell, Provenance, Rule,
  Hierarchy, Permission, Lock, Snapshot, etc.) defined with invariants and a
  marketing-to-finance example.
- [`phase-1-rust-kernel-build-brief.md`](./docs/phase-1-rust-kernel-build-brief.md)
  — what to build *now*. Cargo workspace layout, exact module signatures, a
  60+ test correctness doctrine, and Phase-1A correctness ceilings vs Phase-1B
  optimization targets.

The brief overrides the semantics doc wherever they differ.

## Workspace layout

```
crates/
├── mc-core/      # the kernel (this is the Phase 1 deliverable)
├── mc-fixtures/  # Acme demo cube (skeleton today; lands with cube.rs)
└── mc-cli/       # smoke-test runner (skeleton today)
```

## Phase 1 first deliverable — what's implemented today

The kernel's foundation layer:

- `id` — newtype IDs + monotonic `IdGenerator`
- `revision` — `Revision` re-export
- `value` — `ScalarValue`, `CellDataType`, NaN/Inf rejection
- `error` — `EngineError` (full enum, all variants)
- `element` — `Element`, `MeasureMeta`, `MeasureRole`, `AggregationRule`,
  `VersionState`, `ScenarioMeta`
- `dimension` — `Dimension`, `DimensionKind`, `DimensionBuilder`
- `hierarchy` — `Hierarchy` + builder with cycle detection,
  duplicate-edge / multi-parent / NaN-weight rejection
- `coordinate` — `CellCoordinate`, `CellCoordinateBuilder`
- `cell` — `CellValue`, `Provenance`, `Uncertainty`, `StoredCell`
- `trace` — types only (no walk algorithm yet)
- `store` — `HashMapStore` (concrete; no trait — see brief §3.9)

Four pinned integration tests:

- [`tests/hierarchy_cycle.rs`](./crates/mc-core/tests/hierarchy_cycle.rs)
- [`tests/duplicate_elements.rs`](./crates/mc-core/tests/duplicate_elements.rs)
- [`tests/coordinate_validity.rs`](./crates/mc-core/tests/coordinate_validity.rs)
- [`tests/value_nan.rs`](./crates/mc-core/tests/value_nan.rs)

## Phase 1 — not yet built (out of scope for this deliverable)

`rule`, `dependency`, `dirty`, `consolidation`, `cube`, `slice`,
`permission`, `lock`, `snapshot` (skeleton modules), the full `Acme` fixture,
the demo CLI runner, the benchmark suite. Each lands per the recommended
implementation order in build-brief §15.

## Building

```bash
# Toolchain is pinned in rust-toolchain.toml (1.78).
# Install rustup first if you don't have it:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --workspace
cargo test --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT or Apache-2.0 (see workspace `Cargo.toml`).
