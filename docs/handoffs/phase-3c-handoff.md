# Phase 3C Handoff — Model Test Fixtures and Input Sets

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 3C.
> **You inherit a green Phase 3B** (commit `f4f7fa8`, tag
> `phase-3b-lint-and-diagnostics`).
>
> **This phase closes the visible scaffolding hack from Phase 3B
> deviation 4.3** — `mc model test` currently writes Acme's canonical
> inputs only when `metadata.name == "Acme_MarketingFinance"`. Phase 3C
> moves that data into the model file itself (`canonical_inputs:`
> block referencing `acme.inputs.csv`) and removes the special case
> from `mc-cli`. After Phase 3C, `mc model test` works generically.
>
> **Hard rule:** Phase 3C touches `crates/mc-model/` (new schema
> blocks, new CSV parser, lint/inspect updates for the new schema, 14
> new validators) and `crates/mc-cli/` (removes the Acme special case
> and adds `--fixture` flag). It does NOT touch `crates/mc-core/`,
> `crates/mc-fixtures/`, `docs/specs/`, or any kernel/fixture file.
> The locked-surfaces guarantee from Phase 2D / 3A / 3B carries
> forward.

---

## Where Phase 3B ended

- **Phase 3B commit / tag:** `f4f7fa8` — *phase-3b: model QA, linter, and diagnostics (mc model {validate,inspect,lint,test})* — tag `phase-3b-lint-and-diagnostics`. Backfill commit at `5cb60be`. For-dummies note at `7faa3c7`.
- **Test status:** 293 / 0 passing across all targets. 10/10 deterministic.
- **Demos:** `cargo run --release --bin mc -- demo` matches brief §4.6. `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` produces byte-for-byte identical output.
- **Headline gate:** `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` exits 0 with zero warnings.
- **Gates green:** build / fmt / clippy / test / both demos / lint.
- **Toolchain:** Rust 1.78 pinned. Cargo.lock pins from Phase 1B (`clap`, `clap_lex`, `half`) + Phase 3A (`indexmap → 2.7.0`, `hashbrown → 0.15.5`). Do not bump.
- **`mc-core`, `mc-fixtures` deps unchanged** since Phase 2D (mc-core) and Phase 1A (mc-fixtures).
- **Phase 3B left one visible hack (deviation 4.3):** `mc-cli/src/main.rs:253` has an `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch that calls `mc_fixtures::write_canonical_inputs`. Phase 3C's headline acceptance gate is **REMOVING this branch** while keeping `mc model test acme.yaml` green.

For the full Phase 3B audit see [`../reports/phase-3b-completion-report.md`](../reports/phase-3b-completion-report.md). For the binding strategic context for THIS phase, read [`../decisions/0006-phase-3c-model-test-fixtures.md`](../decisions/0006-phase-3c-model-test-fixtures.md) **before this handoff** — the ADR has 13 acceptance amendments that constitute the contract; this handoff is the build instructions.

---

## Phase 3C prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 3C: Model Test Fixtures and Input Sets.
>
> **Context.** Phase 3B closed the model-quality story but left a visible hack: `mc model test` writes Acme's canonical inputs only when `metadata.name == "Acme_MarketingFinance"`. Any other YAML model passes through with empty input cells, breaking goldens that depend on input values. Phase 3C closes that hack by adding model-owned `canonical_inputs:` and `test_fixtures:` schema blocks, plus a strict CSV parser for sibling input data files.
>
> **Goal.** Ship `mc-model`'s fixture/input layer such that:
>
> 1. `crates/mc-model/examples/acme.yaml` declares `canonical_inputs: { source: "acme.inputs.csv", columns: [Scenario, Version, Time, Channel, Market, Measure, value] }` (or equivalent) and `crates/mc-model/examples/acme.inputs.csv` exists with 1 header row + 2,520 data rows (1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures).
> 2. The Acme-name special case in `mc-cli/src/main.rs:253` is REMOVED. `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returns 0.
> 3. `mc model test crates/mc-model/examples/acme.yaml` exits 0 with all 9 inline goldens passing — using ONLY the new generic fixture-loading path, NO Acme-specific CLI logic.
> 4. The headline equivalence test passes: a YAML+CSV-loaded Acme cube produces byte-identical store state to the Rust-fixture path (`build_acme_cube` + `write_canonical_inputs`) across all 2,520 canonical input coordinates AND all 9 inline goldens. **The test uses ONLY existing public APIs from `mc-core` + `mc-fixtures`** — no new APIs added to either crate.
> 5. `mc model test crates/mc-model/examples/acme.yaml --fixture <name>` runs only goldens that explicitly reference that fixture (filter semantic; not overlay). Reports skipped goldens.
> 6. 14 new fixture-validation rules implemented (MC2012–MC2025), each with a negative-test fixture under `crates/mc-model/tests/fixture_validation_fixtures/` triggering exactly that rule.
> 7. `mc demo --model crates/mc-model/examples/acme.yaml` still produces brief §4.6 output byte-for-byte identical to `mc demo` (Phase 3A demo-equivalence diff stays empty).
> 8. `mc model lint crates/mc-model/examples/acme.yaml` still exits 0 with zero warnings (Phase 3B headline gate carry-forward).
> 9. JSON envelope `schema_version` stays at `"1.0"` — Phase 3C adds new codes but doesn't change the `Diagnostic` struct shape.
>
> **Phase 3C scope** (binding contract — read [`../decisions/0006-phase-3c-model-test-fixtures.md`](../decisions/0006-phase-3c-model-test-fixtures.md) for full strategic rationale; this scope IS what the ADR's 9 Decisions + 13 acceptance amendments commit to):
>
> 1. **Add `canonical_inputs:` and `test_fixtures:` to the YAML schema** (additive, backwards-compatible). `ParsedModel`/`ValidatedModel` gain optional fields. Models without these blocks load identically to Phase 3B behavior.
>
>    Schema shape (illustrative):
>
>    ```yaml
>    canonical_inputs:
>      source: "acme.inputs.csv"           # OR
>      inline:
>        columns: [Scenario, Version, Time, Channel, Market, Measure, value]
>        rows:
>          - [Forecast, Base, Mar_2026, Paid_Search, Tampa, Spend, 11500.0]
>      columns: [Scenario, Version, Time, Channel, Market, Measure, value]
>
>    test_fixtures:
>      - name: aggressive_q1
>        source: "fixtures/aggressive_q1.csv"
>      - name: conservative_drawdown
>        inline: { columns: [...], rows: [...] }
>    ```
>
> 2. **Implement the strict CSV parser** in `mc-model` per ADR-0006 Decision 1's binding subset:
>    - UTF-8 (no BOM)
>    - Required header row, byte-exact match to `columns:`
>    - Comma-separated, no other delimiters
>    - No quoted fields, no embedded commas, no embedded newlines
>    - No comments
>    - Numeric value column (last column reserved as cell value)
>    - Trailing newline tolerated, not required
>    - No empty rows
>
>    **Hand-rolled, ~50 lines of Rust.** No `csv` crate dependency. Real CSV actuals import (with quoted fields, encodings, etc.) is Phase 5, not Phase 3C.
>
> 3. **Implement CSV path resolution per amendment #18:** `source: "<path>"` is relative to the YAML model file's directory. Reject paths that resolve outside that directory tree (no `../../escape.csv`, no absolute paths, no symlinks pointing outside). Path-escape errors emit MC2022 with a "path-escape" message variant (or a dedicated MC2026 if cleaner — implementer's call).
>
> 4. **Implement the 14 new validators (MC2012–MC2025)** per ADR-0006 Decision 6's table:
>    - MC2012 — unknown dimension KEY (typo'd column name)
>    - MC2013 — unknown element VALUE (cell value not in dim's elements)
>    - MC2014 — fixture references unknown measure
>    - MC2015 — fixture writes to derived measure
>    - MC2016 — duplicate fixture names within a model
>    - MC2017 — golden test references unknown fixture name
>    - MC2018 — fixture value type mismatch
>    - MC2019 — fixture coordinate missing required dimension
>    - MC2020 — coordinate points to a consolidated cell
>    - MC2021 — fixture value is NaN
>    - MC2022 — source CSV file not found / unreadable / path-escape
>    - MC2023 — CSV row column count mismatch
>    - MC2024 — CSV header row mismatch
>    - MC2025 — duplicate input coordinate within the same input set
>
>    Each gets a negative-test fixture in `tests/fixture_validation_fixtures/`. Each test asserts exactly its rule fires; no other rule fires spuriously.
>
> 5. **Update `mc model test` to resolve fixture references generically** per ADR-0006 Decision 3:
>    - Load `canonical_inputs` (if declared).
>    - **Snapshot the cube** via `mc_core::Cube::snapshot()` immediately after canonical_inputs load.
>    - For each golden test:
>      - If `fixture:` named, load that fixture's data on top (override semantic).
>      - Run the golden's expect check.
>      - **Rollback to the snapshot** via `mc_core::Cube::rollback_to(snapshot)` between goldens.
>
>    **Performance gate:** `mc model test crates/mc-model/examples/acme.yaml` completes in **< 500 ms** wall-clock on the Phase 1B reference machine. (Stretch: < 200 ms per acceptance amendment #17. Tighten the gate in the completion report if measurement shows headroom.)
>
>    If snapshot/rollback has a behavior gotcha for the fixture-overlay case (dirty-set restoration / consolidation cache interaction), fall back to "only reset between goldens that reference fixtures" — goldens using only canonical_inputs are read-only and need no reset. Document the choice in the completion report.
>
> 6. **Add `mc model test --fixture <name>` filter flag** per ADR-0006 Decision 7:
>    - Filter-only semantic: run only goldens whose `fixture:` field equals the named fixture.
>    - Skip all other goldens (those without `fixture:` or with a different fixture name); report skipped count.
>    - Filter happens before the snapshot/rollback loop, so the perf gate from item 5 still applies.
>    - Do NOT implement `--inputs <fixture_path>` — deferred to Phase 5.
>    - Do NOT implement an "overlay all goldens with fixture" semantic — that's a separate feature for a future phase.
>
> 7. **Migrate Acme** per ADR-0006 Decision 5:
>    - Create `crates/mc-model/examples/acme.inputs.csv` with 1 header + 2,520 data rows. Generate the data by replicating the closed-form formulas from `mc_fixtures::write_canonical_inputs` (or by extracting them via a one-shot dump from a Rust binary you write in `examples/` and discard).
>    - Add `canonical_inputs: { source: "acme.inputs.csv", columns: [...] }` to `crates/mc-model/examples/acme.yaml`.
>    - **REMOVE** the `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch from `crates/mc-cli/src/main.rs:253`. Verify with `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returning 0.
>    - The structural-equivalence test against `build_acme_cube()` (added in Phase 3A) must still pass after the cleanup. The demo-equivalence diff must still be empty. The Phase 3B lint-clean check must still hold.
>
> 8. **Add the headline equivalence test** under `crates/mc-model/tests/`:
>    - Compares Rust path (`build_acme_cube` + `write_canonical_inputs`) against YAML+CSV path (`mc_model::load`).
>    - Uses ONLY existing public APIs from `mc-core` + `mc-fixtures` — **no new APIs added to either crate**.
>    - Iterates all 2,520 canonical input coords; reads each from both cubes; asserts byte-identical values.
>    - Plus: re-runs all 9 inline goldens through both paths, asserts identical results.
>    - If `mc-fixtures` doesn't currently export an iterator over canonical input coords, enumerate inline (1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures = 2,520). Adding a public read-only helper to `mc-fixtures` is allowed ONLY for this case (pure iteration over already-exported IDs); the default is "enumerate inline" to keep the lock guarantee.
>
> 9. **Stability assertions per acceptance amendment #20:** Phase 3C MUST NOT change the `Diagnostic` struct shape. Add an assertion that Phase 3B's snapshot fixtures (`crates/mc-model/tests/expected/lint_*.json`) re-run at Phase 3C completion produce zero diffs. The JSON envelope `schema_version` stays at `"1.0"`.
>
> **Hard rules:**
>
> - **`crates/mc-core/` is LOCKED.** No source change, no Cargo.toml change. `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` returns zero lines.
> - **`crates/mc-fixtures/src/` is LOCKED.** No source change. `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/` returns zero lines. The exception (per Phase 3C scope item 8): if and only if `mc-fixtures` doesn't already export a canonical-input-coord iterator, you may add ONE pure read-only helper. If the public API surface of `mc-fixtures` grows by more than one helper, **stop and write a SPEC QUESTION**.
> - **`crates/mc-fixtures/Cargo.toml` is LOCKED.** No new deps.
> - **`mc-fixtures::build_acme_cube` and `mc-fixtures::write_canonical_inputs` byte-for-byte unchanged.** Phase 1/2 benches and tests depend on these as the canonical Rust-side reference.
> - **`mc-cli/src/main.rs:253` Acme-name special case is REMOVED** (not refactored, REMOVED). The replacement code path is the generic `canonical_inputs:` resolver from item 5.
> - **No `csv` crate dependency.** Hand-rolled parser for the strict subset only.
> - **No `serde_json` dependency added** (Phase 3B used hand-rolled JSON; same approach for any new JSON output in Phase 3C).
> - **CSV path resolution rejects `../../` escapes** (per amendment #18). Test fixture: `source: "../escape.csv"` produces a typed diagnostic.
> - **The two CSV columns contract pieces (header byte-exact + value-type-from-measure)** are binding per amendment #19. Three specific negative fixtures: header mismatch (MC2024), type mismatch (MC2018), NaN value (MC2021).
> - **JSON envelope `schema_version` stays at `"1.0"`.** Adding codes is backwards-compatible. **Acceptance assertion:** Phase 3B's lint snapshot fixtures still parse cleanly and produce zero diffs after Phase 3C ships.
> - **MC3008 stays permanently retired.** No new lint rule reuses the code. (No new lint rules at all in Phase 3C — this is a fixture-and-validator phase, not a lint-additions phase.)
> - **Toolchain stays at Rust 1.78.** No `cargo update`. No new dep that requires `edition2024`.
> - **No `unsafe`, no `async`, no `tokio`, no `rayon`, no threads.** Phase 3C is sync.
> - **All 293 existing tests must still pass.** New total ≥ 293 + (Phase 3C test count, including the 14 negative validator tests + the headline equivalence test + the path-escape test + the snapshot-stability re-run + the perf-gate test).
>
> **Acceptance gate (the headline + supporting):**
>
> Headline: **`grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returns 0** AND **`mc model test crates/mc-model/examples/acme.yaml` exits 0 with all 9 goldens passing** AND **the equivalence test asserts byte-identical store state between Rust and YAML+CSV paths across all 2,520 canonical input coords + 9 goldens**.
>
> Plus all 17 success-gate items from ADR-0006 Decision 9 (read them).
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (≥ 293 + new Phase 3C tests)
> - `cargo run --release --bin mc -- demo` (matches brief §4.6 — Rust path)
> - `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` (byte-identical to Rust path; Phase 3A diff stays empty)
> - `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` (exits 0)
> - `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` (exits 0; `inspect` may have a new "Canonical inputs: 2520 cells from acme.inputs.csv" line — update the snapshot fixture)
> - `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` (exits 0; ZERO warnings — Phase 3B headline gate carry-forward)
> - `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` (exits 0; all 9 goldens pass; under 500 ms)
> - 10 consecutive `cargo test --workspace -q` (deterministic)
> - `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/ crates/mc-fixtures/src/` (≤ 1 helper if a canonical-coord iterator was added; otherwise zero lines)
> - `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` (returns 0)
>
> **Documentation requirements:**
> - Append `docs/reports/phase-3c-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 3C from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 3C status row.
> - Document the diagnostic-code registry update (MC2012–MC2025 added; MC3008 still retired) in the completion report.
> - **Do NOT modify [ADR-0006](../decisions/0006-phase-3c-model-test-fixtures.md).** It's Accepted; amendments go in `0006-amendment-N.md`.
> - **Do NOT modify the brief, engine-semantics doc, ADR-0004, or ADR-0005.** They're contracts.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
> 1. The headline equivalence test requires a new public API on `mc-core` or `mc-fixtures` beyond the one canonical-coord iterator helper. The default is "enumerate inline"; if you find yourself wanting more, surface it.
> 2. `Cube::snapshot()` + `Cube::rollback_to()` has a behavior gotcha that breaks the fixture-overlay reset. Per amendment #17's alternate route, fall back to "only reset between goldens that reference fixtures" and document; if that fallback also has issues, SPEC QUESTION.
> 3. The Acme cleanup somehow breaks the structural-equivalence test against `build_acme_cube()` or the demo-equivalence diff. Shouldn't happen — only `canonical_inputs:` is added to the YAML — but stop and surface if it does.
> 4. Any of MC2020 / MC2021 / MC2022 turns out to be already covered by an existing kernel-side validator. Per amendment (alternate route), drop the redundant code rather than double-validating.
> 5. The CSV `value` reserved-name conflicts with anything (e.g., a future measure literally named `value`). Per amendment #19's alternate route, use `__value` or `_value` as the marker — implementer's call, document in completion report.
> 6. The performance gate (`mc model test acme.yaml < 500 ms`) cannot be hit even with snapshot/rollback. Surface the actual measurement and reason; the gate may relax if there's a structural cause.
> 7. The CSV path-escape rejection turns out to be too restrictive for a real project structure (e.g., a top-level `fixtures/` directory shared across multiple model files). Per amendment #18's alternate route, expand to "nearest ancestor directory containing a Cargo.toml" — but that's a SPEC QUESTION, not a unilateral change.
>
> **Rollback plan (in case complexity explodes):**
>
> If the CSV parser balloons beyond ~150 lines (the "strict subset" should be ~50 lines), or if any single validator requires non-trivial AST-walking infrastructure that doesn't fit cleanly into `&ValidatedModel`, **stop and write a SPEC QUESTION**. Two recovery paths:
> 1. **Narrow the validator surface for Phase 3C.1**: ship a minimum-viable subset (CSV loader + Acme migration + the 5 most load-bearing validators), defer the other 9 to a follow-up phase. Requires ADR-amendment.
> 2. **Reconsider the CSV grammar**: if a real input file genuinely needs quoted fields or escaped commas (which Acme doesn't), pull in the `csv` crate as a small targeted dep. Requires SPEC QUESTION + the deps audit.
>
> Either fallback is a Phase 3C.1 amendment, not a Phase 3C scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 3C Model Test Fixtures and Input Sets
>
> Build:    cargo build --release --workspace ✓
> Format:   cargo fmt --check --all ✓
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓
> Tests:    cargo test --workspace [N] / 0 (was 293 / 0)
> Demo (Rust):     cargo run --release --bin mc -- demo ✓
> Demo (YAML):     cargo run --release --bin mc -- demo --model <acme.yaml> ✓ (Phase 3A diff still empty)
> Validate:        mc model validate <acme.yaml> ✓
> Inspect:         mc model inspect <acme.yaml> ✓ (snapshot updated for canonical_inputs line)
> Lint:            mc model lint <acme.yaml> ✓ (ZERO warnings — Phase 3B carry-forward)
> Test:            mc model test <acme.yaml> ✓ (9/9 goldens pass; < N ms — perf gate)
> Test (filter):   mc model test <acme.yaml> --fixture <name> ✓ (M of 9 goldens skipped as filtered)
> Acme grep:       grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs ✓ (returns 0 — HEADLINE)
> Equivalence:     YAML+CSV path == Rust path on 2,520 coords + 9 goldens ✓
> Determinism:     10 / 10 identical
> Phase 3B snapshot stability: lint_*.json fixtures re-run produce zero diffs ✓
>
> Diagnostic-code registry shipped in this phase:
> - MC1xxx: parse errors (unchanged from Phase 3B)
> - MC2001–MC2010: validation errors (Phase 3A)
> - MC2011: WeightedAverage missing weight (Phase 3B)
> - MC2012–MC2025: NEW — fixture validators (Phase 3C; 14 codes)
> - MC3001–MC3007 + MC3009–MC3011: lint warnings (Phase 3B; unchanged)
> - MC3008: PERMANENTLY RETIRED (Phase 3B; assertion still passes)
> - MC4xxx: reserved
>
> Source manifest:
> - crates/mc-model/src/schema.rs                       (modified — added canonical_inputs + test_fixtures fields)
> - crates/mc-model/src/parse.rs                        (modified — parsing the new schema blocks)
> - crates/mc-model/src/validate.rs                     (modified — 14 new validators)
> - crates/mc-model/src/error.rs                        (modified — new ValidationError variants + codes)
> - crates/mc-model/src/csv.rs                          (NEW — strict CSV parser, ~50 lines)
> - crates/mc-model/src/inputs.rs                       (NEW — canonical_inputs + test_fixtures resolution)
> - crates/mc-model/src/inspect.rs                      (modified — show canonical_inputs / test_fixtures count)
> - crates/mc-model/src/lib.rs                          (modified — exports new public surface)
> - crates/mc-model/examples/acme.yaml                  (modified — added canonical_inputs block)
> - crates/mc-model/examples/acme.inputs.csv            (NEW — 1 header + 2,520 data rows)
> - crates/mc-model/tests/fixture_validation_fixtures/  (NEW dir — 14 negative fixtures)
> - crates/mc-model/tests/equivalence_acme.rs           (NEW — headline equivalence test)
> - crates/mc-model/tests/fixture_validators.rs         (NEW — one test per MC2012-MC2025)
> - crates/mc-model/tests/path_escape.rs                (NEW — MC2022 path-escape rejection)
> - crates/mc-model/tests/perf_gate.rs                  (NEW — mc model test acme.yaml < 500 ms)
> - crates/mc-model/tests/schema_stability.rs           (NEW — Phase 3B snapshot fixtures still parse)
> - crates/mc-model/tests/expected/inspect_acme.txt     (modified — added canonical_inputs line)
> - crates/mc-cli/src/main.rs                           (modified — REMOVED Acme special case; added --fixture flag)
> - docs/reports/phase-3c-completion-report.md          (NEW)
> - docs/CURRENT_STATE.md                               (updated)
> - docs/roadmap/MASTER_PHASE_PLAN.md                   (updated)
>
> Validator coverage (per ADR-0006 Decision 6):
> - MC2012 unknown dimension KEY                  ✓
> - MC2013 unknown element VALUE                  ✓
> - MC2014 fixture references unknown measure     ✓
> - MC2015 fixture writes to derived              ✓
> - MC2016 duplicate fixture names                ✓
> - MC2017 golden references unknown fixture      ✓
> - MC2018 fixture value type mismatch            ✓
> - MC2019 missing required dimension             ✓
> - MC2020 consolidated cell write                ✓ (or dropped if redundant — see deviations)
> - MC2021 NaN value                              ✓ (or dropped if redundant)
> - MC2022 source CSV not found / path-escape    ✓
> - MC2023 CSV row column count mismatch          ✓
> - MC2024 CSV header mismatch                    ✓
> - MC2025 duplicate input coordinate             ✓
>
> Acme migration summary:
> - acme.inputs.csv: 2,520 rows, generated from <method>
> - acme.yaml: canonical_inputs block added (~5 lines)
> - mc-cli: special case removed (~N lines deleted)
> - Equivalence test: 2,520 coords + 9 goldens, all byte-identical
> - Perf gate: mc model test acme.yaml runs in <N ms (target < 500 ms)
>
> Implementation summary:
> - <one paragraph: CSV parser shape; canonical_inputs/test_fixtures resolution; --fixture filter logic; snapshot/rollback approach>
>
> Deviations:
> - <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. Where the Acme canonical inputs come from

`mc_fixtures::write_canonical_inputs(&mut cube, &refs)` is the canonical Rust-side function. Read it in `crates/mc-fixtures/src/lib.rs` before generating the CSV. The function uses closed-form formulas from brief §4.5 to produce 2,520 input cells (1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures).

The CSV generation approach: write a small one-shot Rust binary in `crates/mc-model/examples/` (or use `cargo run --example`) that calls `build_acme_cube` + `write_canonical_inputs` and dumps the resulting input cells to CSV in the strict subset format. Run it once, commit `acme.inputs.csv`, delete the binary. This guarantees the CSV contents match the Rust formula bit-for-bit.

Alternative: hand-derive the CSV from brief §4.5's formulas. Riskier (transcription errors). Use the dump-from-Rust approach unless there's a strong reason not to.

### B. The CSV strict subset implementation pattern

```rust
// Pseudocode — the actual implementation lives in crates/mc-model/src/csv.rs
fn parse_strict_csv(content: &str, expected_columns: &[String]) -> Result<Vec<Vec<String>>, ParseError> {
    let mut lines = content.split('\n').enumerate();

    // Header row (line 1)
    let (line_no, header_line) = lines.next().ok_or(ParseError::EmptyFile)?;
    let header: Vec<&str> = header_line.split(',').collect();
    if header.len() != expected_columns.len() {
        return Err(ParseError::HeaderColumnCount { ... });
    }
    for (h, e) in header.iter().zip(expected_columns) {
        if h.trim() != h || h != e {
            return Err(ParseError::HeaderMismatch { ... });
        }
    }

    // Data rows
    let mut rows = Vec::new();
    for (line_no, line) in lines {
        if line.is_empty() && line_no == content_line_count - 1 {
            break;  // tolerate trailing newline
        }
        if line.is_empty() {
            return Err(ParseError::EmptyRow { line_no });
        }
        if line.contains('"') {
            return Err(ParseError::QuotedField { line_no });
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != expected_columns.len() {
            return Err(ParseError::RowColumnCount { line_no, ... });
        }
        rows.push(fields.into_iter().map(String::from).collect());
    }

    Ok(rows)
}
```

The above is illustrative — the actual code matches the validator codes (MC2023 row column count, MC2024 header mismatch, etc.).

### C. The snapshot/rollback pattern for between-goldens

```rust
// Pseudocode — the actual implementation lives in crates/mc-model/src/inputs.rs
// or wherever mc model test's flow is implemented.
fn run_goldens(cube: &mut Cube, goldens: &[Golden], fixtures: &HashMap<String, Fixture>) -> Vec<GoldenResult> {
    // Cube is already populated with canonical_inputs at this point.
    let snap = cube.snapshot();  // ~30 µs at Acme scale (PERF.md §6.9)
    let mut results = Vec::new();
    for golden in goldens {
        if let Some(fixture_name) = &golden.fixture {
            let fixture = &fixtures[fixture_name];
            apply_fixture(cube, fixture);  // override semantic
        }
        let result = check_golden(cube, golden);
        results.push(result);
        cube.rollback_to(&snap);  // ~75 µs at Acme scale
    }
    results
}
```

If `Cube::snapshot` / `Cube::rollback_to` has a gotcha, fall back to the Phase 3B-style "reload canonical_inputs between every golden" pattern, but ONLY for goldens that reference fixtures (read-only goldens skip the reset).

### D. The canonical-coord iterator question

The headline equivalence test needs to enumerate 2,520 canonical input coords and read each from both cubes. If `mc-fixtures` already exports a public iterator (check `pub fn canonical_input_coords` or similar), use it. If not, the Phase 3C scope item 8 + ADR-0006 Decision 5 allow ONE pure read-only helper to be added to `mc-fixtures` for this case.

The default is "enumerate inline":

```rust
// In crates/mc-model/tests/equivalence_acme.rs
let (mut cube_rust, refs) = mc_fixtures::build_acme_cube().expect("build");
mc_fixtures::write_canonical_inputs(&mut cube_rust, &refs).expect("write");

let cube_yaml = mc_model::load("crates/mc-model/examples/acme.yaml")
    .expect("load yaml");

// 2,520 coords = 1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures
for &scenario in &[refs.scen_baseline] {
    for &version in &[refs.ver_working] {
        for &time in &refs.months {  // 12
            for &channel in &refs.leaf_channels {  // 5
                for &market in &refs.cities {  // 7
                    for &measure in &refs.input_measures {  // 6
                        let coord = CellCoordinate::new(/* 6-tuple */);
                        let val_a = cube_rust.read(&coord, refs.root_principal).expect("read a");
                        let val_b = cube_yaml.read(&coord, /* root from yaml */).expect("read b");
                        assert_eq!(val_a.value.as_f64(), val_b.value.as_f64(),
                            "mismatch at {coord:?}");
                    }
                }
            }
        }
    }
}
```

The exact API names (`refs.months`, `refs.leaf_channels`, etc.) need to match what `mc-fixtures` actually exports as of `phase-3b-lint-and-diagnostics`. Read `crates/mc-fixtures/src/lib.rs` to confirm.

### E. The path-escape rejection implementation

```rust
// In crates/mc-model/src/inputs.rs (or wherever CSV path resolution happens)
fn resolve_csv_path(yaml_path: &Path, source: &str) -> Result<PathBuf, ValidationError> {
    let yaml_dir = yaml_path.parent().ok_or(...)?;
    let resolved = yaml_dir.join(source);
    let canonical = resolved.canonicalize().map_err(|_| ValidationError::SourceNotFound { ... })?;
    let canonical_yaml_dir = yaml_dir.canonicalize().map_err(...)?;

    if !canonical.starts_with(&canonical_yaml_dir) {
        return Err(ValidationError::SourceNotFound {
            // path-escape variant
            source_path: source.to_string(),
            reason: "path resolves outside the YAML model file's directory tree",
        });
    }

    Ok(canonical)
}
```

The negative test: a fixture YAML with `source: "../escape.csv"` triggers MC2022 with a path-escape message.

### F. JSON envelope stability assertion

The Phase 3B snapshot fixtures live at `crates/mc-model/tests/expected/lint_*.json`. Phase 3C must NOT modify the `Diagnostic` struct shape (no new fields, no renamed fields, no removed fields). The stability assertion:

```rust
// In crates/mc-model/tests/schema_stability.rs (new file)
#[test]
fn phase_3b_lint_snapshots_still_parse_at_phase_3c() {
    for fixture_path in glob("tests/expected/lint_*.json") {
        let content = std::fs::read_to_string(&fixture_path).expect("read");
        // Parse the JSON envelope using the current Diagnostic shape.
        // If any field is missing or renamed, this fails.
        let envelope: DiagnosticEnvelope = parse_json(&content).expect(&format!(
            "Phase 3B snapshot {} no longer parses with current Diagnostic shape — \
             schema_version must bump per ADR-0006 acceptance amendment #20",
            fixture_path.display()
        ));
        assert_eq!(envelope.schema_version, "1.0",
            "schema_version drifted from 1.0 — would break Phase 4 / 6 consumers");
    }
}
```

If this test fails, the Phase 3C implementer either rolled back the Diagnostic-shape change OR bumped `schema_version` to `"1.1"` (with documentation in the completion report explaining why). The default is "don't change the shape."

### G. What Phase 4 / 5 / 6 will consume from Phase 3C

- **Phase 4 (LLM authoring):** the LLM emits YAML models with `canonical_inputs:` and `test_fixtures:` blocks. The 14 new validators tell the LLM what's wrong with structured codes (e.g., MC2013 = "you used 'Mar2026' but the dim has 'Mar_2026'"). The LLM iterates against MC2012–MC2025 errors the same way it iterates against MC2001–MC2011 from Phase 3A.
- **Phase 5 (data integration):** real-world actuals are loaded via a separate `actuals_sources:` schema (not Phase 3C). But the cube-state-equivalence test pattern from Phase 3C (Rust path vs YAML+CSV path) is the same shape Phase 5 uses (Rust ingest path vs API-fed ingest path).
- **Phase 6 (UI editor):** the editor's "test fixtures" panel renders `test_fixtures:` entries. The new `Diagnostic` codes from Phase 3C show up in the editor gutter the same way Phase 3B's lint codes do.

Phase 3C's design choices ripple into all three. Get the schema right.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| Schema additions | [`crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) | modify — add `canonical_inputs: Option<...>` and `test_fixtures: Vec<...>` to `ParsedModel` and `ValidatedModel` |
| Parsing the new schema | [`crates/mc-model/src/parse.rs`](../../crates/mc-model/src/parse.rs) | modify — deserialize the new blocks |
| 14 new validators | [`crates/mc-model/src/validate.rs`](../../crates/mc-model/src/validate.rs) | modify — add MC2012–MC2025 |
| Error types + diagnostic codes | [`crates/mc-model/src/error.rs`](../../crates/mc-model/src/error.rs) | modify — new `ValidationError` variants + `code()` mapping |
| Strict CSV parser | `crates/mc-model/src/csv.rs` | new — ~50 lines, hand-rolled |
| Canonical/fixture resolution + path-escape | `crates/mc-model/src/inputs.rs` | new — load canonical_inputs, resolve fixture references, apply to cube via existing public APIs |
| Inspect summary update | [`crates/mc-model/src/inspect.rs`](../../crates/mc-model/src/inspect.rs) | modify — add "Canonical inputs: N cells from path" + "Test fixtures: N named" lines |
| Public API surface | [`crates/mc-model/src/lib.rs`](../../crates/mc-model/src/lib.rs) | modify — export new types as needed |
| The Acme YAML | [`crates/mc-model/examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | modify — add `canonical_inputs:` block (~5 lines) |
| Acme input data | `crates/mc-model/examples/acme.inputs.csv` | new — 1 header + 2,520 data rows; generate via one-shot Rust binary |
| Headline equivalence test | `crates/mc-model/tests/equivalence_acme.rs` | new — Rust path vs YAML+CSV path on 2,520 coords + 9 goldens |
| 14 negative fixtures | `crates/mc-model/tests/fixture_validation_fixtures/` | new dir — one fixture per MC2012–MC2025 |
| Per-rule validator tests | `crates/mc-model/tests/fixture_validators.rs` | new — one test per validator |
| Path-escape test | `crates/mc-model/tests/path_escape.rs` | new — MC2022 path-escape rejection |
| Perf gate | `crates/mc-model/tests/perf_gate.rs` | new — `mc model test acme.yaml` < 500 ms |
| Stability assertion | `crates/mc-model/tests/schema_stability.rs` | new — Phase 3B's lint_*.json fixtures still parse |
| CLI subcommand routing | [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) | modify — REMOVE Acme special case at line ~253; add `--fixture <name>` flag to `model test` |
| Snapshot fixture for inspect | [`crates/mc-model/tests/expected/inspect_acme.txt`](../../crates/mc-model/tests/expected/inspect_acme.txt) | modify — add `Canonical inputs:` line to expected output |
| Phase 3C completion report | `docs/reports/phase-3c-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 3C from `proposed` → `complete` |

**Do not touch:**

- **`crates/mc-core/`** — entire crate locked. Source, tests, benches, Cargo.toml, all of it. `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` returns zero lines.
- **`crates/mc-fixtures/src/`** — locked, with at most ONE new public read-only helper allowed if and only if the equivalence test requires a canonical-coord iterator that doesn't already exist. If any other change is needed, **stop and SPEC QUESTION**.
- **`crates/mc-fixtures/Cargo.toml`** — no new deps.
- **`docs/specs/`** — locked. Brief and engine-semantics doc are contracts.
- **`docs/decisions/0004-*`, `0005-*`, `0006-*`** — Accepted; amendments go in `0006-amendment-N.md`, not in the originals.
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock` (existing pins)** — `clap`, `clap_lex`, `half` from Phase 1B + `indexmap`, `hashbrown` from Phase 3A all stay.
- **PERF.md** — Phase 3C doesn't touch performance documentation. The kernel didn't change; benches don't need to be re-run.
- **`crates/mc-model/examples/acme.yaml` STRUCTURE** — only the new `canonical_inputs:` block may be added; no changes to dimensions, hierarchies, measures (other than what Phase 3B already added), rule bodies, descriptions, or golden test values. The structural-equivalence test against `build_acme_cube()` is your guardrail.
- **The `Diagnostic` struct shape** — adding fields requires bumping `schema_version` per amendment #20. Phase 3C adds codes only; the struct shape is locked.
- **MC3008** — permanently retired. Do not reuse the code under any circumstances.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-3C gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                                  # 293 / 0 (Phase 3B's count)
cargo run --release --bin mc -- demo                                    # matches brief §4.6 (Rust path)
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml   # YAML path
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml     # zero warnings

# Acme demo-equivalence diff — must remain empty throughout
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)
# expected: zero output

# Iteration loop during Phase 3C development:
cargo build -p mc-model
cargo test -p mc-model
cargo test -p mc-model -- equivalence       # the headline equivalence test
cargo test -p mc-model -- fixture_validators # the 14 per-rule tests
cargo test -p mc-model -- path_escape       # MC2022 path-escape
cargo test -p mc-model -- perf_gate         # mc model test acme.yaml < 500 ms
cargo test -p mc-model -- schema_stability  # Phase 3B fixtures still parse

# CLI smoke (after acme.inputs.csv + canonical_inputs block + special-case removal):
cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml
# expected: 9/9 goldens pass, exit 0, < 500 ms

cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml --fixture <name>
# expected: subset of goldens run, others reported as filtered-skipped

# Acceptance-gate verification:
grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs
# expected: 0

# Determinism gate (10 runs, identical pass/fail):
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Verify locked surfaces:
git diff phase-3b-lint-and-diagnostics -- crates/mc-core/
# expected: zero output

git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/ crates/mc-fixtures/Cargo.toml
# expected: zero output (or ≤ 1 helper if canonical-coord iterator was added)

# Verify mc-core/mc-fixtures Cargo.tomls are unchanged:
git diff phase-3b-lint-and-diagnostics -- crates/mc-core/Cargo.toml crates/mc-fixtures/Cargo.toml
# expected: zero output
```

---

## Final checklist before you call Phase 3C done

- [ ] `crates/mc-model/examples/acme.inputs.csv` exists with 1 header + 2,520 data rows.
- [ ] `crates/mc-model/examples/acme.yaml` has `canonical_inputs: { source: "acme.inputs.csv", columns: [...] }` block.
- [ ] `crates/mc-cli/src/main.rs` Acme-name special case REMOVED. `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returns 0.
- [ ] `mc model test crates/mc-model/examples/acme.yaml` exits 0 with 9/9 goldens passing — using ONLY generic fixture-loading path.
- [ ] Headline equivalence test passes: Rust path vs YAML+CSV path byte-identical across 2,520 canonical input coords + all 9 goldens.
- [ ] Equivalence test uses ONLY existing public APIs from `mc-core` + `mc-fixtures` (zero new APIs OR exactly one read-only canonical-coord iterator helper added to `mc-fixtures`, justified in completion report).
- [ ] `mc model test --fixture <name>` filters to only goldens explicitly referencing that fixture; reports skipped count.
- [ ] 14 new validators (MC2012–MC2025) implemented; one negative-test fixture per validator under `tests/fixture_validation_fixtures/`; per-rule test asserts each rule fires + no spurious other-rule firings.
- [ ] CSV parser is hand-rolled, ~50 lines, no `csv` crate dep. Strict subset enforced: UTF-8, required header, comma-separated, no quoted fields / embedded commas / embedded newlines / comments, numeric value column.
- [ ] CSV path resolution: relative to YAML directory; `../` escapes rejected with MC2022 (or MC2026); negative test fixture covers this.
- [ ] CSV columns contract pinned: header byte-exact match `columns:` (MC2024 fires on mismatch); value type derived from row's Measure declaration (MC2018 fires on type mismatch); NaN values rejected (MC2021).
- [ ] Snapshot/rollback used for between-goldens reset; `mc model test acme.yaml` completes < 500 ms (stretch < 200 ms).
- [ ] OR fallback documented: "only reset between goldens that reference fixtures" if snapshot/rollback gotcha surfaces.
- [ ] `mc model lint crates/mc-model/examples/acme.yaml` still exits 0 with zero warnings (Phase 3B carry-forward).
- [ ] `mc demo --model crates/mc-model/examples/acme.yaml` still byte-identical to `mc demo` (Phase 3A carry-forward).
- [ ] JSON envelope `schema_version` stays at `"1.0"`. Phase 3B's `tests/expected/lint_*.json` fixtures still parse cleanly under current Diagnostic shape (stability assertion test passes).
- [ ] MC3008 permanently retired (no new lint reuses the code).
- [ ] All 293 existing tests still pass; new total ≥ 293 + (Phase 3C test count).
- [ ] **`mc-core` Cargo.toml + src/ unchanged.**
- [ ] **`mc-fixtures` src/ unchanged** (or ≤ 1 helper added). `mc-fixtures` Cargo.toml unchanged.
- [ ] `crates/mc-fixtures/build_acme_cube` and `write_canonical_inputs` byte-for-byte unchanged.
- [ ] `rust-toolchain.toml` not bumped — still Rust 1.78.
- [ ] `Cargo.lock` Phase 1B + Phase 3A pins intact.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-model/src/` (test/example/CLI exempt where the existing carve-out applies).
- [ ] No `unsafe` anywhere.
- [ ] No `async` / `tokio` / `rayon` / threads anywhere.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] Completion report at `docs/reports/phase-3c-completion-report.md` written from template, including diagnostic-code registry update + the equivalence-test methodology + the Acme-CSV generation method + perf-gate measurement.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 3C from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 3D (formula syntax), Phase 4 (LLM), Phase 5 (actuals), or Phase 6 (UI).**

If you are uncertain at any point, the resolution order is:

1. The Phase 3C prompt above.
2. **[ADR-0006](../decisions/0006-phase-3c-model-test-fixtures.md) — the binding strategic contract (with all 13 acceptance amendments).**
3. [ADR-0005](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) — the inherited diagnostics contract (Phase 3C extends but does not modify the `Diagnostic` shape).
4. [ADR-0004](../decisions/0004-phase-3a-model-definition-format.md) — the inherited model-format contract.
5. The brief and `engine-semantics.md` for kernel-side semantics.
6. Phase 3B completion report (recent context).
7. Earlier completion reports (1A / 1B / 2A / 2B / 2C / 2D / 3A).
8. `CLAUDE.md`.
9. `docs/roadmap/MASTER_PHASE_PLAN.md`.
10. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 3A / 3B)

**Read ADR-0006 (with its 13 acceptance amendments) before you write any code.** The amendments are the contract, not suggestions. Anything in the prompt above is a derivation; if a derivation seems to contradict the ADR or its amendments, the ADR wins and the prompt is buggy — surface it.

**Source-bounded, but the bound is `crates/mc-model/` + `crates/mc-cli/`.** Phase 3C doesn't change the kernel, doesn't change the fixtures (modulo the one read-only helper), doesn't change the model schema's structural shape (only adds two optional top-level blocks).

**The acceptance gate is the empty-grep + the equivalence test.** `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returning 0 AND the YAML+CSV-vs-Rust equivalence test passing on 2,520 coords + 9 goldens. Everything else is supporting evidence. If the grep returns non-zero or the equivalence test fails, you don't ship.

**The Diagnostic shape is locked.** Adding codes is fine (and expected — 14 new ones). Adding a new field requires a `schema_version` bump to `"1.1"`, which is its own SPEC QUESTION.

**MC3008 is forever retired.** New codes go to MC2026+ (validation) or MC3012+ (lint, but no new lints in Phase 3C).

**Hand-rolled wins.** No `csv` crate. No `serde_json` (carry-forward from Phase 3B). No new transitive deps. The strict CSV subset is small enough to write directly.

**A bench is a contract — but Phase 3C's only perf claim is the `mc model test acme.yaml < 500 ms` gate.** Measure it; report the actual wall-clock; tighten the gate if you have headroom. Beyond that, no new benches.

**Do not pick the next phase.** Phase 3C's deliverable is the fixture/input layer + Acme migration. If the work surfaces opportunities for Phase 3D (friendly formulas), Phase 4 (LLM), Phase 5 (actuals), or Phase 6 (UI), note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 3C handoff drafted 2026-05-03 immediately after [ADR-0006](../decisions/0006-phase-3c-model-test-fixtures.md) was Accepted with 13 project-owner acceptance amendments (9 from GPT review, 4 from Claude Desktop review, plus 1 wording-tightening note). The handoff is the build contract; the ADR is the strategic context behind it.*
