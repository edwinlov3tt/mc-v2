# Phase 3J Handoff — Formula Authoring Deferred Items

> **Audience:** the Claude Code instance that implements Phase 3J.
> **You inherit `main` at `1e57eb0` (830 / 0 / 5 tests). You'll work on
> the branch `phase-3j/formula-deferred-items` — see process-notes §11
> for the git workflow rule (single instance, sequential = branch but
> no worktree).**
>
> **This phase closes the formula-authoring deferred queue from
> ADR-0015 Decision 1.** After 3J ships (and Phase 3H.1 lands the
> fitted-model amendments separately), the post-6A audit's deferred
> formula-engine items are completely closed — future formula
> additions become demand-driven (real customer hits a gap → ADR →
> ship), not speculative.
>
> **Hard rule (binding):** Phase 3J makes 4 kernel-adjacent changes
> (`ScalarValue::Str` first-class promotion through eval; `Scope`
> enum extension; `MeasureRole` enum extension; comparison operators
> on strings) and introduces 1 new top-level YAML schema block
> (`parameters:`) plus 1 new measure role (`Indicator`). Mc-core
> touches limited to: new `Expr` variants, new `ScalarValue::Str`
> handling in eval (NOT in storage/consolidation/dirty-tracker —
> see Decision 2 / item 1 W1 below), new `Scope` and `MeasureRole`
> enum variants. **No new public functions in mc-core. No
> `ScalarValue::Str` in stored cells.** See ADR-0016 Decisions 2 + 4
> for the binding boundary.
>
> **Process directive (binding, per process-notes Rule 11 anti-pattern
> from Phase 3I):** **commit AS YOU GO, per-cluster, NOT at the end.**
> Phase 3I shipped 8 items + 45 tests entirely uncommitted, forcing a
> single big merge commit that lost per-item progression in `git log`.
> Phase 3J implementer commits per cluster (Cluster A as one or
> several commits; Cluster B as one; etc.), with descriptive messages
> like `feat(3J cluster A): ScalarValue::Str first-class in eval`.

---

## The one paragraph you must internalize

7 items in 6 implementation steps closing the formula-authoring
deferred queue. The biggest design decision is **Decision 2** (ADR-0016):
`ScalarValue::Str` exists in expression evaluation only — never reaches
storage, consolidation, writeback, or the dirty tracker. Cells stay
numeric-or-Null. Strings flow through eval and get consumed by `==` /
`!=` (returning F64 0.0/1.0) before reaching any storage path. Get
this boundary right and the rest of the work is mechanical extension
of patterns from 3E-3I. Get it wrong (let `ScalarValue::Str` leak into
`Cube::write` or consolidation) and you've turned Phase 3J into a Phase
4 storage-layer rewrite. **All 8 ADR-0016 amendments are binding** —
they're folded into the per-item Decision Matrices below.

---

## Production-quality framing

Same as Phase 3I: this is a no-second-pass phase. The ADR already
absorbed GPT + Claude Desktop review and locked all design decisions
via 8 amendments (§1–§6 from GPT; §11–§12 from Desktop). Your job is
to execute the binding scope, not re-decide it.

If you hit a wall the matrix doesn't cover, file a SPEC QUESTION using
the format in CLAUDE.md §11. Don't guess.

The 3I audit pattern revealed one process improvement now binding for
3J: **per-item commits during implementation, not at the end.** See
"Process directive" above.

---

## Items (7 total in 6 implementation steps)

### Item 1 — `ScalarValue::Str` first-class in eval (Cluster A part 1)

**Files:** `crates/mc-core/src/value.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-core/src/cube.rs` (eval paths only — NOT storage/consolidation), `crates/mc-model/src/formula.rs`.

**The use case.** Foundation for items 2 (`current_element`), the `is_element` enhancement (already shipped in 3I but currently parser-internal-only for the string arg), and item 6's `scenario_ref(measure, "ScenarioName")`. Without this, every other formula function requiring string awareness is impossible.

**The fix.** Promote `ScalarValue::Str(String)` from "parser-internal lookup-key data only" to "first-class expression-evaluation value." String literals in formula source compile to `Expr::StrLiteral(String)`. Eval produces `ScalarValue::Str(...)` values that flow through the eval AST. **String values must NEVER reach `Cube::write`, `HashMapStore`, consolidation, dirty tracker, or the snapshot machinery.**

Add 3 string equality/inequality operators:
- `Str == Str` → `F64(1.0)` if equal, `F64(0.0)` if not
- `Str != Str` → inverse
- `Str == Null` / `Null == Str` → `Null` (Null propagation)
- `Str == F64` / `F64 == Str` → MC1027 (parse-time if both literals; eval-time error if runtime)

