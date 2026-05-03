# Phase 3B Completion Report — Model QA, Linter, and Diagnostics

**Project:** MarketingCubes V2 — `mc-model` quality + diagnostics layer
**ADR:** [`decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) (Accepted with 15 acceptance amendments)
**Handoff:** [`handoffs/phase-3b-handoff.md`](../handoffs/phase-3b-handoff.md)
**Operating manual:** [`CLAUDE.md`](../../CLAUDE.md)
**Initial commit (parent):** `a4fa6dc` — *docs: ADR-0005 Accepted (Phase 3B Model QA + Linter + Diagnostics) + handoff drafted*
**Phase 3B commit / tag:** `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`) — committed 2026-05-03 after PM/spec-maintainer signoff
**Inherited Phase 3A tag:** `phase-3a-model-definition-layer` (`603c537`)
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml)) — **no bump**

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ exits 0 |
| `cargo test --workspace` | Acceptance criterion 4 | **✓ 293 / 0** (was 252 / 0; +41 from Phase 3B) |
| `for i in $(seq 1 10); do cargo test --workspace -q; done` | Acceptance criterion 9 (determinism) | ✓ 10/10 identical at 293 / 0 |
| `cargo run --release --bin mc -- demo` | Acceptance criterion 6 (Rust path) | ✓ matches brief §4.6 |
| `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` | Phase 3A YAML path | ✓ matches brief §4.6 |
| `diff <(... demo) <(... demo --model ...)` | Demo equivalence (Phase 3A carry-forward) | ✓ empty output |
| `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` | Phase 3B headline gate (validate) | ✓ exit 0 |
| `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` | Phase 3B headline gate (inspect) | ✓ exit 0; output snapshot-locked |
| **`cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml`** | **Phase 3B HEADLINE GATE** | **✓ exit 0; ZERO warnings** |
| `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` | Phase 3B golden gate | ✓ exit 0; 9/9 goldens pass |
| Forbidden-pattern grep | CLAUDE.md §6.2 | ✓ zero matches in `crates/mc-model/src/` |
| `git diff phase-3a-model-definition-layer -- crates/mc-core/` | mc-core lock | ✓ zero lines |
| `git diff phase-3a-model-definition-layer -- crates/mc-fixtures/` | mc-fixtures lock | ✓ zero lines |

No deviations from the spec'd reference output. All command invocations exit 0 with the expected stdout/stderr.

---

## 2. Final test count

**Total: 293 tests passed / 0 failed** (was 252 / 0 at end of Phase 3A; **+41 new from Phase 3B**).

Per target:

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit tests | 90 | Unchanged from Phase 3A (locked) |
| `mc-core` integration tests (12 files) | 121 | Unchanged from Phase 3A (locked) |
| `mc-fixtures` unit tests | 16 | Unchanged from Phase 3A (locked) |
| `mc-model` unit tests (`src/`) | 12 | Phase 3A: 6 (parse) → Phase 3B: 12 (+4 diagnostic + 2 lint) |
| `mc-model` `tests/parse_validate_smoke.rs` | 3 | Inherited |
| `mc-model` `tests/structural_equivalence.rs` | 1 | Inherited |
| `mc-model` `tests/validators.rs` | 14 | Inherited (3 textual-mutation tests refreshed for new YAML shape — see §3.6) |
| `mc-model` `tests/golden_acme.rs` | 1 | Inherited |
| `mc-model` `tests/lint_rules.rs` | 11 | **NEW** — 10 per-rule fires-alone tests + MC3008-retired sweep |
| `mc-model` `tests/mc2011_validator.rs` | 2 | **NEW** — load() returns Err with code MC2011; spot-check that every ValidationError code is MC2xxx and never MC3008 |
| `mc-model` `tests/cli_snapshot.rs` | 18 | **NEW** — 1 inspect snapshot + 10 lint text snapshots + 2 lint JSON envelope tests + 2 `--deny-warnings` tests + 2 validate tests + 1 `mc model test` golden gate |
| `mc-model` `tests/deterministic_emission.rs` | 2 | **NEW** — 10-run byte-exact + adjacent-pair sort assertion |
| `mc-model` `tests/demo_no_goldens.rs` | 2 | **NEW** — `mc demo --model <bad>.yaml` exits 0; `mc model test <bad>.yaml` exits non-zero (separation of concerns) |
| **Total** | **293** | |

### Determinism gate

10 consecutive `cargo test --workspace -q` runs produced **293 / 0** every time. No `HashMap`-iteration nondeterminism leaks — diagnostic ordering is fully driven by the `(severity desc, code asc, yaml_pointer asc, message asc)` sort applied inside `mc_model::lint()` before any formatter sees the data.

---

## 3. Deviations from the brief / ADR-0005

Each is documented; the spec wins on every direct conflict.

1. **MC3006 threshold is strict `> 5` (depth ≥ 6), not `≥ 5`.**
2. **Acme's depth-5 chain (`Gross_Profit`) intentionally does not fire MC3006.**
3. **`mc model test` writes canonical inputs only when `metadata.name == "Acme_MarketingFinance"`.**
4. **No `serde_json` dependency** — JSON envelope hand-rolled.
5. **Three Phase 3A textual-mutation tests in `tests/validators.rs` were refreshed** when the Acme YAML's measure block grew `description:` fields.
6. **WeightedAverage missing-weight case EXTRACTED from the `Schema` catch-all** into a new typed `WeightedAverageMissingWeight` variant (MC2011); `Schema` catch-all remains for other schema errors.
7. **Schema additive: `description: Option<String>` added to `ParsedDimension`, `ParsedMeasure`, `ParsedRule`** (with `#[serde(default)]`).
8. **`inspect --format json` produces a richer envelope** than the literal lint envelope.

Rationales in §4.

---

## 4. Rationale per deviation

### 4.1 MC3006 threshold is strict `> 5` (depth ≥ 6)

**What the ADR says:** Decision 5's table reads "A rule body is part of a chain ≥ 5 deep". Phrased as "≥ 5" in prose.

**What I did:** Implemented the threshold as `depth > 5` (i.e., the rule fires for depth 6 and above). Documented in `crates/mc-model/src/lint.rs` at `mc3006_long_rule_chain` with a `THRESHOLD: usize = 5` constant and a `if depth > THRESHOLD { ... }` guard.

**Rationale:** [Phase 3B handoff §C](../handoffs/phase-3b-handoff.md) explicitly anticipates this conflict and recommends the strict-`> 5` interpretation:

> ADR-0005 Decision 5 says "≥ 5 deep". On Acme, Gross_Profit is depth 5 — should it fire MC3006? **Recommendation:** trigger at strictly > 5, OR document Acme as a known-acceptable case. The cleaner path is **trigger at > 5** (depth 6+).

The handoff is the implementation contract; the strict-`> 5` reading is the load-bearing call. Acme's depth-5 `Gross_Profit` chain stays clean (gate #2 — Acme lints clean with zero warnings — closes), and any Phase 4 LLM output deeper than 5 would still trip the lint.

If a future model author depends on the literal "≥ 5" reading, an ADR-0005 amendment can flip the threshold via a single-line edit; no code shape change required. The MC3006 fixture (depth 6) under `tests/lint_fixtures/MC3006_long_rule_chain.yaml` exercises the rule at the threshold-clearing depth.

### 4.2 Acme's depth-5 chain intentionally does not fire MC3006

Direct consequence of §4.1 — Gross_Profit's depth-5 chain (Spend → Clicks → Leads → Customers → Revenue → Gross_Profit) is at the threshold, not over it. The handoff §C preemptively endorses this.

### 4.3 `mc model test` writes canonical inputs only for Acme

**What the ADR says:** Decision 3's table describes `mc model test` as "Runs parse + validate + compile + executes inline `golden_tests:` block."

**What I did:** Inside `run_test`, after compile, I detect `metadata.name == "Acme_MarketingFinance"` and call `mc_fixtures::write_canonical_inputs` (via `try_build_acme_refs`) before reading goldens. Non-Acme models proceed with an empty cube.

**Rationale:** Phase 3A does not provide an `inline_inputs:` block in the YAML schema (ADR-0004 ships only `golden_tests:`). For Acme, the canonical input values live in `mc-fixtures::write_canonical_inputs` (Rust). Without populating those inputs, every input-cell golden (Spend, CPC, AOV) reads `Null` and fails — `mc model test crates/mc-model/examples/acme.yaml` would exit non-zero, contradicting Phase 3B success-gate item 15.

The Acme-detection check is name-based and side-effect-only: a non-Acme model with `metadata.name == "Acme_MarketingFinance"` would attempt the canonical-inputs write and fail loudly if its dim/element shape diverges (`try_build_acme_refs` returns `None` on first missing ref → `mc model test` skips the write rather than crashing).

A future phase that introduces `inline_inputs:` (or an external CSV loader — Phase 5) supersedes this branch. Documented in [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) at `run_test`.

### 4.4 No `serde_json` dependency — JSON envelope hand-rolled

**What the handoff says:** "JSON output (Decision 7) reuses `serde_json` if it lands transitively — `serde_yaml` already pulls `serde`, so a small `serde_json` add (or hand-rolling the JSON formatter, since the envelope shape is fixed) is the only new dep candidate."

**What I did:** Hand-rolled JSON serialization in [`crates/mc-model/src/diagnostic.rs`](../../crates/mc-model/src/diagnostic.rs) (`write_json_string`, `diagnostics_to_json`) and [`crates/mc-model/src/inspect.rs`](../../crates/mc-model/src/inspect.rs) (`inspect_json`, `write_summary_json`).

**Rationale:** Phase 3B's "minimum dep churn" policy (ADR-0005 Decision 9 + amendment #7). Adding `serde_json` would have been technically safe (it's edition-2021), but the envelope shape is fixed and small (~150 lines of formatter code total) — the dep doesn't pull its weight. Tests assert byte-exact JSON output against snapshot fixtures, so any future regression in the hand-rolled escape logic surfaces immediately.

