# Phase 3B Handoff — Model QA, Linter, and Diagnostics

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 3B.
> **You inherit a green Phase 3A** (commit `603c537`, tag
> `phase-3a-model-definition-layer`).
>
> **This is a READ-ONLY phase over `mc-model`.** Phase 3B adds a
> diagnostics + lint + inspection layer plus four new CLI subcommands,
> plus one validator promotion from lint to hard error (MC2011). The
> kernel is NOT modified. The fixtures crate is NOT modified. The
> Phase 3A model schema is NOT modified beyond minimal cleanup needed
> to satisfy the lint-clean-Acme acceptance gate.
>
> **Hard rule:** Phase 3B touches `crates/mc-model/` (lint module, new
> validator, CLI plumbing for the four subcommands, lint fixtures), and
> `crates/mc-cli/` (subcommand routing for `mc model {validate,
> inspect, lint, test}`). It does NOT touch `crates/mc-core/`,
> `crates/mc-fixtures/`, `docs/specs/`, or any kernel/fixture file.
> Phase 3A's previous-phase precedent ("source-bounded to mc-model")
> is the operating model.

---

## Where Phase 3A ended

- **Phase 3A commit / tag:** `603c537` — *phase-3a: model definition layer (mc-model crate; YAML → Cube)* — tag `phase-3a-model-definition-layer`. Backfill commit at `a41ce77`.
- **Test status:** 252 / 0 passing across all targets. 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6. `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` produces byte-for-byte identical output.
- **Gates green:** build / fmt / clippy / test / both demos.
- **Toolchain:** Rust 1.78 pinned. Cargo.lock pins from Phase 1B (`clap`, `clap_lex`, `half`) + Phase 3A (`indexmap → 2.7.0`, `hashbrown → 0.15.5`). Do not bump.
- **`mc-core`, `mc-fixtures` deps unchanged** since Phase 2D (mc-core) and Phase 1A (mc-fixtures).