**Decision Matrix (the load-bearing matrix for this phase):**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Where is the boundary between "Str OK in eval" and "Str must not appear here"? | **Allowed:** `Expr::StrLiteral`, `current_element` return value, `==`/`!=` operands, `is_element`'s 2nd arg, `scenario_ref`'s 2nd arg, lookup-key conversions (existing `scalar_to_lookup_key`). **Forbidden:** `Cube::write`, `HashMapStore` storage, consolidation, dirty tracker, snapshot, NaN-check, trace, writeback validation. **Walk every call site of `Cube::write` and `cube.store.insert(...)` and assert the value is NEVER `ScalarValue::Str`** — add a `debug_assert!(!matches!(value, ScalarValue::Str(_)))` at writeback's NaN-check site, plus a runtime check in `Cube::write` returning `WritebackError::TypeMismatch` (MC2059). | Strings as cell values would cascade through every kernel subsystem. Bounded scope = Phase 3J shippable; unbounded = Phase 4 storage layer rewrite. |
| W2: What happens at writeback if a rule's body evaluates to Str at runtime? | **MC2058 — rule body returned non-numeric.** Validate-time best-effort detection (walk the AST; if the outermost expression is `StrLiteral` or `current_element` with no comparison consuming it, flag MC2058). Runtime fallback: rule eval returns `Str` → cube writeback rejects with MC2058 (eval-time variant) AND propagates as a typed error to the user. | Defense in depth. Validate catches the static cases; runtime catches the dynamic cases (e.g., `if(cond, "Houston", "Austin")` whose static type might be ambiguous to the validator). |
| W3 (Amendment §1): Where ELSE is Str forbidden beyond arithmetic + numeric comparisons? | **Also forbidden** in: `if(cond, ...)` condition argument; `and(...)` / `or(...)` / `not(...)` operands; `parameters:` block values (per item 3 — F64 only); rule body return; writeback. All emit MC1027 (extended) or the relevant validate code. | Amendment §1 closes the "Str leaks into truthy/falsy contexts" surface. Required tests: `if(current_element(Market), 1, 0)` fails MC1027; `not(current_element(Market))` fails MC1027. |
| W4: How does the parser distinguish a `Str` literal `"Houston"` from a YAML string identifier? | **Double-quoted in formula source.** `body: 'is_element(Market, "Houston")'` — the outer YAML string contains formula source; double-quoted segments inside are Str literals. The formula parser tokenizes them as `Token::StrLiteral("Houston")`. Single-quotes are NOT string literals (used for YAML escaping only). | Standard convention; 3I already does this for `is_element`. |
| W5: What about Unicode in Str literals? | **UTF-8 strings throughout** (Rust's default). Validator does NOT restrict character set. Element names with Unicode work as expected. | No reason to restrict; mc-model already handles UTF-8 element names. |
| W6: Empty string `""`? | **Allowed** as `ScalarValue::Str(String::new())`. `"" == ""` → 1.0. `is_element(Market, "")` validate-time errors with MC1022 (no element matches empty name). | Empty is a valid string; comparisons just return appropriately. |
| W7: Should `Str("Houston") == Str("houston")` (case difference) be 1.0 or 0.0? | **0.0 (case-sensitive comparison).** Element names are case-sensitive throughout Mosaic; comparison must match. | Consistency with element name lookup. |
| W8: Comparison ordering operators (`<`, `>`, `<=`, `>=`) on Str? | **Forbidden — MC1028.** Locale-dependent; out of scope per ADR-0016 Decision 3. | Bounded scope. |
| W9: Should the existing `scalar_to_lookup_key` function be touched? | **No.** Its current handling of `ScalarValue::Str` (transient lookup-key conversion) is correct and already-shipped. Don't refactor what works. | Don't expand scope. |

**Required regression tests (10 minimum):**
1. `test_str_literal_in_formula_parses_to_expr_strliteral`.
2. `test_str_equality_returns_f64_one_when_equal`.
3. `test_str_equality_returns_f64_zero_when_unequal`.
4. `test_str_inequality_inverse_of_equality`.
5. `test_str_eq_null_returns_null` (Null propagation).
6. `test_str_eq_f64_fails_with_mc1027` (type mismatch).
7. `test_str_in_if_condition_fails_with_mc1027` (Amendment §1).
8. `test_str_in_and_operand_fails_with_mc1027` (Amendment §1).
9. `test_str_in_arithmetic_fails_with_mc1026`.
10. `test_str_writeback_rejected_with_mc2059`.

---

### Item 2 — `current_element(Dim) -> Str` (Cluster A part 2)

**Files:** `crates/mc-model/src/formula.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-core/src/cube.rs`.

**The use case.** Inline branching on which dimension element the current coordinate sits at, without declaring a separate Indicator measure or repeating `is_element(Dim, "X")` for every possible value. Common in `if`/`switch`/conditional formulas.

**Example:**
```yaml
body: "if(current_element(Channel) == 'Email', 0.05, 0.10)"
```

**The fix.** New formula function `current_element(Dim)` that returns `ScalarValue::Str(<element_name>)` at the current coordinate. Compiles to `Expr::CurrentElementName(DimensionId)` (parse-time dim resolution). Eval reads the coord's element in `Dim` and returns the element's name.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: How is the dim arg resolved? Parse-time or eval-time? | **Parse-time.** Validator resolves `current_element(Channel)` → `Expr::CurrentElementName(DimensionId(N))`. Unknown dim → MC1023 (existing). | Consistency with `is_element` from Phase 3I. |
| W2: What if `Dim` is the Measure dimension? | **Allowed.** Returns the current measure's name (e.g., "Spend"). Useful for measure-aware conditional logic. | No special case needed. |
| W3: Eval at consolidated coord (where Dim has multiple leaf elements)? | **Returns Null.** Consolidated coords don't have a single element in any dimension; the answer is undefined. Document explicitly. | Avoids "first leaf" or "AllX" ambiguity. |
| W4: Can `current_element` return value be stored as a measure? | **No** (per item 1 W2 — MC2058 rule body returned Str). Use Indicator role (item 4) or `is_element` (3I) for storeable boolean flags. | Decision 2 binding. |
| W5: Composability with `==` (item 1)? | **Required test:** `current_element(Market) == "Houston"` returns 1.0 at Houston coords, 0.0 elsewhere. This is the canonical use case. | Smoke test for the foundation. |

**Required regression tests (4 minimum):**
1. `test_current_element_returns_element_name_at_leaf_coord`.
2. `test_current_element_at_consolidated_coord_returns_null`.
3. `test_current_element_unknown_dim_fails_mc1023`.
4. `test_current_element_eq_str_literal_works` (the integration test for items 1+2).

---

### Item 3 — `parameters:` block (Cluster C part 1)

**Files:** `crates/mc-model/src/schema.rs`, `crates/mc-model/src/validate.rs`, `crates/mc-model/src/compile.rs`, `crates/mc-model/src/formula.rs`, `crates/mc-core/src/cube.rs` (or wherever ParsedModel becomes CompiledCube).

**The use case (partial closure of M-14 per Amendment §2).** Named scalar constants for use in formulas. v1 is **constants only** (no `body:` for computed parameters). Closes the "global f64 constant" subset of M-14; the per-Scenario/per-Market scoped anchors remain deferred to Phase 3J.1.

**Schema (binding):**
```yaml
parameters:
  - name: q1_anchor_revenue
    value: 1234.56              # required, f64 only
    description: "Q1 2026 revenue baseline"   # optional
```

**Reference syntax in formulas:** `param(name)`. Bare `name` is forbidden (collides with measure names + dim element names; ambiguous).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Where in the schema does `parameters:` live? | **Top-level field on `ParsedModel`** (sibling of `dimensions`, `measures`, `rules`). | Standard YAML schema additive pattern. |
| W2: What if a parameter name collides with a measure name? | **MC2060** at validate. | Decision 6 binding. |
| W3: What if a parameter name collides with a dim element name? | **MC2061** at validate. | Same. |
| W4: What if `param(unknown)` is referenced in a formula? | **MC2062** at validate. | Reference resolution. |
| W5: What's the eval-path lookup cost? | **Single HashMap lookup.** `CompiledCube` carries a `parameters: HashMap<String, f64>`. Eval of `Expr::ParamRef(name)` is `parameters.get(&name).copied().unwrap_or(0.0_f64)` — wait, no — `unwrap_or` would mask MC2062. Validate must enforce all `Expr::ParamRef` names exist; eval can `expect()` since validate guaranteed it. | Defense in depth: validator catches; eval trusts. |
| W6: Are parameters referenced in the dependency graph? | **No.** Parameters are constants, not cells. They have no dependencies and are not invalidated by writes. Eval of `param(name)` does NOT add a coord to the rule's `actual_reads`. | Constants don't participate in dirty propagation. |
| W7: Can a parameter value be a string? | **No** (per Amendment §1 / Decision 6). `value:` must be a f64 literal. Non-f64 → MC2060 extended (parameter value must be numeric). | Decision 6 + Amendment §1 binding. |
| W8: Can parameters reference other parameters (e.g., `value: param(other)`)? | **No** in v1. Parameters are LITERAL values only. If demanded, parameter chaining ships in Phase 3J.1. | Constants only per Decision 6. |
| W9: Schema version bump? | **No.** Additive top-level field; existing parsers see it as ignored. | Backward compat. |

**Required regression tests (5 minimum):**
1. `test_parameters_block_loads_from_yaml`.
2. `test_param_function_returns_value_in_formula`.
3. `test_param_unknown_fails_mc2062`.
4. `test_parameter_name_collides_with_measure_fails_mc2060`.
5. `test_parameter_name_collides_with_element_fails_mc2061`.

---

### Item 4 — `Indicator` measure role (Cluster C part 2)

**Files:** `crates/mc-core/src/measure.rs` (or wherever `MeasureRole` is defined), `crates/mc-model/src/schema.rs`, `crates/mc-model/src/validate.rs`, `crates/mc-model/src/compile.rs`.

**The use case (per Amendment §6).** Reusable declarative indicator measures (`IsHouston`, `IsAustin`, etc.) for MMM regression feature vectors. Closes the email-matchback `prepare_mmm_inputs.py` 464-row CSV generation.

**Schema (binding):**
```yaml
measures:
  - name: IsHouston
    role: Indicator                # NEW MeasureRole variant
    dimension: Market              # required for Indicator
    element: Houston               # required for Indicator
    description: "1.0 at Houston coords, 0.0 elsewhere"
    # NO body, NO inputs (validator rejects with MC2063)
```

**The fix (per Amendment §6 binding):** `Indicator` measures **MUST compile to the same `Expr::IsElement(DimensionId, ElementId)` AST that `is_element(Dim, "Element")` produces.** No second evaluation path. The role is a schema-layer convenience for declarative reusable Indicators; under the hood, it's identical to writing `is_element(Market, "Houston")` inline.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: How is `MeasureRole::Indicator` represented? | **New variant on existing `MeasureRole` enum** (`pub enum MeasureRole { Input, Derived, Indicator, ... }`). The enum is `#[non_exhaustive]` per existing convention. | Standard enum extension. |
| W2: What does the `Cube` see for an Indicator measure? | **A derived measure with an implicit rule body** that's equivalent to `is_element(Dim, "Element")`. The compiler synthesizes this rule at compile time; the user's YAML doesn't have a `body:` field. | Per Amendment §6: same Expr AST as is_element function. |
| W3: Is the Indicator measure a "Derived" or "Input" measure semantically? | **Derived** (it's evaluated, not stored). Cells aren't writable; `Cube::write` to an Indicator coord fails with `WritebackError::DerivedNotWritable`. | Implicit derivation. |
| W4: What if the user writes `body:` AND `role: Indicator`? | **MC2063** — Indicator measures must have NO `body:` field. | Schema clarity. |
| W5: What if `dimension:` or `element:` is missing? | **MC2064** — Indicator measures require both. | Schema clarity. |
| W6: Can Indicators reference dim elements that don't exist? | **MC1022** (existing — same code is_element uses). Reuse the existing reference-resolution path. | Code reuse. |
| W7: Aggregation rule for Indicator measures (consolidation)? | **Sum** by default. At consolidated coords, the Indicator's value is the count of leaf coords matching (Indicator is 1.0 at matching leaves). User can override via `aggregation:` field if needed. | Default consolidation matches existing measure pattern. |
| W8: Should the snapshot test (Amendment §6) compare AST byte-for-byte? | **Yes.** A test fixture with one model declaring `IsHouston: role: Indicator, dimension: Market, element: Houston` and another model with `body: "is_element(Market, \"Houston\")"` should produce byte-identical compiled `Expr::IsElement(...)` nodes. Snapshot the AST or assert structural equality. | Amendment §6 binding. |

**Required regression tests (5 minimum):**
1. `test_indicator_measure_loads_from_yaml`.
2. `test_indicator_returns_one_at_matching_coord`.
3. `test_indicator_returns_zero_at_non_matching_coord`.
4. `test_indicator_with_body_fails_mc2063`.
5. `test_indicator_compiles_to_same_ast_as_is_element` (Amendment §6 — snapshot or structural equality test).

---

### Item 5 — `Scope` enum extension (Cluster B)

**Files:** `crates/mc-core/src/scope.rs` (or wherever `Scope` lives), `crates/mc-model/src/schema.rs`, `crates/mc-model/src/validate.rs`, `crates/mc-model/src/compile.rs`, `crates/mc-core/src/rule.rs` (or wherever rule eval respects scope).

**The use case (per Amendment §4).** Foundation for item 7 (`extrapolate_last_value` requires `FutureLeaves` scope). Generally enables future-only / past-only / current-only rules without complex dep-graph hacks.

**Binding:**
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
    AllLeaves,           // existing
    FutureLeaves,        // NEW: leaves where is_future() is true
    PastLeaves,          // NEW: leaves where is_past() is true
    CurrentLeaves,       // NEW: leaves where is_current() is true
}
```

**YAML usage:**
```yaml
rules:
  - name: extend_adspend
    target: AdSpend
    scope: FutureLeaves            # NEW; was AllLeaves only
    body: "extrapolate_last_value(AdSpend)"
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: How does the rule eval respect scope? | **At eval time**, before computing each leaf coord, check `scope.matches(coord, time_anchor)`. If false, skip — don't write or invalidate. The rule's `target` measure cells outside scope retain whatever value they had (typically Null or input). | Sparse-rule semantics. |
| W2 (Amendment §4): What if `time_anchor` isn't configured but a `FutureLeaves` rule exists? | **MC2069** at validate — "scope variant requires `time_anchor` configured on Time dim". `Scope::AllLeaves` continues to work without `time_anchor`. | Amendment §4 binding. |
| W3: Unknown scope name in YAML (e.g., typo `FutureLeves`)? | **MC1029** at parse. | Standard. |
| W4: Compile-time defense if validator misses an unknown name? | **MC2068** internal/defense (should never fire if validator is correct). | Defense in depth. |
| W5: Can a single rule have multiple scopes (e.g., `scope: [PastLeaves, CurrentLeaves]`)? | **No.** v1 supports exactly one scope per rule. Multiple-scope rules can ship in future amendment if demanded. | Bounded scope. |
| W6: Backward compat — existing rules without `scope:` field? | **Default to `AllLeaves`** (matches existing behavior). | Backward compat. |
| W7: Performance — does scope checking add per-coord overhead? | **Yes, one comparison per leaf coord.** Negligible. The bigger cost is iterating leaf coords; scope just filters before that. | Acceptable. |
| W8: How do scope-restricted rules interact with the dependency graph? | **The reverse-edge entries are unchanged** (a downstream rule still depends on the scope-restricted rule's measure). The scope just affects WHICH coords get computed, not the dep graph topology. | Don't touch dep graph. |

**Required regression tests (6 minimum):**
1. `test_scope_all_leaves_default_when_field_absent` (backward compat).
2. `test_scope_future_leaves_only_writes_future_coords`.
3. `test_scope_past_leaves_only_writes_past_coords`.
4. `test_scope_current_leaves_only_writes_current_coords`.
5. `test_scope_unknown_name_fails_mc1029`.
6. `test_scope_future_leaves_without_time_anchor_fails_mc2069` (Amendment §4).

---

### Item 6 — `scenario_ref` + `actual_ref(measure, fallback)` (Cluster D part 1)

**Files:** `crates/mc-model/src/formula.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-core/src/cube.rs::resolve_cross_coord_read`.

**The use case (per Decision 8 + Amendments §3, §12).** Cross-scenario reads + Plan-as-fallback for Actual. Closes email-matchback's "mirror Plan→Actual" Python workaround.

**Binding additions:**

- **`scenario_ref(measure, "ScenarioName")`** — read `measure` from the named scenario at the current coordinate. New `Expr::ScenarioRef(MeasureId, String_or_ElementId)`. Validator resolves scenario name → ElementId at parse time.

- **`actual_ref(measure, fallback_expr)`** — extends existing 1-arg `actual_ref(measure)`. If actual_ref returns Null, evaluate `fallback_expr` instead (lazy). New optional 2nd arg in existing `Expr::ActualRef`.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1 (Amendment §3): Does the cross-coord nesting prohibition (MC1013) block `actual_ref(m, scenario_ref(m, "Plan"))`? | **No — Amendment §3 explicitly relaxes MC1013 for actual_ref's fallback expression only.** The fallback is evaluated lazily; cross-coord functions inside the fallback are allowed if independently valid (`scenario_ref`, `lag`, `prev`, `lookup` all permitted). All OTHER cross-coord nesting patterns remain rejected by MC1013. | Amendment §3 binding. |
| W2: Is `scenario_ref` synonymous with `actual_ref` when `"ScenarioName"` is the actuals_element? | **Behaviorally equivalent BUT use the more specific function** when the intent is "read actuals." Linter MAY warn (Phase 3J.1+) on `scenario_ref(m, "Actual")` suggesting `actual_ref(m)` — but no warning in 3J. | No premature linting. |
| W3: Lazy evaluation of `fallback_expr` — do any side effects fire? | **Only if fallback is needed** (i.e., actual_ref returns Null). Side effects = adding coords to the rule's `actual_reads` set for dep-graph tracking. If fallback isn't evaluated, its potential reads aren't tracked. | Lazy semantics; matches user expectation. |
| W4: Unknown scenario name in `scenario_ref(m, "FooBar")`? | **MC2065** at validate (scenario name resolved against Scenario dim's elements). | Reference resolution. |
| W5: Type mismatch in fallback expression (e.g., fallback returns Str when measure is F64)? | **MC2066** at validate (best-effort static type check). Runtime check still fires if validator misses. | Defense in depth. |
| W6 (Amendment §12 — performance note): Do these inherit cross-coord dep-graph debt? | **Yes — both `scenario_ref` and `actual_ref(m, fallback)` are cross-coordinate reads. They share the existing dep-graph debt (every write invalidates all derived cells; correctness preserved via revision-bumping; performance fix deferred to a future ADR).** Document this in the function's doc comment AND in the cartridge READMEs that use these functions extensively. | Amendment §12 binding — known debt, inherited not introduced. |
| W7: Can `scenario_ref` reference the current scenario (i.e., `scenario_ref(Spend, "<current scenario name>")`)? | **Allowed but pointless.** Returns the same value as just `Spend`. No special-case error; document as inefficient. | No special case. |
| W8: What if `scenario_ref` references a scenario element that's locked or frozen? | **Returns the value as stored** (read-only operation; no permission check needed for reads). | Standard read semantics. |

**Required regression tests (7 minimum):**
1. `test_scenario_ref_reads_from_named_scenario`.
2. `test_scenario_ref_unknown_scenario_fails_mc2065`.
3. `test_actual_ref_with_fallback_uses_fallback_when_actual_null` (Amendment §3 lazy eval).
4. `test_actual_ref_with_fallback_uses_actual_when_present` (fallback NOT evaluated).
5. `test_actual_ref_fallback_with_scenario_ref_works` (Amendment §3 — the canonical pattern).
6. `test_actual_ref_fallback_type_mismatch_fails_mc2066`.
7. `test_actual_ref_one_arg_form_unchanged` (backward compat).

---

### Item 7 — `extrapolate_last_value` + LOCF (Cluster D part 2; depends on item 5)

**Files:** `crates/mc-model/src/formula.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-core/src/cube.rs::resolve_cross_coord_read`, `crates/mc-model/src/schema.rs` (for `allow_past_extrapolation` rule field).

**The use case.** Filling future-period gaps (e.g., extending AdSpend from October data to Nov/Dec for forecasting). Closes email-matchback's `prepare_v2_inputs.py` Nov/Dec extension hack.

**Binding (per Decision 9 + Amendments §5, §11):**

```yaml
rules:
  - name: extend_adspend
    target: AdSpend
    scope: FutureLeaves                    # required (per W2 below)
    body: "extrapolate_last_value(AdSpend)"
```

**Function semantics:** `extrapolate_last_value(measure)` at a coord scans backward through the Time dimension, returning the most recent non-Null value of `measure` at that coord (with all other dim elements held constant). If no prior non-Null value exists, returns Null.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Direction of scan — which is "backward"? | **Backward = earlier time periods** (lower index in the Time dim's element ordering). Same direction as `prev()`/`lag()` from Phase 3F. | Consistency with shipped time-series functions. |
| W2 (Amendment §11): Used at scope other than `FutureLeaves`? | **MC2067** at validate — "extrapolate_last_value used at scope other than FutureLeaves without `allow_past_extrapolation: true`". The override flag is `allow_past_extrapolation: true` (renamed from the original `extrapolate_anywhere` per Amendment §11). | Amendment §11 binding. Specific naming resists misuse. |
| W3 (Amendment §5): Reserve future `max_periods` 2nd arg? | **Yes — document but DON'T implement.** v1 ships only the 1-arg form `extrapolate_last_value(measure)`. The handoff and the function's doc comment note the future shape `extrapolate_last_value(measure, max_periods?)`. Implementer should NOT ship max_periods in 3J. | Amendment §5 binding. |
| W4: What if no prior non-Null exists? | **Returns Null.** Matches `prev()` boundary behavior. | Consistency. |
| W5: Performance — backward scan through Time elements? | **O(time_periods) per coord per eval.** For typical models (12-60 time periods), negligible. For large time dims, might want to cache; v1 doesn't cache (pre-optimization). | Bounded; future Phase 2-style perf work if needed. |
| W6: Interaction with `actual_ref` — does extrapolate_last_value scan only over Plan or also Actual values? | **Scans only the cells visible to the rule's input read** (which respect the rule's scenario context). In the typical Plan-scenario rule, it scans Plan values. To extrapolate from Actual values, use `extrapolate_last_value(actual_ref(measure))` — note this is permitted by Amendment §3's MC1013 relaxation IF wrapped in `actual_ref(m, fallback)` form, otherwise blocked. | Lazy: extrapolate sees what the rule would read at each scanned coord. |
| W7: Can extrapolate be combined with `if`/`is_future`/etc.? | **Yes** (it's just a function; combines normally). The scope-gate (W2) is the safety; combinatorial use is encouraged for richer forecasting. | No restriction. |
| W8: What if `time_anchor` isn't configured but a `FutureLeaves`-scoped rule uses `extrapolate_last_value`? | **MC2069** fires first** (item 5 W2). The `extrapolate_last_value` validator just confirms scope is `FutureLeaves` — the time_anchor check is the scope's responsibility. | Single source of truth. |

**Required regression tests (6 minimum):**
1. `test_extrapolate_last_value_at_future_period_returns_last_actual`.
2. `test_extrapolate_last_value_no_prior_non_null_returns_null` (Amendment §5 explicit test).
3. `test_extrapolate_last_value_in_all_leaves_scope_fails_mc2067` (Amendment §11).
4. `test_extrapolate_last_value_with_allow_past_extrapolation_works` (Amendment §11 override).
5. `test_extrapolate_last_value_in_future_leaves_without_time_anchor_fails_mc2069` (Amendment §4 + Amendment §11).
6. `test_extrapolate_last_value_combined_with_actual_ref_fallback_works` (cross-feature integration).

---

## Out of Scope (deferred — DO NOT implement)

These were on the original deferred queue but are NOT 3J:

| Item | Why deferred from 3J | Future phase |
|---|---|---|
| `output_bound: {min: 0}` on fitted models | Fitted-model evaluation layer concern | Phase 3H.1 (separate ADR-0017) |
| Adstock + saturation transforms native to `fitted_models:` | Fitted-model evaluation layer | Phase 3H.1 |
| Computed parameters (`parameters:` with `body:`) | v1 is constants only per Decision 6 + Amendment §2 | Phase 3J.1 if demanded |
| Scoped parameters (per-Scenario, per-Market, etc.) | Partial-coordinate parameters need their own design | Phase 3J.1 if demanded |
| String ordering operators (`<`, `>`, `<=`, `>=`) | Locale-dependent; out of scope per Decision 3 | Future phase if demanded |
| Custom Scope variants beyond the 4 (`InputScope`, `RandomLeaves`, etc.) | Ad-hoc scope design needs separate ADR | Future phase |
| Multi-dimensional Indicators (`Indicator` over Market AND Channel) | v1 is single dimension per Decision 7 | Future phase if demanded |
| Stochastic measure roles (`Random`, `Sampled`) | Out of scope for deterministic kernel | Phase 4+ |
| `parameters:` of types other than `f64` | Per Decision 6 + Amendment §1 | Future phase if demanded |
| Cross-cube `scenario_ref` | Out of scope for v1 | Phase 5+ |
| String-valued cells (storage of `Str`) | Phase 4+ kernel storage layer change | Phase 4+ |
| `extrapolate_last_value(measure, max_periods)` 2nd arg | Per Amendment §5 — reserved future shape, not v1 | Phase 3J.1 if demanded |
| Cross-coord dep-graph performance fix | Per Amendment §12 — known debt inherited, not addressed | Future ADR |

If you encounter any of these and feel the urge to "while I'm here, just add..." — **resist**. Each is its own scoping exercise.

---

## Hard Rules (binding)

1. **Locked surfaces (zero-line diff against `1e57eb0`):**
   - `crates/mc-fixtures/`
   - `crates/mc-recipe/`
   - `crates/mc-drivers/`
   - `crates/mc-tessera/`
   - `mosaic-plugin/`
   - `crates/mc-cli/` **except** test files (you'll add tests covering 3J behavior to `agent_cli_integration.rs`; CLI source files stay unchanged)

2. **Allowed touch (binding scope):**
   - `crates/mc-model/src/{formula,schema,validate,compile,error,inspect,lib,lint}.rs`
   - `crates/mc-model/tests/` (new test files OK; add `formula_str_literals.rs`, `parameters_block.rs`, `indicator_role.rs`, `scope_extension.rs`, `scenario_ref_actual_fallback.rs`, `extrapolate_locf.rs` as logical groupings)
   - `crates/mc-core/src/value.rs` — `ScalarValue::Str` promotion (already exists; just remove transient-only restriction in eval paths)
   - `crates/mc-core/src/rule.rs` — new `Expr` variants (`StrLiteral`, `CurrentElementName`, `ScenarioRef`, `ParamRef`, `ExtrapolateLastValue`, `StrEq`, `StrNeq`); existing `IsElement`, `If`, etc. unchanged
   - `crates/mc-core/src/cube.rs` — eval dispatch for new variants; **NO changes to `Cube::write`, `HashMapStore::insert`, consolidation, dirty tracker, snapshot beyond the writeback validation hook for MC2059** (item 1 W1 binding)
   - `crates/mc-core/src/scope.rs` — `Scope` enum extension
   - `crates/mc-core/src/measure.rs` — `MeasureRole::Indicator` variant
   - `crates/mc-cli/tests/agent_cli_integration.rs` — Phase 3J integration tests as needed
   - Acceptance gate verification: `git diff 1e57eb0 -- crates/mc-fixtures/ crates/mc-recipe/ crates/mc-drivers/ crates/mc-tessera/ mosaic-plugin/` returns 0 lines.

3. **No new dependencies.** All new functionality via existing patterns + hand-rolled per process-notes Rule 5.

4. **Toolchain stays Rust 1.78.** No `rust-toolchain.toml` edit.

5. **No `Cargo.lock` pin churn.** Pure source-code phase; no dep changes expected.

6. **Backward compat (process-notes Rule 7):** every existing test passes. Acme + NBA + email-matchback models all continue to validate, lint, and test cleanly. The Phase 6A query/trace JSON envelope schemas (1.0 + 1.1) stay unbumped (this phase adds only additive behavior to existing envelopes).

7. **`mc-core` API surface:** new public types are `Scope::FutureLeaves|PastLeaves|CurrentLeaves` variants and `MeasureRole::Indicator` variant only. **No new public functions.** The `ScalarValue::Str` variant already exists — just lift its eval restriction.

8. **`ScalarValue::Str` boundary (the load-bearing constraint):** strings flow through eval; never reach storage. Item 1 W1 walks the rule. **Document this as the binding invariant in a doc comment on `ScalarValue::Str`'s definition and in `Cube::write`'s doc comment.**

9. **Per-cluster commit discipline (process-notes Rule 11 anti-pattern fix from 3I):** commit AS YOU GO. Each cluster gets at least one commit; cluster boundaries make natural commit boundaries:
   - Commit after Item 1 (`feat(3J cluster A.1): ScalarValue::Str first-class in eval`)
   - Commit after Item 2 (`feat(3J cluster A.2): current_element function`)
   - Commit after Item 3 (`feat(3J cluster C.1): parameters block`)
   - Commit after Item 4 (`feat(3J cluster C.2): Indicator measure role`)
   - Commit after Item 5 (`feat(3J cluster B): Scope enum extension`)
   - Commit after Item 6 (`feat(3J cluster D.1): scenario_ref + actual_ref fallback`)
   - Commit after Item 7 (`feat(3J cluster D.2): extrapolate_last_value + LOCF`)

   Do NOT batch all 7 items into one commit at the end. The PM's review depends on per-cluster diff visibility.

---

## Acceptance Gates (lean — same as 3I)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (830 → expect ~+45-60 = ~875-890; depends on cross-cluster integration tests added).
- [ ] Locked-surfaces grep returns 0 lines (per Hard Rule 1).
- [ ] All 7 items shipped with their required regression tests (10+4+5+5+6+7+6 = 43 minimum required tests + integration tests).
- [ ] No SPEC QUESTION drift (or every SPEC QUESTION resolved before merge).
- [ ] **Per-cluster commits visible** in `git log a1488c5..HEAD` (Hard Rule 9).

Per-item smoke checks (paste each in completion report):
- [ ] **Item 1:** `mc model query <fixture> --where 'current_element(Channel) == "Email"' --format json` returns Email coords only (proves Items 1+2 integration).
- [ ] **Item 1:** Attempt to write a Str cell value via `mc model write` → exit 1 with MC2059.
- [ ] **Item 3:** YAML with `parameters: [- name: x value: 1.5]` + formula using `param(x)` validates and evaluates correctly.
- [ ] **Item 4:** YAML with `Indicator` measure validates clean; `mc model query` of the indicator returns 1.0 at matching coords.
- [ ] **Item 5:** YAML with `scope: FutureLeaves` rule + configured `time_anchor` validates; same YAML without `time_anchor` fails MC2069.
- [ ] **Item 6:** YAML with `actual_ref(Spend, scenario_ref(Spend, "Plan"))` validates and evaluates lazily.
- [ ] **Item 7:** `extrapolate_last_value` rule extends a measure into future periods correctly; same rule at `scope: AllLeaves` without override fails MC2067.

---

## Order of Operations

1. Read this handoff in full.
2. Read [`docs/decisions/0016-phase-3j-formula-deferred-items.md`](../decisions/0016-phase-3j-formula-deferred-items.md) — the source of truth for binding scope, especially the Acceptance Amendments §1–§12.
3. Skim [`docs/process-notes.md`](../process-notes.md) Rules 1, 5, 7, 9, 10, 11. Pay attention to Rule 11's "all-uncommitted-at-end" anti-pattern from Phase 3I.
4. Skim [`docs/research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md) for Amendment §12 context.
5. **Implementation order (dependency-respecting):**
   - **Item 1 first** (Cluster A.1 — `ScalarValue::Str` foundation). This is the largest single piece and unlocks items 2 and 6.
   - **Item 2** (Cluster A.2 — `current_element`). Depends on item 1.
   - **Item 3** (Cluster C.1 — `parameters:` block). Independent; schema-only.
   - **Item 4** (Cluster C.2 — `Indicator` measure role). Independent; depends on `is_element` from Phase 3I.
   - **Item 5** (Cluster B — `Scope` enum extension). Independent; required for item 7.
   - **Item 6** (Cluster D.1 — `scenario_ref` + `actual_ref` fallback). Depends on item 1 (string literals for scenario name).
   - **Item 7** (Cluster D.2 — `extrapolate_last_value`). Depends on item 5.
6. **Commit per item** with descriptive messages (`feat(3J cluster A.1): ScalarValue::Str first-class in eval`).
7. Run gates after each item lands. **Don't batch-test all 7 at the end** — a regression in item 1 would be invisible until you finish item 7.
8. Write the completion report at `docs/reports/phase-3j-completion-report.md`.
9. **Stop.** Do not push the branch. PM merges + tags + pushes after audit review (per process-notes Rule 11).

---

## Completion Report Expectations

Per process-notes Rule 10. Same shape as 3I:
- **Shipped** — what landed for each item with file:line citations.
- **Per-item smoke check outputs** — paste each command + actual output.
- **All 16 reserved diagnostic codes shipped** (MC1026-1029, MC2058-2069). Confirm none collide with shipped codes (re-sweep against current main per Rule 3 pre-flight pattern).
- **List of new public mc-core types** (Scope variants + MeasureRole::Indicator). Confirm NO new public functions.
- **Acceptance gates checklist.**
- **Known debt** — anything noticed but not fixed (file follow-ups).
- **Per-cluster commit log** — paste `git log a1488c5..HEAD` showing the per-cluster commits (Hard Rule 9 verification).
- **Locked surfaces grep** — paste output.
- **Email-matchback Python residual** — re-survey what fraction of M-14 closed (Amendment §2 obligation): test against `~/Projects/email-matchback/scripts/mosaic/prepare_v2_inputs.py` Q1-anchor pattern. Document what's closed vs what's still Python (constants vs scoped parameters).

---

## SPEC QUESTION Format

Same as before (CLAUDE.md §11):

```
SPEC QUESTION: [one-line summary]

Context: [where in the handoff this came up]
Spec text: [literal quote]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

Most likely SPEC QUESTION candidates in 3J:
- Item 1 W1: enforcing the Str-no-storage boundary at every call site; confirm the debug_assert! + WritebackError::TypeMismatch combo is sufficient or needs more.
- Item 4 W7: default aggregation for Indicator measures (Sum vs other) — confirm Sum is right.
- Item 5 W8: dep-graph topology when scope-restricted rules feed into AllLeaves rules.
- Item 6 W3: lazy-eval coord-tracking for fallback expressions — confirm the "potential reads aren't tracked if fallback isn't evaluated" semantics is correct.
- Item 7 W6: extrapolate-over-actual_ref interaction — confirm Amendment §3's MC1013 relaxation covers the right shape.

---

*End of handoff. Phase 3J + Phase 3H.1 (separate, ADR-0017 to be drafted later) close out the formula-engine deferred queue from ADR-0015 entirely. After both ship, formula additions are demand-driven; Phase 3 area transitions to "stable, demand-driven only."*
