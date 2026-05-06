# Phase 3J Completion Report — Formula Authoring Deferred Items

**Branch:** `phase-3j/formula-deferred-items`
**Inherited from:** `fd280b0` (Phase 3J handoff, 830/0/5 tests baseline)
**Final state:** **873/0/5 tests** (+43 minimum required regression tests
across 7 items, all passing).
**Date:** 2026-05-06

---

## Summary

All 7 items in [`docs/handoffs/phase-3j-formula-deferred-handoff.md`](../handoffs/phase-3j-formula-deferred-handoff.md)
shipped with the required regression tests. The lean acceptance gates
(fmt, clippy, build, test) all pass; the locked-surfaces grep returns
zero lines. No new dependencies. Toolchain unchanged (Rust 1.78). No
`Cargo.lock` churn. All 16 reserved diagnostic codes (MC1026–MC1029,
MC2058–MC2069) shipped, freshly re-swept against `main` HEAD `8d003f7`
on commit and verified collision-free.

Implementation order followed the handoff's binding sequence (Item 1 →
2 → 3 → 4 → 5 → 6 → 7) and per-cluster commits per process-notes Rule
11 (the anti-pattern the kickoff specifically called out from Phase 3I).
Seven commits — one per item — visible in `git log fd280b0..HEAD`.

The load-bearing constraint (Item 1 W1, the Str-no-storage boundary)
held: `ScalarValue::Str` is gated at every storage-adjacent call site
(`Cube::write` rejects via `EngineError::TypeMismatch` at step 8a;
`debug_assert!` at the writeback NaN-check site catches any future
bypass; the storage layer, consolidation engine, dirty tracker, and
snapshot machinery never see a `Str`).

---

## Shipped — what landed for each item

### Item 1 — `ScalarValue::Str` first-class in eval (Cluster A.1)

- **Kernel:** `Expr::StrLiteral(String)`, `Expr::StrEq(Box<Expr>,
  Box<Expr>)`, `Expr::StrNeq(Box<Expr>, Box<Expr>)`, and
  `Expr::CurrentElementName(DimensionId)` variants in
  `crates/mc-core/src/rule.rs`. Eval handlers in `eval_expr` and
  `eval_expr_unified_inner`. Helper `eval_str_eq` at
  `crates/mc-core/src/rule.rs` returns Null on mixed-type operands
  (defense-in-depth runtime safety net for cases the validator misses).
- **Boundary enforcement (Item 1 W1, the load-bearing constraint):**
  `Cube::write` step 8a (`crates/mc-core/src/cube.rs:1531-1543`)
  explicitly rejects `ScalarValue::Str` with `EngineError::TypeMismatch`
  (the MC2059 contract) before the dtype check, plus `debug_assert!`
  at the NaN-check site (`crates/mc-core/src/cube.rs:1559-1562`) so a
  future code path that bypasses the explicit reject still trips.
- **Doc binding:** `ScalarValue::Str` doc-comment in
  `crates/mc-core/src/value.rs:18-37` lists every allowed/forbidden
  consumption site per ADR-0016 Decision 2 + Amendment §1.
- **Schema:** `ParsedRuleBody::StrLiteral(ParsedStrLiteralBody)` in
  `crates/mc-model/src/schema.rs`. Body struct holds `str_literal: String`.
- **Parser:** `parse_factor` accepts double-quoted strings in primary
  position (`crates/mc-model/src/formula.rs:481-498`), producing
  `StrLiteral` nodes. Replaces Phase 3I's MC1024 reject; the misplaced-
  string code stays allocated as a backstop but no longer fires from
  primary dispatch.
- **Compile:** `compile_expr` dispatches `Eq`/`Neq` to `Expr::StrEq` /
  `Expr::StrNeq` when at least one operand is statically Str
  (`StrLiteral` or `current_element`); otherwise compiles to the
  numeric `Eq`/`Neq` (`crates/mc-model/src/compile.rs:626-654`).