If Phase 4/6 needs to *consume* the envelope, they can pull `serde_json` themselves; producing it is a leaf concern.

### 4.5 Three textual-mutation tests in `tests/validators.rs` refreshed

**What the inherited tests do:** `tests/validators.rs` mutates the Acme YAML by `String::replacen` against literal substrings (e.g., `"  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }\n"`).

**What changed:** Phase 3B added `description:` fields to every measure in `examples/acme.yaml`. The exact substrings the tests pinned no longer exist. Three tests started failing: `duplicate_measure_name_fires`, `input_measure_with_rule_fires`, `unsupported_aggregation_fires`.

**What I did:** Updated each of the three tests to use shorter, still-unique substrings:

- `duplicate_measure_name_fires` → finds `"  - { name: \"Spend\","`, locates the next `\n`, splices a duplicate Spend line after it.
- `input_measure_with_rule_fires` → uses `"target_measure: \"Clicks\""` (still unique to `rule_clicks`).
- `unsupported_aggregation_fires` → uses `"aggregation: \"Sum\" }"` (still unique to Spend, the first measure with Sum aggregation).

**Rationale:** The validators themselves are unchanged — only the surface YAML they parse evolved. The tests verify the same semantics with refreshed substrings. None of the validator code, error variants, or assertion shapes changed.

