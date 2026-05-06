# Phase 3I Handoff — Formula Language Completion

> **Audience:** the Claude Code instance that implements Phase 3I.
> **You inherit `main` at `58448a5` (785 / 0 / 5 tests). You'll work on
> the branch `phase-3i/formula-language-completion` — see process-notes
> §11 for the git workflow rule (single instance, sequential = branch
> but no worktree).**
>
> **This phase closes the formula-language gaps surfaced by the
> post-6A audit.** It's the last formula-expansion phase before the
> roadmap pivots to data integration polish (5D) or UI (6B). After
> 3I, ~170 lines of email-matchback Python are eliminated and the
> formula engine reaches full coverage for the marketing-finance
> + sports-betting + FP&A planning domains.
>
> **Hard rule:** Phase 3I is an **additive parser + validator + eval
> layer** following the pattern established by Phase 3E/3F/3G/3H.
> No kernel API change. Mc-core touches limited to: new `Expr`
> variants in `rule.rs`, new eval dispatch in `cube.rs::resolve_cross_coord_read`
> for any new cross-coord operators (none expected in 3I), and any
> new entries in the `crates/mc-core/src/value.rs` type-check
> machinery for new function arities. **No `ScalarValue::Str`
> expansion** (string-literal support is explicitly DEFERRED to a
> future phase — see §"Out of Scope").
>
> **Process note (handoff-first parallel flow per process-notes §1):**
> ADR-0015 documenting Phase 3I's design decisions will land in
> parallel with this implementation. The handoff is the binding
> contract; the ADR is the audit-trail artifact. If they conflict,
> surface as a SPEC QUESTION.

---

## The one paragraph you must internalize

8 items. All follow the Phase 3E-3H pattern: extend the formula
parser, add an `Expr` variant or reuse an existing one, wire the
eval, write 3-5 regression tests per item, ship. The two items with
novelty are item 3 (multi-key lookup tables — additive schema field)
and item 8 (filter-formula parser unification — closes the two-parser
state per research-notes §7I.8). Items requiring real design work
(`parameters:` blocks, `scenario_ref()`, `extrapolate_last_value()`,
broader string-literal support) are explicitly DEFERRED to Phase
3J or later — see §"Out of Scope" for the binding list. Stay inside
that scope. The 6A audit already verified each in-scope item as
"clear-path, no design needed"; you're executing, not deciding.

---

## Production-quality framing

Same as Phase 6A.2 / 6A.3: this is a no-second-pass phase. The 6A
audit pattern (Decision Matrix per item + SPEC QUESTION escape valve)
caught real bugs that would have required a second-pass fix. Apply
the same discipline here.

The audits at `docs/audits/master-gap-report.md` (Sonnet) and
`docs/audits/codex-phase-6a-followup.md` (independent verification)
already verified every in-scope item. Where Codex's verification
narrowed Sonnet's claim (e.g., M-17 norm_cdf NaN was overstated;
runtime already returns Null), the binding decision in this handoff
follows Codex.

---

## Items (8 total)

### Item 1 — `is_element(Dim, "Element")` narrow numeric form

**File:** `crates/mc-model/src/formula.rs` (parser), `crates/mc-core/src/rule.rs` (Expr + eval).

**The use case.** `prepare_mmm_inputs.py` (email-matchback)
hand-generates 464 indicator rows — `IsHouston=1.0` at Houston
coords, `0.0` elsewhere — across 4 markets × 29 months. The engine
already knows which Market a cell is at; this should be a formula,
not 464 CSV rows.

**The fix.** Add a new formula function `is_element(DimName, "ElementName")`
that returns `1.0` if the current coordinate's element in `DimName`
matches `"ElementName"`, `0.0` otherwise. **Numeric only — no
string literals leak into stored cell values.**

**Codex's binding decision (audit M-11):** prefer the narrow
`is_element(Dim, "Element")` over the broader `current_element() ==
"Houston"` because narrow keeps `ScalarValue::Str` confined to
parser-internal use. **Take the narrow path.**

**Implementation pattern (mirrors `Expr::DimElement` at `rule.rs:108`):**