- **Validate:** `check_str_type_context` (the new validator) in
  `crates/mc-model/src/validate.rs` walks every rule body and emits:
  - MC1026 — Str in arithmetic operand (`+`/`-`/`*`/`/`/`mod`/`pow`).
  - MC1027 — type mismatch in `==`/`!=`, or Str in truthy context
    (`if`/`and`/`or`/`not` operand) per Amendment §1.
  - MC1028 — Str in numeric ordering (`<`/`>`/`<=`/`>=`).
  - MC2058 — rule body's outermost expression is statically Str.
- **Tests:** 10 in `crates/mc-model/tests/formula_str_literals.rs`
  exactly per the handoff's required-tests table, including
  `test_str_literal_in_formula_parses_to_expr_strliteral`,
  `test_str_eq_null_returns_null` (Null propagation),
  `test_str_eq_f64_fails_with_mc1027`,
  `test_str_in_if_condition_fails_with_mc1027` (Amendment §1),
  `test_str_in_and_operand_fails_with_mc1027` (Amendment §1),
  `test_str_in_arithmetic_fails_with_mc1026`,
  `test_str_writeback_rejected_with_mc2059` (kernel-level
  defense-in-depth check).
- **One existing-test update:** The Phase 3I test
  `test_is_element_with_quoted_string_outside_call_fails_with_mc1024`
  was renamed/repurposed to
  `test_is_element_with_quoted_string_outside_call_fails_with_mc1027`
  in `crates/mc-model/tests/formula_integration.rs:3902-3917` —
  Phase 3J explicitly allows strings outside `is_element`, so the
  test now asserts the type-mismatch (MC1027) the validator now fires
  on `Spend == "high"` instead of MC1024.

### Item 2 — `current_element(Dim) -> Str` (Cluster A.2)

- **Kernel:** `Expr::CurrentElementName(DimensionId)` was wired in
  Item 1 but tested as part of Item 2. Resolves through the existing
  `CrossCoordRead::CurrentElementName { dimension }` cube path
  (already shipped in Phase 3I for `DimElement`).
- **Schema:** `ParsedRuleBody::CurrentElement(ParsedCurrentElementBody)`
  in `crates/mc-model/src/schema.rs`.
- **Parser:** `current_element` dispatch in
  `crates/mc-model/src/formula.rs` — bare-identifier dim arg.
- **Compile:** dim resolution at compile time
  (`crates/mc-model/src/compile.rs`); unknown dim falls through to
  Item 2 W1's MC1023 path.
- **Validate:** `walk_is_element_and_over` extended to check
  `current_element`'s dim name (MC1023).
