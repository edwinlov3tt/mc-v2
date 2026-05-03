# Phase 3C Completion Report — Model Test Fixtures and Input Sets

**Project:** MarketingCubes V2 — Rust kernel
**ADR:** [`../decisions/0006-phase-3c-model-test-fixtures.md`](../decisions/0006-phase-3c-model-test-fixtures.md) (Accepted with 13 project-owner amendments)
**Handoff:** [`../handoffs/phase-3c-handoff.md`](../handoffs/phase-3c-handoff.md)
**Operating manual:** [`../../CLAUDE.md`](../../CLAUDE.md)
**Inherited HEAD:** `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`)
**Phase 3C work-tree HEAD (pre-commit):** uncommitted; awaiting user review per handoff hard rule
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml)) — unchanged

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ |
| `cargo test --workspace` | Acceptance criterion 4 | ✓ **328 / 0** (was 293 / 0; +35 new) |
| `for i in 1..=10; do cargo test --workspace -q; done` | Acceptance criterion 9 (determinism) | ✓ 10 / 10 identical at 328 / 0 |
| `cargo run --release --bin mc -- demo` | Demo Rust path | ✓ matches brief §4.6 |
| `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` | Demo YAML path | ✓ byte-identical to Rust path (Phase 3A diff stays empty) |
| `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` | Phase 3C validate | ✓ exit 0 (now also runs resolve_inputs) |
| `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` | Phase 3C inspect (snapshot updated) | ✓ exit 0; new "Canonical inputs: 2520 cells from acme.inputs.csv" line |
| `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` | Phase 3B headline carry-forward | ✓ exit 0; ZERO warnings |
| `cargo run --release --bin mc -- model lint <...> --deny-warnings` | Phase 3B carry-forward | ✓ exit 0 |
| `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` | Phase 3C headline | ✓ exit 0; 9/9 goldens pass; **32 ms wall-clock** |
| `cargo run --release --bin mc -- model test <...> --fixture nonexistent` | Phase 3C `--fixture` filter | ✓ exit 0; 9 skipped (filtered) |
| `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` | **HEADLINE** acceptance gate | ✓ **0** (special case removed) |
| `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` | Locked-surface gate | ✓ 0 lines |
| `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/` | Locked-surface gate | ✓ 0 lines (no helper added) |
| `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/Cargo.toml` | Locked-surface gate | ✓ 0 lines |
| Forbidden-pattern grep on `crates/mc-model/src/` | CLAUDE.md §6.2 | ✓ matches confined to `#[cfg(test)]` (CSV parser unit tests) |
| Banned-deps grep | CLAUDE.md §3.1 | ✓ no `serde_json`, `tokio`, `rayon`, `anyhow`, or external `csv` crate |

**Headline equivalence test (`tests/equivalence_acme.rs`):** ✓ both paths byte-identical at all 2,520 canonical input coordinates AND all 9 inline goldens. The canonical-input check uses `f64::to_bits()` equality (the strongest possible); the goldens check uses a 1e-9 epsilon to absorb hierarchy-rollup floating-point order-of-operation noise.

---

## 2. Final test count

**Total: 328 tests passed / 0 failed.** Was 293 / 0 at Phase 3B inheritance; +35 new from Phase 3C.