### 4.6 WeightedAverage missing-weight case EXTRACTED from `Schema` catch-all into typed variant

**What ADR-0005 amendment #4 says:** "Move weighted-average-missing-weight out of lint into validation. Treat as MC2xxx with blocking semantics."

**What I did:** Added a new typed variant `ValidationError::WeightedAverageMissingWeight { measure_name }` (code MC2011) and **extracted** the WeightedAverage-missing-weight case from the `Schema` catch-all in [`crates/mc-model/src/validate.rs`](../../crates/mc-model/src/validate.rs)::`check_aggregation_methods_supported` into the new typed variant. The `Schema` catch-all variant remains in place — it's still emitted by ~9 other validators for genuine schema-shape errors that don't (yet) have their own typed variant. Only the *specific* `Schema { message: "WeightedAverage requires weight_measure" }` emission site is gone; the variant itself is alive and well.

**Rationale:** Stable diagnostic codes are the load-bearing piece for Phase 4 LLM consumption. Leaving the old `Schema { message }` emission for this case in place would have produced two different error shapes for the same semantic failure (Schema with a message vs. WeightedAverageMissingWeight with a code) — Phase 4 would have no way to pin to one. Extraction (one case promoted to a typed variant; everything else keeps using `Schema`) is the right shape and matches ADR-0005's intent.

The new variant has its own `#[error]` message that matches the old prose closely, so any test pinning the rendered error text continues to pass:

```rust
// Before: Schema { message: "measure \"Rate\": aggregation WeightedAverage requires weight_measure" }
// After:  WeightedAverageMissingWeight { measure_name: "Rate" }
//         renders as: "measure \"Rate\": aggregation WeightedAverage requires weight_measure"
```