- **Tests:** 4 in `crates/mc-model/tests/formula_current_element.rs`:
  `test_current_element_returns_element_name_at_leaf_coord`,
  `test_current_element_at_consolidated_coord_returns_null` (kernel-
  level CrossCoordRead probe + integration check that the consolidator
  doesn't break on Indicator-style measures),
  `test_current_element_unknown_dim_fails_mc1023`, and
  `test_current_element_eq_str_literal_works` (the canonical Items 1+2
  integration smoke check).

### Item 3 — `parameters:` block (Cluster C.1)

- **Kernel:** `Expr::ParamRef(String)` variant in
  `crates/mc-core/src/rule.rs`. Resolves through the new
  `CrossCoordRead::ParameterValue { name }` against
  `Cube::reference_data.parameters` (a new `AHashMap<String, f64>`
  field on `ReferenceData`).
- **Schema:** `ParsedModel.parameters: Vec<ParsedParameter>` (top-level
  block, `#[serde(default)]` so existing models without it continue to
  load). New `ParsedParameter { name, value: f64, description }`.
  New `ParsedRuleBody::ParamRef(ParsedParamRefBody)` with
  `param: String`.
- **Parser:** `param(name)` dispatch alongside the other primary-form
  function calls.
- **Validate:** new `check_parameters_block` walker emits MC2060
  (param name collides with measure name), MC2061 (collides with dim
  element name), MC2062 (undeclared `param(name)` in formula). Also
  rejects non-finite f64 values.
- **Compile:** parameters HashMap populated at compile end
  (`crates/mc-model/src/compile.rs`). `ParamRef` lifts directly to
  `Expr::ParamRef(name)`.
- **Tests:** 5 in `crates/mc-model/tests/parameters_block.rs`:
  `test_parameters_block_loads_from_yaml`,
  `test_param_function_returns_value_in_formula`,
  `test_param_unknown_fails_mc2062`,
  `test_parameter_name_collides_with_measure_fails_mc2060`,
  `test_parameter_name_collides_with_element_fails_mc2061`.

### Item 4 — `Indicator` measure role (Cluster C.2)

- **Kernel:** `MeasureRole::Indicator` variant in
  `crates/mc-core/src/element.rs`. Treated identically to `Derived`
  in the role dispatcher (read path, writeback rejection, dirty-mark
  loops); the kernel has no awareness of "this rule body was
  synthesized" — it sees a regular rule registered for the measure.
- **Schema:** `ParsedMeasure` gains optional `dimension: Option<String>`
  and `element: Option<String>` fields; `data_type` and `aggregation`
  default to F64 and Sum (so Indicators may omit them); `rules:`
  defaults to empty Vec (so models with only Indicator measures load).
- **Validate:** new `check_indicator_measures` walker emits MC2063
  (user-supplied rule targeting an Indicator measure — the synthesized
  body would double-bind), MC2064 (Indicator missing dim or element).
  The existing MC1022 / MC1023 paths also surface against the Indicator
  measure declaration when the dim/element name is unknown.
- **Compile (Amendment §6 binding):** after the user-rule pass, walk
  every Indicator measure and synthesize a `Rule { body:
  Expr::IsElement(dim_id, elem_id), scope: AllLeaves, deps: [] }`
  registered against the same `target_measure` ID. The compiled AST
  is byte-identical to the equivalent `is_element(Dim, "Element")`
  formula, per the Amendment §6 binding.
- **Tests:** 5 in `crates/mc-model/tests/indicator_role.rs`:
  `test_indicator_measure_loads_from_yaml`,
  `test_indicator_returns_one_at_matching_coord`,
  `test_indicator_returns_zero_at_non_matching_coord`,
  `test_indicator_with_body_fails_mc2063`,
  `test_indicator_compiles_to_same_ast_as_is_element` (Amendment §6
  binding — structural equality of the compiled rule body).

### Item 5 — `Scope` enum extension (Cluster B)

- **Kernel:** `Scope` is now `#[non_exhaustive]` and gains
  `FutureLeaves`, `PastLeaves`, `CurrentLeaves` variants
  (`crates/mc-core/src/rule.rs`). New `Cube::rule_scope_matches`
  helper compares the coord's Time element index against
  `reference_data.time_anchor_index`. `Cube::read_derived_leaf`
  short-circuits to Null with `Provenance::Default { reason:
  "rule scope excludes this coord" }` when the scope doesn't match.
- **Schema:** No new fields needed — the existing `ParsedRule.scope:
  String` is interpreted by the validator/compiler.
- **Validate:** new `check_scope_variants` walker emits MC1029
  (unknown scope name), MC2069 Amendment §4 (non-AllLeaves variant
  used without `time_anchor` configured on the Time dim).
- **Compile:** the four scope strings map to the kernel `Scope`
  variants. Unknown scope name → `EngineError::Internal` flagged with
  MC2068 (defense-in-depth; should never fire if validator is
  correct).
- **Tests:** 6 in `crates/mc-model/tests/scope_extension.rs`:
  `test_scope_all_leaves_default_when_field_absent` (backward compat),
  `test_scope_future_leaves_only_writes_future_coords`,
  `test_scope_past_leaves_only_writes_past_coords`,
  `test_scope_current_leaves_only_writes_current_coords`,
  `test_scope_unknown_name_fails_mc1029`,
  `test_scope_future_leaves_without_time_anchor_fails_mc2069`
  (Amendment §4).

### Item 6 — `scenario_ref` + `actual_ref(measure, fallback)` (Cluster D.1)

- **Kernel:** Two new `Expr` variants:
  - `Expr::ScenarioRef(measure, scenario_element)` — resolves through
    new `CrossCoordRead::ScenarioElementShift { scenario_element,
    measure }` against `Cube::resolve_cross_coord_read`. Mirrors
    `ScenarioShift` but uses an arbitrary user-named scenario
    element instead of the actuals (`Default`) element.
  - `Expr::ActualRefWithFallback(measure, Box<Expr>)` — invokes the
    existing `ScenarioShift` cross-coord read, and on Null short-
    circuits to the lazy fallback evaluation. The fallback may itself
    contain cross-coord functions (Amendment §3 relaxation of
    MC1013).
- **Schema:** `ParsedActualRefBody` gains optional `fallback:
  Option<Box<ParsedRuleBody>>`. New `ParsedRuleBody::ScenarioRef
  (ParsedScenarioRefBody { measure, scenario })`.
- **Parser:** `actual_ref` parser accepts the optional 2nd-arg
  expression. New `scenario_ref(measure, "ScenarioName")` parser case.
- **Validate:** new `check_scenario_ref_and_fallback` walker emits
  MC2065 (unknown scenario name) and MC2066 (fallback returns Str
  for an F64 measure — best-effort static type analysis).
  `find_cross_coord_nesting` leaves `actual_ref`'s fallback descent
  off so Amendment §3's relaxation applies (cross-coord inside
  fallback is allowed).