```rust
// In rule.rs
Expr::IsElement(DimensionId, ElementId)

// In formula.rs parser
"is_element" => parse_is_element(args)?  // 2 args: Dim ident + element name (parsed as a quoted-string LITERAL ARG, not as a full ScalarValue::Str)

// In cube.rs eval (or rule.rs eval_expr_unified)
Expr::IsElement(dim_id, elem_id) => {
    let cur_elem = ctx.current_coord.element_at(dim_id);
    Ok(if cur_elem == elem_id { ScalarValue::F64(1.0) } else { ScalarValue::F64(0.0) })
}
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Where does the element-name string get resolved? Parse-time or eval-time? | **Parse-time.** `validate.rs` resolves `"Houston"` to `ElementId(N)` against the named dimension; the resolved AST is `Expr::IsElement(DimId, ElemId)`. Eval has no string handling. | Faster eval; matches how `predict()` resolves model names at compile time. |
| W2: What if the element name doesn't exist in the dimension? | **Validation error MC1022 (next available MC1xxx code).** Refuse to compile the model. | Fail-fast like other reference resolution. |
| W3: What if the dimension name doesn't exist? | **Validation error MC1023.** | Same as W2. |
| W4: What if the formula uses a quoted string OUTSIDE `is_element()` (e.g., `if(Spend == "high", ...)`)? | **Parse error MC1024 — string literals only allowed as the second arg of `is_element()`.** Rejecting other uses of strings keeps `ScalarValue::Str` from leaking. | Locked-down scope. The general string-literals path is Phase 3J+. |
| W5: What if `is_element` is called with 0, 1, or 3+ args? | **Parse error MC1004 (existing arity-mismatch code).** | Standard arity check. |
| W6: How does `is_element` interact with `wavg_over(measure, ..., weighted_by=is_element(Market, "Houston"))` (item 5)? | **Works naturally** — `is_element` returns 0.0/1.0; multiplication and weighted sums handle that correctly. **Test this** as a regression. | Composability check. |

**Regression tests (4 required):**
1. `test_is_element_returns_one_at_matching_coord`.
2. `test_is_element_returns_zero_elsewhere`.
3. `test_is_element_unknown_element_fails_validation_with_mc1022`.
4. `test_is_element_with_quoted_string_outside_call_fails_with_mc1024`.

---

### Item 2 — Math primitives (9 functions)

**File:** `crates/mc-model/src/formula.rs` (parser), `crates/mc-core/src/rule.rs` (Expr + eval).

**The use case.** NPV, compound-growth (FP&A), safety stock (demand
planning), Kelly criterion (sports betting), SaaS churn curves.
The 6A audit M-15 cited ~100+ lines across domain models that
require Python without these primitives.

**The fix.** Add 9 new pure-math formula functions. Each follows
the exact pattern of `exp()` and `norm_cdf()` from Phase 3H: parser
case + `Expr` variant + eval dispatch in `eval_expr_unified` (or
the binary-op family if dyadic).

**Functions to add (binding):**

| Function | Arity | Eval | Edge case (Null policy) |
|---|---|---|---|
| `pow(base, exp)` | 2 | `base.powf(exp)` | If base < 0 and exp is non-integer → Null |
| `sqrt(x)` | 1 | `x.sqrt()` | If x < 0 → Null |
| `ln(x)` | 1 | `x.ln()` | If x ≤ 0 → Null |
| `log10(x)` | 1 | `x.log10()` | If x ≤ 0 → Null |
| `round(x)` | 1 | `x.round()` | (no edge case) |
| `floor(x)` | 1 | `x.floor()` | (no edge case) |
| `ceil(x)` | 1 | `x.ceil()` | (no edge case) |
| `mod(a, b)` | 2 | `a.rem_euclid(b)` | If b == 0 → Null |
| `norm_inv(p, mu, sigma)` | 3 | inverse-CDF (see W4) | If p ≤ 0 or p ≥ 1 → Null; if sigma ≤ 0 → Null |

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: What's the AST shape — one variant per function or a single `MathFunc(MathFuncKind, Vec<Expr>)`? | **One variant per function** (`Expr::Pow(Box<Expr>, Box<Expr>)`, etc.). Mirrors Phase 3H's `Exp(Box<Expr>)` + `NormCdf(Vec<Expr>)`. Don't introduce a new enum mid-refactor. | Stylistic consistency with shipped 3H. |
| W2: How is `norm_inv(p, mu, sigma)` computed? | **Beasley-Springer-Moro algorithm** (no external dep; ~30 lines hand-rolled per process-notes Rule 5). Accuracy good to ~1e-9, sufficient for planning use. **Reference:** standard implementation widely available; cite the algorithm in the function's doc comment. | Matches Mosaic's hand-rolled-wins pattern. |
| W3: What if a function is called with the wrong number of args? | **Parse error MC1004 (existing).** Standard arity check; reuse the pattern from existing functions. | Standard. |
| W4: Edge case Null behavior — propagate or error? | **Always propagate Null** (return `ScalarValue::Null`). Never error at eval-time for math edge cases. | Matches Mosaic's Null-propagation semantics throughout. |
| W5: Should there be lint warnings for "obviously-Null-producing" calls (e.g., `sqrt(-1)` with literal -1)? | **No.** Lint stays quiet. The runtime Null is the user's signal. | Don't over-engineer. |
| W6: Should `pow(2, 0.5)` and `sqrt(2)` produce identical results? | **Yes** (both `f64::powf` paths). Test that `pow(x, 0.5) == sqrt(x)` for positive x within `1e-9`. | Sanity. |
| W7: Performance — is `pow` significantly slower than other ops? | **Don't optimize.** Phase 3I is correctness; Phase 2-style perf work happens later if needed. | Process-notes Rule 5 (hand-rolled wins). |

**Regression tests (12 required — 1 per function + 3 cross-function):**
1-9: One regression test per function (correct value + edge-case Null).
10. `test_pow_and_sqrt_equivalence_for_positive` — `pow(x, 0.5) ≈ sqrt(x)`.
11. `test_norm_inv_inverts_norm_cdf` — `norm_cdf(norm_inv(p, 0, 1), 0, 1) ≈ p` for several p in (0, 1).
12. `test_math_primitives_propagate_null` — every function returns Null when given Null input.

---

### Item 3 — Multi-key `lookup_tables`

**Files:** `crates/mc-model/src/schema.rs`, `crates/mc-model/src/validate.rs`, `crates/mc-model/src/compile.rs`, `crates/mc-core/src/cube.rs::resolve_cross_coord_read` (LookupTable arm), formula parser.

**The use case.** `tide-matchback.yaml` has 5 single-key Houston
seasonality lookup tables (one per measure). With multi-key support,
ONE table covers Market × Time × Measure (4 markets × 12 months ×
5 measures = 240 entries vs. 60 entries × 5 tables today).

**The fix.** Add a `key_dimensions: Vec<String>` field to
`ParsedLookupTable` alongside the existing `key_dimension: String`.
The validator accepts EITHER `key_dimension: <single>` OR
`key_dimensions: [<one or more>]` but not both. The `lookup()`
function becomes variadic in the dimension args:

```yaml
lookup_tables:
  - name: "seasonality"
    key_dimensions: ["Market", "Time"]
    values:
      "Houston|Jan_2026": 1.05
      "Houston|Feb_2026": 1.12
      ...
