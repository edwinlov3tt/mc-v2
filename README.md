# Mosaic

> **Mosaic** — a Large Numbers Model: an n-dimensional planning engine where every cell of your business is computed, traceable, and tied to the inputs that move it.

What an LLM is to language, Mosaic is to the numbers that run a business: every cell predicted, every dependency tracked, every assumption auditable. See [`docs/strategy/POSITIONING.md`](./docs/strategy/POSITIONING.md) for the full positioning.

**Status:** Phase 6A.1 complete (review-driven fixes after the Phase 6A agent-ready CLI shipped). The kernel + model layer + Tessera ingestion engine + Claude Code plugin + Python LLM adapters + 14-verb CLI are all live. **731 / 0 / 5 tests passing, 10/10 deterministic.** Phase 6B (web UI) is the natural next phase.

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
├── mc-core/      # the kernel (single-threaded, sparse multidim store, rules, consolidation, dirty tracking, snapshots, WriteBatch)
├── mc-fixtures/  # the Acme demo cube + scaled fixtures (locked since Phase 1A)
├── mc-model/     # YAML model authoring + validation + lint + diagnostics + test fixtures + 30+ formula functions (3A → 3H)
├── mc-cli/       # `mc demo`, `mc model {validate,inspect,lint,test,query,whatif,trace,sweep,diff,write}`, `mc tessera {init,apply,recipe-init,list-imports,status,schedule,transform}`, `mc mcp`
├── mc-recipe/    # recipe schema + validator + MC5xxx codes (Phase 5A + 5C)
├── mc-drivers/   # SourceDriver trait + 11 source drivers (csv-local, csv-https, postgres, sqlite, http-json, duckdb, mysql, d1-rest, snowflake, bigquery)
└── mc-tessera/   # orchestrator: apply / dry-run / history / rollback / cron-schedule / time_format strptime subset
mosaic-plugin/    # Claude Code plugin (6 skills + 4 agents + 7 commands + .mcp.json with 12 MCP tools + Python adapters for Anthropic & OpenAI)
examples/         # Acme demo (workspace canonical) + sports-betting/nba-totals cartridge (14/14 goldens passing)
```

Crate names keep the `mc-` prefix per the naming-convention rule (see CLAUDE.md). Six placeholder crate names reserved on crates.io ahead of Phase 6C distribution: `mosaic-cli`, `mosaic-engine`, `mosaic-lnm`, `mosaic-core`, `mosaic-recipe`, `mosaic-tessera`.

---

## What's shipping today (post-Phase 6A.1)

| Phase | Tag | What it added |
|---|---|---|
| 1A | `4aa674a` | The kernel: dimensions, hierarchies, rules, consolidation, dirty tracking, snapshots, deterministic recompute. 6 dims, 11 measures, 5 rules in Acme demo. |
| 1B + 2A | `phase-2a-cold-path-baseline` | Benchmark baseline + cold-path bench expansion. |
| 2B | `phase-2b-consolidation-fast-path` | Removed the per-call hierarchy clone. 3-leaf cold consol: 14.3 µs → 2.53 µs. |
| 2C | `phase-2c-workload-baseline` | Production-shaped benchmarks at 10× / 50× / 100× Acme. Surfaced the `load_canonical_inputs` cliff. |
| 2D | `phase-2d-bitset-and-invalidated-fix` | Bitset DirtyTracker + `WritebackResult.invalidated` semantic correction. 50× ingest: 230.80 s → 1.06 s. |
| 3A | `phase-3a-model-definition-layer` | New `mc-model` crate: YAML → Cube via three-stage pipeline. Acme YAML + `mc demo --model` flag. |
| 3B | `phase-3b-lint-and-diagnostics` | `mc model {validate, inspect, lint, test}` + 10 lint rules + JSON diagnostic envelope for LLM/UI consumption. |
| 3C | `phase-3c-fixtures-and-inputs` | `canonical_inputs:` + `test_fixtures:` schema. Acme inputs moved to sibling CSV; the Acme-name special case removed from CLI. |
| 3D | `phase-3d-friendly-formula-syntax` | Rule bodies as formula strings (`Revenue = Customers * AOV`). Hand-rolled recursive-descent parser. Acme migrated. |
| 3E–3G | `phase-3e-3f-3g-formula-expansion` | Conditionals (`if/elif/else`), time-series ops (`lag/lead/cumsum/period_delta`), reference-data blocks (`lookup_table/segment_map`). Plus 3F.1 runtime time anchor. |
| 3H | `phase-3h-fitted-model-evaluation` | `predict()` / `calibrate()` / `exp()` / `norm_cdf()` — fitted statistical models inline in formulas. |
| 4A | `phase-4a-mosaic-plugin` | Mosaic Claude Code plugin: 6 skills + 4 agents + 6 commands + 5 MCP tools. LLM-assisted authoring with structured knowledge package. |
| 4B | `phase-4b-python-adapters` | Anthropic + OpenAI reference adapters; best-of-3 gate cleared 3/3 on both providers. |
| 5A | `phase-5a-tessera-core` | Tessera ingestion engine: `WriteBatch`, recipe format (`mc-recipe`), 6 source drivers (`mc-drivers`), orchestrator (`mc-tessera`), 5 CLI verbs. |
| 5B | `phase-5b-llm-recipe-authoring` | Plugin import skills + `mosaic-importer` agent + `--mode propose-recipe` on adapters. |
| 5C | `phase-5c-driver-expansion` | 5 new drivers (MySQL, D1 REST, Snowflake/BigQuery via ODBC) + cron scheduling + incremental loads + ADR-0014 `time_format` enforcement. |
| 6A | `phase-6a-agent-ready-cli` | 7 new agent-ready CLI verbs (`query`, `whatif`, `trace`, `sweep`, `diff`, `write`, `transform`) + 12 MCP tools + JSON envelope discipline + stable exit codes. The CLI is now a complete capability layer. |
| 6A.1 | `phase-6a-1-review-fixes` | Closes 11 findings from the post-6A code review including silent-correctness CRIT-1 (`predict()` standardization name-keyed at eval) and MAJ-1 (`time_format` actually wired into Tessera). NBA cartridge goldens went 4/14 → 14/14 as a side effect. |

**731 tests passing, 0 failed, 5 ignored (live external services), 10/10 deterministic.**

---

## Building

```bash
# Toolchain is pinned in rust-toolchain.toml (Rust 1.78).
cargo build --release --workspace
cargo test --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings

# Install the mc binary on your PATH:
cargo install --path crates/mc-cli --locked

# Run the Acme demo:
mc demo
mc demo --model crates/mc-model/examples/acme.yaml   # YAML path (byte-identical)

# Author / validate / lint / test a model:
mc model validate crates/mc-model/examples/acme.yaml
mc model inspect  crates/mc-model/examples/acme.yaml
mc model lint     crates/mc-model/examples/acme.yaml
mc model test     crates/mc-model/examples/acme.yaml

# Phase 6A agent-ready verbs (--format json|csv|text on every one):
mc model query    crates/mc-model/examples/acme.yaml --where 'Measure=Revenue' --format json
mc model whatif   crates/mc-model/examples/acme.yaml --override 'Spend[Q1]=10000'
mc model trace    crates/mc-model/examples/acme.yaml 'Revenue[Q1]'
mc model sweep    crates/mc-model/examples/acme.yaml --coefficient X --range 0..100
mc model diff     <(mc demo) <(mc demo --model ...)
mc model write    crates/mc-model/examples/acme.yaml --coord 'Spend[Q1]' --value 12345 --dry-run

# Tessera ingestion + scheduling:
mc tessera init        # scaffold .tessera/ alongside a model
mc tessera apply       # run a recipe against external data
mc tessera schedule    # cron daemon for scheduled imports
mc tessera transform   # one-shot ETL with column-mapping recipes

# JSON-RPC server for AI agents (e.g. Claude Code via the Mosaic plugin):
mc mcp
```

---

## License

MIT or Apache-2.0 (see workspace `Cargo.toml`).
