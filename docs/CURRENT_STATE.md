# CURRENT_STATE

> **What's live right now.** Update whenever a phase ships, a gate flips, or a deferral closes.

**Last updated:** 2026-05-06 — **PHASE 3 COMPLETE.** Phase 3H.2 shipped (fitted-model adstock + saturation transforms — the final formula-engine phase). Adds geometric adstock with required `max_lookback` + Hill + Log saturation forms native to `fitted_models.transforms:`; cross-coord backward-scan in `predict()` (inherits dep-graph debt per Amendment §11 cumulative tracking obligation); Decision 3 ships the only Phase 3 exception to Mosaic's Null-propagation discipline (Null-as-zero in adstock, deliberately matching MMM convention, documented loudly at the eval site). Combined transform pipeline per Decision 7 binding order: feature → adstock → saturation → standardization → coefficient → sum + intercept → link → output_bound. 6 active diagnostic codes (MC2071-MC2076) + MC2077 reserved (serde catches unknown saturation types). Tag `phase-3h-2-fitted-model-adstock-saturation` at `d240802`. Self-audit caught a Hard Rule 7 violation (`SaturationSpec::feature_name` shipped as `pub fn` instead of `pub(crate)`; demoted mid-audit) and surfaced 2 coverage gaps (filled). M-14 closure is **aspirational, not committed** — 3H.2 ships the CAPABILITY for general MMM authors, but the existing Tide MMM cartridge has not been migrated (separate work at the cartridge maintainer's discretion). **The formula-engine deferred queue from ADR-0015 is now EMPTY.** Project transitions to "demand-driven only" for formula work — future formula or fitted-model additions require a real customer use case. Phase 3 arc spans 11 sub-phases (3A → 3J + 3H.1 + 3H.2). **912 / 0 / 5 tests** (880 → 912, +32 in 3H.2). Next-phase candidates: 4C (multi-domain workspaces), 5D (Tessera xlsx), 6B (UI), 6C (distribution). A Phase 3 retrospective document is the next deliverable per Claude Desktop's recommendation.

Earlier today — Phase 3H.1 shipped (fitted-model `output_bound`). Small additive amendment to ADR-0015's Phase 3H: `output_bound: { min, max }` field on `ParsedFittedModel`; `predict()` clamps output after the link function. Closes audit M-20 (Amarillo -$5,706 case from Tide MMM). 1 new code (MC2070). Tag `phase-3h-1-fitted-model-output-bound` at `de119dd`. **One phase remains in the formula-engine deferred queue: Phase 3H.2** (adstock + saturation transforms native to `fitted_models:`, ADR-0018 pending). After 3H.2 ships, the entire ADR-0015 deferred queue is empty and Phase 3 is genuinely complete. **880 / 0 / 5 tests** (874 → 880, +6 in 3H.1).

Earlier today — Phase 3J shipped (formula authoring deferred items). Closes 7 items from the ADR-0015 deferred queue across 4 clusters: `ScalarValue::Str` first-class in eval (transient-only, never stored — the load-bearing kernel boundary; Amendment §13 documents the audit-discovered Str-leakage bug fix at `Cube::read_derived_leaf`); `current_element(Dim)` function; `parameters:` block (constants only v1, partially closes M-14 per Amendment §2); `Indicator` measure role (compiles to same `Expr::IsElement` AST as `is_element` per Amendment §6); `Scope` enum extension (`FutureLeaves`/`PastLeaves`/`CurrentLeaves` requiring `time_anchor` per Amendment §4); `scenario_ref` + `actual_ref(measure, fallback)` (cross-coord nesting prohibition relaxed for fallback only per Amendment §3); `extrapolate_last_value` + LOCF (with `allow_past_extrapolation` override per Amendment §11). 16 new diagnostic codes: MC1026-1029, MC2058-2069. Tag `phase-3j-formula-deferred-items` at `4a4ac9c`. **After Phase 3H.1 ships (separate, ADR-0017 pending — `output_bound` + adstock/saturation), the formula-engine deferred queue from ADR-0015 is empty.** Phase 3 transitions to "demand-driven only." **874 / 0 / 5 tests** (830 → 874, +44 regression tests in 3J including 1 audit-discovered Section L regression test). 7 crates in workspace. Toolchain stays Rust 1.78. **6 placeholder crate names reserved on crates.io** (`mosaic-cli`, `mosaic-engine`, `mosaic-lnm`, `mosaic-core`, `mosaic-recipe`, `mosaic-tessera`).

**Selected commit / tag index:**
- Phase 1A kernel: `4aa674a` / Phase 1B+2A: `48d52e9` (tag `phase-2a-cold-path-baseline`) / Phase 2B: `6ea58ab` (tag `phase-2b-consolidation-fast-path`) / Phase 2C: `789db15` (tag `phase-2c-workload-baseline`) / Phase 2D: `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`)
- Phase 3A: `603c537` (tag `phase-3a-model-definition-layer`) / Phase 3B: `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`) / Phase 3C: `8d2691a` (tag `phase-3c-fixtures-and-inputs`) / Phase 3D: `d5ab355` (tag `phase-3d-friendly-formula-syntax`)
- Phase 3E–3G: `78a2193` (tag `phase-3e-3f-3g-formula-expansion`) / Phase 3F.1: `8adbe2b` / Phase 3H: `99477ef` (tag `phase-3h-fitted-model-evaluation`)
- Phase 4A: `36af56c` (tag `phase-4a-mosaic-plugin`) / Phase 4B: `b5b6229` (tag `phase-4b-python-adapters`)
- Phase 5A: `2f20d24` (tag `phase-5a-tessera-core`) / Phase 5B: `2f20d24` (tag `phase-5b-llm-recipe-authoring`) / Phase 5C: `0790bce` (tag `phase-5c-driver-expansion`)
- Phase 6A: `e696379` (tag `phase-6a-agent-ready-cli`) / Phase 6A.1: `44a7437` (tag `phase-6a-1-review-fixes`) / Phase 6A.2: `7888f20` (tag `phase-6a-2-correctness-patch`) / Phase 6A.3: `46b1f7a` (tag `phase-6a-3-agent-surface-polish`)
- Phase 3I: `1265f78` (tag `phase-3i-formula-language-completion`) / Phase 3J: `4a4ac9c` (tag `phase-3j-formula-deferred-items`) / Phase 3H.1: `de119dd` (tag `phase-3h-1-fitted-model-output-bound`) / Phase 3H.2: `d240802` (tag `phase-3h-2-fitted-model-adstock-saturation`)

**Branch:** `main` at HEAD `d240802` (tracking `origin/main` at github.com/edwinlov3tt/mc-v2)