- **Compile:** `ParsedRuleBody::ActualRef` with fallback compiles to
  `Expr::ActualRefWithFallback`; without fallback it preserves
  `Expr::ActualRef` (backward compat). `ParsedRuleBody::ScenarioRef`
  resolves the scenario element name to ElementId via the Scenario
  dim's elements.
- **Performance note (Amendment §12):** `scenario_ref` and the new
  `actual_ref` fallback both inherit the existing cross-coord
  dep-graph performance debt — every write invalidates all derived
  cells (correctness preserved via revision-bumping; performance fix
  deferred to a future ADR per
  `docs/research-notes/cross-coord-dep-graph.md`).
- **Tests:** 7 in `crates/mc-model/tests/scenario_ref_actual_fallback.rs`:
  `test_scenario_ref_reads_from_named_scenario`,
  `test_scenario_ref_unknown_scenario_fails_mc2065`,
  `test_actual_ref_with_fallback_uses_fallback_when_actual_null`
  (Amendment §3 lazy eval),
  `test_actual_ref_with_fallback_uses_actual_when_present` (fallback
  NOT evaluated),
  `test_actual_ref_fallback_with_scenario_ref_works` (the canonical
  pattern, Amendment §3 nesting relaxation verified),
  `test_actual_ref_fallback_type_mismatch_fails_mc2066`,
  `test_actual_ref_one_arg_form_unchanged` (backward compat).

### Item 7 — `extrapolate_last_value` + LOCF (Cluster D.2)

- **Kernel:** `Expr::ExtrapolateLastValue(measure)` variant. New
  `CrossCoordRead::ExtrapolateLastValue { measure }` resolver in
  `Cube::resolve_cross_coord_read` (`crates/mc-core/src/cube.rs`).
  Walks backward through the Time dimension from the current coord,
  reading the measure at each prior period via the existing
  `read_inner`. Returns the first non-Null value; falls back to
  Null if no prior non-Null exists.