```

```yaml
# Formula usage:
body: "Spend * lookup(seasonality, Market, Time)"
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Backward compat — existing `key_dimension: String` recipes/models | **MUST still work.** Validator: if `key_dimension` is set, treat as single-key (one-element `key_dimensions`). If `key_dimensions` is set, treat as multi-key. Both set → error MC2050. | Backward compat is a hard gate (process-notes Rule 7). |
| W2: Key separator — what character joins multi-dim keys in the YAML? | **Pipe (`\|`)** — matches the example in the gap report (`"Houston\|Jan_2026"`). Document explicitly. Reject keys with literal pipe in element names (validator MC2051). | Pipe is rare in element names; explicit separator avoids ambiguity. |
| W3: Order of dimensions in the key — must match `key_dimensions` order? | **YES, strictly.** `key_dimensions: ["Market", "Time"]` requires keys like `"Houston\|Jan_2026"`, NOT `"Jan_2026\|Houston"`. Validator emits MC2052 if any key arity doesn't match. | Deterministic resolution. |
| W4: `lookup()` function signature — `lookup(name, dim1)` vs `lookup(name, dim1, dim2)` | **Variadic — N+1 args where N is `key_dimensions.len()`.** Parse-time arity check against the table's declared key dimensions. | Standard. |
| W5: Schema version bump? | **No** — additive field on existing `ParsedLookupTable`. Existing parsers see the new field as ignored if `key_dimension` is also present. | Additive. |
| W6: Performance — does multi-key lookup require dep-graph changes? | **No** — it's a same-coord computation (the result is stored at the calling cell's coord). The dep graph is unchanged. | Codex confirmed in audit M-16 §"Why this is Major rather than Critical." |