| Target | Passed | Phase 3C delta |
|---|---:|---|
| `mc-cli` lib (no tests) | 0 | — |
| `mc-core` unit tests | 90 | — |
| `mc-core` `tests/acme_demo.rs` | 20 | — |
| `mc-fixtures` unit tests | 12 | — |
| `mc-core` `tests/writeback.rs` | 9 | — |
| `mc-core` `tests/correctness.rs` | 16 | — |
| `mc-core` `tests/dependency.rs` | 7 | — |
| `mc-core` `tests/duplicate_elements.rs` | 6 | — |
| `mc-core` `tests/hierarchy_cycle.rs` | 10 | — |
| `mc-core` `tests/locks_permissions.rs` | 8 | — |
| `mc-core` `tests/trace.rs` | 9 | — |
| `mc-core` `tests/value_nan.rs` | 8 | — |
| `mc-core` `tests/consolidation.rs` | 11 | — |
| `mc-core` `tests/writeback_invalidated.rs` | 5 | — |
| `mc-core` `tests/coordinate_validity.rs` | 16 | — |
| `mc-model` unit tests (`src/*.rs`) | 21 | **+9** (new `csv::tests`) |
| `mc-model` `tests/cli_snapshot.rs` | 18 | — |
| `mc-model` `tests/deterministic_emission.rs` | 2 | — |
| `mc-model` `tests/demo_no_goldens.rs` | 2 | — |
| `mc-model` `tests/equivalence_acme.rs` | **2** | **+2 (NEW)** |
| `mc-model` `tests/fixture_validators.rs` | **16** | **+16 (NEW)** |
| `mc-model` `tests/golden_acme.rs` | 1 | — |
| `mc-model` `tests/lint_rules.rs` | 11 | — |
| `mc-model` `tests/mc2011_validator.rs` | 2 | — |
| `mc-model` `tests/parse_validate_smoke.rs` | 3 | — |
| `mc-model` `tests/path_escape.rs` | **3** | **+3 (NEW)** |
| `mc-model` `tests/perf_gate.rs` | **1** | **+1 (NEW)** |
| `mc-model` `tests/schema_stability.rs` | **4** | **+4 (NEW)** |
| `mc-model` `tests/structural_equivalence.rs` | 1 | — |
| `mc-model` `tests/validators.rs` | 14 | — |
| **Total** | **328** | **+35** |

### Phase 3C new tests by file

- **`equivalence_acme.rs` (2)** — headline. (a) all 2,520 canonical input coords bit-identical between Rust path and YAML+CSV path. (b) all 9 inline goldens identical within 1e-9.
- **`fixture_validators.rs` (16)** — 14 per-MC2xxx fixtures + 1 sweep ("every MC2012-2025 has a fixture") + 1 uniqueness check on the full `ValidationError::code()` set (also asserts MC3008 is never reused).
- **`path_escape.rs` (3)** — `..` rejected with path-escape message; absolute path rejected with path-escape message; sibling path inside model dir resolves cleanly (positive control).
- **`perf_gate.rs` (1)** — full `mc model test` flow under 500 ms in release; 5 s in debug.
- **`schema_stability.rs` (4)** — `SCHEMA_VERSION` constant unchanged; every Phase 3B `lint_*.json` carries `"schema_version": "1.0"`; every Phase 3B fixture has all 5 Diagnostic fields + 4 ModelPath sub-fields; live `diagnostics_to_json` round-trip emits the same shape.
- **`csv::tests` (9)** — basic parse, trailing-newline tolerance, header mismatch (MC2024), row count mismatch (MC2023), quoted field rejection, BOM rejection, CRLF handling, empty CSV rejection, internal empty row rejection.

### Determinism gate

10 consecutive `cargo test --workspace -q` runs all reported `328 passed, 0 failed` (exit 0). Logged in §1 above.

---

## 3. Deviations from the brief / handoff

**Phase 3C had four notable deviations from the verbatim handoff prompt — all surfaced here per CLAUDE.md §11.**

1. **Resolve-inputs is a named architectural stage (not folded into `validate()`).** Per the project owner's clarification on top of "Option A" (recorded in conversation, before any code was written): `validate()` stays filesystem-free; `resolve_inputs(&ValidatedModel, Option<&Path>)` is a distinct exported stage; `mc_model::load(path)` runs all four stages but does **not** apply inputs to the cube (the returned `Cube` is empty of input data). `mc model test` is the only consumer that calls `apply_canonical_inputs` / `apply_fixture`.
2. **`ParsedRowCell` enum added for inline-row cells (broader than `ParsedScalar`).** Inline rows mix string dim-values with numeric cell-values; `ParsedScalar` (rule constants) intentionally excludes `String`. Added `ParsedRowCell` with `Float / Int / Bool / Str` variants.
3. **MC2012's negative fixture keeps the real `Scenario` column alongside the typo'd `Scenrio` to isolate the rule.** Without the duplicate, the typo'd column also fires MC2019 ("missing Scenario dim") which is correct co-firing semantics but breaks the test's no-spurious-codes discipline.
4. **`_acme_with_bad_golden.yaml` (a Phase 3B test fixture) gained a 1-row inline `canonical_inputs:` block.** Previously this fixture was relying on the Acme-name special case to populate inputs; with that branch removed, the bad-golden test would have read `Null` (ERROR) instead of `11500.0` (FAIL). The 1-row inline block restores the FAIL semantic intentionally.