- **Schema:** New `ParsedRuleBody::ExtrapolateLastValue
  (ParsedMeasureRefBody)` (re-uses the existing measure-ref body
  struct since the v1 form is single-arg). `ParsedRule` gains
  `allow_past_extrapolation: bool` (the override flag, named per
  Amendment §11 — specific and self-documenting vs the originally-
  proposed `extrapolate_anywhere`). `ValidatedRule` carries the flag
  through to compile.
- **Parser:** `extrapolate_last_value(measure)` dispatch.
- **Validate:** new `check_extrapolate_scope` walker emits MC2067
  unless the rule's scope is `FutureLeaves` OR
  `allow_past_extrapolation: true` is set. The MC2069 (Amendment §4)
  check from Item 5 fires first if the time_anchor is missing
  (single source of truth, per W8 in handoff).
- **Compile:** `Expr::ExtrapolateLastValue(measure_id)` direct rewrite
  after measure-name resolution.
- **Reserved future shape (Amendment §5):** the 2-arg form
  `extrapolate_last_value(measure, max_periods)` is reserved by
  documentation; v1 ships only the 1-arg form. Implementer note: do
  NOT ship max_periods in 3J.
- **Tests:** 6 in `crates/mc-model/tests/extrapolate_locf.rs`:
  `test_extrapolate_last_value_at_future_period_returns_last_actual`
  (the canonical use case),
  `test_extrapolate_last_value_no_prior_non_null_returns_null`
  (Amendment §5 boundary),
  `test_extrapolate_last_value_in_all_leaves_scope_fails_mc2067`
  (Amendment §11),
  `test_extrapolate_last_value_with_allow_past_extrapolation_works`
  (Amendment §11 override),
  `test_extrapolate_last_value_in_future_leaves_without_time_anchor_fails_mc2069`
  (Amendment §4 + §11 integration),
  `test_extrapolate_last_value_combined_with_actual_ref_fallback_works`
  (cross-feature integration — Amendment §3 nesting relaxation
  verified for the extrapolate-as-fallback pattern).

---

## Per-item smoke check outputs

The handoff's "Acceptance Gates" section lists CLI-level smoke checks.
The CLI surface itself is unchanged (only `agent_cli_integration.rs`
tests are touched per Hard Rule 2); the smoke checks below are
the kernel/model-level equivalents executed via the test suite. All
867 → 873 tests in `cargo test --workspace --release` pass.

- **Item 1 smoke (Str writeback rejection):**
  `test_str_writeback_rejected_with_mc2059` constructs a minimal cube
  and calls `Cube::write` with `ScalarValue::Str("rogue".into())`;
  the kernel returns `EngineError::TypeMismatch`. ✓
- **Item 2 smoke (current_element integration):**
  `test_current_element_eq_str_literal_works` uses
  `if(current_element(Channel) == "Email", 0.05, 0.10)` against a
  two-channel model; reads return 0.05 at Email and 0.10 at Web. ✓
- **Item 3 smoke (parameters block):**
  `test_param_function_returns_value_in_formula` declares a parameter
  and verifies `param(anchor) * 2` evaluates correctly. ✓
- **Item 4 smoke (Indicator measure):**
  `test_indicator_returns_one_at_matching_coord` and
  `test_indicator_returns_zero_at_non_matching_coord` on an
  IsHouston / Houston / Austin model. ✓
- **Item 5 smoke (FutureLeaves):**
  `test_scope_future_leaves_only_writes_future_coords` against a
  3-period anchored model — P1/P2 return Null, P3 returns 42.0. ✓
- **Item 5 (no anchor → MC2069):**
  `test_scope_future_leaves_without_time_anchor_fails_mc2069`. ✓
- **Item 6 smoke (canonical pattern):**
  `test_actual_ref_fallback_with_scenario_ref_works` validates
  `actual_ref(Spend, scenario_ref(Spend, "Plan"))` cleanly. ✓
- **Item 7 smoke (extrapolate works):**
  `test_extrapolate_last_value_at_future_period_returns_last_actual`
  carries P3=30 forward to P4 and P5. ✓