**Regression tests (5 required):**
1. `test_lookup_table_single_key_backward_compat`.
2. `test_lookup_table_multi_key_two_dims`.
3. `test_lookup_table_both_key_fields_set_fails_mc2050`.
4. `test_lookup_table_pipe_in_element_name_fails_mc2051`.
5. `test_lookup_table_key_arity_mismatch_fails_mc2052`.

---

### Item 4 — `predict()` arity validation

**File:** `crates/mc-model/src/validate.rs`.

**The bug.** `cube.rs:1108-1110` returns Null at runtime if the
feature count doesn't match the coefficient count. Codex confirmed
runtime behavior is correct (returns Null, not NaN — Sonnet's M-17
NaN claim was overstated). But there's no LOAD-TIME validation
catching the mismatch in the model definition itself.

**The fix.** In `validate.rs::check_fitted_model_blocks` (around line
1706 per the audit), walk all `predict(model_name, feat1, feat2, ...)`
calls in rule bodies. For each, look up the named fitted model and
compare the call's feature-arg count to the model's coefficient count.
Mismatch → MC1021 (already reserved per audit B:G-OPEN-9).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Should the validation be parse-time (MC1xxx) or validate-time (MC2xxx)? | **Validate-time (MC2053).** The check requires resolved fitted-model names, which only exist after parse → validate. MC1021 was reserved but not the right namespace; promote to MC2053. | Correct namespace per ADR-0005 conventions. |
| W2: What if the same formula uses `predict("model_a", x, y)` and `predict("model_b", x, y, z)` and they have different coefficient counts? | **Validate each call independently.** Two separate diagnostics if both are wrong. | Per-call validation. |
| W3: What if the predict call uses a measure ref that resolves to a Vec at runtime (e.g., a varargs feature pattern)? | **Out of scope.** Today predict() takes scalar args one-by-one; runtime returns Null on mismatch. Don't over-engineer. | Defer. |
| W4: Should the lint also surface this even when validate is disabled? | **Lint-as-validation already covers this.** Don't add a duplicate lint code. | One source of truth. |

**Regression tests (3 required):**
1. `test_predict_too_few_features_fails_mc2053`.
2. `test_predict_too_many_features_fails_mc2053`.
3. `test_predict_correct_arity_validates_clean`.

---

### Item 5 — `avg_over` / `min_over` / `max_over` / `wavg_over`

**File:** `crates/mc-model/src/formula.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-core/src/cube.rs::resolve_cross_coord_read`.

**The use case.** Market-average ROAS (requires weighted average
over markets), max-channel performance, top-N ranking. The audit
M-18 cited several real planning models that need these.

**The fix.** Four new functions following the exact pattern of
`sum_over` from Phase 3G:

```yaml
body: "avg_over(Spend, Market)"            # avg over all markets
body: "min_over(Spend, Channel)"           # min channel spend
body: "max_over(Revenue, Market)"          # max market revenue
body: "wavg_over(CPC, Market, Spend)"      # CPC weighted by Spend across markets
```

