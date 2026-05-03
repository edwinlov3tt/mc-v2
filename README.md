# Mosaic

> **Mosaic** — a Large Numbers Model: an n-dimensional planning engine where every cell of your business is computed, traceable, and tied to the inputs that move it.

What an LLM is to language, Mosaic is to the numbers that run a business: every cell predicted, every dependency tracked, every assumption auditable. See [`docs/strategy/POSITIONING.md`](./docs/strategy/POSITIONING.md) for the full positioning.

**Status:** Phase 3D complete (formula-syntax authoring layer over the YAML model schema). Kernel + model layer + diagnostics + test fixtures + friendly formulas all shipped. Phase 4 (LLM-assisted authoring) is next.

> **Naming convention:** the project was renamed from "MarketingCubes V2" → "Mosaic" on 2026-05-03. The `mc-` crate prefix and `MC` diagnostic-code prefix stay (they're now backronyms — "Mosaic Core" / "Mosaic Code"). See [`CLAUDE.md`](./CLAUDE.md) for the binding naming-convention rule. Historical docs (ADRs, past completion reports, original specs) keep their original "MarketingCubes" naming for audit-trail integrity.

---

## Documentation entry points

Read these in order on a fresh session:

1. [`CLAUDE.md`](./CLAUDE.md) — operating manual (read first; binding for any code change)
2. [`docs/HANDOFF.md`](./docs/HANDOFF.md) — 5-minute orientation
3. [`docs/CURRENT_STATE.md`](./docs/CURRENT_STATE.md) — what's live right now
4. [`docs/roadmap/MASTER_PHASE_PLAN.md`](./docs/roadmap/MASTER_PHASE_PLAN.md) — what's been built and what's next
5. [`docs/strategy/POSITIONING.md`](./docs/strategy/POSITIONING.md) — Mosaic as an LNM platform; TM1 scope comparison
6. [`docs/process-notes.md`](./docs/process-notes.md) — operational rules (handoff-first vs ADR-first flow, etc.)

For plain-English explanations of what each phase did:

- [`docs/for-dummies/phases/`](./docs/for-dummies/phases/) — analogy-driven walkthroughs of phases 2C onward.

The two **kernel contractual specs** (locked since Phase 1A — these retain "MarketingCubes" naming):

- [`docs/specs/engine-semantics.md`](./docs/specs/engine-semantics.md) — what the kernel *means* (invariants, semantics).
- [`docs/specs/phase-1-rust-kernel-build-brief.md`](./docs/specs/phase-1-rust-kernel-build-brief.md) — what was built in Phase 1.

---

## Workspace layout

```
crates/
├── mc-core/      # the kernel (single-threaded, sparse multidim store, rules, consolidation, dirty tracking, snapshots)
├── mc-fixtures/  # the Acme demo cube (canonical Rust-side reference)
├── mc-model/     # YAML model authoring + validation + lint + diagnostics + test fixtures + formula syntax
└── mc-cli/       # `mc demo` + `mc model {validate,inspect,lint,test}` runner
```

Crate names keep the `mc-` prefix per the naming-convention rule (see CLAUDE.md).

---

## What's shipping today (post-Phase 3D)

| Phase | Tag | What it added |
|---|---|---|
| 1A | `4aa674a` | The kernel: dimensions, hierarchies, rules, consolidation, dirty tracking, snapshots, deterministic recompute. 6 dims, 11 measures, 5 rules in Acme demo. |
| 1B + 2A | `phase-2a-cold-path-baseline` | Benchmark baseline + cold-path bench expansion. |
| 2B | `phase-2b-consolidation-fast-path` | Removed the per-call hierarchy clone. 3-leaf cold consol: 14.3 µs → 2.53 µs. |
| 2C | `phase-2c-workload-baseline` | Production-shaped benchmarks at 10× / 50× / 100× Acme. Surfaced the `load_canonical_inputs` cliff. |
| 2D | `phase-2d-bitset-and-invalidated-fix` | Bitset DirtyTracker + WritebackResult.invalidated semantic correction. 50× ingest: 230.80 s → 1.06 s. |
| 3A | `phase-3a-model-definition-layer` | New `mc-model` crate: YAML → Cube via three-stage pipeline. Acme YAML + `mc demo --model` flag. |
| 3B | `phase-3b-lint-and-diagnostics` | `mc model {validate, inspect, lint, test}` + 10 lint rules + JSON diagnostic envelope for LLM/UI consumption. |
| 3C | `phase-3c-fixtures-and-inputs` | `canonical_inputs:` + `test_fixtures:` schema. Acme inputs moved to sibling CSV; the Acme-name special case removed from CLI. |
| 3D | `phase-3d-friendly-formula-syntax` | Rule bodies as formula strings (`Revenue = Customers * AOV`). Hand-rolled recursive-descent parser. Acme migrated. |

396 tests passing, 10/10 deterministic.

---

## Building

```bash
# Toolchain is pinned in rust-toolchain.toml (Rust 1.78).
cargo build --release --workspace
cargo test --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings

# Run the Acme demo (Rust path):
cargo run --release --bin mc -- demo

# Run the Acme demo via the YAML model path (byte-for-byte identical output):
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml

# Validate / inspect / lint / test the YAML model:
cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model inspect  crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model lint     crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model test     crates/mc-model/examples/acme.yaml
```

---

## License

MIT or Apache-2.0 (see workspace `Cargo.toml`).