Each rationale is in §4.

---

## 4. Rationale per deviation

### 4.1 Resolve-inputs is a named stage

**What the handoff says (§5):** *"Update `mc model test` to resolve fixture references generically. Load `canonical_inputs` (if declared) [and apply via cube.write]…"* The handoff implies a single `mc_model::load` that returns a populated cube — cf. handoff §D's example `let cube_yaml = mc_model::load(...).expect("load yaml")` with the `// load() resolves canonical_inputs … internally; cube_b returns already-populated` comment.

**What I did:** Per the project owner's architectural clarification on top of "Option A" (acknowledged before writing any code), `mc_model::load(path)` runs the four-stage pipeline `parse → validate → resolve_inputs → compile` for **error reporting** purposes (so `mc model validate` catches CSV / fixture errors), but the returned `Cube` has no input data applied. A new `mc_model::apply_canonical_inputs(...)` / `mc_model::apply_fixture(...)` pair is the only place that mutates the cube with input rows; `mc model test` is the only caller. `validate()` itself is filesystem-free.

**Rationale:** The clarification kept four contracts clean:
- `validate()` is testable without a filesystem.
- `load()` doesn't silently mutate (the `Cube` shape is exactly what `compile()` produces; the input data is a separate concern).
- `mc demo --model` keeps using the Rust formula path (`mc_fixtures::write_canonical_inputs`) — adding YAML-side application would create a confusing dual-path situation.
- `mc model test`'s flow is explicit: `load → resolve_inputs (again, for the data) → compile → apply → snapshot → goldens`.