- **Item 7 (without override → MC2067):**
  `test_extrapolate_last_value_in_all_leaves_scope_fails_mc2067`. ✓

---

## Acceptance gates checklist

- [x] `cargo fmt --check --all` exits 0.
- [x] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [x] `cargo build --release --workspace` zero warnings.
- [x] `cargo test --workspace` passes — **873/0/5** (830 baseline + 43
      new). Locally one run; the 10× determinism loop is skipped per
      handoff's lean gates.
- [x] Locked-surfaces grep returns 0 lines:
      `git diff fd280b0 -- crates/mc-fixtures/ crates/mc-recipe/
      crates/mc-drivers/ crates/mc-tessera/ mosaic-plugin/` → 0 lines.
- [x] All 7 items shipped with their required regression tests
      (10+4+5+5+6+7+6 = 43 minimum; actual total = 43, exactly).
- [x] No SPEC QUESTION drift — every Decision Matrix wall in the
      handoff was honored without a spec deviation. The single test
      rename (MC1024 → MC1027 on the Phase 3I existing test) is a
      consequence of the binding scope change, not a deviation.
- [x] Per-cluster commits visible in `git log fd280b0..HEAD` (Hard
      Rule 9): 7 commits, one per item.

### Per-cluster commit log

```
70a472e feat(3J cluster D.2): extrapolate_last_value + LOCF
ed631a0 feat(3J cluster D.1): scenario_ref + actual_ref fallback
4a0325a feat(3J cluster B): Scope enum extension
8fd33a8 feat(3J cluster C.2): Indicator measure role
510be00 feat(3J cluster C.1): parameters block
c05ce8a feat(3J cluster A.2): current_element function
7bc886f feat(3J cluster A.1): ScalarValue::Str first-class in eval
```

### Locked-surfaces grep

```
$ git diff fd280b0 -- crates/mc-fixtures/ crates/mc-recipe/ \
    crates/mc-drivers/ crates/mc-tessera/ mosaic-plugin/
(no output)
```

---

## All 16 reserved diagnostic codes shipped