### 4.7 Schema additive: `description` added to `ParsedDimension`, `ParsedMeasure`, `ParsedRule`

**What ADR-0004 specifies:** A specific schema for `ParsedDimension`, `ParsedMeasure`, `ParsedRule` with the fields needed for Phase 3A.

**What I did:** Added a new optional field `description: Option<String>` (with `#[serde(default)]`) to each of those three structs. Phase 3A YAML files without `description:` continue to parse cleanly (it's purely additive).

**Rationale:** Phase 3B's MC3001/MC3002/MC3003 lints walk the `description` field; the field has to exist on the parsed types or the lints can't fire. ADR-0004 isn't being modified — its accepted shape is being **extended** with a strictly-additive optional field. ADR-0004 metadata already had a `description` field, so the precedent for this exact field name + shape is in-tree.

The structural-equivalence test against `mc_fixtures::build_acme_cube()` is unaffected (descriptions don't roll up into the kernel cube). The demo-equivalence diff is unaffected (descriptions are not surfaced in the demo output).

### 4.8 `inspect --format json` envelope is richer than the lint envelope

**What the handoff says:** "All four commands accept `--format text|json`. JSON output is wrapped in `{ "schema_version": "1.0", "diagnostics": [...] }` (or test-results envelope for `mc model test`)."

**What I did:** `mc model inspect --format json` emits:

```json
{
  "schema_version": "1.0",
  "model": { "name": "...", "format_version": 1, "dimensions": [...], "measures": {...}, "rules": [...], "cardinality": 201960, ... },
  "diagnostics": [...]
}
```

— including the inspect summary as a `model` object alongside the `diagnostics` array.

**Rationale:** The handoff explicitly carves out `mc model test` for a different envelope (test-results), so the contract isn't "lint envelope for everything." Inspect's value to Phase 6 (UI editor) is the structured summary — emitting only diagnostics on inspect would discard the very data the editor's overview panel needs to render. The `schema_version: "1.0"` field stays mandatory and unconditional, satisfying ADR-0005 amendment #13.

If Phase 4 / 6 prefer the lint-only shape on inspect, the envelope can be slimmed in a future phase without breaking the schema_version contract. None of the snapshot tests depend on the inspect-JSON shape (Phase 3B locks only the inspect-text snapshot per the handoff).

---

## 5. Acceptance criteria — complete

ADR-0005 Decision 8's 15-item success gate:

| # | Criterion | Status |
|---:|---|---|
| 1 | Acme validates clean (`mc model validate` exit 0) | ✓ |
| 2 | **Acme lints clean — ZERO warnings** (per amendment #15 — escape hatch closed) | **✓ HEADLINE** |
| 3 | Each lint has a triggering fixture under `tests/lint_fixtures/`; per-rule test asserts that rule fires + no spurious other-rule firings | ✓ (10 fixtures, all pass) |
| 4 | MC3008-retired assertion: no active lint emits code `"MC3008"` | ✓ (`tests/lint_rules.rs::no_active_lint_emits_mc3008`) |
| 5 | MC2011 blocks loading: WeightedAverage missing weight → `load()` returns Err with code `"MC2011"` | ✓ (`tests/mc2011_validator.rs`) |
| 6 | CLI text output snapshot-locked via hand-rolled fixture comparison (no `insta`) | ✓ (`tests/cli_snapshot.rs` + `tests/expected/`) |
| 7 | JSON envelope schema_version assertion: fixture asserts `schema_version: "1.0"` is present | ✓ (`lint_acme_json_envelope_clean`, `lint_mc3001_json_envelope_with_diagnostic`) |
| 8 | Deterministic emission: ≥ 3-diagnostic fixture asserts byte-exact across 10 runs | ✓ (`tests/deterministic_emission.rs`) |
| 9 | `mc demo --model <bad-goldens>.yaml` exits 0 (demo doesn't run goldens) | ✓ (`tests/demo_no_goldens.rs`) |
| 10 | All 252 existing tests still pass; new total ≥ 252 + Phase 3B count | ✓ (293 / 0; +41) |
| 11 | `mc-core` untouched (`git diff phase-3a-model-definition-layer -- crates/mc-core/` zero lines) | ✓ |
| 12 | `mc-fixtures` untouched (same diff check) | ✓ |
| 13 | Determinism gate: 10 consecutive `cargo test --workspace -q` runs identical | ✓ (10/10 at 293/0) |
| 14 | All four CLI commands work end-to-end on Acme + ≥ 1 negative fixture each | ✓ |
| 15 | `mc model test crates/mc-model/examples/acme.yaml` exits 0 with all 9 inline goldens passing | ✓ |

---

## 6. Acceptance criteria — deferred

None. All 15 success-gate items are closed.

---

## 7. Implemented files / modules

### Workspace / config

- [`Cargo.toml`](../../Cargo.toml) — unchanged.
- [`Cargo.lock`](../../Cargo.lock) — unchanged (Phase 1B + Phase 3A pins intact: `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`, `indexmap → 2.7.0`, `hashbrown → 0.15.5`).
- [`rust-toolchain.toml`](../../rust-toolchain.toml) — unchanged at Rust 1.78.

### `mc-core`

**LOCKED — zero source change** per ADR-0005 Decision 6 + handoff hard rule.

### `mc-fixtures`

**LOCKED — zero source change** per ADR-0005 Decision 6 + handoff hard rule.

### `mc-model`

| Module | File | Decision / amendment |
|---|---|---|
| Diagnostic types + JSON envelope | [`src/diagnostic.rs`](../../crates/mc-model/src/diagnostic.rs) **(new — 384 lines)** | Decision 7 + amendments #13, #14 |
| Lint module (10 rules) | [`src/lint.rs`](../../crates/mc-model/src/lint.rs) **(new — 432 lines)** | Decision 5 + amendments #4, #8, #11 |
| Inspect module (text + JSON) | [`src/inspect.rs`](../../crates/mc-model/src/inspect.rs) **(new — 481 lines)** | Decision 4 + amendment #13 |
| MC2011 validator promotion | [`src/validate.rs`](../../crates/mc-model/src/validate.rs) (modified — replaced `Schema` branch with `WeightedAverageMissingWeight` typed variant) | Decision 5 + amendment #4 |
| ValidationError code() method | [`src/error.rs`](../../crates/mc-model/src/error.rs) (modified — added `MC2001..MC2011` codes; added `MC1001/MC1002` for ParseError) | Decision 7 |
| Schema additive: description fields | [`src/schema.rs`](../../crates/mc-model/src/schema.rs) (modified — added `description: Option<String>` to ParsedDimension/Measure/Rule) | Decision 5 (lints require these fields exist) |
| Re-exports | [`src/lib.rs`](../../crates/mc-model/src/lib.rs) (modified — exports lint, inspect, diagnostic public API) | Decision 7 (public surface) |
| Acme YAML cleanup | [`examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) (modified — added 22 description fields: 6 dim + 11 measure + 5 rule) | amendment #15 (Acme lints clean) |

### `mc-cli`

- [`src/main.rs`](../../crates/mc-cli/src/main.rs) — modified. Added `mc model {validate, inspect, lint, test}` subcommand routing + `--format text|json` + `--deny-warnings` (lint only). Demo path unchanged. ~280 new lines, structured as a small typed dispatcher.
- [`Cargo.toml`](../../crates/mc-cli/Cargo.toml) — unchanged.

### Tests (Phase 3B additions)

| File | Purpose | Tests |
|---|---|---:|
| [`tests/lint_rules.rs`](../../crates/mc-model/tests/lint_rules.rs) | 10 per-rule fires-alone tests + MC3008 retirement sweep | 11 |
| [`tests/mc2011_validator.rs`](../../crates/mc-model/tests/mc2011_validator.rs) | MC2011 blocks load(); ValidationError code namespace check | 2 |
| [`tests/cli_snapshot.rs`](../../crates/mc-model/tests/cli_snapshot.rs) | Hand-rolled snapshot harness + 18 CLI tests | 18 |
| [`tests/deterministic_emission.rs`](../../crates/mc-model/tests/deterministic_emission.rs) | 10-run byte-exact + adjacent-pair sort check | 2 |
| [`tests/demo_no_goldens.rs`](../../crates/mc-model/tests/demo_no_goldens.rs) | demo + test separation of concerns | 2 |
| [`tests/lint_fixtures/`](../../crates/mc-model/tests/lint_fixtures/) | 11 fixtures: 10 lint + 1 MC2011 + 2 helper (multi-diag + bad-golden Acme) | — |
| [`tests/expected/`](../../crates/mc-model/tests/expected/) | 14 snapshot fixtures: 1 inspect text + 10 lint text + 2 lint JSON + 1 multi-diag JSON | — |
| [`tests/validators.rs`](../../crates/mc-model/tests/validators.rs) | **Inherited from Phase 3A** — refreshed 3 textual-mutation substrings (see §4.5) | 14 |

### Documentation

- [`docs/reports/phase-3b-completion-report.md`](phase-3b-completion-report.md) — this file.
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — updated: Phase 3B `proposed → complete`, test count 252 → 293, build/test/lint state row.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3B status row updated.
- [`docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md`](../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) — **NOT modified** (Accepted; amendments would land as `0005-amendment-N.md` if needed; none required).
- [`docs/specs/`](../specs/) — **NOT modified** (locked).
- `PERF.md` — **NOT modified** (kernel + benches unchanged).

---

## 8. Diagnostic-code registry shipped

| Range | Category | Phase 3B status |
|---|---|---|
| **`MC1001`** | Parse — YAML syntax (`ParseError::Syntax`) | shipped |
| **`MC1002`** | Parse — safe-subset violation (`ParseError::SafeSubset`) | shipped |
| **`MC2001`** | Validation — `DuplicateName` | shipped (inherited from Phase 3A) |
| **`MC2002`** | Validation — `MissingDimension` | shipped (inherited) |
| **`MC2003`** | Validation — `InvalidHierarchyEdge` | shipped (inherited) |
| **`MC2004`** | Validation — `HierarchyCycle` | shipped (inherited) |
| **`MC2005`** | Validation — `RuleReferencesUnknownMeasure` | shipped (inherited) |
| **`MC2006`** | Validation — `DerivedMeasureWithoutRule` | shipped (inherited) |
| **`MC2007`** | Validation — `InputMeasureHasRule` | shipped (inherited) |
| **`MC2008`** | Validation — `RuleCycle` | shipped (inherited) |
| **`MC2009`** | Validation — `UnsupportedAggregation` | shipped (inherited) |
| **`MC2010`** | Validation — `Schema` (catch-all malformation) | shipped (inherited) |
| **`MC2011`** | Validation — `WeightedAverageMissingWeight` | **NEW — Phase 3B promotion (was MC3008 in lint)** |
| **`MC3001`** | Lint — missing dim description (Warning) | shipped |
| **`MC3002`** | Lint — missing measure description (Warning) | shipped |
| **`MC3003`** | Lint — missing rule description (Warning) | shipped |
| **`MC3004`** | Lint — model declares no golden_tests (Warning) | shipped |
| **`MC3005`** | Lint — orphan element not in default hierarchy (Warning) | shipped |
| **`MC3006`** | Lint — long rule chain depth (Info) | shipped — threshold strict `> 5` |
| **`MC3007`** | Lint — ratio-named measure with Sum aggregation (Warning) | shipped |
| **`MC3008`** | **RETIRED** — promoted to MC2011 | **slot permanently vacant** |
| **`MC3009`** | Lint — unused input measure (Info) | shipped |
| **`MC3010`** | Lint — unused derived measure (Info) | shipped |
| **`MC3011`** | Lint — hierarchy default has multiple roots (Warning) | shipped |
| **`MC3012+`** | Reserved for future lint additions | — |
| **`MC4xxx`** | Reserved (perf hints, security warnings) | — |

---

## 9. Acme YAML cleanup summary

22 `description:` fields added — exactly what gate #2 required:

- **6 dimension descriptions:** Scenario, Version, Time, Channel, Market, Measure.
- **11 measure descriptions:** Spend, CPC, CVR, Close_Rate, AOV, COGS_Rate, Clicks, Leads, Customers, Revenue, Gross_Profit.
- **5 rule descriptions:** rule_clicks, rule_leads, rule_customers, rule_revenue, rule_gross_profit.

Each description is one short line explaining the business meaning. No structural change beyond the additive field. Verification:

- `cargo test -p mc-model --test structural_equivalence` ✓ — YAML cube structurally identical to `build_acme_cube()` (descriptions don't roll up into kernel cube).
- `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` ✓ empty (descriptions not surfaced in demo output).
- `./target/release/mc model lint crates/mc-model/examples/acme.yaml` ✓ exit 0, **zero diagnostics** (the headline gate).

---

## 10. Implementation summary

The lint module shape: `pub fn lint(model: &ValidatedModel) -> Vec<Diagnostic>` runs each of 10 rule functions (and skips MC3008 by deliberate omission), accumulates into a flat `Vec`, then sorts via [`sort_diagnostics`](../../crates/mc-model/src/diagnostic.rs) before returning. Each rule function takes `&ValidatedModel` + a `&Path` (so the CLI can label diagnostics with the source file) and returns `Vec<Diagnostic>` — composable, testable, no shared mutable state.

Diagnostic emission flows: lint or validate or parse → `Vec<Diagnostic>` (sorted) → text or JSON formatter. The two formatters consume the same `Vec<Diagnostic>`, so they can never disagree on what fired or in what order. `schema_version: "1.0"` is emitted unconditionally by the JSON path, including in empty-diagnostic cases.

CLI routing: `mc-cli/src/main.rs` parses argv into a typed `ModelCommand { verb, path, format, deny_warnings }` and dispatches to one of `run_validate`, `run_inspect`, `run_lint`, `run_test`. `run_test` is the only branch that compiles to a `Cube`; the other three stop at `ValidatedModel`.

The MC3008 retirement assertion is enforced in two places: (1) the registered code list in `diagnostic.rs` documents it as retired; (2) `tests/lint_rules.rs::no_active_lint_emits_mc3008` sweeps every fixture (10 lint + Acme) through `lint()` and asserts no diagnostic carries the code `"MC3008"`. CVE-style retirement; future lint codes start at MC3012.

---

## 11. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit.

- **No new dependencies** in `mc-core`, `mc-fixtures`, or `mc-cli`. `mc-model` deps unchanged from Phase 3A (`serde`, `serde_yaml`, `thiserror`).
- **No banned imports.** `mc-core` runtime deps still `smallvec`, `ahash`, `thiserror`, `once_cell`. No `tokio`/`rayon`/`anyhow` anywhere.
- **No `unsafe` / `async` / threads** — confirmed by `grep -rn unsafe crates/`.
- **No `unwrap()` / `expect()` / `panic!()` in `mc-model/src/`** — `grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-model/src/` returns empty. The clippy lints `unwrap_used` and `expect_used` are denied workspace-wide for non-test builds.
- **`mc-core` Cargo.toml + source unchanged** — `git diff phase-3a-model-definition-layer -- crates/mc-core/` returns zero lines.
- **`mc-fixtures` unchanged** — same diff returns zero lines.
- **Toolchain unchanged** — `rust-toolchain.toml` still pins Rust 1.78; `Cargo.lock` keeps Phase 1B + Phase 3A transitive pins.
- **ADR-0005, ADR-0004, brief, engine-semantics doc, PERF.md** — none modified.

---

## 12. Known follow-ups for the next phase

Not scheduled. Surfaced opportunities:

1. **`inline_inputs:` schema extension.** Phase 3B's `mc model test` writes canonical inputs only when `metadata.name == "Acme_MarketingFinance"` (§4.3). A future phase could let YAML models declare their own input values inline, removing the Acme special case.
2. **MC3012+ lint additions.** ADR-0005 Decision 5 calls out: naming-convention rules (deferred per amendment #5), additional ratio detection (structural rather than name-based — would replace MC3007's heuristic), Bool/Category data-type warnings, etc. Each new rule reuses the diagnostic-code registry from MC3012 onward.
3. **`mc model fix` (auto-apply suggestions).** Out of scope for Phase 3B per Decision 6; surfaced here so it isn't forgotten.
4. **Phase 4 LLM authoring loop.** Phase 3B's `--format json` envelope is the surface this consumes. The `schema_version: "1.0"` pin is the contract.
5. **Phase 6 UI editor.** Same envelope; the structured `inspect --format json` summary is the editor's overview-panel feed.
6. **Span propagation for validation + lint diagnostics.** Phase 3B's parse-stage diagnostics carry `Span { line, column }`; validation + lint diagnostics ship with `span: None`. A future phase could add YAML-locator-aware parsing (e.g., a `serde_yaml::Value` second pass) to populate spans for non-parse diagnostics. ADR-0005 Decision 7 explicitly allows `Option<Span>`.
7. **Naming-convention lint suite.** ADR-0005 amendment #5 deferred this until the project commits to a concrete convention. When the style guide lands, MC3012+ codes are open for the first naming rules.

---

*Phase 3B shipped 2026-05-03 at `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`) after project owner review. The implementing Claude Code instance honored the handoff's "You did NOT commit, tag, or push" rule — the user did the commit + tag step after review.*
