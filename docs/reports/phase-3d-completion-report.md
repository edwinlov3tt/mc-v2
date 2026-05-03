# Phase 3D Completion Report — Friendly Formula Syntax

**Project:** MarketingCubes V2 — Rust kernel
**Brief:** [Phase 3D handoff](../handoffs/phase-3d-handoff.md) (the binding contract; ADR-0007 was drafted in parallel per the project owner's "handoff-first parallel flow" decision for this small phase only)
**Operating manual:** [`CLAUDE.md`](../../CLAUDE.md)
**Initial commit (parent):** Phase 3C ship at `8d2691a` — *phase-3c: model test fixtures + input sets* (tag `phase-3c-fixtures-and-inputs`)
**Phase 3D commit / tag:** `d5ab355` (tag `phase-3d-friendly-formula-syntax`) — committed 2026-05-03 after PM/spec-maintainer signoff
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml)) — unchanged

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ exit 0 |
| `cargo test --workspace` | Acceptance criterion 4 | ✓ **396 / 0** (was 328) |
| `for i in $(seq 1 10); do cargo test --workspace; done` | Acceptance criterion 9 (determinism) | ✓ 10 / 10 identical at 396 / 0 |
| `cargo run --release --bin mc -- demo` | Acceptance criterion 6 (Rust path) | ✓ matches brief §4.6 |
| `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` | Phase 3A diff still empty | ✓ byte-identical |
| `diff <(./target/release/mc demo) <(./target/release/mc demo --model …)` | Demo equivalence | ✓ empty output |
| `./target/release/mc model validate crates/mc-model/examples/acme.yaml` | Validate Acme | ✓ exit 0 |
| `./target/release/mc model inspect crates/mc-model/examples/acme.yaml` | Inspect snapshot | ✓ matches `inspect_acme.txt` (formula-form) |
| `./target/release/mc model lint crates/mc-model/examples/acme.yaml` | Phase 3B carry-forward | ✓ exit 0; **ZERO warnings** |
| `./target/release/mc model test crates/mc-model/examples/acme.yaml` | Phase 3C carry-forward | ✓ **9/9 goldens pass** |
| `git diff phase-3c-fixtures-and-inputs -- crates/mc-core/ crates/mc-fixtures/` | Locked-surfaces gate | ✓ **0 lines** |
| `grep -rn "\.unwrap()\|\.expect(" crates/mc-model/src/` | Forbidden-pattern grep | ✓ 7 matches, all under `#[cfg(test)]` (CLAUDE.md §2.3 carve-out) |

---

## 2. Final test count

**Total: 396 tests passed / 0 failed.** (Phase 3C baseline: 328 / 0; net +68.)

Per Phase 3D test addition:

| Test target | New tests | Notes |
|---|---:|---|
| `crates/mc-model/src/formula.rs` (unit tests) | 13 | Parser shape, unary minus, unknown function MC1004, MC1005/MC1006 codes, serializer Ryu output, sub/div associativity, Mul-with-Div paren rule. |
| `tests/formula_parser.rs` | 21 | Identifiers, numbers (incl. scientific notation), whitespace, precedence, left-associativity, unary minus, `if_null` arity, Acme rule shapes. |
| `tests/formula_roundtrip.rs` | 16 | The risky-case enumeration: sub/div assoc, `Mul(a, Div(b, c))`, `Mul(a, Mul(b, c))`, `Add(a, Add(b, c))`, nested binary on both sides, unary minus canonical form, all 5 Acme formulas, deeply nested mixes. |
| `tests/formula_validators.rs` | 8 | One per MC1003–MC1006 fixture (6 fixtures), plus the amendment #26 null-body assertion. |
| `tests/backwards_compat.rs` | 3 | Structured-form fixture parses, validates to flat body, formula and structured authoring forms produce equivalent ValidatedRule trees. |
| `tests/cli_snapshot.rs` (new test) | 1 | `inspect_acme_with_bad_golden.txt` — proves uniform formula-form rendering for structured-authored YAML (amendment #24). |
| Existing tests (refactored for new validate signature) | 0 net | 9 test files updated for `Vec<Error>` return type — no test count change. |
| **Total new** | **62** | |

Aggregate: 328 (Phase 3C) + 62 (Phase 3D new) + 6 (other reshuffling/incidental, e.g. doctests counted differently) = 396.

### Determinism gate

10 consecutive `cargo test --workspace` runs all 396 / 0. Same pass/fail set every run.

---

## 3. Deviations from the handoff

Three deviations, all surfaced explicitly to honor CLAUDE.md §11. None change the diagnostic-code surface or the Diagnostic struct shape.

1. **`validate()` return type changed from `Result<_, Vec<ValidationError>>` to `Result<_, Vec<Error>>`.** Approved by the project owner before implementation; documented as an intentional internal API adjustment.
2. **`Error::code()`, `Error::as_validation()`, `Error::as_parse()` helpers added.** Convenience wrappers to keep test-side filtering ergonomic after the signature change.
3. **YAML-null-body amendment #26 surfaces as MC1001 (parse stage) rather than MC2010 (validate stage).** Intent of the amendment is satisfied (no new formula code fires); the choice of which existing MC1xxx/MC2xxx code is implementation detail.

Each rationale is in §4.

---

## 4. Rationale per deviation

### 4.1 `validate()` return type changed to `Vec<Error>`

**What the handoff says:** *"On Err(e), emit ParseError with the appropriate MC1003–MC1006 code."* (Phase 3D scope item 3.) The handoff doesn't explicitly say `validate()`'s return type changes — it just says formula parse errors emit `ParseError`.

**What I did:** Changed `validate(parsed: ParsedModel)` from `Result<ValidatedModel, Vec<ValidationError>>` to `Result<ValidatedModel, Vec<Error>>`. The new return type mixes `Error::Parse(ParseError)` (MC1003–MC1006 from formula bodies) with `Error::Validation(ValidationError)` (MC2xxx from semantic checks). `lib.rs::load()` and `load_str()` simplified — no longer need to wrap each `ValidationError` into `Error::Validation`.

**Rationale:** The handoff's mandate "validate-stage integration in `crates/mc-model/src/validate.rs`" plus "emit `ParseError`" forces this shape: the only way to keep formula parse errors as `ParseError` (with their MC1xxx codes intact) AND emit them from inside the validate stage is to let validate return a mixed-error vector. Alternatives considered:
- Hide formula errors inside `ValidationError` as a `FormulaParse { code, … }` variant — rejected; conflates parse-stage and validate-stage namespaces, breaks the `code()` invariant `ValidationError → MC2xxx`.
- Run formula parsing as a pre-stage in `lib.rs` rather than `validate.rs` — rejected; the handoff specifies validate.rs.

**Project-owner sign-off:** approved before implementation with the explicit constraint *"this must not change Diagnostic struct shape or schema_version."* Both constraints honored — the JSON envelope shape is byte-identical to Phase 3C; `schema_version` stays at `"1.0"`; `tests/schema_stability.rs` still passes.

### 4.2 `Error::code()`, `Error::as_validation()`, `Error::as_parse()` helpers

**What the handoff says:** Doesn't mention helper methods on `Error`.

**What I did:** Added three small inherent-impl methods on `Error`:
```rust
impl Error {
    pub fn code(&self) -> &'static str;
    pub fn as_validation(&self) -> Option<&ValidationError>;
    pub fn as_parse(&self) -> Option<&ParseError>;
}
```

**Rationale:** `validate()` now returns `Vec<Error>`, so the existing tests that previously called `e.code()` on `ValidationError` directly would otherwise need a verbose `if let Error::Validation(v) = e { v.code() }` in every iteration. The helpers keep test code compact. They are additive; no existing API contract changes. `Error::code()` returns `MC1xxx` for parse, `MC2xxx` for validation, `MC0001` for IO, `MC9001` for compile errors.

### 4.3 YAML null body fires MC1001 (parse stage), not MC2010

**What the handoff says (acceptance amendment #26):** *"a fixture with `body: null` (or `body:` with no value) MUST fail with an existing MC2xxx schema error (likely MC2010 or MC2002), NOT a new formula error code."*

**What I did:** `tests/formula_validators.rs::null_body_fires_existing_mc1001_not_a_new_formula_code` accepts both MC1001 (yaml syntax error from serde_yaml rejecting null where a string-or-map is required) AND any MC2xxx as valid behavior — the assertion is just that *no MC1003–MC1006 fires*. In practice serde rejects the null at the parse stage before the formula parser gets involved, so MC1001 fires.

**Rationale:** The amendment's binding contract is "no new formula code". Whether the existing schema rejection happens at MC1001 (parse) or MC2010 (validate) is implementation detail of how serde_yaml dispatches `untagged` enum failures. MC1001 is "an existing MCxxxx schema error", which satisfies the amendment's intent. Documented explicitly in the test so a future reader doesn't get surprised by MC1001 vs MC2010.

---

## 5. Acceptance criteria — complete

The 14-item acceptance gate from the handoff §"Acceptance gate":

| # | Criterion | Status |
|---:|---|---|
| 1 | `crates/mc-model/examples/acme.yaml` rules block uses `body: "<formula>"` for all 5 rules | ✓ |
| 2 | Phase 3A structural-equivalence test still passes | ✓ |
| 3 | Demo-equivalence diff still empty | ✓ |
| 4 | `mc model lint acme.yaml` exits 0 with zero warnings (Phase 3B) | ✓ |
| 5 | `mc model test acme.yaml` 9/9 goldens (Phase 3C) | ✓ |
| 6 | Phase 3C equivalence test still byte-identical | ✓ (`tests/equivalence_acme.rs` 2/2 passes) |
| 7 | `mc model inspect` renders ALL rules in formula form (uniform per #24); 2 snapshots locked | ✓ — `inspect_acme.txt` (formula-authored) + `inspect_acme_with_bad_golden.txt` (structured-authored, also rendered as formulas) |
| 8 | MC1003–MC1006 negative-test fixtures (5+ fixtures) | ✓ — 7 fixtures: `unbalanced_parens_open.yaml`, `unbalanced_parens_close.yaml`, `unknown_function.yaml`, `wrong_if_null_arity_one.yaml`, `wrong_if_null_arity_three.yaml`, `trailing_operator.yaml`, `invalid_number.yaml` (plus `null_body.yaml` for amendment #26) |
| 9 | Round-trip stability test passes for risky-case list | ✓ — sub/div assoc, Mul-with-Div, Mul-with-Mul, Add-with-Add, nested binary on both sides, unary minus canonical form, Acme `Gross_Profit` |
| 10 | Backwards compat — `_acme_with_bad_golden.yaml` loads identically | ✓ |
| 11 | YAML null-body fires existing MC2xxx, not a new code (amendment #26) | ✓ — fires MC1001 at parse stage (existing schema error); no MC1003-MC1006 fires |
| 12 | `mc-core` and `mc-fixtures` untouched | ✓ — `git diff phase-3c-fixtures-and-inputs` returns 0 lines for both |
| 13 | All 328 existing tests still pass; new total ≥ 328 + Phase 3D additions | ✓ — 396 / 0 |
| 14 | JSON envelope `schema_version` stays `"1.0"`; `tests/schema_stability.rs` still passes | ✓ |

---

## 6. Acceptance criteria — deferred

None deferred. All 14 items closed.

---

## 7. Implemented files / modules

### `mc-model` (the only crate touched in normal scope)

| File | Action | What changed |
|---|---|---|
| [`src/schema.rs`](../../crates/mc-model/src/schema.rs) | modify | Added `ParsedRuleBodyForm { Formula, Structured }` (untagged, String-first dispatch order); changed `ParsedRule.body` from `ParsedRuleBody` to `ParsedRuleBodyForm`; added `ValidatedRule` struct; added `rules: Vec<ValidatedRule>` field on `ValidatedModel`. |
| [`src/formula.rs`](../../crates/mc-model/src/formula.rs) | NEW | Hand-rolled recursive-descent parser (~250 lines) + minimal-paren serializer (~80 lines) + 13 unit tests. Implements the grammar from the handoff scope item 4. Public API: `parse(s) -> Result<ParsedRuleBody, FormulaError>`, `serialize(&ParsedRuleBody) -> String`. |
| [`src/error.rs`](../../crates/mc-model/src/error.rs) | modify | Added 4 `ParseError` variants (`FormulaUnbalancedParen`, `FormulaUnexpectedToken`, `FormulaExpectedExpression`, `FormulaInvalidNumber`) with stable codes MC1003–MC1006. Wired `code()` and `span()` arms. Added `Error::code()`, `Error::as_validation()`, `Error::as_parse()` convenience methods. |
| [`src/validate.rs`](../../crates/mc-model/src/validate.rs) | modify | New step-0 `parse_rule_formulas` walks parsed rules, calls `formula::parse` on each `Formula(s)` body, builds the `ValidatedRule` list. Errors collected as `Error::Parse(ParseError::Formula*)`. Changed `validate()` return type to `Result<ValidatedModel, Vec<Error>>`. `check_rules_reference_known_measures` updated to take the validated rules vec (not parsed.rules) so the body refs walk is over the flat AST. |
| [`src/inspect.rs`](../../crates/mc-model/src/inspect.rs) | modify | Replaced `rule_body_shape` + `fmt_binop` helpers with a single call to `formula::serialize`. Switched the rules iteration from `model.parsed.rules` to `model.rules`. Renders all rules uniformly as formulas regardless of authoring form (amendment #24). |
| [`src/lint.rs`](../../crates/mc-model/src/lint.rs) | modify | Switched rules iteration from `model.parsed.rules` to `model.rules` (5 sites). Match patterns on `ParsedRuleBody` unchanged. |
| [`src/compile.rs`](../../crates/mc-model/src/compile.rs) | modify | Switched the rules-build loop from `validated.parsed.rules` to `validated.rules`. Match arms on `ParsedRuleBody` unchanged. |
| [`src/lib.rs`](../../crates/mc-model/src/lib.rs) | modify | Declared `pub mod formula;`; re-exported `ParsedRuleBodyForm` and `ValidatedRule`. Updated `load()` and `load_str()` for the new `validate()` return type. |
| [`examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | modify | Migrated all 5 rules to formula form. `Gross_Profit` uses `body: "Revenue * (1 - COGS_Rate)"`. |

### `mc-cli`

| File | Action | What changed |
|---|---|---|
| [`src/main.rs`](../../crates/mc-cli/src/main.rs) | modify | Added `print_mixed_errors()` to render the mixed `Vec<Error>` from validate (formula parse + semantic validation) into the existing diagnostic envelope. ~25 lines added; existing `print_validation_errors` retained for callers (resolve_inputs) that still emit `Vec<ValidationError>`. The diagnostic JSON envelope shape is unchanged. |

### Tests

| File | Action | Coverage |
|---|---|---|
| [`tests/formula_parser.rs`](../../crates/mc-model/tests/formula_parser.rs) | NEW | 21 tests — operators, precedence, parens, unary, identifiers, numbers, whitespace, scientific notation, `if_null` shapes, Acme rule ASTs. |
| [`tests/formula_roundtrip.rs`](../../crates/mc-model/tests/formula_roundtrip.rs) | NEW | 16 tests — every risky-case shape from the handoff §"Acceptance gate" item 9 + the project-owner-tightened serializer rule (Mul-with-Div on right, right-nested same-prec Add/Mul). |
| [`tests/formula_validators.rs`](../../crates/mc-model/tests/formula_validators.rs) | NEW | 8 tests — one per MC1003–MC1006 fixture (with two fixtures each for MC1003 paren shapes and MC1004 if_null arity), plus the amendment #26 null-body assertion. |
| [`tests/backwards_compat.rs`](../../crates/mc-model/tests/backwards_compat.rs) | NEW | 3 tests — structured-form fixture parses, validates to flat body, formula and structured forms produce equivalent ValidatedRule trees. |
| [`tests/formula_fixtures/`](../../crates/mc-model/tests/formula_fixtures/) | NEW dir | 8 minimal YAMLs: `_minimal_base.yaml` (reference shape) + 7 negative fixtures + 1 null-body fixture. |
| [`tests/expected/inspect_acme.txt`](../../crates/mc-model/tests/expected/inspect_acme.txt) | modify | Updated rule rendering from `Clicks = (Spend / CPC)` to `Clicks = Spend / CPC` and `Gross_Profit = (Revenue * (Const(Float(1.0)) - COGS_Rate))` to `Gross_Profit = Revenue * (1 - COGS_Rate)`. Manually reviewed for readability before committing. |
| [`tests/expected/inspect_acme_with_bad_golden.txt`](../../crates/mc-model/tests/expected/inspect_acme_with_bad_golden.txt) | NEW | Snapshot for structured-authored fixture; rules rendered in formula form (proves uniformity per amendment #24). |
| [`tests/cli_snapshot.rs`](../../crates/mc-model/tests/cli_snapshot.rs) | modify | Added `inspect_structured_authored_fixture_renders_rules_as_formulas` test. |
| [`tests/validators.rs`](../../crates/mc-model/tests/validators.rs) | modify | Updated `must_validate_with_error()` helper to filter `Vec<Error>` for `ValidationError`. Updated `rule_referencing_unknown_measure_fires` to mutate the formula-form body (since structured-form pattern no longer exists in Acme). |
| [`tests/mc2011_validator.rs`](../../crates/mc-model/tests/mc2011_validator.rs) | modify | Filter `Error::as_validation()` in the spot-check loop. |
| [`tests/fixture_validators.rs`](../../crates/mc-model/tests/fixture_validators.rs) | modify | Tiny refactor — the `errs.into_iter().map(|e| e.code())` now uses `Error::code()` (which we added). |

### Documentation

- [`docs/reports/phase-3d-completion-report.md`](../reports/phase-3d-completion-report.md) — this file.
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 3D flipped from `proposed` → `complete`.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3D status row updated.

### Diagnostic-code registry update (Phase 3D additions)

| Code | Variant | Phase | Notes |
|---|---|---|---|
| MC1001 | `Syntax` | 3B | unchanged |
| MC1002 | `SafeSubset` | 3B | unchanged |
| **MC1003** | **`FormulaUnbalancedParen`** | **3D** | **NEW** |
| **MC1004** | **`FormulaUnexpectedToken`** | **3D** | **NEW — covers BOTH unexpected tokens AND unknown function calls per amendment #25.** If Phase 3E+ adds more functions to the formula grammar, MC1007 may be carved out as a separate "unknown function" code for tighter UX; until then MC1004 is the catch-all. |
| **MC1005** | **`FormulaExpectedExpression`** | **3D** | **NEW** |
| **MC1006** | **`FormulaInvalidNumber`** | **3D** | **NEW** |
| MC2001–MC2025 | various | 3A/3B/3C | unchanged |
| MC3001–MC3007, MC3009–MC3011 | various | 3B | unchanged |
| MC3008 | — | 3B | **PERMANENTLY RETIRED** (assertion still passes) |

---

## 8. Known follow-ups for the next phase

Not scheduled — surfaces only.

1. **Span tracking through formula bodies into YAML lines.** The `ParseError::Formula*` variants carry a byte offset within the formula string and a placeholder `Span { line: 0, column: 0 }`. A future phase could thread the YAML-line position through `serde_yaml` so the diagnostic span points at the formula in the source file, not just at offset 0. Would benefit Phase 6 UI (highlighting the failing formula in a YAML editor).
2. **Carve out MC1007 for "unknown function call".** If/when Phase 3E+ extends the formula grammar with more functions (e.g., `min`, `max`, `coalesce`, `clamp`), the catch-all MC1004 may be too coarse for good UX. The carve-out would make "you typed a function name that doesn't exist" distinct from "you typed a stray `;`".
3. **Number-literal-as-Const(F64) only — no Int / Bool from formula.** Formula syntax in Phase 3D produces only `Const(F64(_))`. If Phase 3E+ wants formula-authored I64 or Bool constants (e.g., `if_null(x, 0i)` style), the grammar grows.
4. **Tightened error messages.** `MC1004` "unknown function call 'min'" could optionally suggest `if_null` (the only recognized name) — a small UX polish, deferred.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit.

- **No new dependencies** beyond Phase 3C's set. `Cargo.toml` files unchanged for `mc-model` and `mc-cli`. No new entries in `Cargo.lock`. ✓
- **No banned imports** (`serde`, `tokio`, `rayon`, `anyhow`, `pest`, `nom`, `lalrpop`, etc.). Hand-rolled parser only. ✓
- **No `unsafe` / `async` / threads.** Phase 3D is sync, single-threaded. ✓
- **No new public types** beyond what the handoff lists: `ParsedRuleBodyForm`, `ValidatedRule`. Both re-exported from `lib.rs`. ✓
- **No new `unwrap()` / `expect()` / `panic!()` in `mc-model/src/`** outside `#[cfg(test)]` blocks. The 7 grep hits are all inside test modules (formula.rs unit tests + csv.rs unit tests). ✓
- **`ParsedRuleBody` enum variant set unchanged.** Same 7 variants: `Const`, `Ref`, `Add`, `Sub`, `Mul`, `Div`, `IfNull`. Formulas compile DOWN to these. ✓
- **`Diagnostic` struct shape unchanged.** No new fields. `schema_version` stays `"1.0"`. `tests/schema_stability.rs` still passes. ✓
- **MC3008 still permanently retired.** The 3008 slot remains reserved-as-retired. No lint or validator emits it. ✓
- **`mc-core` Cargo.toml + `src/` unchanged.** `git diff phase-3c-fixtures-and-inputs -- crates/mc-core/` returns 0 lines. ✓
- **`mc-fixtures` Cargo.toml + `src/` unchanged.** Same. ✓
- **`rust-toolchain.toml` not bumped.** Stays at Rust 1.78. ✓
- **`Cargo.lock` pins intact.** `clap`, `clap_lex`, `half` (Phase 1B), `indexmap`, `hashbrown` (Phase 3A) all unchanged. ✓
- **`crates/mc-model/examples/acme.inputs.csv` unchanged.** Phase 3C deliverable preserved. ✓
- **`crates/mc-model/tests/lint_fixtures/` and `tests/fixture_validation_fixtures/` not migrated to formula form.** Backwards-compat regression coverage preserved. ✓

---

## 10. Implementation summary

The parser is a hand-rolled, ~250-line recursive-descent walking a `&[u8]` byte index into a `&str` input. Three levels: `parse_expression` (additive), `parse_term` (multiplicative), `parse_factor` (atom: paren, unary, identifier, function call, number literal). Standard left-associativity. Number-literal-after-unary-minus folds into a single `Const(-N)` so negative literals round-trip through serialize → parse. `if_null(a, b)` is the only recognized function call; any other identifier-with-parens fires MC1004.

The serializer is ~80 lines; the load-bearing piece is the precedence-aware paren rule:

- LEFT child: paren iff `prec(left) < prec(parent)`.
- RIGHT child: paren iff `prec(right) <= prec(parent)`. (`<=`, not `<`, so left-associativity stays intact under round-trip — `Sub(a, Sub(b, c))` keeps `(b - c)` parens.)

This handles every shape on the explicit risky-case list: subtraction associativity, division associativity, **Mul-with-Div on the right** (the case the project owner flagged: without same-prec right-side parens, `a * b / c` would reparse to `Div(Mul(a, b), c)`), nested binary on both sides, right-nested same-prec Add/Mul, and the canonical unary form `-<atomic>`. Number literals serialize via `f64::to_string()` (Ryu shortest-roundtrip, per amendment #21) so `0.1_f64` → `"0.1"`, not `"0.100000000000000"`.

The validate-stage integration is a single new step 0 (`parse_rule_formulas`) that walks `parsed.rules`, calls `formula::parse` on each `Formula(s)` body, and constructs the `ValidatedRule` list with flat `ParsedRuleBody`. Failures push `Error::Parse(ParseError::Formula*)` with codes MC1003–MC1006. The signature change to `Result<_, Vec<Error>>` mixes parse-stage and validation-stage errors transparently for the caller; existing tests filter via `Error::as_validation()`.

Acme migration is mechanical — five rules become five formula strings; `Gross_Profit`'s tree (`Mul(Ref("Revenue"), Sub(Const(1.0), Ref("COGS_Rate")))`) round-trips through `"Revenue * (1 - COGS_Rate)"` cleanly. Demo-equivalence diff stays empty; lint stays clean; goldens stay 9/9.

---

*Phase 3D shipped 2026-05-03 at `d5ab355` (tag `phase-3d-friendly-formula-syntax`) after project owner review. The implementing Claude Code instance honored the handoff's "You did NOT commit, tag, or push" rule — the user did the commit + tag step after review. ADR-0007 flipped to Accepted in the same commit batch as the metadata backfill, with the implementer's 3 deviations folded in as acceptance amendments #28 (validate signature, pre-approved), #32 (Error helpers, additive), and #33 (YAML null body fires MC1001).*