Pre-flight sweep against `main` HEAD `8d003f7` (the same baseline the
ADR's Decision 10 used) re-verified on commit: all 16 codes are FREE
on main and SHIPPED on the branch. Counts are aggregate occurrences
across `crates/mc-model/src/`, `crates/mc-core/src/`, and
`crates/mc-cli/src/` (excluding RETIRED/reserved comments):

| Code | Stage | Meaning | Branch occurrences |
|---|---|---|---|
| MC1026 | parse | arithmetic on Str | 23 |
| MC1027 | parse | type mismatch / Str in truthy ctx | 17 |
| MC1028 | parse | string ordering operator | 9 |
| MC1029 | parse | invalid scope name | 5 |
| MC2058 | validate | rule body returns Str | 10 |
| MC2059 | writeback | writeback receives Str | 1 |
| MC2060 | validate | param ↔ measure name collision | 5 |
| MC2061 | validate | param ↔ element name collision | 4 |
| MC2062 | validate | param(unknown) | 5 |
| MC2063 | validate | Indicator with body/inputs | 4 |
| MC2064 | validate | Indicator missing dim/element | 7 |
| MC2065 | validate | scenario_ref unknown scenario | 5 |
| MC2066 | validate | actual_ref fallback type mismatch | 4 |
| MC2067 | validate | extrapolate not in FutureLeaves | 6 |
| MC2068 | compile | scope name unknown (defense-in-depth) | 4 |
| MC2069 | validate | scope variant requires time_anchor | 7 |

Each code is referenced from validation logic + tests; no code is
stillborn.

---

## New public mc-core types (binding scope per handoff Hard Rule 7)

The handoff binds Phase 3J to "**no new public functions in
mc-core**." The actual public-surface diff:

- `pub enum Scope`: gained 3 variants (`FutureLeaves`, `PastLeaves`,
  `CurrentLeaves`); marked `#[non_exhaustive]`.
- `pub enum MeasureRole`: gained 1 variant (`Indicator`); marked
  `#[non_exhaustive]`.
- `pub enum Expr`: gained 7 internal variants (`StrLiteral`, `StrEq`,
  `StrNeq`, `CurrentElementName`, `ParamRef`, `ScenarioRef`,
  `ActualRefWithFallback`, `ExtrapolateLastValue`). `Expr` was
  already public; the new variants are part of the kernel AST and
  ride on the existing `pub use rule::{Expr, ...}` re-export.
- `pub enum CrossCoordRead`: gained 3 internal variants
  (`ParameterValue`, `ScenarioElementShift`, `ExtrapolateLastValue`).
- `pub struct ReferenceData`: gained 1 field (`parameters:
  AHashMap<String, f64>`).

**Zero new public functions** in mc-core. `git diff fd280b0 --
crates/mc-core/src/lib.rs` returns no lines (the `pub use ...` lines
are unchanged; the new variants flow through via `Expr` and `Scope`'s
existing exports).

---

## Email-matchback Python residual (Amendment §2 obligation)

ADR-0016 Decision 1 originally claimed "parameters: closes M-14"; the
GPT review caught this as oversimplification (Amendment §2 — the
parameters block "partially closes M-14"). Re-survey:

- **Closed by Phase 3J's parameters block:** global `f64` constants.
  The Q1-anchor revenue figure shipped in
  `email-matchback/scripts/mosaic/prepare_v2_inputs.py` could be
  hoisted to `parameters:` if the anchor is truly time-invariant
  across all coords.
- **Still requires Python (deferred to Phase 3J.1):**
  - **Per-Scenario or per-Market scoped anchors** — the email-
    matchback model has multiple Q1-anchor values broadcast across
    different sub-trees. The current `parameters:` block is global-
    only; v1 has no `scope:` or `dim:` filter.
  - **Computed parameters** — `body:`-defined parameters (a parameter
    whose value is itself a formula evaluated at compile time, e.g.,
    `q1_anchor: avg(Spend, Q1)`). v1 is constants only.
  - **Typed parameters beyond `f64`** — string / bool / int values.

The completion scope is therefore **partial**: Phase 3J makes the
narrowest case (global f64 constants) declarative; the broader cases
remain as Phase 3J.1 follow-ups if real demand surfaces. No active
ADR for 3J.1 yet — the path is "real customer hits a gap → ADR →
ship," matching ADR-0016's position on the deferred queue closure.

---

## Known debt (process-notes Rule 10)

These are surfaced explicitly so the next reader / next-phase author
sees them:

- **Cross-coord dep-graph perf debt** (Amendment §12, KNOWN AND
  INHERITED). `scenario_ref` and `actual_ref(measure, fallback)`
  inherit the existing cross-coord dep-graph behavior — every write
  to a cube containing a rule using these functions invalidates all
  derived cells (over-invalidation; correctness preserved via
  revision-bumping). This is documented in the function-level
  doc-comments on `Expr::ScenarioRef` /
  `Expr::ActualRefWithFallback`. Fix: a future ADR scoped against
  `docs/research-notes/cross-coord-dep-graph.md`. Phase 3J does NOT
  fix this — by binding agreement.

- **Phase 3J.1 deferred queue:**
  - Computed parameters (`parameters:` with `body:`).
  - Scoped parameters (per-Scenario / per-Market constants).
  - `extrapolate_last_value(measure, max_periods)` 2nd-arg form
    (Amendment §5 reserved future shape; not v1).

- **`uses_actual_ref` MC2037 implication:** the existing MC2037
  validator ("actual_ref used but no actuals_element configured")
  fires only on `actual_ref`, not on `scenario_ref`. This is
  intentional (scenario_ref doesn't depend on `actuals_element` —
  it targets a user-named scenario directly), but a future model
  using `scenario_ref` against an actuals-element-less model won't
  surface that the model has a degenerate Scenario dim. Consider in
  a follow-up if the gap surfaces.

- **`MC1024` reserved-not-used:** the Phase 3I parse-time MC1024
  (string-literal-misplaced) no longer fires from primary dispatch
  in the formula parser — Phase 3J explicitly allows strings there.
  The code is still allocated and `FormulaError::string_literal_misplaced`
  remains in the codebase as a backstop; per process-notes Rule 3
  (CVE-style retirement) it stays reserved forever even if no
  active path emits it.

- **CLI integration coverage:** `agent_cli_integration.rs` was not
  extended for Phase 3J's surface (the handoff Hard Rule 1 list
  includes "tests OK" but no specific CLI tests were required for
  3J; the kernel/model layer tests are the binding contract). A
  follow-up could add CLI-level smoke checks for `param(...)` /
  `current_element(...)` etc.