Each adds a parser case + Expr variant (`AvgOver(DimensionId, ElementId)`,
etc.) + eval dispatch in `cube.rs`.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Null handling in averages — count Nulls toward the divisor or skip? | **Skip Nulls (don't count them in the divisor).** `avg_over([1, Null, 3])` returns 2, not 1.33. | Matches statistical convention; matches Excel's `AVERAGE`. |
| W2: Null handling in `min_over` / `max_over` | **Skip Nulls.** Empty after Null-filter → return Null. | Same convention. |
| W3: `wavg_over` arity — 3 args or 4? | **3 args:** `wavg_over(measure, dim, weight_measure)`. Ranges over all elements of `dim`; weight comes from `weight_measure` evaluated at each element. | Pattern matches `sum_over` + adds the weight measure. |
| W4: What if all weights are zero in `wavg_over`? | **Return Null.** Don't divide by zero. | Standard Mosaic pattern. |
| W5: Performance — these enumerate all leaf elements of the dim. Different from sum_over? | **Same as sum_over** — already pays this cost in 3G. | Existing baseline. |
| W6: Should `avg_over` and `wavg_over(m, d, ones)` produce the same result? | **Yes when ones is constant 1.0** — sanity test. | Sanity. |

**Regression tests (8 required):**
1-4. One per function with happy path.
5. `test_avg_over_skips_nulls`.
6. `test_min_over_with_all_nulls_returns_null`.
7. `test_wavg_over_zero_weights_returns_null`.
8. `test_avg_over_equals_wavg_over_with_unit_weights`.

---

### Item 6 — `ifs()` and `switch()`

**File:** `crates/mc-model/src/formula.rs` (parser), no new Expr variants.

**The use case.** Models with 4-6 branching cases. Today's
`if(if(if(...)))` chains are hard to read and miss-an-else-silently
returns Null.

**The fix.** Both desugar to nested `If` at compile time. **No new
Expr variants needed.**

```yaml
body: "ifs(Spend > 1000, 'high', Spend > 100, 'med', 'low')"
body: "switch(Channel, Email, 0.05, Search, 0.10, 0.02)"  # Email→0.05, Search→0.10, default 0.02
```

**Wait — `switch()` uses string literals!** Per item 1's W4, string
literals are restricted to `is_element()`'s second arg. So `switch()`
must take its match-targets via `is_element()` calls or numeric
comparisons. Drop the string-literal example. Use:

```yaml
body: "ifs(Spend > 1000, 0.05, Spend > 100, 0.10, 0.02)"
body: "switch(period_index(), 0, 0.05, 1, 0.10, 0.02)"  # period 0 → 0.05, period 1 → 0.10, default 0.02
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: `ifs(c1, v1, c2, v2, ..., default)` — even or odd argument count? | **Odd** — N condition/value pairs followed by 1 default. Total args = 2N+1. Even arg count → MC1004 parse error. | Default is mandatory; otherwise users hit the silent-Null trap from Phase 3E. |
| W2: `switch(expr, match1, val1, match2, val2, ..., default)` — odd or even? | **Even** — initial `expr`, then N (match, val) pairs, then default. Total args = 2N+2. | Forces explicit default. |
| W3: Compile to nested If or new Expr variant? | **Nested If at parse-time.** No new Expr variant. The cube doesn't need to know about `ifs/switch` beyond what it already knows about `If`. | Minimal kernel touch. |
| W4: Should the lint warn on "unreachable default" patterns (e.g., conditions that cover all cases)? | **No.** Out of scope; not a correctness issue. | Don't over-engineer. |
| W5: `ifs()` with only the default (zero pairs) — `ifs(default_value)` | **Allow it** (degenerate case; the parser emits `Const(default_value)`). One-arg form is identical to a constant. | Trivial. |
| W6: String-comparison case in `switch()` — what about `switch(Channel, Email_Element_Id, ...)`? | **Use `is_element(Channel, "Email")` as the match expression** for dimension matching. Don't add string literals. | Aligns with item 1. |

**Regression tests (5 required):**
1. `test_ifs_three_branches_picks_correct`.
2. `test_ifs_even_arg_count_fails_mc1004`.
3. `test_switch_with_period_index_branches`.
4. `test_switch_default_when_no_match`.
5. `test_ifs_compiles_to_nested_if` — assert the parsed AST is exactly nested If nodes (snapshot test).

---

### Item 7 — Filter parser tokenizer accepts hyphens

**File:** `crates/mc-cli/src/query.rs:486-490` (the filter tokenizer).

**The bug.** The filter tokenizer rejects hyphens in identifier
values. `--where "Time=Q1-2026"` errors because `Q1-2026` isn't a
valid identifier.

**Note:** this is item 7 in 3I scope BUT will become moot after item
8 (filter-formula parser unification) lands — because the formula
parser already accepts hyphens. **Decide before starting:** if you
ship item 8 first (which is the recommended order — see §"Order"
below), item 7 is auto-closed. If you ship 7 separately as a
quick fix, that's also fine. The handoff treats them as separate
items for accountability.

**Decision Matrix:** see item 8. Item 7's matrix is empty —
implementation is "tokenizer rule change to allow `-` after the
first character of an identifier value." 1-line fix in the
tokenizer; 1 regression test (`test_filter_accepts_hyphenated_value`).

---

### Item 8 — Filter-formula parser unification

**File:** `crates/mc-cli/src/query.rs`, `crates/mc-model/src/formula.rs` (potentially).

**The use case (audit M-40 + research-notes §7I.8).** Today the
`--where` filter has its own bespoke parser. This causes drift:
formula parser supports hyphens; filter parser doesn't (item 7).
Formula parser supports `is_element`; filter parser doesn't.
Future formula additions in Phase 3J+ would need duplicate
implementation. Phase 3I is the explicit commitment to unify.

**The fix.** Replace `query.rs::Filter::parse` with a wrapper around
`mc_model::formula::parse(...)` that interprets the resulting AST
as a coordinate-filter predicate.

**Implementation pattern:**

```rust
// In query.rs
pub fn parse_filter(expr: &str) -> Result<Filter, ParseError> {
    // Wrap the filter expression in a predicate evaluation context.
    // Reuse the formula parser's expression rule (the `Expr` non-terminal).
    let ast = mc_model::formula::parse_expression(expr)?;
    // Validate: the expression must be a boolean predicate (return Null/0.0/1.0 for filtering).
    Ok(Filter { ast })
}

// At eval time:
impl Filter {
    pub fn matches(&self, coord: &CellCoordinate, refs: &Refs) -> bool {
        // Eval the AST against a context where current_coord = coord.
        // 1.0 (truthy) → match; 0.0 (falsy) → skip; Null → skip.
        let result = eval_expr_unified(&self.ast, /* coord context */, refs);
        matches!(result, Ok(ScalarValue::F64(v)) if v.abs() > 1e-9)
    }
}
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Does `mc_model::formula::parse` accept fragment expressions (just an `Expr`, not a full rule body), or only full bodies? | **Verify before implementing.** Read `formula.rs` — there should be a `parse_expression(&str) -> Result<Expr, ParseError>` entry point separate from `parse(yaml: &str) -> Result<ParsedRuleBody, ParseError>`. If not, ADD it as a public function in mc-model (touches mc-model — explicitly authorized for this item). | This unification REQUIRES exposing the expression-parser entry point. |
| W2: Cross-coord reads in filter expressions — `--where "lag(Revenue, 1) > 100"` — should they work? | **NO.** Codex's answer (audit M-40) and process-notes Rule 9 logic: filter is evaluated against coord enumeration, not against a temporal/cross-coord context. Reject cross-coord operators in filter ASTs with MC1025 ("cross-coord operator not allowed in filter expression"). | Performance + semantics. Cross-coord refs in filters are a future Phase 3J feature. |
| W3: How is "cross-coord operator" detected in the AST? | **Walk the AST; reject `Expr::SelfRef` (lookups outside the current coord), `prev`, `lag`, `lead`, `cumsum`, `period_delta`, `actual_ref`, `predict`, `calibrate`, `lookup`, `bucket`, `period_index`, `is_past/current/future`, `sum_over/avg_over/min_over/max_over/wavg_over`.** Allow: arithmetic, comparison, `if`/`ifs`/`switch`, `is_element`, math primitives, constants. | Hardcoded allowlist is simpler than a feature-flag system; the allowed set is the "single-coord predicate" set. |
| W4: Backward compat — every existing `--where` invocation must continue to work | **Yes — verify by running every Phase 6A.2/6A.3 test that uses `--where` after the unification lands.** All ~15 tests must still pass. | Hard gate. |
| W5: Schema version bump for the query envelope? | **No** — the filter syntax is more permissive (accepts strictly more expressions), so old filter strings still parse. Additive in the spec sense. | No bump needed. |
| W6: Should the `Filter` type stay public (or pub(crate)) so item 5's `--metric-where` can reuse it? | **Yes — `Filter` and `parse_filter` stay accessible from `sweep.rs`** (which already uses them per Phase 6A.3 item 3). Just verify they're at the right visibility level. | No regression. |

**Regression tests (5 required):**
1. `test_filter_unified_parser_handles_hyphens` (closes item 7).
2. `test_filter_unified_parser_handles_is_element` (cross-item 1 + 8 integration).
3. `test_filter_rejects_cross_coord_operators_with_mc1025`.
4. `test_filter_backward_compat_acme_where_queries` — run a representative sample of existing `--where` queries; all pass.
5. `test_filter_with_math_primitives` (`--where "sqrt(Spend) > 50"`).

---

## Out of Scope (explicitly deferred — DO NOT implement)

These were either part of the "Phase 3I" wishlist or look like they
could fit, but they require ADRs or separate scoping. The handoff
forbids touching them.

| Item | Why deferred | Future phase |
|---|---|---|
| **`parameters:` block** (named scalar derivations) | Type system, override rules, lineage all undecided | Phase 3J ADR |
| **`scenario_ref()` / `actual_ref(measure, fallback)`** | 3 viable shapes; cross-coord dep-graph implications | Phase 3J ADR |
| **`extrapolate_last_value()` / LOCF** | Past-gap vs future-gap semantics; needs `Scope` system extension (current scope is `AllLeaves` only — see audit S-1) | Phase 3J ADR |
| **General string-literal support beyond `is_element()` arg** | Kernel-adjacent (`ScalarValue::Str` propagation through `Cube::read`); design spike required | Phase 3J or 4+ ADR |
| **`current_element(Dim) -> Str`** (vs `is_element(Dim, "X") -> 0.0/1.0`) | Requires general string-literal support (see above) | Phase 3J ADR |
| **`Indicator` measure role** (declarative `IsHouston` measure that doesn't need rules at all) | Different shape than item 1's `is_element` function; needs ADR for measure-role enum extension | Phase 3J ADR |
| **`output_bound: {min: 0}` on fitted models** (Amarillo -$5,706 case) | Phase 3H amendment; touches `mc-model/src/schema.rs` differently than 3I | Phase 3H.1 amendment |
| **Adstock + saturation transforms native to `fitted_models:`** | Phase 3H.2 — biggest of the model-layer gaps but kernel-adjacent | Phase 3H.2 |
| **Aggregation methods beyond Sum/WeightedAvg/Min/Max** (Median, Variance, etc.) | Requires mc-core consolidation change | New phase, ADR-required |
| **Multi-frequency Time dimensions** (week/month/quarter cohabitating) | High-cost change; relaxes MC2036 | New phase, ADR-required |

If you encounter any of these in your work and feel the urge to "while I'm here, just add..." — **resist.** Each is its own scoping exercise.

---

## Hard Rules (binding)

1. **Locked surfaces (zero-line diff against `58448a5`):**
   - `crates/mc-fixtures/`
   - `crates/mc-recipe/`
   - `crates/mc-drivers/`
   - `crates/mc-tessera/`
   - `mosaic-plugin/`
   - `crates/mc-cli/` **except** `query.rs` and `sweep.rs` (item 5 + item 8 touch these CLI sites)
2. **Allowed touch (binding scope):**
   - `crates/mc-model/src/formula.rs` — parser additions
   - `crates/mc-model/src/schema.rs` — multi-key field on `ParsedLookupTable` (item 3)
   - `crates/mc-model/src/validate.rs` — new validation (items 3, 4) + new MC codes (1022, 1023, 1024, 1025, 2050, 2051, 2052, 2053)
   - `crates/mc-model/src/compile.rs` — multi-key lookup compilation (item 3); ifs/switch desugaring may live here or in formula.rs
   - `crates/mc-model/src/lib.rs` — possible new public function `parse_expression` (item 8)
   - `crates/mc-core/src/rule.rs` — new `Expr` variants (items 1, 2, 5)
   - `crates/mc-core/src/cube.rs` — eval dispatch for new variants (item 5's `*_over` family; possibly item 1)
   - `crates/mc-cli/src/query.rs` — filter parser unification (item 8) + hyphen tolerance (item 7, possibly auto-closed by 8)
   - `crates/mc-model/tests/formula_integration.rs` — regression tests
3. **No new dependencies.** Hand-rolled per process-notes Rule 5. The `norm_inv` Beasley-Springer-Moro algorithm is ~30 lines of pure math.
4. **Toolchain stays Rust 1.78.** No `rust-toolchain.toml` edit.
5. **No `Cargo.lock` pin churn.** This phase is pure source-code; no dep changes expected.
6. **Backward compat (process-notes Rule 7):** every existing test passes. The Acme model + NBA cartridge + email-matchback models must all continue to validate, lint, and test cleanly.
7. **`mc-core` changes are limited to Expr enum extensions + eval dispatch.** No new public types beyond Expr variants. No public function additions in `cube.rs`. The kernel surface stays locked from a public-API perspective; the Expr enum is internal and growing it is the only allowed Phase 3-5 expansion vector.

---

## Acceptance Gates (lean — same as 6A.2 / 6A.3)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (785 → expect ~+50 = ~835 from regression tests).
- [ ] Locked-surfaces grep (per §"Hard Rules" rule 1) returns 0 lines.
- [ ] All 8 items shipped with their required regression tests.
- [ ] No SPEC QUESTION drift (or every SPEC QUESTION resolved before merge).

Per-item smoke checks (paste each output in the completion report):
- [ ] **Item 1:** `mc model query <fixture> --where 'is_element(Market, "Houston")' --format json` returns Houston coords only.
- [ ] **Item 2:** `mc model query <fixture> --where 'sqrt(Spend) > 50' --format json` works.
- [ ] **Item 3:** YAML with `key_dimensions: ["Market", "Time"]` validates clean and `lookup(table, Market, Time)` returns expected value.
- [ ] **Item 4:** YAML with `predict("model_a", x, y)` where `model_a` has 3 coefficients fails validation with MC2053.
- [ ] **Item 5:** `mc model query` of a derived measure that uses `avg_over(Spend, Market)` returns the average across markets.
- [ ] **Item 6:** YAML with `body: "ifs(Spend > 1000, 0.05, Spend > 100, 0.10, 0.02)"` validates and evaluates.
- [ ] **Item 7 + 8:** `mc model query <fixture> --where 'Time=Q1-2026'` works (covers item 7 if shipped separately, OR demonstrates item 8's hyphen tolerance).

---

## Order of Operations

1. Read this handoff in full.
2. Skim `docs/process-notes.md` Rules 1, 5, 7, 9, 10, 11.
3. Skim ADR-0011 + 0012 + 0013 (the formula expansion precedent — your shape template).
4. Skim `docs/research-notes/formula-language-expansion.md` §7I.8 (the explicit Phase 3I commitment).
5. **Order to ship items:**
   - **Item 8 first** (filter-formula parser unification) — enables the rest. **Item 7 is auto-closed** when 8 lands. (If 8 turns out larger than expected, ship 7 separately first and revisit 8.)
   - **Items 2, 6 next** (math primitives + ifs/switch) — pure additive, lowest blast radius.
   - **Items 1, 5** (is_element, *_over family) — additive but more intricate eval logic.
   - **Items 3, 4** (multi-key lookup, predict arity) — schema + validator changes.
6. Run gates after each item. **Don't batch-test all 8 at the end** — a regression in item 1 would be invisible until you finish item 8.
7. Commit per item with descriptive messages (`feat(3I item N): ...`); the PM will review each diff incrementally.
8. Write the completion report at `docs/reports/phase-3i-completion-report.md`.
9. **Stop.** Do not push the branch. PM does that after audit review (per process-notes Rule 11 anti-pattern: branches stay local until reviewed).

---

## Completion Report Expectations

Per process-notes Rule 10. Same shape as 6A.1 / 6A.2 / 6A.3:
- **Shipped** — what landed for each item with file:line citations.
- **Per-item smoke check outputs** — paste each command + actual output.
- **New diagnostic codes shipped:** MC1022, 1023, 1024, 1025; MC2050, 2051, 2052, 2053. Confirm none overlap with shipped/retired codes (sweep `validate.rs` + `lint.rs` to verify).
- **Acceptance gates checklist.**
- **Known debt** — anything noticed but not fixed.
- **Locked surfaces grep** — paste output.
- **List of new public APIs added to mc-model** (if `parse_expression` is added per item 8 W1, document it).

---

## SPEC QUESTION Format

Same as before:

```
SPEC QUESTION: [one-line summary]

Context: [where in the handoff this came up]
Spec text: [literal quote]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

Most likely SPEC QUESTION candidates in 3I:
- Item 1 W1: parse-time vs eval-time element resolution (the matrix says parse-time, but if the codebase pattern actually defers this, surface)
- Item 2: norm_inv accuracy — if Beasley-Springer-Moro turns out insufficient, surface before adding a dependency
- Item 3 W2: pipe character collision with element names that contain pipes
- Item 8 W1: whether `parse_expression` needs to be added to mc-model's public API

---

## Process note (handoff-first parallel flow)

Per process-notes §1, this phase is shipping under the handoff-first
parallel flow. ADR-0015 (Phase 3I formula language completion) will
be drafted in parallel with this implementation. If the ADR review
surfaces a binding decision that conflicts with this handoff,
surface as SPEC QUESTION and the PM will reconcile. Otherwise,
proceed with the handoff as the binding contract.

---

*End of handoff. Phase 3I is the last formula-expansion phase before
the roadmap pivots to data integration polish (5D) or UI (6B).
After this ships, the formula engine is at full coverage for the
marketing-finance + sports-betting + FP&A + demand-planning
domains.*