---

## What's shipping

- **Phase 1A — Rust kernel for the Acme demo.** Complete. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).
- **Phase 1B — Benchmark Baseline + PERF.md.** Complete 2026-05-01. Acceptance criterion 5 closed via Cargo.lock transitive pins (no toolchain bump). See [`PERF.md`](./PERF.md).
- **Phase 2A — Cold-Path Benchmark Expansion.** Complete 2026-05-01. Both Phase 1B measurement gaps closed: cold consolidation rows added against §11.2 ceilings (PERF.md §6.7); synthetic no-deps write fixture added against §11.1 50 µs ceiling (PERF.md §6.8). Two new diagnostic suites (snapshot clone PERF.md §6.9; hierarchy ancestor mark microbench PERF.md §6.10). **No `crates/mc-core/src/` files modified.** See [`reports/phase-2a-completion-report.md`](./reports/phase-2a-completion-report.md).
- **Phase 2B — Consolidation Fast Path.** Complete 2026-05-01, committed at `6ea58ab` (tag `phase-2b-consolidation-fast-path`). One targeted kernel change in [`cube.rs::read_consolidated`](../crates/mc-core/src/cube.rs) plus a `Vec<Arc<Hierarchy>>` shape change in [`dimension.rs`](../crates/mc-core/src/dimension.rs); replaces per-call `Vec<Dimension>` + `Vec<Hierarchy>` deep-clones with one `Arc::clone` + a `Vec<Arc<Hierarchy>>` collect (refcount-bumps). PERF.md §6.7 3-leaf cold consol drops 14.3 µs → **2.53 µs** (clears brief §11.2 1B target ≤ 3 µs); every other §6.7 row improves by ~12 µs absolute. New kernel unit test `consecutive_recompute_reads_match_phase_2b` (handoff item 3). One contract test rewritten (`t_consolidation_caches_value_within_revision`, semantic-not-timing) per ADR-0002 + the SPEC QUESTION round-trip approval. See [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md) and [`PERF.md`](./PERF.md) §6.11 + §9.4 + §10.
- **Phase 2C — Production-Shaped Workload Benchmarks.** Complete 2026-05-02, committed at `789db15` (tag `phase-2c-workload-baseline`). Measurement-only phase; **no `crates/mc-core/src/` change.** Adds internal `mc_fixtures::build_scaled_acme_cube(scale)` (`pub(crate)`) + three public wrappers `_10x` / `_50x` / `_100x` + 6 unit tests including the mandatory scale-1× equivalence test against brief §4.5.1 anchor goldens. Adds 27 new bench rows extending the existing five Phase 1B/2A bench files at 10× / 50× / 100×. Adds new [`combined_workflow.rs`](../crates/mc-core/benches/combined_workflow.rs) that simulates a 100-iteration planner session at 50× (100× attempted then abandoned) with stacked-snapshot hold (TM1 sandbox pattern per ADR-0003 Decision 6). PERF.md §6.12 / §6.13 / §6.14 written from the gate run. Headline finding: `load_canonical_inputs` super-linear cliff between 10× (4.33×/write) and 50× (19.7×/write) — points at §9.3 as the Phase 2D candidate. **Did not pick a Phase 2D winner** in §9; the pick is in [`handoffs/phase-2d-handoff.md`](./handoffs/phase-2d-handoff.md). See [`reports/phase-2c-completion-report.md`](./reports/phase-2c-completion-report.md).
- **Phase 3A — Model Definition Layer (`mc-model` crate).** **Complete 2026-05-02, committed at `603c537` (tag `phase-3a-model-definition-layer`).** Ships a new `crates/mc-model/` crate that translates a human-authored YAML cube definition into an `mc_core::Cube` via the three-stage pipeline per [ADR-0004](./decisions/0004-phase-3a-model-definition-format.md) Decision 9: YAML bytes → `ParsedModel` → `ValidatedModel` → `Cube`. Each stage has its own error type (`ParseError` / `ValidationError` / `EngineError`) so blame is unambiguous (Phase 4's LLM-feedback loop and Phase 6's UI editor consume these). The Acme cube is re-expressed as [`crates/mc-model/examples/acme.yaml`](../crates/mc-model/examples/acme.yaml) (264 lines, 9 inline goldens covering brief §4.5.1 anchor values + 1 consolidation rollup). `mc-cli` gains a `--model <path>` flag that routes through `mc_model::load`. **Acceptance gate cleared:** `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` produces empty output (byte-for-byte stdout equality between Rust and YAML paths). 14 validator negative tests cover ADR-0004 Decision 6's 10-row table (one extra split for the 10th row's structural vs value sides). Structural-equivalence test diffs YAML-loaded Acme against `build_acme_cube()` on dim count, element names, hierarchy edges, measure metadata, weight-measure targets, and rule body shapes. **`mc-core` not modified** — same 4 runtime deps as Phase 2D. **`mc-fixtures` not modified** — `build_acme_cube()` byte-for-byte unchanged. **Toolchain stayed at Rust 1.78** — `serde_yaml 0.9.34`'s transitive `indexmap 2.14.0` pinned to `2.7.0` per [ADR-0004](./decisions/0004-phase-3a-model-definition-format.md) Decision 3 escape hatch (Phase 1B precedent); ADR-0005 was *not* opened. See [`reports/phase-3a-completion-report.md`](./reports/phase-3a-completion-report.md).
- **Phase 3B — Model QA, Linter, and Diagnostics.** **Complete 2026-05-03, committed at `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`).** Adds a read-only quality + diagnostics layer over `mc-model`: four CLI subcommands (`mc model validate / inspect / lint / test`); 10 starting lint rules (MC3001–MC3007 + MC3009–MC3011 — MC3008 permanently retired and promoted to MC2011 in validation); structured `Diagnostic { code, severity, path: ModelPath, message, suggestion }` shape with stable `&'static str` codes; JSON envelope `{ "schema_version": "1.0", "diagnostics": [...] }` with deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)` for Phase 4 LLM consumption + Phase 6 UI consumption. **Headline gate cleared:** `mc model lint crates/mc-model/examples/acme.yaml` exits 0 with **zero warnings** (no documented exceptions per ADR-0005 amendment #15). 22 `description:` fields added to Acme YAML (6 dim + 11 measure + 5 rule); structural-equivalence + demo-equivalence diff still empty. `mc-core` and `mc-fixtures` untouched (`git diff phase-3a-model-definition-layer` returns 0 lines for both). Toolchain stayed at Rust 1.78 — JSON serialization hand-rolled (no `serde_json` dep). 41 new tests across 5 new test files + 18 new snapshot fixtures. See [`reports/phase-3b-completion-report.md`](./reports/phase-3b-completion-report.md).
- **Phase 3C — Model Test Fixtures and Input Sets.** **Complete 2026-05-03, committed at `8d2691a` (tag `phase-3c-fixtures-and-inputs`).** Closes the visible scaffolding hack from Phase 3B deviation 4.3 — the `mc model test` Acme-name special case in `mc-cli/src/main.rs:253` is **REMOVED** (`grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` → 0). Adds model-owned `canonical_inputs:` and `test_fixtures:` schema; hand-rolled strict CSV parser (no `csv` crate); 14 new fixture validators (MC2012–MC2025); `mc model test --fixture <name>` filter flag (filter-only semantic). **Architecture clarification (project-owner-pinned):** `validate()` stays filesystem-free; new named stage `resolve_inputs(&ValidatedModel, Option<&Path>)` reads CSVs and emits MC2012–MC2025; `mc_model::load(path)` runs all four stages (parse → validate → resolve_inputs → compile) but does NOT apply inputs to the cube; `mc model test` is the only consumer of `apply_canonical_inputs` / `apply_fixture`. **Headline gates:** `tests/equivalence_acme.rs` proves YAML+CSV path produces byte-identical store state to the Rust path on Acme across all 2,520 canonical input coords (`f64::to_bits()` equality) AND all 9 inline goldens (within 1e-9). Equivalence test uses ONLY existing public APIs from `mc-core` + `mc-fixtures` — no helper added to `mc-fixtures` (Decision 5's "default 'enumerate inline'" honored). **Perf gate cleared:** `mc model test acme.yaml` runs in **32 ms** wall-clock (under both 500 ms gate and 200 ms stretch from amendment #17). All 17 ADR-0006 Decision 9 success-gate items closed. `mc-core` and `mc-fixtures` untouched (`git diff phase-3b-lint-and-diagnostics` returns 0 lines for both). Toolchain stayed at Rust 1.78. 35 new tests across 5 new test files + 14 negative fixtures + 2 sibling CSVs. JSON envelope `schema_version` stays `"1.0"` (Diagnostic struct shape unchanged; `tests/schema_stability.rs` enforces). See [`reports/phase-3c-completion-report.md`](./reports/phase-3c-completion-report.md).
- **Phase 2D — Bitset-Backed Dirty Tracker + WritebackResult.invalidated semantic correction.** Complete 2026-05-02, committed at `0678a98` (tag `phase-2d-bitset-and-invalidated-fix`). Acceptance gate cleared by ~47×: `load_canonical_inputs/50x` drops from 230.80 s → **1.06 s (−99.5 %)**; 100× ingest (abandoned at >38 min in phase-2c) now runs in **2.13 s**. Two changes shipped per [Phase 2D handoff §A](./handoffs/phase-2d-handoff.md): (1) `DirtyTracker` internal repr replaced with a Cartesian-product flat bitset behind `Arc<CubeShape>` (foundation), and (2) `WritebackResult.invalidated` semantic correction in `cube.rs::write` from cumulative-dirty (Phase 1A reading of brief line-1938 pseudocode shorthand) to marginal-per-write (brief type-doc + engine-semantics.md §13 + I-WB-7 reading). A/B isolation confirmed the writeback semantic correction is the load-bearing change for the §6.14 cliff; the bitset is enabling (makes `is_dirty` O(1) so the marginal capture is bounded by per-write fan-out, not cumulative set size) but moves the cliff by < 0.2 % in isolation. New test file [`tests/writeback_invalidated.rs`](../crates/mc-core/tests/writeback_invalidated.rs) with five tests pinning the marginal semantics. Public API surface unchanged; the brief's `WritebackResult.invalidated: Vec<CellCoordinate>` field name + type + re-export are byte-for-byte identical — only the *contents* differ per the spec audit in [PERF.md §6.15](./PERF.md). See [`reports/phase-2d-completion-report.md`](./reports/phase-2d-completion-report.md).
- **Phase 3E–3G — Formula Language Expansion.** Complete, tag `phase-3e-3f-3g-formula-expansion` at `78a2193`. Conditionals (`if/elif/else`), time-series ops (`lag/lead/cumsum/period_delta`), reference-data blocks (`lookup_table/segment_map`). ADRs 0011–0013 Accepted. Phase 3F.1 (runtime time anchor + time metadata validation) bundled in the same merge.
- **Phase 3H — Fitted-Model Evaluation.** Complete, tag `phase-3h-fitted-model-evaluation` at `99477ef`. `predict()`, `calibrate()`, `exp()`, `norm_cdf()` formula functions. Closes the model-backed-cell capability gap for deterministic statistical formulas (regression coefficients, calibration curves) inline in formulas.
- **Phase 4A — Mosaic Claude Code plugin.** Complete, tag `phase-4a-mosaic-plugin` at `36af56c`. `mosaic-plugin/` with 6 skills, 4 agents, 6 commands, `.mcp.json` (5 initial MCP tools); `mc mcp` JSON-RPC subcommand. See [`reports/phase-4a-completion-report.md`](./reports/phase-4a-completion-report.md).
- **Phase 4B — Python Reference Adapters.** Complete, tag `phase-4b-python-adapters` at `b5b6229`. Anthropic + OpenAI adapters under `mosaic-plugin/examples/adapters/`; best-of-3 gate cleared 3/3 on both providers. See [`reports/phase-4b-completion-report.md`](./reports/phase-4b-completion-report.md).
- **Phase 5A — Tessera Core Engine.** Complete, tag `phase-5a-tessera-core` at `2f20d24`. New `mc-recipe`, `mc-drivers` (6 source drivers: csv-local, csv-https, postgres, sqlite, http-json, duckdb), `mc-tessera` orchestrator, `WriteBatch` API on `mc-core`. 5 CLI verbs (`init`, `apply`, `recipe-init`, `list-imports`, `status`).
- **Phase 5B — LLM-Assisted Recipe Authoring.** Complete, tag `phase-5b-llm-recipe-authoring` at `2f20d24`. 3 new plugin import skills (csv / sql / api), `mosaic-importer` agent, `/mosaic-import` command, `--mode propose-recipe` on both Phase 4B adapters. Best-of-3 gate Anthropic 3/3 ✓ + OpenAI 3/3 ✓. See [`reports/phase-5b-completion-report.md`](./reports/phase-5b-completion-report.md).
- **Phase 5C — Driver Expansion + Cron + Incremental.** Complete, tag `phase-5c-driver-expansion` at `0790bce`. 5 additional Tessera drivers (MySQL native, D1 REST, Snowflake/BigQuery via ODBC, expanded HTTP-JSON), `mc tessera schedule` cron daemon, incremental load support (watermark sidecars), ADR-0014 `time_format` enforcement at recipe-validate time.
- **Phase 6A — Agent-Ready CLI.** Complete 2026-05-05, tag `phase-6a-agent-ready-cli` at `e696379`. 7 new CLI verbs (`mc model {query, whatif, trace, sweep, diff, write}` + `mc tessera transform`); 12 MCP tools (5 original + 7 new); JSON envelope discipline (`schema_version: "1.0"` on all responses); stable exit codes (0 success / 1 model error / 2 CLI usage / 3 I/O); `--dry-run` on state-changing verbs; idempotence on read-only verbs. **The CLI is now a complete capability layer** — Phase 6B (UI) renders this data; doesn't add capability.
- **Phase 6A.1 — Review-driven fixes.** Complete 2026-05-06, tag `phase-6a-1-review-fixes` at `44a7437`. Closes all 11 findings from the post-6A Sonnet code review: **CRIT-1** (`predict()` standardization now name-keyed at eval — `FittedModelData.coefficients: Vec<(String, f64)>`, `standardization: Option<Vec<(String, f64, f64)>>`); **MAJ-1** (`time_format` strptime subset wired into `mc-tessera::time_format` + transform.rs paths; non-ISO date columns now parse and match Time-dim elements correctly); **CRIT-2** (`schema_version: "1.0"` added to all Phase 6A verb JSON envelopes via shared `push_json_envelope_header` helper); **CRIT-3** (`LoadModelError { Io, Model }` enum; I/O failures exit 3, parse/validate failures exit 1); **MIN-5** (Phase 6A MCP tools return parsed `structured` JSON field); **MAJ-2** (`ScheduleRegistry::save` is atomic via tmp+rename); **MIN-6** (epsilon `1e-9` swap in `not()`/`if()` falsy check applied in BOTH eval paths after self-audit caught the misapplication); **MIN-1/4** (suppression cleanup + CLAUDE.md `unsafe` exception note). NBA totals cartridge goldens went 4/14 → 14/14 as a side effect of CRIT-1's eval-site refactor. See [`reports/phase-6a-1-completion-report.md`](./reports/phase-6a-1-completion-report.md) and [`research-notes/cross-coord-dep-graph.md`](./research-notes/cross-coord-dep-graph.md) for the deferred MAJ-3 architectural debt.

## What's queued

- **Phase 6B — Web UI / planning grid.** Not started. The natural next phase. Phase 6A made the CLI a complete capability layer; 6B renders the same data visually with drill-down, edit, snapshot/rollback, and version comparison.
- **Phase 6A.2 — single-compile sweep + transform polish (P1 known debt).** Filed in [`reports/phase-6a-1-completion-report.md`](./reports/phase-6a-1-completion-report.md) §"Known Debt": (a) `mc model sweep` reads the YAML 2N times for N parameter points (P1); (b) `mc tessera transform` uses curl subprocess for HTTP fetches — should switch to `ureq` (P1); (c) write-log replay not yet wired into `load_model` (P0 — `mc model write` is silently ignored by subsequent reads; documented in process-notes Rule 9).
- **Phase 3I — Formula-parser unification + string literals.** Filed in [`research-notes/formula-language-expansion.md`](./research-notes/formula-language-expansion.md) §7I.8. Phase 6A's `--where` filter parser is currently a separate hand-rolled implementation (the formula parser doesn't support string literals yet). 3I unifies them.
- **Phase 6C — Distribution & install pipeline (TBD).** Not yet ADR'd. Anchor: `cargo-dist` cross-compile matrix → GitHub Releases + Homebrew tap + `curl | sh` installer + `mosaic update` self-update verb. 6 placeholder crate names already reserved on crates.io (`mosaic-cli`, `mosaic-engine`, `mosaic-lnm`, `mosaic-core`, `mosaic-recipe`, `mosaic-tessera`).
- **Phase 2 housekeeping — Q1/Q3 (workload sketch ADR + criterion baselines).** Both closed 2026-05-01–02. ADR-0003 Accepted–Provisional (sunset 2026-11-01). Baselines through `phase-2d` saved under [`reports/bench-data/`](./reports/bench-data/).
- **Phase 2 housekeeping — Q2 (toolchain bump).** Deferred; Phase 3A solved with Cargo.lock pins instead. Currently no driver of a 1.85+ bump.

## Active ADRs

- [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) — Phase 1 scope: smallest kernel that runs the Acme demo. **Status:** Accepted.
- [`decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](./decisions/0002-perf-assertions-in-benchmarks-not-tests.md) — Performance assertions belong in criterion benchmarks, not in `cargo test`. **Status:** Accepted (Phase 2B). Authorizes the `t_consolidation_caches_value_within_revision` rewrite from a wall-clock ratio to semantic cache-state assertions.
- [`decisions/0003-workload-sketch.md`](./decisions/0003-workload-sketch.md) — Workload sketch & perception thresholds (Phase 2 housekeeping Q1). **Status:** Accepted — Provisional. Sunset clause: auto-flips to "Needs revision" on first real planner usage data, or 2026-11-01, whichever comes first. Defines the workload curve (10× / 50× / 100× Acme) and 100 ms click-instant threshold that Phase 2C calibrates against.
- [`decisions/0004-phase-3a-model-definition-format.md`](./decisions/0004-phase-3a-model-definition-format.md) — Phase 3A model-definition format & parser scope. **Status:** Accepted (with project-owner acceptance amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: YAML safe subset; `mc-model` crate; no parser deps in `mc-core`; structured expression trees for rules (formula strings deferred to Phase 3C); one cube per file; exhaustive blocking validation; inline golden tests; LLM authoring is Phase 4 not 3A; mandatory three-stage `YAML → ParsedModel → ValidatedModel → Cube` pipeline. Phase 3A handoff at [`handoffs/phase-3a-handoff.md`](./handoffs/phase-3a-handoff.md).
- [`decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](./decisions/0005-phase-3b-model-qa-linter-diagnostics.md) — Phase 3B Model QA, Linter, and Diagnostics. **Status:** Accepted (with 15 project-owner acceptance amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: four error layers (parse < validation < golden < lint, with golden exclusively `mc model test`'s responsibility, not `mc demo`); lint advisory by default with `mc_model::load()` ignoring lint output unconditionally; four CLI subcommands (`mc model validate / inspect / lint / test` plus `--format text|json`); `inspect` summary covering 11 fields; 10 starting lint rules (MC3001–MC3007 + MC3009–MC3011, MC3008 permanently retired and promoted to MC2011 in validation); strict out-of-scope (no formula strings, LLM, UI, actuals, mc-core changes, auto-fix); JSON envelope `{schema_version: "1.0", diagnostics: [...]}` with deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)`; 15-item success gate including Acme lints clean with zero documented warnings; no Rust toolchain bump (hand-rolled snapshot fixtures preferred over `insta`). Phase 3B handoff at [`handoffs/phase-3b-handoff.md`](./handoffs/phase-3b-handoff.md).
- [`decisions/0006-phase-3c-model-test-fixtures.md`](./decisions/0006-phase-3c-model-test-fixtures.md) — Phase 3C Model Test Fixtures and Input Sets. **Status:** Accepted (with 13 project-owner acceptance amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: two data forms (tabular inline YAML + sibling CSV; per-row inline dropped); strict CSV subset (UTF-8, required header, comma-separated, no quotes/embedded commas/embedded newlines/comments) hand-rolled with no `csv` crate dep; test fixtures (Phase 3C) ≠ actuals import (Phase 5); golden tests reference fixtures by name; two distinct concepts (`canonical_inputs` always-load + `test_fixtures` named/multiple) with snapshot/rollback for between-goldens reset (perf gate `mc model test acme.yaml < 500 ms`); Acme migrates to `acme.inputs.csv` and the `metadata.name` Acme special case in `mc-cli` is removed (mandatory); 14 new validators (MC2012–MC2025) including "unknown dimension KEY" (MC2012, narrowed) vs "unknown element VALUE" (MC2013, separate) and "duplicate input coordinate within input set" (MC2025, repurposed pre-acceptance); `mc model test --fixture <name>` filter flag (filter-only semantic; `--inputs` deferred to Phase 5); JSON `schema_version` stays at `"1.0"` (adding codes is backwards-compatible; only repurposing or new fields requires a bump); 17-item success gate including byte-identical equivalence between Rust and YAML+CSV paths using ONLY existing public APIs; no Rust toolchain bump. **Roadmap impact:** Phase 3C redefined from formulas to fixtures; friendly-formula syntax becomes Phase 3D. Phase 3C handoff at [`handoffs/phase-3c-handoff.md`](./handoffs/phase-3c-handoff.md).
- [`decisions/0007-phase-3d-friendly-formula-syntax.md`](./decisions/0007-phase-3d-friendly-formula-syntax.md) — Phase 3D Friendly Formula Syntax. **Status:** Accepted (with 16 acceptance amendments + 3 implementer-side amendments — see ADR §"Acceptance amendments" for the audit trail). Fixes 9 decisions: operator scope exactly the existing 7-variant `ParsedRuleBody` AST (`+ - * /` + parens + unary + `if_null(a, b)`; no other functions/comparisons/strings); diagnostic codes MC1003–MC1006 in MC1xxx parse-time namespace (MC1004 covers unknown function calls per amendment #25; MC1007 NOT introduced); schema shape `ParsedRuleBodyForm { Formula, Structured }` in `ParsedModel` ONLY, `ValidatedModel.body` flattens to `ParsedRuleBody` (amendment #23); round-trip serialization with explicit paren rules including `Mul([a, Div([b, c])])` → `"a * (b / c)"` (amendment #27 from GPT execution note #3); unary minus pre-picked as `Sub([Const(F64(0.0)), x])` (amendment #22); Acme migrates to formula form, structured fixtures stay structured (backwards-compat); inspect rendering uniform formula form regardless of authoring (amendment #24); `validate()` return type widened to `Vec<Error>` per amendment #28 + GPT execution note #1; `Diagnostic` shape unchanged + `schema_version` stays `"1.0"`. **Process note:** first ADR drafted under the new "handoff-first parallel flow" — see [`process-notes.md`](./process-notes.md) §1 for when to use which flow. Phase 3D handoff at [`handoffs/phase-3d-handoff.md`](./handoffs/phase-3d-handoff.md).
- [`decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md`](./decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) — Phase 4 LLM-Assisted Authoring + Mosaic Plugin Ecosystem. **Status:** Accepted (9 amendments A–I). Dropped `mc-author` crate; Phase 4B is Python adapters. Marketing-mix is the only domain schema in Phase 4A.
- [`decisions/0009-lnm-substrate-as-product-vision.md`](./decisions/0009-lnm-substrate-as-product-vision.md) — LNM substrate as product vision (post-rename strategic anchor). **Status:** Accepted.
- [`decisions/0010-phase-5-tessera-architecture.md`](./decisions/0010-phase-5-tessera-architecture.md) — Phase 5 Tessera Architecture (recipe format, source drivers, orchestrator, sidecar audit log, idempotency). **Status:** Accepted (with two amendments: `0010-amendment-1-stream-c-pin-corrections.md` for DuckDB transitive pins on Rust 1.78; `0010-amendment-2-long-format-recipe-support.md` for the `format: long` schema extension).
- [`decisions/0011-phase-3e-conditionals-and-basic-operations.md`](./decisions/0011-phase-3e-conditionals-and-basic-operations.md) — Phase 3E conditionals and basic operations in the formula engine. **Status:** Accepted.
- [`decisions/0012-phase-3f-time-series-operations.md`](./decisions/0012-phase-3f-time-series-operations.md) — Phase 3F time-series operations (`lag`, `lead`, `cumsum`, `period_delta`). **Status:** Accepted.
- [`decisions/0013-phase-3g-reference-data-blocks.md`](./decisions/0013-phase-3g-reference-data-blocks.md) — Phase 3G reference-data blocks (`lookup_table`, `segment_map`). **Status:** Accepted.
- [`decisions/0014-time-representation.md`](./decisions/0014-time-representation.md) — Time representation in Mosaic: `time_format` enforcement, `time_anchor` runtime parameter, canonical period-string grammar. **Status:** Accepted.
- [`decisions/0001-amendment-1-flexible-dimension-count.md`](./decisions/0001-amendment-1-flexible-dimension-count.md) — Amendment 1 to ADR-0001: relax "exactly 6 dims" to "4 structural + ≥1 domain-specific (min 5 total)". **Status:** Proposed (pending review).

---

## Build / test / lint state (at HEAD)

| Gate | Command | Status |
|---|---|---|
| Build | `cargo build --release --workspace` | ✓ zero warnings |
| Format | `cargo fmt --check --all` | ✓ |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ |
| Tests | `cargo test --workspace` | ✓ **912 / 0 / 5** (5 ignored require live external services — Postgres + DuckDB-Postgres scanner; documented as acceptable) |
| Determinism (10×) | `for i in $(seq 1 10); do cargo test --workspace -q ...; done` | ✓ verified at 731 prior; not re-run for 6A.2 (no concurrency/ordering changes — see process-notes Rule 11 §"Lean gates") |
| CLI demo (Rust path) | `./target/release/mc demo` | ✓ matches brief §4.6 |
| CLI demo (YAML path) | `./target/release/mc demo --model crates/mc-model/examples/acme.yaml` | ✓ matches brief §4.6 |
| Phase 3A acceptance | `diff <(./target/release/mc demo) <(./target/release/mc demo --model ...)` | ✓ empty output (still holds after Acme description-only cleanup) |
| `mc model validate` | `./target/release/mc model validate crates/mc-model/examples/acme.yaml` | ✓ exit 0 |
| `mc model inspect` | `./target/release/mc model inspect crates/mc-model/examples/acme.yaml` | ✓ exit 0 (output snapshot-locked) |
| **Phase 3B headline** | **`./target/release/mc model lint crates/mc-model/examples/acme.yaml`** | **✓ exit 0; ZERO warnings** |
| `mc model test` | `./target/release/mc model test crates/mc-model/examples/acme.yaml` | ✓ exit 0; 9/9 goldens pass; **32 ms wall-clock** (Phase 3C perf gate < 500 ms cleared by 15×) |
| `mc model test --fixture` | `./target/release/mc model test crates/mc-model/examples/acme.yaml --fixture nonexistent` | ✓ exit 0; reports 9 skipped (filter-only semantic; Acme has no fixture-referencing goldens) |
| **Phase 3C HEADLINE: special case removed** | `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` | **✓ 0** |
| **Phase 3C HEADLINE: equivalence** | `cargo test -p mc-model --test equivalence_acme` | **✓ 2 / 2** (2,520 canonical input coords bit-equal + 9 inline goldens within 1e-9; uses only existing public APIs from `mc-core` + `mc-fixtures`) |
| Locked surfaces (mc-core) | `git diff phase-3c-fixtures-and-inputs -- crates/mc-core/` | ✓ 0 lines |
| Locked surfaces (mc-fixtures src) | `git diff phase-3c-fixtures-and-inputs -- crates/mc-fixtures/src/ crates/mc-fixtures/Cargo.toml` | ✓ 0 lines |
| **Phase 4A HEADLINE: locked-surfaces** | **`git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/`** | **✓ 0 lines** (Phase 4A added zero changes to the kernel/fixtures/model layer) |
| **Phase 4A HEADLINE: MCP smoke** | **`echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \| target/release/mc mcp`** | **✓ valid JSON-RPC; lists 5 tools (`mosaic.demo`, `mosaic.model.{validate,inspect,lint,test}`)** |
| **Phase 4A HEADLINE: plugin example round-trips through kernel** | **`diff <(mc demo) <(mc demo --model mosaic-plugin/examples/models/acme-marketing.yaml)`** | **✓ empty output** (proves plugin example is byte-identical AND runnable end-to-end) |
| Phase 4A: byte-identity tests | `cargo test -p mc-cli --test example_byte_identity` | ✓ 2 / 2 (plugin YAML + plugin CSV match source) |
| Phase 4A: MCP smoke tests | `cargo test -p mc-cli --test mcp_smoke` | ✓ 8 / 8 (initialize / tools/list / tools/call ×3 / unknown-tool / parse-error / notification) |
| Phase 4A: plugin lint tests | `cargo test -p mc-cli --test plugin_lint` | ✓ 10 / 10 (manifest keys, mcp config, frontmatter validity per type, no provider-specific tags, markdown-only, marketing-mix is sole domain, MC3008 retired, examples/adapters Phase 4B placeholder) |
| **Phase 3D HEADLINE: formula authoring** | **`grep "body:" crates/mc-model/examples/acme.yaml \| head -5`** | **✓ all 5 rules use `body: "<formula>"`** |
| Phase 3D: round-trip stability | `cargo test -p mc-model --test formula_roundtrip` | ✓ 16 / 16 (sub/div assoc + Mul-with-Div + nested + unary + Acme Gross_Profit) |
| Phase 3D: backwards compat | `cargo test -p mc-model --test backwards_compat` | ✓ 3 / 3 (`_acme_with_bad_golden.yaml` structured-form fixture validates to flat body identically to formula form) |
| Benchmarks | `cargo bench --workspace` | ✓ Phase 1B baseline + Phase 2A cold-path expansion + Phase 2B fast path + **Phase 2C workload-shaped benches** all green. Numbers in [`PERF.md`](./PERF.md) §6 (Phase 1B), §6.7–§6.10 (Phase 2A), §6.11 (Phase 2B before/after), and **§6.12 / §6.13 / §6.14 (Phase 2C 10× / 50× / 100× rows + combined-workflow + scaling-shape summary)**. Phase 2C scaled rows compared against `--baseline phase-2b`; no Phase 1B/2A/2B regression beyond ±10% noise. **Phase 2C did not pick a Phase 2D winner** — §9 row priorities stay unspecified per the handoff hard rule. |

---

## Test count by target

| Target | Count |
|---:|---|
| `mc-core` unit tests | 90 |
| `tests/acme_demo.rs` | 20 |
| `tests/writeback.rs` | 11 |
| `tests/writeback_invalidated.rs` (Phase 2D) | 5 |
| `tests/consolidation.rs` | 12 |
| `tests/trace.rs` | 9 |
| `tests/dependency.rs` | 7 |
| `tests/locks_permissions.rs` | 8 |
| `tests/correctness.rs` | 16 |
| `tests/hierarchy_cycle.rs` | 10 |
| `tests/duplicate_elements.rs` | 6 |
| `tests/coordinate_validity.rs` | 9 |
| `tests/value_nan.rs` | 8 |
| `mc-fixtures` unit tests (Phase 1A: 4 + Phase 2A: 6 + Phase 2C: 6) | 16 |
| `mc-model` unit tests (Phase 3A — `src/parse/tests`) | 6 |
| `mc-model` `tests/parse_validate_smoke.rs` (Phase 3A) | 3 |
| `mc-model` `tests/structural_equivalence.rs` (Phase 3A) | 1 |
| `mc-model` `tests/validators.rs` (Phase 3A — one negative test per ADR-0004 Decision 6 row) | 14 |
| `mc-model` `tests/golden_acme.rs` (Phase 3A — runs the 9 inline goldens from acme.yaml) | 1 |
| `mc-model` unit tests — Phase 3B additions (`src/diagnostic::tests` + `src/lint::tests`) | 6 |
| `mc-model` `tests/lint_rules.rs` (Phase 3B — 10 per-rule fires-alone + 1 MC3008 retirement sweep) | 11 |
| `mc-model` `tests/mc2011_validator.rs` (Phase 3B — load() blocks with code MC2011 + ValidationError code namespace) | 2 |
| `mc-model` `tests/cli_snapshot.rs` (Phase 3B — hand-rolled snapshot harness; 1 inspect text + 10 lint text + 2 lint JSON + 2 deny-warnings + 2 validate + 1 model test) | 18 |
| `mc-model` `tests/deterministic_emission.rs` (Phase 3B — 10-run byte-exact + adjacent-pair sort assertion) | 2 |
| `mc-model` `tests/demo_no_goldens.rs` (Phase 3B — `mc demo --model <bad>.yaml` exits 0; `mc model test` exits non-zero — separation of concerns) | 2 |
| `mc-model` `csv::tests` (Phase 3C — strict CSV parser unit tests: basic parse / trailing-newline / header-mismatch / row-count-mismatch / quoted-field-rejection / BOM-rejection / CRLF / empty-CSV / internal-empty-row) | 9 |
| `mc-model` `tests/equivalence_acme.rs` (Phase 3C HEADLINE — Rust path vs YAML+CSV path on 2,520 canonical input coords + 9 inline goldens) | 2 |
| `mc-model` `tests/fixture_validators.rs` (Phase 3C — 14 per-MC2xxx fires-alone + 1 fixture-coverage sweep + 1 ValidationError code-uniqueness check) | 16 |
| `mc-model` `tests/path_escape.rs` (Phase 3C — `..` rejection + absolute-path rejection + sibling-resolve positive control) | 3 |
| `mc-model` `tests/perf_gate.rs` (Phase 3C — full pipeline under 500 ms in release; 5 s in debug) | 1 |
| `mc-model` `tests/schema_stability.rs` (Phase 3C — SCHEMA_VERSION pinned at 1.0; Phase 3B fixtures still parse; field-set unchanged; round-trip envelope shape) | 4 |
| `mc-cli` `tests/mcp_smoke.rs` (Phase 4A — MCP lifecycle smoke: initialize / tools/list / tools/call ×3 against validate, lint, test / unknown-tool error / malformed-request parse error / notification produces no response) | 8 |
| `mc-cli` `tests/example_byte_identity.rs` (Phase 4A — plugin YAML byte-identical to `crates/mc-model/examples/acme.yaml`; plugin CSV byte-identical to `crates/mc-model/examples/acme.inputs.csv`) | 2 |
| `mc-cli` `tests/plugin_lint.rs` (Phase 4A — `.claude-plugin/plugin.json` keys + no stale ADR-0008-sketch keys + `.mcp.json` shape + skills/agents/commands frontmatter + no provider-specific tags + markdown-only under skills/agents/commands + marketing-mix is sole domain + MC3008 documented retired + examples/adapters Phase 4B placeholder) | 10 |
| `mc-model` formula-expansion tests (Phase 3E–3G — conditionals, time-series, reference-data) | (included in above counts; +~80 vs 3D baseline) |
| `mc-model` time-anchor tests (Phase 3F.1) | (included above) |
| `mc-model` fitted-model tests (Phase 3H — `predict` / `calibrate` / `exp` / `norm_cdf`) | (included above) |
| `mc-tessera` + `mc-recipe` + `mc-drivers` expansion tests (Phase 5C) | (included above) |
| `mc-cli` Phase 6A integration tests (`tests/agent_cli_integration.rs` — 7 verbs + MCP isolation + exit-code paths + envelope schema_version) | (included above) |
| `mc-model/tests/formula_integration.rs` Phase 6A.1 additions (predict-out-of-order standardization regression + 60 other formula-engine integration tests) | (included above) |
| `mc-tessera/tests/time_format_ingest.rs` (Phase 6A.1 — non-ISO date ingestion through strptime subset) | (included above) |
| `mc-tessera/src/time_format.rs` unit tests (Phase 6A.1 — strptime subset behavior under each format token) | (included above) |
| `mc-core/src/rule.rs` Phase 6A.1 additions (`eval_unified_not_near_zero_is_true` + `eval_unified_if_near_zero_takes_else_branch` — near-zero falsy under epsilon) | (included above) |
| **Total** | **731** (was 704 pre-6A.1; +27 net for the round-trip — 25 in the worktree + 2 added during the self-audit MIN-6 correction) |

`mc-core` unit tests are 90 (was 84) after Phase 2D added 4
`cube_shape::tests` (cardinality + linearize round-trip + arity
mismatch + unknown element) and 2 `dirty::tests` (bitset
equivalence under a long mixed mark/clear/clear_all script + bitset
mark_closure parity with the AHashSet path) per Phase 2D handoff
item 4 + §A.6. New file [`tests/writeback_invalidated.rs`](../crates/mc-core/tests/writeback_invalidated.rs)
adds 5 tests (A–E) pinning the corrected marginal semantics of
`WritebackResult.invalidated` — Test D ("bulk-ingest preserves the
§10.1 per-write bound") is the regression net that, had it
existed, would have caught the Phase 1A bug originally. Phase 2C
added 6 tests in `mc-fixtures` (10 → 16): mandatory scale-1×
equivalence test + invariant tests at 10× / 50× / 100× +
extra-leaf round-trip at 10× + scale-zero rejection.

---

## Toolchain + dependency state

- Rust 1.78 pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml).
- `mc-core` runtime deps: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else.
- `mc-core` dev deps: `mc-fixtures` (path), `criterion = "0.5"` (workspace, default-features=false). Added in Phase 1B.
- `mc-fixtures` and `mc-cli` depend on `mc-core` only.
- **Cargo.lock pins (Phase 1B):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. These pre-edition2024 versions keep criterion buildable on Rust 1.78. Documented in [`PERF.md`](./PERF.md) §5.
- **Cargo.lock pins (Phase 3A):** `indexmap → 2.7.0`, `hashbrown → 0.15.5`. Pre-edition2024 versions keep `serde_yaml 0.9.34` buildable on Rust 1.78. Per ADR-0004 Decision 3 escape hatch (Phase 1B precedent reused). ADR-0005 was *not* opened — toolchain stays at 1.78.
- **`mc-model` (new in Phase 3A) runtime deps:** `serde 1` (derive), `serde_yaml 0.9.34`, `thiserror`. Dev-deps: `mc-fixtures` (path).
- **Still deferred:** `proptest` and `insta` declared at workspace level only; not pulled into `mc-core`. The toolchain blocker is no longer the reason — they're paired with §10.7 doctrines and snapshot tests that are Phase 2 work. See CLAUDE.md §1.1.

---

## Open deferrals (Phase 1A acceptance criteria)

None. Acceptance criterion 5 (`cargo bench --release` under §11 1A ceilings) closed 2026-05-01 in Phase 1B. See [`PERF.md`](./PERF.md) §6 for the table and [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §6 for the closure record. One known fixture-mismatch (`write_input_leaf_no_deps`) documented in PERF.md §7.3 as a non-regression for Phase 2 attention.

All Phase 1A criteria (1–10) now satisfied. Full table in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §5.

---

## Deviations from the brief that are still in effect

These are documented in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §3–§4 and remain active until a future spec or amendment supersedes them:

1. **`proptest` / `insta` still out of `mc-core` dev-deps.** `criterion` was restored in Phase 1B (2026-05-01) via Cargo.lock transitive pins (`clap` → 4.4.18, `clap_lex` → 0.6.0, `half` → 2.4.1) — the §11 bench gate is now active. `proptest` and `insta` remain deferred for a different reason: the §10.7 doctrines and snapshot-style tests that need them are Phase 2 work, not Phase 1B scope. Pulling the crates in without using them would just lengthen `cargo build`. See CLAUDE.md §1.1.
2. **§10.1 dirty-set assertions reframed as deltas** — the bound is preserved (215); the comparison frame changed because `write_canonical_inputs` legitimately accumulates marks across 2,520 input writes.
3. **§10.5 `t_dependency_graph_rejects_undeclared_dependency_in_test_mode`** asserts `RuleBodyTypeMismatch` (registration-time) rather than `UndeclaredDependency` (runtime). Strictly stronger guarantee.
4. **§10.7 `doctrine_no_mutation_of_frozen_dimensions`** asserts `dim.is_frozen()` post-build because no public mutation API exists in Phase 1; `EngineError::DimensionFrozen` variant retained for Phase 2.
5. **§10.7 `doctrine_atomicity_of_write` and `doctrine_causality`** are no-op stubs per §0.A; deterministic equivalents in `tests/acme_demo.rs`.
6. ~~**§11.1 `bench_write_input_leaf_no_deps`** measures ~165 µs (1A ceiling: 50 µs).~~ **Closed 2026-05-01 in Phase 2A.** The synthetic minimal-hierarchy fixture `mc_fixtures::build_minimal_cube` now lets the brief's "no-dependents" cost be measured directly — see PERF.md §6.8. The Acme `bench_write_input_leaf_no_deps` row remains in `leaf_read_write.rs` as a documented Acme-fixture path measurement; the new `synthetic_no_deps::write_input_leaf_no_deps_synthetic` row evaluates the brief's 50 µs 1A ceiling.

---

## Known Phase 2 follow-ups

Source-tagged hooks and surfaced findings. **Not scheduled.** Full lists in [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) §8 (Phase 1A) and [`PERF.md`](./PERF.md) §8 / §9 (Phase 1B + Phase 2A).

**Phase 2A closed Phase 1B's measurement gaps.** **Phase 2B closed PERF.md §9.4** (consolidation hierarchy clone). **Phase 2C produced the workload-shaped data ADR-0003 anchored to** but did *not* pick a Phase 2D winner — see PERF.md §6.14 for the scaling-shape table that the next phase reads from. **Phase 2D closed PERF.md §9.3** by shipping the bitset and (per the SPEC QUESTION amendment §A) correcting `WritebackResult.invalidated` from cumulative to marginal semantics; A/B isolation in [PERF.md §6.15](./PERF.md) shows the writeback semantic correction is the load-bearing change for the §6.14 cliff and the bitset is enabling foundation rather than the closer.

Optimization candidates surfaced from current data:

- ~~Hierarchy-clone hot-path in `cube.rs::read_consolidated`.~~ **Closed in Phase 2B** ([PERF.md](./PERF.md) §6.11 + §9.4).
- Per-dim leaf-flag caching to fast-path `is_consolidated_coord` ([PERF.md §9.2](./PERF.md)). **Phase 2C signal:** *opportunistic* — combined-workflow data shows per-edit total cost is flat at 50× across the session (≈ 422 µs amortized over `dirty_delta`; no within-session blow-up); §9.2's payoff is the per-write fixed cost, not session-length growth. **Phase 2D update:** combined-workflow per-edit cost dropped to ~11 µs at 50× (was ~2.4 ms; ~200× faster) as a side-effect of the writeback semantic correction; §9.2's payoff window is much smaller now.
- ~~Hierarchy mark closure cost.~~ **Closed in Phase 2D** ([PERF.md §6.15](./PERF.md) + §9.3 closure note). The §6.14 cliff was attributable to the cumulative-`invalidated`-collection bug, not to the AHashSet hash cost the Phase 2C handoff framing assumed; the bitset shipped as the structural foundation but moves the cliff by < 0.2 % in isolation.
- `Snapshot` copy-on-write at scale (Phase 1 ships deep-clone — [PERF.md §9.5](./PERF.md)). **Phase 2C signal:** *stays deferred* — TM1 stacked-sandbox pattern (10 live snapshots at 50×) shows linear scaling, no super-linear stacked-depth tax.
- `CellStore` trait introduction (Phase 1 ships concrete `HashMapStore`).
- Lock-acquisition capability check hardening.
- Toolchain bump → unlocks `proptest` / `insta` for the §10.7 doctrines and any insta-driven snapshot tests ([PERF.md §9.7](./PERF.md) housekeeping checklist).

---

## Repo layout (top level)

```
.
├── crates/
│   ├── mc-core/           kernel — dimensions, hierarchies, rules, consolidation, dirty tracking, snapshots
│   ├── mc-fixtures/       Acme demo cube + scaled fixtures (Phase 1A locked surface)
│   ├── mc-model/          model definition layer — YAML → mc_core::Cube (Phase 3A+); formulas (3D-3H); diagnostics (3B)
│   ├── mc-cli/            CLI runner — `mc demo` + `mc model {validate,inspect,lint,test,query,whatif,trace,sweep,diff,write}` + `mc tessera {init,apply,recipe-init,list-imports,status,schedule,transform}` + `mc mcp` (Phase 6A)
│   ├── mc-recipe/         recipe schema + validator + MC5xxx codes (Phase 5A + 5C)
│   ├── mc-drivers/        SourceDriver trait + 11 source drivers (Phase 5A: 6; Phase 5C: +5)
│   └── mc-tessera/        orchestrator — apply / dry-run / history / rollback / cron-schedule (Phase 5A); time_format strptime subset (Phase 6A.1)
├── mosaic-plugin/         Claude Code plugin (6 skills + 4 agents + 7 commands + .mcp.json with 12 MCP tools + Python adapters)
├── examples/              Acme demo + sports-betting NBA totals cartridge (14/14 goldens passing)
├── docs/                  this folder
├── research/              raw reference PDFs
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