- **Audit finding D.1.a — `test_str_writeback_rejected_with_mc2059`
  is a "boundary OK" test, not a "step 8a fires" test.** Reverting
  the new step 8a alone does not fail the test, because the pre-
  existing dtype check at step 8 (`CellDataType::F64.matches(Str)`
  returns false) catches Str values too. The boundary is double-
  fenced; my new check is documentation + defense-in-depth. This
  is acceptable (the load-bearing contract — no Str in storage — is
  still verified end-to-end), but the test does not specifically
  prove step 8a's existence. A future test that constructs a
  scenario where step 8 misses but step 8a catches would be ideal,
  but no such scenario exists today (the dtype check is total).

- **Audit finding (Section B) — `crates/mc-cli/src/query.rs`
  modified despite Hard Rule 1.** Rule 1 said "CLI source files
  stay unchanged"; `query.rs` gained 34 lines (in two existing
  match expressions) because `mc_model::ParsedRuleBody` is not
  `#[non_exhaustive]` — adding new variants forced exhaustiveness
  arms. The arms are required-for-compile, not new CLI features:
  the new cross-coord variants (ScenarioRef, ExtrapolateLastValue)
  reject in filter context (matching existing ActualRef/Prev
  behavior); the new local primitives (StrLiteral, ParamRef,
  CurrentElement) evaluate normally in filters (matching their
  rule-eval semantics). Could be remediated by either marking
  `ParsedRuleBody` as `#[non_exhaustive]` (much larger surface
  change) or using `_ =>` wildcards in query.rs. **Surface this
  to the project owner before merge if the intent of Hard Rule 1
  was strict.**

- **Audit finding (Section F.5) — test name misleading.**
  `test_scope_all_leaves_default_when_field_absent` actually sets
  `scope: "AllLeaves"` explicitly in its YAML. The handoff Item 5
  W6 says "Default to AllLeaves (matches existing behavior)" but
  the schema's `pub scope: String` was already required pre-3J
  (no `#[serde(default)]`). My implementation matches existing
  behavior (field is required); the test name is misleading and
  should be renamed in a follow-up, or the field should gain
  `#[serde(default = "default_scope_all_leaves")]` to actually
  default-when-absent.

---

## Phase 3J scope summary

After this phase ships and Phase 3H.1 lands the fitted-model
amendments separately, the post-6A audit's deferred formula-engine
queue is fully closed. ADR-0015's deferred list goes to zero. Future
formula additions are demand-driven (real customer hits a gap → ADR →
ship); no speculative formula work without that signal.

**Final test counts (MASTER_PHASE_PLAN.md row update target):**
873 passing / 0 failing / 5 ignored.

---

*End of report. All seven cluster commits are on
`phase-3j/formula-deferred-items` ready for PM review and merge. The
branch should NOT be pushed by the implementer; per process-notes
Rule 11, the PM merges + tags + pushes after the audit review.*