For the full Phase 3A audit see [`../reports/phase-3a-completion-report.md`](../reports/phase-3a-completion-report.md). For the binding strategic context for THIS phase, read [`../decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) **before this handoff** — the ADR has 15 acceptance amendments that constitute the contract; this handoff is the build instructions.

---

## Phase 3B prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 3B: Model QA, Linter, and Diagnostics.
>
> **Context.** Phase 3A shipped the YAML → Cube pipeline. `mc-model` can load Acme; `mc demo --model acme.yaml` produces byte-for-byte identical output to the Rust path. What's missing: (1) a way to *inspect* a model at-a-glance; (2) a *quality signal* beyond "is it buildable?"; (3) a *stable diagnostic vocabulary* for Phase 4 LLM consumption + Phase 6 UI consumption; (4) a *CLI surface* for any of the above. Phase 3B fills these four gaps without touching the kernel.
>
> **Goal.** Ship `mc-model`'s diagnostics + lint surface such that:
>
> 1. `mc model validate <path>` parses + validates a YAML model and exits 0 on success or non-zero with structured errors.
> 2. `mc model inspect <path>` prints a one-screen structured summary of the model (dim count, element counts, hierarchy summaries, measure breakdown by role, rule count + longest chain depth, golden test count, cardinality, diagnostics summary).
> 3. `mc model lint <path>` runs 10 lint rules (MC3001–MC3007 + MC3009–MC3011) and prints findings. Exit 0 unless `--deny-warnings` is set.
> 4. `mc model test <path>` parses + validates + compiles + executes inline `golden_tests:` block, exits 0 only if every golden passes.
> 5. All four commands accept `--format text|json`. JSON output is wrapped in `{ "schema_version": "1.0", "diagnostics": [...] }` (or test-results envelope for `mc model test`).
> 6. The Acme YAML at `crates/mc-model/examples/acme.yaml` lints clean with **ZERO** warnings (per ADR-0005 Decision 8 + acceptance amendment #15 — escape hatch closed; no documented exceptions allowed). Any cleanup needed to clear all 10 lints is in scope; the demo-equivalence diff with `build_acme_cube()` must remain empty after the cleanup.
> 7. One validator promoted from lint to hard error per ADR-0005 amendment #4: MC2011 (weighted-average measure missing weight). Lives in the validator, blocks `mc_model::load()`, surfaces with structured `ValidationError` shape.
>
> **Phase 3B scope** (binding contract — read [`../decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) for full strategic rationale; this scope IS what the ADR's 9 Decisions + 15 acceptance amendments commit to):
>
> 1. **Add the `lint` module to `mc-model`** at `crates/mc-model/src/lint.rs` (or `lint/` directory if multi-file is cleaner). Public API: `mc_model::lint(model: &ValidatedModel) -> Vec<Diagnostic>`. The function returns the full diagnostic list sorted per the deterministic emission order (Decision 7 + amendment #14). It does NOT take any options; lint is run-or-don't, not configurable mid-flight.
>
> 2. **Implement the 10 starting lint rules** per ADR-0005 Decision 5's table:
>    - MC3001 — missing description on dimension (warning)
>    - MC3002 — missing description on measure (warning)
>    - MC3003 — missing description on rule (warning)
>    - MC3004 — model has no golden tests (warning)
>    - MC3005 — orphan element not in default hierarchy (warning)
>    - MC3006 — long rule chain depth ≥ 5 (info; framed as model-complexity primary, performance secondary per amendment #8)
>    - MC3007 — ratio measure using `Sum` aggregation (warning; name-based heuristic with caveat in suggestion text)
>    - **MC3008 — RETIRED.** Do NOT implement a lint with this code. Per amendment #11 the slot is permanently vacant. Add a test asserting no active lint emits `"MC3008"`.
>    - MC3009 — unused input measure (info)
>    - MC3010 — unused derived measure (info)
>    - MC3011 — hierarchy root ambiguity (warning)
>
>    Each rule is a function taking `&ValidatedModel` and returning `Vec<Diagnostic>`. The lint module's top-level `lint()` calls each rule, concatenates results, and applies the deterministic sort.
>
> 3. **Promote MC2011 (weighted-average missing weight) into the validator** per amendment #4. Add to `crates/mc-model/src/validate.rs` (or wherever the existing 10 ADR-0004 validators live). Code: `MC2011`. Severity: error. Behavior: blocks `mc_model::load()` with a `ValidationError` whose code field is `"MC2011"`. Add a negative-test fixture proving load fails with this code when a `WeightedAverage` measure is missing `weight_measure:`.
>
> 4. **Implement the `Diagnostic` type + envelope** per ADR-0005 Decision 7:
>    - `Diagnostic { code, severity, path: ModelPath, message, suggestion: Option<String> }`
>    - `Severity { Error, Warning, Info }`
>    - `ModelPath { file, span: Option<Span>, yaml_pointer, model_path }`
>    - `DiagnosticCode` is a stable string type; codes documented in a registry module (in-source comment table is sufficient for Phase 3B).
>    - JSON envelope: `{ "schema_version": "1.0", "diagnostics": [Diagnostic, ...] }`. The `schema_version` field is mandatory and unconditional, including in empty-diagnostic cases.
>    - **Deterministic emission order** (binding contract per amendment #14): sort by `(severity desc, code asc, yaml_pointer asc, message asc)` BEFORE the formatter runs. Apply to text format AND JSON format. Apply to library API output (the `Vec<Diagnostic>` returned from `lint()` is pre-sorted).
>
> 5. **Add four CLI subcommands** to `crates/mc-cli/src/main.rs` under a `mc model` group:
>    - `mc model validate <path>` — parse + validate; exit 0 / non-zero
>    - `mc model inspect <path>` — parse + validate + summary print (Decision 4); exit 0 / non-zero
>    - `mc model lint <path>` — parse + validate + lint; exit 0 (or non-zero with `--deny-warnings`)
>    - `mc model test <path>` — parse + validate + compile + run goldens; exit 0 only if all goldens pass
>    - All four accept `--format text|json` (default text)
>    - `mc model lint` uniquely accepts `--deny-warnings` (no other subcommand has it)
>
> 6. **Update `mc demo --model` to NOT run goldens** per amendment #12. Demo loads, validates, runs the cube, prints brief §4.6 output, exits. Goldens are exclusively `mc model test`'s responsibility. Add an integration test asserting `mc demo --model <fixture-with-bad-goldens>.yaml` exits 0 even when the model's golden tests would fail (because `mc demo` doesn't run them). If `mc demo` was previously running goldens (which it shouldn't have been per Phase 3A scope but verify), remove that.
>
> 7. **Lint fixtures.** Create `crates/mc-model/tests/lint_fixtures/` with one minimal YAML file per lint rule:
>    - `MC3001_missing_dim_description.yaml`
>    - `MC3002_missing_measure_description.yaml`
>    - `MC3003_missing_rule_description.yaml`
>    - `MC3004_no_golden_tests.yaml`
>    - `MC3005_orphan_element.yaml`
>    - `MC3006_long_rule_chain.yaml`
>    - `MC3007_ratio_with_sum.yaml`
>    - (No `MC3008` fixture — code is retired)
>    - `MC3009_unused_input_measure.yaml`
>    - `MC3010_unused_derived_measure.yaml`
>    - `MC3011_hierarchy_root_ambiguity.yaml`
>    - Plus `MC2011_weighted_average_missing_weight.yaml` (validation error fixture)
>
>    Each fixture is the **smallest** model that triggers exactly its rule. Each fixture has a paired test asserting (a) its rule fires, (b) no other rule fires spuriously.
>
> 8. **Snapshot tests for CLI output** per amendment #7: hand-rolled fixture comparison preferred over `insta`. Implement a small `assert_snapshot(actual: &str, fixture_path: &Path)` helper in `crates/mc-model/tests/`. Snapshot fixtures live under `crates/mc-model/tests/expected/<test-name>.txt` (text) or `<test-name>.json` (JSON). Lock the text-format output of `mc model inspect crates/mc-model/examples/acme.yaml` and `mc model lint <each-lint-fixture>`. **Do NOT pull in `insta` unless** you can prove it builds cleanly on Rust 1.78 with zero transitive churn AND adding it produces meaningfully better tests; otherwise stick with the hand-rolled approach.
>
> 9. **Update Acme YAML to lint clean** per amendment #15. Add `description:` fields to every dimension, measure, and rule. Verify all 10 lints clear. The structural-equivalence test against `build_acme_cube()` and the demo-equivalence diff MUST remain green after the cleanup. Acme's role is now both "the canonical YAML example" AND "the gold standard for model quality." If any lint trap demonstration is desired (e.g., MC3007's ratio-Sum trap), put it in `tests/lint_fixtures/`, NOT in `examples/acme.yaml`.
>
> **Hard rules:**
>
> - **`crates/mc-core/` is LOCKED.** No source change, no Cargo.toml change, no anything.
> - **`crates/mc-fixtures/` is LOCKED.** `build_acme_cube()` byte-for-byte unchanged. `Cargo.toml` unchanged.
> - **`crates/mc-model/examples/acme.yaml` may only change to add `description:` fields and any other minimal cleanup needed for gate #2 (Acme lints clean).** Do NOT change dimension structure, hierarchy edges, measure metadata (other than adding descriptions), rule bodies, or golden test values. The structural-equivalence test against `build_acme_cube()` is your guardrail.
> - **`mc_model::load()` IGNORES lint output entirely** (binding contract per ADR-0005 Decision 2). Lint runs through `mc_model::lint(&ValidatedModel) -> Vec<Diagnostic>`. The two paths are decoupled at the library boundary. Do NOT add a `lint_on_load: bool` flag or any similar coupling.
> - **`mc demo --model` does NOT run goldens** (per amendment #12). Demo's job: load + validate + run + print + exit. Test's job: load + validate + compile + check goldens. Don't merge the two.
> - **JSON envelope `{ "schema_version": "1.0", "diagnostics": [...] }` is mandatory** even in empty-diagnostic cases (per amendment #13). The version is `"1.0"` for Phase 3B; bump only on breaking diagnostic shape changes.
> - **Deterministic emission order** (per amendment #14): sort by `(severity desc, code asc, yaml_pointer asc, message asc)` before any formatter runs. Apply uniformly across text, JSON, library APIs.
> - **MC3008 retirement is permanent** (per amendment #11). The slot is vacant. A test asserts no active lint emits `"MC3008"`. Do NOT renumber MC3009/3010/3011.
> - **Acme lints clean with ZERO documented warnings** (per amendment #15). Escape hatch closed.
> - **Toolchain stays at Rust 1.78.** No `cargo update`. No new dep that requires `edition2024`. Hand-rolled snapshot fixtures over `insta` unless `insta` is proven 1.78-clean (and even then it's a workspace dev-dep, never in `mc-core`).
> - **No `unsafe`, no `async`, no `tokio`, no `rayon`, no threads.** Phase 3B is sync.
> - **All 252 existing tests must still pass.** New total ≥ 252 + (Phase 3B test count).
>
> **Acceptance gate (the headline + supporting):**
>
> Headline: **`mc model lint crates/mc-model/examples/acme.yaml` exits 0 with zero warnings (no `--allow` flags, no documented exceptions).**
>
> Plus all 15 success-gate items from ADR-0005 Decision 8:
>
> 1. Acme validates clean (`mc model validate` exits 0).
> 2. **Acme lints clean — ZERO warnings.** [Headline.]
> 3. Each lint rule has a triggering fixture under `tests/lint_fixtures/`; each fixture's test asserts that rule fires + no other fires spuriously.
> 4. MC3008-retired assertion: no active lint emits code `"MC3008"`.
> 5. MC2011 blocks loading: a fixture with a WeightedAverage measure missing weight causes `load()` to return Err with code `"MC2011"`.
> 6. CLI text output snapshot-locked via hand-rolled fixture comparison.
> 7. JSON envelope schema_version assertion: a JSON fixture asserts `schema_version: "1.0"` is present.
> 8. Deterministic emission test: ≥ 3-diagnostic fixture asserts byte-exact output across 10 runs.
> 9. `mc demo --model <bad-goldens>.yaml` exits 0 (demo doesn't run goldens).
> 10. All 252 existing tests still pass; new total ≥ 252 + new Phase 3B tests.
> 11. `mc-core` untouched (`git diff phase-3a-model-definition-layer -- crates/mc-core/` returns zero lines).
> 12. `mc-fixtures` untouched.
> 13. 10 consecutive `cargo test --workspace -q` runs identical.
> 14. All four CLI commands work end-to-end on Acme + ≥ 1 negative fixture each.
> 15. `mc model test crates/mc-model/examples/acme.yaml` exits 0 with all 9 inline goldens passing.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (≥ 252 + new Phase 3B tests)
> - `cargo run --release --bin mc -- demo` (matches brief §4.6 — Rust path)
> - `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` (matches brief §4.6 — YAML path; demo-equivalence diff still empty)
> - `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` (exits 0)
> - `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` (exits 0; output snapshot-locked)
> - `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` (exits 0 with zero warnings — the headline)
> - `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` (exits 0; all goldens pass)
> - 10 consecutive `cargo test --workspace -q` (deterministic)
> - `git diff phase-3a-model-definition-layer -- crates/mc-core/ crates/mc-fixtures/` (zero lines)
>
> **Documentation requirements:**
> - Append `docs/reports/phase-3b-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 3B from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 3B status row.
> - Document the diagnostic-code registry (MC1xxx parse, MC2001–MC2011 validation incl. new MC2011, MC3xxx lint with MC3008 retired) in the completion report.
> - **Do NOT modify [ADR-0005](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md).** It's Accepted; amendments go in `0005-amendment-N.md`.
> - **Do NOT modify the brief, engine-semantics doc, or ADR-0004.** They're contracts.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
> 1. Acme's `description:`-only cleanup somehow breaks the structural-equivalence test against `build_acme_cube()` or the demo-equivalence diff. (Shouldn't happen — descriptions are metadata-only — but stop and surface if it does.)
> 2. A lint rule's "best implementation" requires touching `mc-core` or `mc-fixtures`. (Per Decision 6, neither is in scope.)
> 3. The `insta` dep would meaningfully improve snapshot tests AND you can prove it builds 1.78-clean. (Per amendment #7, hand-rolled is preferred; `insta` is escape-hatch only.)
> 4. The deterministic-emission sort (Decision 7 + amendment #14) doesn't have a unique total ordering for some pair of diagnostics, leading to test flakes. (Tiebreaker may need revision; surface it before silently changing the spec.)
> 5. A measure name appears to fall in MC3007's ratio-detection regex but is genuinely a Sum measure (e.g., a hypothetical `customer_score_rate` that legitimately sums). False positives are expected (the suggestion text says "verify"); but if Acme itself trips a false positive, surface it before suppressing.
> 6. The completion report's diagnostic-code registry conflicts with what's actually emitted at runtime.
>
> **Rollback plan (in case complexity explodes):**
>
> If lint module size balloons beyond ~1500 lines or any single rule requires non-trivial AST-walking infrastructure that doesn't fit cleanly into `&ValidatedModel`, **stop and write a SPEC QUESTION**. Two recovery paths:
> 1. **Narrow the rule set for Phase 3B.1**: ship a minimum-viable subset (parse + validate + 5 most-load-bearing lints + the 4 CLI commands), defer the other 5 lints to a follow-up phase. Requires ADR-amendment.
> 2. **Reconsider the diagnostic shape**: if the `Diagnostic` struct is the bottleneck (e.g., span propagation requires invasive parser changes), simplify to a thinner shape with file-only paths. Requires SPEC QUESTION.
>
> Either fallback is a Phase 3B.1 amendment, not a Phase 3B scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 3B Model QA, Linter, and Diagnostics
>
> Build:    cargo build --release --workspace ✓
> Format:   cargo fmt --check --all ✓
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓
> Tests:    cargo test --workspace [N] / 0 (was 252 / 0)
> Demo (Rust):  cargo run --release --bin mc -- demo ✓
> Demo (YAML):  cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml ✓
> Demo equivalence: diff <(...) <(...) ✓ EMPTY (carryover from Phase 3A)
> Validate:  cargo run --release --bin mc -- model validate <acme.yaml> ✓
> Inspect:   cargo run --release --bin mc -- model inspect <acme.yaml> ✓ (snapshot-locked)
> Lint:      cargo run --release --bin mc -- model lint <acme.yaml> ✓ (ZERO warnings — headline)
> Test:      cargo run --release --bin mc -- model test <acme.yaml> ✓ (all 9 goldens pass)
> Determinism: 10/10 identical
>
> Diagnostic-code registry shipped in this phase:
> - MC1xxx: parse errors (N codes assigned at handoff time)
> - MC2001–MC2010: validation errors (Phase 3A's Decision-6 ten validators)
> - MC2011 (NEW): weighted-average missing weight (promoted from lint per amendment #4)
> - MC3001–MC3007: lint warnings (descriptions, golden tests, orphan element, chain depth, ratio-Sum)
> - MC3008: PERMANENTLY RETIRED (assertion test passes; promoted to MC2011)
> - MC3009–MC3011: lint warnings (unused measures, hierarchy root)
> - MC4xxx: reserved
>
> Source manifest:
> - crates/mc-model/src/lint.rs                         (new — N lines, 10 rules)
> - crates/mc-model/src/validate.rs                     (modified — added MC2011 validator)
> - crates/mc-model/src/diagnostic.rs (or in lib.rs)    (new — Diagnostic, Severity, ModelPath types + JSON envelope)
> - crates/mc-model/src/cli.rs (or split modules)       (new — model {validate,inspect,lint,test} routing helpers)
> - crates/mc-model/examples/acme.yaml                  (modified — added description fields)
> - crates/mc-model/tests/lint_fixtures/                (new dir — 10 fixtures: MC3001-MC3007 + MC3009-MC3011)
> - crates/mc-model/tests/lint_rules.rs                 (new — one test per rule)
> - crates/mc-model/tests/mc3008_retired.rs             (new — assertion no active lint emits MC3008)
> - crates/mc-model/tests/mc2011_validator.rs           (new — load() blocks on weighted-avg missing weight)
> - crates/mc-model/tests/cli_snapshot.rs               (new — hand-rolled snapshot helper + fixtures)
> - crates/mc-model/tests/expected/*.txt                (new — text snapshots)
> - crates/mc-model/tests/expected/*.json               (new — JSON envelope snapshots)
> - crates/mc-model/tests/demo_no_goldens.rs            (new — mc demo --model <bad-goldens>.yaml exits 0)
> - crates/mc-model/tests/deterministic_emission.rs     (new — 10-run byte-exact assertion)
> - crates/mc-cli/src/main.rs                           (modified — model subcommand routing)
> - crates/mc-cli/Cargo.toml                            (likely unchanged — mc-model dep already present)
>
> Lint rule coverage (per ADR-0005 Decision 5):
> - MC3001 missing dim description           ✓
> - MC3002 missing measure description       ✓
> - MC3003 missing rule description          ✓
> - MC3004 model has no goldens              ✓
> - MC3005 orphan element                    ✓
> - MC3006 long rule chain depth             ✓
> - MC3007 ratio measure with Sum            ✓
> - MC3008 [RETIRED — assertion passes]      ✓
> - MC3009 unused input measure              ✓
> - MC3010 unused derived measure            ✓
> - MC3011 hierarchy root ambiguity          ✓
>
> MC2011 (validator promotion):
> - WeightedAverage measure missing weight   ✓ blocks load() with code MC2011
>
> Acme YAML cleanup summary:
> - N descriptions added (X dim, Y measure, Z rule)
> - Structural-equivalence test still passes
> - Demo-equivalence diff still empty
>
> Implementation summary:
> - <one paragraph: lint module shape; how diagnostic emission works; CLI routing>
>
> Deviations:
> - <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The shape of `ValidatedModel` your lint rules iterate over

Phase 3A shipped `ValidatedModel` in `crates/mc-model/src/schema.rs`. Read it before writing lint rules — your rules walk this type, not the YAML or `ParsedModel`. Key fields you'll need:

- `dimensions: Vec<ValidatedDimension>` — each has name, kind, elements, an optional `description`
- `hierarchies: Vec<ValidatedHierarchy>` — per-dim default hierarchy with edges
- `measures: Vec<ValidatedMeasure>` — name, role (Input/Derived), data_type, aggregation, optional weight_measure, optional description
- `rules: Vec<ValidatedRule>` — id, target measure, body (the structured expression tree), declared_dependencies, optional description
- `golden_tests: Vec<ValidatedGoldenTest>` — name, coord, expected value (or epsilon-tolerant variant)

Lint rules walk these, NOT raw YAML — that's why they don't need YAML span info to fire (though the diagnostic carries span info from the parse stage when available).

### B. The MC3007 ratio-detection heuristic

ADR-0005 Decision 5 specifies the MC3007 trigger as a measure name matching `*_rate`, `*_ratio`, `*_pct`, `cpc`, `cvr`, `aov`, `cpa`, `roas` AND aggregation = Sum. Phase 3B implementation:

```rust
fn is_ratio_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("_rate")
        || lower.ends_with("_ratio")
        || lower.ends_with("_pct")
        || lower == "cpc"
        || lower == "cvr"
        || lower == "aov"
        || lower == "cpa"
        || lower == "roas"
}
```

Acme's measures include CPC, CVR, AOV — all with `WeightedAverage` aggregation already in the Phase 3A YAML. So MC3007 should NOT fire on Acme. (If it does, the Phase 3A YAML has a bug; surface it.)

The suggestion text MUST include the caveat: *"Verify the aggregation rule matches the measure's intent — this lint is heuristic and may produce false positives."*

### C. MC3006 long-chain detection

Walk the rules' `body.refs` to build a chain-depth map: each derived measure's depth is `1 + max(depth of refs)`. Inputs are depth 0. A rule body referencing only inputs is depth 1. The Acme `Gross_Profit` rule chains as: Gross_Profit (5) → Revenue (4) → Customers (3) → Leads (2) → Clicks (1) → [Spend (0), CPC (0)]. So depth 5 — at the threshold.

ADR-0005 Decision 5 says "≥ 5 deep". On Acme, Gross_Profit is depth 5 — should it fire MC3006?

**Recommendation:** trigger at strictly > 5, OR document Acme as a known-acceptable case. The cleaner path is **trigger at > 5** (depth 6+). Document this in the rule's implementation comment + completion report. Acme's depth-5 chain stays clean. If this contradicts the ADR's "≥ 5" wording, surface as a SPEC QUESTION before locking in the threshold; the implementer's call has to be principled and documented.

### D. The deterministic emission order — practical implementation

```rust
fn sort_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|a, b| {
        // 1. severity desc (Error < Warning < Info → reverse)
        let severity_cmp = (b.severity as u8).cmp(&(a.severity as u8));
        if severity_cmp != std::cmp::Ordering::Equal { return severity_cmp; }

        // 2. code asc
        let code_cmp = a.code.as_ref().cmp(b.code.as_ref());
        if code_cmp != std::cmp::Ordering::Equal { return code_cmp; }

        // 3. yaml_pointer asc
        let pointer_cmp = a.path.yaml_pointer.cmp(&b.path.yaml_pointer);
        if pointer_cmp != std::cmp::Ordering::Equal { return pointer_cmp; }

        // 4. message asc (final tiebreaker)
        a.message.cmp(&b.message)
    });
}
```

Apply this AFTER `lint()` accumulates all rule outputs, BEFORE returning. The text/JSON formatters then assume input is pre-sorted.

`Severity` enum should have explicit numeric values (Error = 2, Warning = 1, Info = 0) so the desc sort works as a `u8` comparison. Or use `match` patterns; either is fine.

### E. The Acme-cleanup pattern

To make Acme lint clean (gate #2), add `description:` fields. The minimal cleanup is:

- 6 dim descriptions (one per dim)
- 11 measure descriptions (one per measure)
- 5 rule descriptions (one per rule)

Total: 22 short text fields added to `crates/mc-model/examples/acme.yaml`. Keep them concise (~one line each) and accurate. Example:

```yaml
dimensions:
  - name: "Time"
    description: "Calendar time periods used for plan-vs-actual comparison."
    kind: "Standard"
    elements:
      ...
```

After the cleanup, re-run the demo-equivalence diff — it MUST still be empty (descriptions are metadata, don't affect output). Re-run the structural-equivalence test against `build_acme_cube()` — it MUST still pass (descriptions don't affect cube structure).

### F. JSON envelope examples for snapshots

Empty case:

```json
{
  "schema_version": "1.0",
  "diagnostics": []
}
```

One-diagnostic case:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC3001",
      "severity": "Warning",
      "path": {
        "file": "crates/mc-model/tests/lint_fixtures/MC3001_missing_dim_description.yaml",
        "span": {"line": 5, "column": 5},
        "yaml_pointer": "/dimensions/0",
        "model_path": "dimensions.Time"
      },
      "message": "Dimension 'Time' has no description",
      "suggestion": "Add a one-line description explaining what the dim represents"
    }
  ]
}
```

These shapes are stable contracts for Phase 4 + Phase 6 consumption; lock them in your snapshot fixtures.

### G. What Phase 4 (LLM authoring) will consume

Phase 4 doesn't exist yet, but Phase 3B is its diagnostic surface. Phase 4 will:

1. Take a natural-language prompt.
2. Have an LLM emit YAML against ADR-0004's schema.
3. Call `mc model validate <path> --format json`. If errors → feed JSON envelope back to LLM, re-prompt.
4. Call `mc model lint <path> --format json`. If warnings → feed envelope back to LLM, optionally request refinement.
5. Call `mc model test <path>` to verify against any inline goldens the user supplied.

Every diagnostic the LLM gets back will be one of:

- `code: "MCxxxx"` — stable across releases, mappable to a documentation page or a re-prompting strategy
- `severity: "Error"|"Warning"|"Info"` — Phase 4 chooses how aggressively to re-prompt based on severity
- `path.model_path` — names the schema location ("measures.CPC.aggregation") rather than YAML offsets
- `suggestion: "..."` — direct hint Phase 4 can include verbatim in the re-prompt

Phase 3B's stable codes + envelope are the contract. Don't change shape mid-flight; if a rule needs a different field, add a new optional field, don't repurpose an existing one.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| The lint module | `crates/mc-model/src/lint.rs` (or `lint/` dir) | new — implements the 10 rules + the `lint()` entry point |
| The promoted MC2011 validator | [`crates/mc-model/src/validate.rs`](../../crates/mc-model/src/validate.rs) | modify — add MC2011 alongside the 10 ADR-0004 validators |
| Diagnostic types | `crates/mc-model/src/diagnostic.rs` (or in `lib.rs`) | new — `Diagnostic`, `Severity`, `ModelPath`, `DiagnosticEnvelope` types + JSON serialization |
| Public API surface | [`crates/mc-model/src/lib.rs`](../../crates/mc-model/src/lib.rs) | modify — export `lint`, `Diagnostic`, etc. |
| The Acme YAML | [`crates/mc-model/examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | modify — add `description:` fields to clear all 10 lints |
| Lint fixtures | `crates/mc-model/tests/lint_fixtures/` | new dir — 10 fixtures + 1 (MC2011) for validator |
| Per-rule tests | `crates/mc-model/tests/lint_rules.rs` | new — one test per rule + retirement assertion |
| MC2011 validator test | `crates/mc-model/tests/mc2011_validator.rs` | new — load() blocks with MC2011 code |
| Snapshot harness | `crates/mc-model/tests/cli_snapshot.rs` + `tests/expected/` | new — hand-rolled snapshot helper + text + JSON expected fixtures |
| Demo-no-goldens test | `crates/mc-model/tests/demo_no_goldens.rs` | new — integration test asserting `mc demo --model <bad-goldens>.yaml` exits 0 |
| Deterministic emission test | `crates/mc-model/tests/deterministic_emission.rs` | new — 10-run byte-exact assertion |
| CLI subcommand routing | [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) | modify — add `model {validate,inspect,lint,test}` routing |
| Phase 3B completion report | `docs/reports/phase-3b-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 3B from `proposed` → `complete` |

**Do not touch:**

- **`crates/mc-core/`** — entire crate locked. Source, tests, benches, Cargo.toml, all of it.
- **`crates/mc-fixtures/`** — entire crate locked. Source, Cargo.toml, all of it.
- **`docs/specs/`** — locked. Brief and engine-semantics doc are contracts.
- **`docs/decisions/0004-*` and `0005-*`** — Accepted; amendments go in `0004-amendment-N.md` / `0005-amendment-N.md`, not in the originals.
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock` (existing pins)** — `clap`, `clap_lex`, `half` from Phase 1B + `indexmap`, `hashbrown` from Phase 3A all stay.
- **PERF.md** — Phase 3B doesn't touch performance documentation. The kernel didn't change; benches don't need to be re-run.
- **`crates/mc-model/examples/acme.yaml` STRUCTURE** — only `description:` fields may be added. Dimensions, hierarchies, measures (other than descriptions), rule bodies, and golden test values stay byte-for-byte.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-3B gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                                  # 252 / 0 (Phase 3A's count)
cargo run --release --bin mc -- demo                                    # matches brief §4.6 (Rust path)
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml   # YAML path

# Acme demo-equivalence diff — must remain empty throughout
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)
# expected: zero output (exit 0)

# Iteration loop during Phase 3B development:
cargo build -p mc-model
cargo test -p mc-model
cargo test -p mc-model -- lint                       # the per-rule tests
cargo test -p mc-model -- mc2011                     # the validator promotion test
cargo test -p mc-model -- snapshot                   # hand-rolled CLI snapshots
cargo test -p mc-model -- deterministic_emission     # 10-run byte-exact

# CLI smoke (after subcommand wiring lands):
cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml         # the headline gate — exit 0, ZERO warnings
cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml         # all 9 goldens pass

# Acceptance-gate verification:
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml
# expected: zero stdout (no warnings), exit 0

# Determinism gate (10 runs, identical pass/fail):
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Verify locked surfaces are untouched:
git diff phase-3a-model-definition-layer -- crates/mc-core/ crates/mc-fixtures/
# expected: zero output

# Verify mc-core/mc-fixtures Cargo.tomls are unchanged:
git diff phase-3a-model-definition-layer -- crates/mc-core/Cargo.toml crates/mc-fixtures/Cargo.toml
# expected: zero output
```

---

## Final checklist before you call Phase 3B done

- [ ] `mc-model::lint(model: &ValidatedModel) -> Vec<Diagnostic>` implemented; pre-sorted output per deterministic emission order.
- [ ] 10 lint rules implemented (MC3001–MC3007 + MC3009–MC3011); MC3008 deliberately absent.
- [ ] MC3008-retirement assertion in tests passes (no active lint emits `"MC3008"`).
- [ ] MC2011 validator promoted into `mc-model::validate`; blocks `load()` with code `"MC2011"` when WeightedAverage measure missing weight.
- [ ] `Diagnostic { code, severity, path, message, suggestion }` shape implemented + tested.
- [ ] JSON envelope `{ "schema_version": "1.0", "diagnostics": [...] }` always emits `schema_version: "1.0"`, including in empty-diagnostic cases.
- [ ] Deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)` applied uniformly across text + JSON + library output.
- [ ] Four CLI subcommands work end-to-end: `mc model {validate,inspect,lint,test}` plus `--format text|json` modifier and `mc model lint --deny-warnings` flag.
- [ ] **`mc demo --model` does NOT run goldens** (per amendment #12) — integration test passes with a bad-goldens fixture.
- [ ] **Acme lints clean — ZERO warnings** (per amendment #15) — the headline gate.
- [ ] One lint fixture per rule under `crates/mc-model/tests/lint_fixtures/`; per-rule test asserts each rule fires + no spurious other-rule firings.
- [ ] Hand-rolled snapshot fixture comparison in place; `mc model inspect <acme.yaml>` and `mc model lint <each-fixture>` snapshots locked.
- [ ] (If `insta` was used) — proven to build cleanly on Rust 1.78 with no transitive churn; documented in completion report.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-model/src/` (test/example/CLI exempt where the existing carve-out applies; new production paths return `Result`).
- [ ] No `unsafe` anywhere.
- [ ] No `async` / `tokio` / `rayon` / threads anywhere.
- [ ] All 252 existing tests still pass; new total ≥ 252 + (Phase 3B test count).
- [ ] **`mc-core` Cargo.toml unchanged.**
- [ ] **`mc-fixtures` not modified.**
- [ ] **`crates/mc-model/examples/acme.yaml` only added `description:` fields** — structural-equivalence test against `build_acme_cube()` still passes; demo-equivalence diff still empty.
- [ ] **`rust-toolchain.toml` not bumped** — still Rust 1.78.
- [ ] **`Cargo.lock`** Phase 1B + Phase 3A pins intact.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] Completion report at `docs/reports/phase-3b-completion-report.md` written from template, including the diagnostic-code registry table.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 3B from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 3C (formula syntax), Phase 4 (LLM), Phase 5 (actuals), or Phase 6 (UI).**

If you are uncertain at any point, the resolution order is:

1. The Phase 3B prompt above.
2. **[ADR-0005](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) — the binding strategic contract (with all 15 acceptance amendments).**
3. [ADR-0004](../decisions/0004-phase-3a-model-definition-format.md) — the inherited model-format contract (Phase 3B doesn't modify it).
4. The brief and `engine-semantics.md` for kernel-side semantics that the schema must respect.
5. Phase 3A completion report (recent context).
6. Earlier completion reports (1A / 1B / 2A / 2B / 2C / 2D).
7. `CLAUDE.md`.
8. `docs/roadmap/MASTER_PHASE_PLAN.md`.
9. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 3A)

**Read ADR-0005 (with its 15 acceptance amendments) before you write any code.** The amendments aren't suggestions — they're the contract. Anything in the prompt above is a derivation from the ADR; if a derivation seems to contradict the ADR or its amendments, the ADR wins and the prompt is buggy — surface it.

**Source-bounded, but the bound is read-only over `mc-model`.** Phase 3B doesn't change the kernel, doesn't change the fixtures, doesn't change the model schema (only adds descriptions to Acme). The new code is lint rules + diagnostic types + CLI subcommand handlers + tests + fixtures + snapshot expected output. That's it.

**The acceptance gate is the lint-clean Acme.** `mc model lint crates/mc-model/examples/acme.yaml` exiting 0 with zero warnings is the headline. If Acme can't lint clean, Phase 3B can't ship.

**Diagnostic codes are forever.** MC3008 is permanently retired; new lint codes go to MC3012+. Reusing a code silently breaks any consumer pinned to a code-to-meaning map. CVE-style retirement is cheaper than reuse.

**Stable codes + JSON envelope are the load-bearing piece for Phase 4.** The LLM-iteration loop needs structured feedback; the diagnostic shape + the schema_version envelope are how it gets that. Don't skimp on the envelope; don't deviate from the sort order.

**Hand-rolled snapshots are preferred over `insta`.** Decision 9 + amendment #7 are explicit: the project's policy is "minimum dep churn." A `assert_snapshot(actual, fixture_path)` helper is ~30 lines of code; pull `insta` only if you can prove it adds meaningful value AND builds 1.78-clean.

**A bench is a contract — but Phase 3B has no benches.** The lint module is not on a hot path; CLI subcommands run on demand. The kernel's PERF.md / bench-data baselines are unchanged. If you find yourself wanting to add a `mc-model` benchmark, **stop and check** — that's scope creep.

**Do not pick the next phase.** Phase 3B's deliverable is the diagnostics + lint + CLI surface + Acme cleanup. If the work surfaces opportunities for Phase 3C (formula strings) or other follow-ons, note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 3B handoff drafted 2026-05-02 immediately after [ADR-0005](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) was Accepted with 15 project-owner acceptance amendments (10 from GPT review, 5 from Claude Desktop review). The handoff is the build contract; the ADR is the strategic context behind it.*