The cost is a second `resolve_inputs` call inside `mc model test` (CSV is read twice). At Acme scale (2,520 rows / ~150 KB), this is microseconds — well inside the perf gate. **No public API semantics changed**: `CompiledCube`'s field set is byte-identical to Phase 3B; `load()`'s return type is unchanged; only behavior under a YAML that declares `canonical_inputs` / `test_fixtures` is new (and that's the contract Phase 3C ships).

This was approved by the project owner before any Phase 3C code was written ("approved - keep going" message in Phase 3C kickoff).

### 4.2 `ParsedRowCell` enum for inline rows

**What the brief / ADR-0006 says:** Inline rows form per Decision 1 — `rows: [[Forecast, Base, Mar_2026, Paid_Search, Tampa, Spend, 11500.0]]`. The example mixes strings (dim values) with a number (the value column).

**What I did:** Added a separate `ParsedRowCell { Float, Int, Bool, Str }` enum for inline-row cells. Did NOT extend `ParsedScalar` (which is used for rule body `{ const: 1.0 }` literals).

**Rationale:** `ParsedScalar` deliberately excludes `String` because rule-body constants are typed cell values (F64 / I64 / Bool only — Phase 1 brief's `ScalarValue` shape minus Null). Adding `String` to `ParsedScalar` would let `{ const: "hello" }` parse as a valid rule constant, then fail at compile when the validator tries to coerce it to a `CellDataType`. Keeping the two enums separate preserves the type-soundness invariant on rule bodies while allowing the inline-row mix.

### 4.3 MC2012 negative fixture has both `Scenrio` and `Scenario` columns

**What the brief / ADR says:** Each MC2012–MC2025 fixture should isolate one rule.

**What I did:** The MC2012 fixture's `columns` is `[Scenrio, Scenario, Version, Measure, value]` (five columns; `Scenrio` is the typo). The row has a sentinel value at the typo'd-column position.

**Rationale:** The naive fixture (`columns: [Scenrio, Version, Measure, value]`) produces TWO diagnostics: MC2012 ("Scenrio isn't a dim") AND MC2019 ("Scenario dim is missing from columns"). Both are technically correct — a typo'd column header genuinely also makes the real dim "missing". To isolate MC2012 cleanly (so the per-rule test asserts no spurious other-Phase-3C codes), the fixture keeps a real `Scenario` column alongside the typo'd `Scenrio`. The sentinel value at the typo'd-column slot is harmless — `resolve_inputs` skips unknown columns after MC2012 fires.

If a future review prefers the "natural" fixture and accepts the MC2012 + MC2019 co-fire, the `assert_only_target_fires` helper can be relaxed to allow co-firing pairs. The current discipline is stricter and arguably more useful for catching unintended fan-out.

### 4.4 `_acme_with_bad_golden.yaml` gained inline canonical_inputs

**What the brief / ADR says:** Phase 3C removes the Acme-name special case; any model that needs canonical inputs must declare `canonical_inputs:`.

**What I did:** The `_acme_with_bad_golden.yaml` fixture (used by `tests/demo_no_goldens.rs` to verify `mc demo --model` doesn't run goldens AND `mc model test` does fail goldens) gained a 1-row inline `canonical_inputs` block writing `Spend = 11500.0` at the bad-golden's coord.

**Rationale:** Without it, the bad-golden test produces ERROR (read returned Null) instead of FAIL (read returned 11500.0; expected 999_999.0). FAIL is the test's specific assertion (`stdout.contains("FAIL")`). The inline form keeps the fixture self-contained — using `source: "../../examples/acme.inputs.csv"` would have triggered MC2022's path-escape rejection. A 1-row inline write is the surgical fix.

---

## 5. Acceptance criteria — complete

Walking the 17-item gate from ADR-0006 Decision 9:

| # | Criterion | Status |
|---:|---|---|
| 1 | Acme canonical inputs in model-owned data | ✓ `crates/mc-model/examples/acme.inputs.csv` (1 header + 2,520 rows); YAML declares `canonical_inputs: { source: "acme.inputs.csv", columns: [...] }` |
| 2 | Acme-name special case REMOVED from `mc-cli/src/main.rs` | ✓ `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returns **0** |
| 3 | Headline equivalence test passes — uses only existing public APIs | ✓ `tests/equivalence_acme.rs` covers 2,520 coords + 9 goldens; no new `mc-core` or `mc-fixtures` APIs added |
| 4 | `mc model test acme.yaml` exits 0; 9/9 goldens pass via generic flow | ✓ |
| 5 | `mc model test --fixture <name>` filter ships | ✓ filter-only semantic; reports skipped count |
| 6 | `mc demo --model acme.yaml` byte-identical to `mc demo` | ✓ Phase 3A diff stays empty |
| 7 | `mc model validate / inspect / lint` work on updated Acme YAML | ✓ |
| 8 | `mc model lint acme.yaml` exits 0 with ZERO warnings | ✓ Phase 3B carry-forward |
| 9 | All 14 fixture validators (MC2012–MC2025) implemented + per-rule fixture each | ✓ 14 negative fixtures; per-rule test passes |
| 10 | CSV path-escape rejection (`../escape.csv` → MC2022 path-escape) | ✓ `tests/path_escape.rs` |
| 11 | Snapshot/rollback used for between-goldens reset; `mc model test acme.yaml < 500 ms` | ✓ **32 ms wall-clock** measured via `time` (well under 500 ms gate AND under 200 ms stretch from amendment #17) |
| 12 | JSON envelope `schema_version` stays at `"1.0"`; Phase 3B snapshots re-run produce zero diffs | ✓ `SCHEMA_VERSION = "1.0"` unchanged; `tests/schema_stability.rs` enforces this. `tests/cli_snapshot.rs` 18 tests still pass byte-for-byte against Phase 3B's `expected/lint_*.json` fixtures |
| 13 | All 293 existing tests still pass; new total ≥ 293 + Phase 3C count | ✓ **328 / 0** (was 293 / 0); 10/10 deterministic |
| 14 | `mc-core` untouched (`git diff` returns 0 lines) | ✓ 0 lines |
| 15 | `mc-fixtures` src untouched, no Cargo.toml change | ✓ 0 lines (no helper added — equivalence test uses inline coord enumeration per Decision 5's "default 'enumerate inline'") |
| 16 | Toolchain stays at Rust 1.78; CSV hand-rolled, no `csv` crate | ✓ `Cargo.lock` Phase 1B + 3A pins intact; CSV parser is `crates/mc-model/src/csv.rs` (~140 LoC including tests, ~80 LoC parser body). No new dep added |
| 17 | `MASTER_PHASE_PLAN.md` updated for the 3C/3D swap | ✓ done as part of this commit |

---

## 6. Acceptance criteria — deferred

None. All 17 items closed.

---

## 7. Implemented files / modules

### Workspace / config

- `Cargo.toml` — unchanged.
- `rust-toolchain.toml` — unchanged.
- `Cargo.lock` — unchanged (Phase 1B's clap/clap_lex/half pins + Phase 3A's indexmap/hashbrown pins still in place; verified by grep).

### `mc-core`

**Locked.** `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` returns 0 lines. No source / test / bench / Cargo.toml change.

### `mc-fixtures`

**Locked.** `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/ crates/mc-fixtures/Cargo.toml` returns 0 lines. The optional canonical-coord helper allowed by ADR-0006 Decision 5 was **not** added — the headline equivalence test enumerates coords inline, preserving the lock guarantee.

### `mc-model`

| Module / file | Action | ADR-0006 anchor |
|---|---|---|
| [`src/schema.rs`](../../crates/mc-model/src/schema.rs) | modified — added `canonical_inputs: Option<ParsedInputSet>`, `test_fixtures: Vec<ParsedFixture>`, `golden_test.fixture: Option<String>`, `ParsedInputSet`, `ParsedFixture`, `ParsedInlineRows`, `ParsedRowCell` types | Decision 1, 4 |
| [`src/error.rs`](../../crates/mc-model/src/error.rs) | modified — 14 new `ValidationError` variants (`FixtureUnknownDimensionKey`...`FixtureDuplicateCoordinate`); `code()` mapping extended | Decision 6 |
| [`src/csv.rs`](../../crates/mc-model/src/csv.rs) | **NEW** — strict-subset CSV parser (~80 LoC body + 9 unit tests) | Decision 1 (amendment (b)) |
| [`src/inputs.rs`](../../crates/mc-model/src/inputs.rs) | **NEW** — `resolve_inputs`, `apply_canonical_inputs`, `apply_fixture`; `ResolvedInputs` / `ResolvedInputSet` / `ResolvedFixture` / `ResolvedRow` types; CSV path resolution with path-escape rejection | Decisions 1, 3, 6, 7 |
| [`src/inspect.rs`](../../crates/mc-model/src/inspect.rs) | modified — `summarize` / `inspect_text_with_diagnostics` / `inspect_json` gain optional `&ResolvedInputs` parameter; new "Canonical inputs:" / "Test fixtures:" lines + JSON fields | Decision 9 #11 (handoff bullet) |
| [`src/lib.rs`](../../crates/mc-model/src/lib.rs) | modified — `load()` runs four-stage pipeline (parse → validate → resolve_inputs → compile); `load_str` likewise (with `model_dir = None`); new public re-exports | architecture clarification |
| [`examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | modified — added 5-line `canonical_inputs:` block referencing `acme.inputs.csv`; no other structural changes | Decision 5 |
| [`examples/acme.inputs.csv`](../../crates/mc-model/examples/acme.inputs.csv) | **NEW** — 1 header + 2,520 data rows; generated via a one-shot `examples/dump_acme_inputs.rs` binary that called `mc_fixtures::canonical_inputs_for(...)`, then deleted (binary not in tree) | Decision 5 |

### `mc-cli`

| File | Action |
|---|---|
| [`src/main.rs`](../../crates/mc-cli/src/main.rs) | **REMOVED** the `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch (line 253). Added `--fixture <name>` flag to `mc model test` (filter-only semantic per amendment (g)). `load_validated` now also runs `resolve_inputs` so MC2012–MC2025 surface in `mc model {validate,inspect,lint}`. `run_test` rewired to: load → resolve_inputs → compile → apply_canonical_inputs → snapshot → for each golden { optionally apply_fixture; check; rollback if mutated }. New `print_goldens_text` / `print_goldens_json` accept `skipped_count`. |

### Tests

| File | Action |
|---|---|
| [`tests/equivalence_acme.rs`](../../crates/mc-model/tests/equivalence_acme.rs) | **NEW** — headline equivalence (2 tests) |
| [`tests/fixture_validators.rs`](../../crates/mc-model/tests/fixture_validators.rs) | **NEW** — 14 per-validator tests + 1 sweep + 1 uniqueness check |
| [`tests/fixture_validation_fixtures/`](../../crates/mc-model/tests/fixture_validation_fixtures/) | **NEW dir** — 14 minimal YAML fixtures (one per MC2xxx) + 2 sibling CSVs (for MC2023 and MC2024 source-based tests) |
| [`tests/path_escape.rs`](../../crates/mc-model/tests/path_escape.rs) | **NEW** — 3 tests (`..` reject; absolute reject; sibling resolves) |
| [`tests/perf_gate.rs`](../../crates/mc-model/tests/perf_gate.rs) | **NEW** — 1 test (full pipeline under 500 ms in release; 5 s in debug) |
| [`tests/schema_stability.rs`](../../crates/mc-model/tests/schema_stability.rs) | **NEW** — 4 tests (SCHEMA_VERSION constant; Phase 3B fixtures still parse; field-set unchanged; round-trip envelope shape) |
| [`tests/lint_fixtures/_acme_with_bad_golden.yaml`](../../crates/mc-model/tests/lint_fixtures/_acme_with_bad_golden.yaml) | modified — added 1-row inline `canonical_inputs:` (see deviation 4.4) |
| [`tests/expected/inspect_acme.txt`](../../crates/mc-model/tests/expected/inspect_acme.txt) | modified — added `Canonical inputs:` / `Test fixtures:` lines per inspect summary update |

### Documentation

- [`docs/reports/phase-3c-completion-report.md`](./phase-3c-completion-report.md) — this file.
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — flipped Phase 3C from `proposed` → `complete`; added test count delta (293 → 328); added Cargo.lock pin status.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3C status row updated to `complete`.

### What was NOT touched

- `docs/specs/engine-semantics.md` — locked.
- `docs/specs/phase-1-rust-kernel-build-brief.md` — locked.
- ADR-0001..ADR-0006 — Accepted; amendments would go in `0006-amendment-N.md` files (none needed for Phase 3C).
- PERF.md — kernel didn't change; no benches re-run.

---

## 8. Diagnostic-code registry update

| Range | Category | Status after Phase 3C |
|---|---|---|
| MC1001..1002 | Parse errors (YAML syntax + safe-subset) | Unchanged from Phase 3B |
| MC2001..2010 | Validation errors (Phase 3A's 10 ADR-0004 rules) | Unchanged from Phase 3A |
| MC2011 | `WeightedAverageMissingWeight` (Phase 3B promotion from MC3008) | Unchanged from Phase 3B |
| **MC2012..2025** | **Phase 3C fixture/input validators** | **NEW (14 codes, all shipping)** |
| MC3001..3007 + MC3009..3011 | Lint warnings | Unchanged from Phase 3B |
| MC3008 | **PERMANENTLY RETIRED** — promoted to MC2011 in Phase 3B | Unchanged; assertion `tests/fixture_validators.rs::all_validation_error_codes_are_unique_and_in_range` enforces no reuse |
| MC4xxx | Reserved | Reserved |

The `Diagnostic` struct shape is byte-identical to Phase 3B — `code`, `severity`, `path`, `message`, `suggestion` (5 fields) plus `path.{file, span, yaml_pointer, model_path}` (4 sub-fields). No new fields added; `schema_version` stays `"1.0"`. `tests/schema_stability.rs` enforces all of this.

### Phase 3C-specific code disambiguations (per ADR-0006 amendments (d), (e), #9, #19)

- **MC2012** (was "unknown dim VALUE" in early ADR drafts; pinned per amendment (d) to "unknown dimension KEY") and **MC2013** ("unknown element VALUE") are SEPARATE codes — typo'd column headers vs typo'd row values.
- **MC2025** (repurposed pre-acceptance per amendment (e) + #9 from "missing required dimension" to "duplicate input coordinate within input set"; MC2019 holds the missing-dim semantic). After Phase 3C ships, MC2025's meaning is locked forever.
- **MC2018** (value type mismatch), **MC2021** (NaN), **MC2024** (CSV header mismatch) are the three explicitly-required fixtures per amendment #19; all three ship with dedicated negative fixtures.
- **MC2022** carries a `reason` field that disambiguates "not found" / "path-escape" / "no file context" / "absolute path"; the path-escape variant satisfies amendment #18 without needing a separate MC2026 code (deferred in case of future need).

---

## 9. Implementation summary

The CSV parser is hand-rolled per amendment (b)'s strict-subset spec — UTF-8 (BOM rejected), required header byte-exact match `columns:`, comma-separated only, no quoted fields, no embedded commas/newlines, no comments, no empty rows, trailing newline tolerated. ~80 LoC body via `str::lines()` (which gives correct trailing-newline tolerance and `\r\n` handling for free). No `csv` crate dep.

`canonical_inputs` and `test_fixtures` resolution lives in `inputs.rs::resolve_inputs`. Inline rows and source-based CSV rows funnel through the same row-typing path (`ParsedRowCell` → string → measure-typed `ScalarValue` parse). CSV path resolution canonicalizes both the candidate and the model directory and asserts `candidate.starts_with(canonical_dir)`; absolute paths and `..` segments are rejected before `canonicalize` even runs (so the failure message stays user-facing rather than libc-flavored).

`mc model test`'s flow uses `Cube::snapshot()` once after `apply_canonical_inputs`, then calls `Cube::rollback_to(&snap)` only between goldens that mutated the cube via a fixture overlay. Read-only goldens (no `fixture:`) skip the rollback since `Cube::read` is the only operation that touched the cube. For Acme (no fixtures), zero rollbacks occur, total wall-clock ≈ 32 ms.

`--fixture <name>` is filter-only (per amendment (g)): goldens whose `fixture:` field doesn't match are reported as skipped. The skipped count appears in both text (`"… N skipped (filtered)"`) and JSON (`"skipped": N`) outputs.

The Acme migration was generated via a one-shot `examples/dump_acme_inputs.rs` binary that called `mc_fixtures::canonical_inputs_for(...)` and emitted CSV rows in the canonical write order. The binary was deleted after `acme.inputs.csv` was committed; the equivalence test guards against any drift between the CSV and the Rust formula by reading both paths' output at every one of the 2,520 coords with `f64::to_bits()` equality.

---

## 10. Known follow-ups for the next phase

These are explicit hooks left in the code or surfaced during this phase. **They are not scheduled.**

1. **Inline `String` in `ParsedScalar` for rule constants.** `ParsedScalar` excludes strings on purpose — but if Phase 3D's friendly-formula syntax ever needs to lift a string literal into a `Const` (unlikely; formulas are arithmetic), the deviation between `ParsedScalar` and `ParsedRowCell` becomes load-bearing.
2. **CSV-actuals importer (Phase 5).** Phase 3C's strict subset is fixture-shaped; real actuals will need quoted fields, escaped commas, multi-line cells, encoding tolerance. Per ADR-0006 Decision 2, this is a separate phase using the `csv` crate (or equivalent) under an `actuals_sources:` schema, NOT an extension of the `canonical_inputs:` parser.
3. **Multi-cube workspace fixture root (per ADR-0006 amendment #18 alternate route).** If a real two-model project surfaces a need for a shared `fixtures/` directory at the workspace root, the strict per-model-directory rule expands to "nearest ancestor directory containing a Cargo.toml or a project-marker file". Phase 3C's default is the strict rule until concrete need argues otherwise.
4. **MC2026 dedicated path-escape code.** Phase 3C folded path-escape into MC2022 with a `reason` discriminator. If LLM authoring (Phase 4) wants to handle path-escape errors specially, a separate MC2026 code can be split out — that requires a `schema_version` bump per ADR-0006 amendment #20's stability rule.
5. **`mc demo --model` could optionally apply YAML's canonical_inputs.** Currently `mc demo --model` uses the Rust formula path (`mc_fixtures::write_canonical_inputs`) for cell population. Switching to the YAML's resolved inputs would unify the two demo paths but at the cost of a second source of truth for "what does Acme look like populated"; deferred until Phase 4 / 6 actually exercise non-Acme YAML demos.
6. **Phase 3D — friendly formula syntax.** Per ADR-0004 Decision 4 + ADR-0006 roadmap impact, this is now Phase 3D; no handoff yet.
7. **`proptest` / `insta` still deferred** — not pulled in by Phase 3C either. Same reasoning as Phase 3B (CLAUDE.md §1.1).

---

## 11. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit.

- ✓ **No `mc-core` source change** — `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` returns 0 lines.
- ✓ **No `mc-fixtures` source change** — `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/` returns 0 lines. No helper added (lock guarantee preserved).
- ✓ **No `mc-fixtures` Cargo.toml change** — 0 lines.
- ✓ **No new dependencies** — workspace `Cargo.toml` and per-crate `Cargo.toml`s unchanged. `Cargo.lock` Phase 1B + 3A pins (clap 4.4.18, clap_lex 0.6.0, half 2.4.1, indexmap 2.7.0, hashbrown 0.15.5) all intact.
- ✓ **No banned imports** — `grep -rn "use serde_json\|use tokio\|use rayon\|use anyhow\|csv::" crates/mc-model/src/` returns only `crate::csv::parse_strict` (our own module, not the `csv` crate).
- ✓ **No `unsafe` / `async` / threads** — `grep -rn "unsafe" crates/mc-model/src/` returns 0 lines.
- ✓ **No `unwrap()` / `expect()` / `panic!()` / `unimplemented!()` / `todo!()` in production `mc-model` code** — `grep` returns hits only inside `#[cfg(test)] mod tests` (CSV parser unit tests).
- ✓ **No new lint rules** — lint module unchanged from Phase 3B; MC3008 still retired.
- ✓ **`Diagnostic` struct shape unchanged** — `tests/schema_stability.rs` enforces.
- ✓ **`schema_version` stays `"1.0"`** — `SCHEMA_VERSION` constant unchanged; live emission verified.
- ✓ **Acme YAML structure unchanged except for `canonical_inputs:`** — `tests/structural_equivalence.rs` still passes; demo diff still empty.
- ✓ **Toolchain stays at Rust 1.78** — `rust-toolchain.toml` unchanged.

---

## 12. Acceptance gate — final attestation

| Gate | Command | Result |
|---|---|---|
| **HEADLINE: special case removed** | `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` | **0** |
| **HEADLINE: equivalence test passes** | `cargo test -p mc-model --test equivalence_acme` | **2 / 2 passed** (2,520 coords + 9 goldens) |
| Acme `mc model test` exits 0 with 9/9 goldens via generic flow | `mc model test crates/mc-model/examples/acme.yaml` | **9 / 9 passed; exit 0; 32 ms** |
| Phase 3B headline carry-forward | `mc model lint crates/mc-model/examples/acme.yaml` | **exit 0; ZERO warnings** |
| Phase 3A demo equivalence | `diff <(mc demo) <(mc demo --model acme.yaml)` | **empty** |
| Determinism | 10× `cargo test --workspace -q` | **10 / 10 at 328 / 0** |
| Locked surfaces | `git diff phase-3b...` on `mc-core/` + `mc-fixtures/src/` + `mc-fixtures/Cargo.toml` | **0 lines** |

**Phase 3C ships pending user review of this report and the diff.** No commit, no tag, no push performed by the implementer per the handoff hard rule.
