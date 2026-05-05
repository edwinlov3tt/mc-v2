# Phase 3E + 3F + 3G Combined Handoff — Formula Language Expansion

> **Audience:** a single Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that implements all three
> formula-language expansion phases back-to-back on one branch.
>
> **You inherit a green main branch** (commit `262633f`, 524 passing
> tests across the workspace). Phase 3D shipped the formula parser.
> You now extend it three times.
>
> **Branch:** `phase-3e-3f-3g/formula-expansion`. Create it from main.
> Work sequentially: implement 3E, report DONE, continue to 3F, report
> DONE, continue to 3G, report DONE.
>
> **Hard rule:** Follow `CLAUDE.md` (the operating manual) for all
> process, naming, and quality gates. The hierarchy of authority for
> semantic questions is: ADR-0011/0012/0013 > this handoff >
> `CLAUDE.md` > your intuition.
>
> **Locked crates (zero-line diff at every milestone):**
> `mc-fixtures`, `mc-cli`, `mc-drivers`, `mc-recipe`, `mc-tessera`.
> `mc-core` gets ONLY new `Expr` variants + eval match arms + helpers
> for cross-coordinate reads. No new mc-core deps. No unsafe. No async.

---

## Combined prompt (verbatim contract)

> We are implementing Phases 3E, 3F, and 3G of the Mosaic formula
> language expansion — three sequential milestones on one branch.
>
> **Phase 3E:** Conditionals and Basic Operations (ADR-0011)
> **Phase 3F:** Time-Series and Period Operations (ADR-0012)
> **Phase 3G:** Reference-Data Blocks (ADR-0013)
>
> Work on branch `phase-3e-3f-3g/formula-expansion`, created from main.
> Report DONE at each milestone before continuing to the next. Do NOT
> commit or tag — the user reviews first. At each DONE checkpoint, all
> 524 existing tests must still pass plus the new tests for that phase.
>
> ---
>
> ### Milestone 1: Phase 3E — Conditionals and Basic Operations
>
> Add 17 new `ParsedRuleBody` variants per ADR-0011 Decision 1:
>
> **Comparison operators (6):** `Gt`, `Lt`, `Gte`, `Lte`, `Eq`, `Neq`
> — each with `{ left, right }` holding `Box<ParsedRuleBody>`.
>
> **Logical operators (3):** `And { left, right }`, `Or { left, right }`,
> `Not { operand }`.
>
> **Functions (8):** `If { condition, then_branch, else_branch }`,
> `Min { args: Vec<Box<ParsedRuleBody>> }`,
> `Max { args: Vec<Box<ParsedRuleBody>> }`, `Abs { operand }`,
> `SafeDiv { numerator, denominator, default }`,
> `Clamp { value, lo, hi }`,
> `Coalesce { args: Vec<Box<ParsedRuleBody>> }`,
> `ActualRef { measure: String }`.
>
> **Precedence order** (lowest to highest binding):
> 1. `or`
> 2. `and`
> 3. `not` (unary logical)
> 4. Comparisons: `==`, `!=`, `<`, `>`, `<=`, `>=` (NON-ASSOCIATIVE —
>    `a > b > c` fires MC1008)
> 5. Addition: `+`, `-`
> 6. Multiplication: `*`, `/`
> 7. Unary arithmetic: `+`, `-`
> 8. Function call / primary / parentheses
>
> **Null semantics (critical — from ADR-0011 Decision 3 amendment):**
> - Comparison involving Null returns **Null** (NOT 0.0)
> - `if(Null_condition, then, else)` returns the **else branch**
> - Logical: `Null and x` = Null; `Null or x` = Null; `not(Null)` = Null
> - Booleans are f64-encoded: 1.0 = true, 0.0 = false
> - `if()` treats non-zero as truthy, zero as falsy, Null as else-branch
>
> **`actual_ref` specification (ADR-0011 Decision 4):**
> - Requires exactly ONE Scenario-kind dimension with `actuals_element:`
>   field in its YAML declaration
> - `actual_ref(Spend)` reads Spend at [actuals_element, same Version,
>   same Time, same Channel, same Market, Measure=Spend]
> - Returns Null if no value exists at target coordinate
> - MC2037 if `actuals_element` field missing on Scenario-kind dim
> - Cross-coordinate nesting forbidden: MC1013
>
> **`CoordinateRead` enum (shared infrastructure for 3F/3G):**
> ```rust
> enum CoordinateRead {
>     Local { measure: MeasureId },
>     ScenarioShift { measure: MeasureId },
>     TimeOffset { offset: i32, measure: MeasureId },
>     DimensionScan { dimension: DimensionId, measure: MeasureId },
> }
> ```
>
> **Diagnostic codes:**
> - MC1007: unknown function call
> - MC1008: wrong argument count / chained non-associative comparison
> - MC1009: `actual_ref` with non-identifier argument
> - MC1013: cross-coordinate function nesting
> - MC2037: `actual_ref` used but no `actuals_element` on Scenario dim
>
> **MC1004 narrows:** after 3E, MC1004 covers "unexpected token" only;
> unknown functions are MC1007. This is a diagnostic-code split.
>
> **Acceptance (Milestone 1):**
> - All 524 existing tests pass unchanged
> - New tests for every new function (parse, serialize, eval, round-trip)
> - `actual_ref` correctness tests (cross-scenario read + dirty prop)
> - Round-trip stability: `parse(serialize(parse(x))) == parse(x)` for
>   all new syntax
> - `cargo fmt --check --all` + `cargo clippy --workspace --all-targets
>   -- -D warnings` clean
> - No `unwrap()`/`expect()`/`panic!()` in `mc-core/src/` or
>   `mc-model/src/`
>
> ---
>
> ### Milestone 2: Phase 3F — Time-Series and Period Operations
>
> Add 5 new `ParsedRuleBody` variants per ADR-0012 Decision 6:
>
> - `Prev { measure: String }` — previous time-period value
> - `Lag { measure: String, periods: Box<ParsedRuleBody> }` — n periods ago
> - `Cumulative { measure: String }` — running sum
> - `RollingAvg { measure: String, window: Box<ParsedRuleBody> }` — moving avg
> - `PeriodIndex` — current element's 0-based position in Time dim
>
> **New dimension kind:** `"Time"` (ADR-0012 Decision 1).
> - MC2035: no `kind: "Time"` dim but time-series functions used
> - MC2036: multiple `kind: "Time"` dimensions
> - MC1010: `lag` with non-numeric period argument
> - MC1011: `rolling_avg` window resolves to non-positive integer
> - MC1012: time-series function used but no Time-kind dimension
> - MC3010: time elements with `date:` metadata in non-chronological order
> - MC3012: `cumulative` dirty-prop estimated > 50,000 entries (lint)
>
> **Boundary semantics:**
> - `prev(X)` / `lag(X, n)` at out-of-range -> Null
> - `cumulative(X)` at period 1 -> X itself
> - `rolling_avg(X, n)` at period < n -> partial window average
> - `lag(X, -n)` = lead (future periods); out-of-range -> Null
> - `lag(X, 0)` = current period value
>
> **Dirty propagation:**
> - `prev(X)` at index N: writing X at N-1 dirties N
> - `cumulative(X)`: writing at P dirties P+1 through P_max (worst case)
> - `rolling_avg(X, W)`: writing at K dirties K+1 through K+W-1
>
> **Cross-coordinate nesting still forbidden (MC1013):**
> `prev(actual_ref(X))` and `actual_ref(prev(X))` both rejected.
>
> **Acceptance (Milestone 2):**
> - All prior tests pass (524 + 3E additions)
> - MoM growth, YoY, cumulative, rolling average correctness tests
> - Boundary tests (first period, last period, partial windows)
> - Dirty-propagation tests for time-series writes
> - MC3012 lint fires on large cubes
> - Round-trip stability for all new functions
>
> ---
>
> ### Milestone 3: Phase 3G — Reference-Data Blocks
>
> **New YAML top-level blocks** (ADR-0013 Decision 1):
>
> ```yaml
> benchmarks:
>   - name: "industry_cpc"
>     source: "WordStream 2025"
>     last_updated: "2025-03-15"
>     key_dimension: "Channel"
>     values: { Paid_Search: 5.50, Paid_Social: 3.20 }
>
> lookup_tables:
>   - name: "tax_rate"
>     key_dimension: "Market"
>     values: { Florida: 0.055, Georgia: 0.0575 }
>
> status_thresholds:
>   - name: "cpc_health"
>     bands:
>       - { label: "Good", max: 3.0 }
>       - { label: "Warning", max: 7.0 }
>       - { label: "Critical" }
> ```
>
> **New schema types:** `ParsedBenchmark`, `ParsedLookupTable`,
> `ParsedStatusThreshold`, `ParsedThresholdBand`. All live in
> `schema.rs`. `ParsedModel` gains three new `#[serde(default)]`
> Vec fields.
>
> **New AST nodes (4):**
> - `Benchmark { name: String, key_expr: Box<ParsedRuleBody> }`
> - `Lookup { table: String, key_expr: Box<ParsedRuleBody> }`
> - `Bucket { value: Box<ParsedRuleBody>, threshold_name: String }`
> - `SumOver { dimension: String, measure: String }`
>
> **`bucket()` returns zero-based band index** as f64 (0.0, 1.0, 2.0...).
> Returns Null if input is Null.
>
> **`sum_over` semantics:** sums across all LEAF elements of the named
> dimension at the current coordinate for all other dims. Leaf-only
> (no double-counting consolidated values).
>
> **Diagnostic codes:**
> - MC1013: formula references unknown benchmark name (reuse of code)
> - MC1014: formula references unknown lookup table name
> - MC1015: formula references unknown threshold name
> - MC1016: `sum_over` first argument is not a declared dimension
> - MC2030: benchmark `last_updated` > 12 months old (lint warning)
> - MC2031: reference-data block unreferenced by any formula (lint)
> - MC2037: duplicate reference-data name across blocks
> - MC2038: `key_dimension` references undeclared dimension
> - MC2039: value key not a valid element in key dimension
> - MC2040: threshold has fewer than 2 bands
> - MC2041: threshold bands have non-ascending max values
> - MC2042: last threshold band has a max (should be unbounded)
> - MC3011: `sum_over` on dimension with > 50 elements (lint); Error > 10,000
> - MC3013: benchmark `source` field empty (lint)
> - MC3014: benchmark/lookup key is complex expression, not dim name (lint)
> - MC3015: benchmark > 12 months stale (suggestion to refresh)
> - MC5025: threshold bands have gaps
> - MC5026: threshold bands overlap
>
> **Dep-graph for `sum_over`:** writing measure M at ANY element of
> dimension D dirties every cell using `sum_over(D, M)` at EVERY
> element of D (N-to-N fan-out within one dimension).
>
> **Acceptance (Milestone 3):**
> - All prior tests pass (524 + 3E + 3F additions)
> - Tests for each new YAML block (parse, validate, reject malformed)
> - `sum_over` correctness tests (share-of-total patterns)
> - `benchmark`/`lookup`/`bucket` eval tests
> - Threshold validation tests (gaps, overlaps, ordering)
> - MC3011 lint fires on large dimensions
> - Round-trip stability for all new functions
> - Existing models without reference-data blocks still parse unchanged
>
> ---
>
> **Hard rules (all three milestones):**
>
> - `mc-core` changes: ONLY new `Expr` variants + eval arms +
>   cross-coordinate read helpers. No new deps. No unsafe. No async.
> - `mc-model` is the primary target: parser, schema, validator,
>   formula module, and (for 3G) new schema types.
> - `mc-fixtures`, `mc-cli`, `mc-drivers`, `mc-recipe`, `mc-tessera`:
>   LOCKED (0-line diff). Exception: `mc-cli` if inspect output changes.
> - All 524 existing tests pass unchanged at every milestone.
> - No new dependencies in any crate.
> - `cargo fmt --check --all` + `cargo clippy --workspace --all-targets
>   -- -D warnings` clean at every milestone.
> - No `unwrap()`/`expect()`/`panic!()` in `mc-core/src/` or
>   `mc-model/src/`.
> - Round-trip stability: `parse(serialize(parse(x))) == parse(x)` for
>   all new syntax at every milestone.
> - Toolchain stays at Rust 1.78. No `cargo update`.
>
> **Completion report format (per milestone):**
> ```
> DONE: Phase 3E [or 3F, 3G] -- [title]
>
> Build/Format/Lint/Tests: [status]
> New AST nodes: [list]
> New diagnostic codes: [list]
> New tests: [count]
> Round-trip stability: [pass/fail]
> Locked surfaces: [0-line diff on locked crates]
>
> Proceeding to Phase 3F [or 3G, or "all three milestones complete"].
> ```
>
> Do NOT commit, tag, or push. The user reviews first.

---

## How the parser works (implementation guide)

The existing parser lives at `crates/mc-model/src/formula.rs` (~743 lines).
It is a hand-rolled recursive-descent parser with these key methods:

| Method | Precedence level | Current operators |
|---|---|---|
| `parse_expression` | Additive | `+`, `-` |
| `parse_term` | Multiplicative | `*`, `/` |
| `parse_factor` | Primary | parens, unary, identifiers, numbers |

**To add new precedence levels (Phase 3E):**

Insert new methods between the existing ones. The final precedence
chain becomes:

```
parse_or_expression        -> `or`
  parse_and_expression     -> `and`
    parse_not_expression   -> `not` (unary)
      parse_comparison     -> ==, !=, <, >, <=, >= (NON-ASSOCIATIVE)
        parse_expression   -> +, - (existing)
          parse_term       -> *, / (existing)
            parse_factor   -> primary (existing)
```

The top-level entry point (`parse()`) calls `parse_or_expression`
instead of `parse_expression`.

**Non-associative comparisons:** `parse_comparison` parses ONE comparison
only. After parsing `left op right`, if another comparison operator
follows, fire MC1008 instead of chaining.

**Function dispatch in `parse_identifier_or_call`:**

Currently, only `if_null` is recognized. Phase 3E expands the function
table. The pattern is:

```rust
match name {
    "if_null" => { /* existing: 2 args */ }
    "if" => { /* 3 args: condition, then, else */ }
    "min" | "max" => { /* variadic: 2+ args */ }
    "abs" | "not" => { /* 1 arg */ }
    "safe_div" | "clamp" => { /* 3 args */ }
    "coalesce" => { /* variadic: 1+ args */ }
    "actual_ref" => { /* 1 arg: bare identifier only (MC1009 if not) */ }
    "prev" | "cumulative" => { /* 1 arg: bare identifier */ }
    "lag" | "rolling_avg" => { /* 2 args: identifier, expression */ }
    "period_index" => { /* 0 args */ }
    "benchmark" | "lookup" => { /* 2 args: string literal, expression */ }
    "bucket" => { /* 2 args: expression, string literal */ }
    "sum_over" => { /* 2 args: identifier, identifier */ }
    _ => { /* MC1007: unknown function */ }
}
```

**String literal parsing:** Phase 3G's `benchmark("name", Channel)` needs
string literals. Add support for double-quoted strings in `parse_factor`
(a new primary type). These are NOT measure refs; they produce a
`Const(ParsedScalar::Str(...))` or alternatively carry the string inline
in the AST node. Per the ADR, the `name` field is stored directly in the
AST node struct (`Benchmark { name: String, ... }`), so parse the string
at the function-call level, not as a general expression.

---

## How the evaluator works (mc-core)

The evaluator lives at `crates/mc-core/src/rule.rs`, function
`eval_expr` (line 358). Current shape:

```rust
pub fn eval_expr<F>(expr: &Expr, lookup_self: &mut F) -> Result<ScalarValue, EngineError>
where
    F: FnMut(ElementId) -> Result<ScalarValue, EngineError>,
```

The `Expr` enum (line 49) has 7 variants matching `ParsedRuleBody`:
`Const`, `SelfRef`, `Add`, `Sub`, `Mul`, `Div`, `IfNull`.

**For Phase 3E local operations** (comparisons, logical, min, max, abs,
safe_div, clamp, coalesce, if): add new `Expr` variants and match arms.
These only need the existing `lookup_self` closure.

**For cross-coordinate reads** (actual_ref, prev, lag, cumulative,
rolling_avg, sum_over): the evaluator needs access to cells at OTHER
coordinates. This means `eval_expr`'s signature must be extended OR
the cross-coordinate reads must be lowered to a different closure type.

**Recommended approach:** Add a second closure parameter (or a trait
object) for cross-coordinate reads:

```rust
pub fn eval_expr<F, G>(
    expr: &Expr,
    lookup_self: &mut F,
    lookup_cross: &mut G,
) -> Result<ScalarValue, EngineError>
where
    F: FnMut(ElementId) -> Result<ScalarValue, EngineError>,
    G: FnMut(&CoordinateRead) -> Result<ScalarValue, EngineError>,
```

The `lookup_cross` closure is provided by the caller (`Cube::read` or
equivalent) which has access to the full coordinate context. For Phase 3E,
`CoordinateRead::ScenarioShift` is the only variant used. Phase 3F adds
`TimeOffset`. Phase 3G adds `DimensionScan`.

**Alternative:** keep the single-closure signature and embed
cross-coordinate logic in specialized `Expr` variants that carry enough
context to resolve themselves. Either approach works; choose the one that
keeps the eval loop simplest.

---

## How to add a new function (step-by-step recipe)

For each new function (e.g., `min(a, b)`):

### Step 1: Add the AST variant to `ParsedRuleBody` in `schema.rs`

```rust
/// `min(a, b, ...)` — returns the minimum of its arguments.
Min(ParsedMinBody),
```

With the body struct:
```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedMinBody {
    pub min: Vec<ParsedRuleBody>,
}
```

### Step 2: Add the parser dispatch in `formula.rs`

In `parse_identifier_or_call`, add:
```rust
"min" => {
    self.advance(); // consume '('
    let args = self.parse_arg_list(2, None)?; // 2+ args
    Ok(ParsedRuleBody::Min(ParsedMinBody { min: args }))
}
```

### Step 3: Add the serializer arm in `formula.rs`

In `write_node_bare`:
```rust
ParsedRuleBody::Min(b) => {
    out.push_str("min(");
    for (i, arg) in b.min.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        write_node(out, arg, 0, false);
    }
    out.push(')');
}
```

### Step 4: Update the `prec()` function

Function-call nodes are atomic (highest precedence = 3 in the current
scheme, or whatever the max becomes after adding logical/comparison
levels).

### Step 5: Add the `Expr` variant in `mc-core/src/rule.rs`

```rust
Min(Vec<Box<Expr>>),
```

### Step 6: Add the eval arm in `eval_expr`

```rust
Expr::Min(args) => {
    let mut result: Option<f64> = None;
    for arg in args {
        match eval_expr(arg, lookup_self, lookup_cross)? {
            ScalarValue::Null => return Ok(ScalarValue::Null),
            ScalarValue::F64(v) => {
                result = Some(match result {
                    None => v,
                    Some(curr) => curr.min(v),
                });
            }
            _ => return Ok(ScalarValue::Null),
        }
    }
    Ok(result.map_or(ScalarValue::Null, |v| ScalarValue::F64(v)))
}
```

### Step 7: Add the compile bridge in `mc-model`

The compile stage translates `ParsedRuleBody` -> `Expr`. Add the arm
that converts `ParsedRuleBody::Min` to `Expr::Min`.

### Step 8: Write tests

- Parse test: `parse("min(Spend, CPC)")` produces `Min { ... }`
- Serialize test: `serialize(min_node)` produces `"min(Spend, CPC)"`
- Round-trip: `parse(serialize(parse("min(Spend, CPC)"))) == parse(...)`
- Eval test: `min(5.0, 3.0)` = 3.0; `min(Null, 3.0)` = Null

### Step 9: Update `serde` Deserialize for structured form (if needed)

If the structured-form YAML should support `{ min: [a, b] }`, add the
serde untagged variant. If structured form is formula-only, skip this.

---

## Key file pointers

| File | Role | Phases |
|---|---|---|
| `crates/mc-model/src/formula.rs` | Parser + serializer (extend) | 3E, 3F, 3G |
| `crates/mc-model/src/schema.rs` | `ParsedRuleBody` enum + schema types | 3E, 3F, 3G |
| `crates/mc-model/src/validate.rs` | Validation rules + formula parse step | 3E, 3F, 3G |
| `crates/mc-model/src/error.rs` | Diagnostic codes | 3E, 3F, 3G |
| `crates/mc-model/src/inspect.rs` | Inspect rendering (serialize update) | 3E, 3F, 3G |
| `crates/mc-model/src/lib.rs` | Module declarations | 3E, 3F, 3G |
| `crates/mc-core/src/rule.rs` | `Expr` enum + `eval_expr` | 3E, 3F, 3G |
| `crates/mc-core/src/cube.rs` | Cube read/write (cross-coord calling context) | 3E, 3F, 3G |
| `crates/mc-model/examples/acme.yaml` | Demo model (may add `actuals_element`) | 3E |
| `docs/decisions/0011-phase-3e-conditionals-and-basic-operations.md` | ADR-0011 (binding) | 3E |
| `docs/decisions/0012-phase-3f-time-series-operations.md` | ADR-0012 (binding) | 3F |
| `docs/decisions/0013-phase-3g-reference-data-blocks.md` | ADR-0013 (binding) | 3G |
| `docs/research-notes/formula-language-expansion.md` | Master research doc | all |
| `CLAUDE.md` | Operating manual (process gates) | all |

---

## Reproducible commands

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# Create the working branch
git checkout -b phase-3e-3f-3g/formula-expansion main

# Pre-flight gate (must be green before any changes)
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
cargo test --workspace                                    # 524 tests

# Iteration loop (per-crate)
cargo build -p mc-model
cargo test -p mc-model
cargo build -p mc-core
cargo test -p mc-core

# Full workspace check (run before each DONE)
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Determinism gate (run before each DONE)
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Forbidden-pattern grep (mc-core)
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
grep -rn "unsafe" crates/mc-core/src/

# Forbidden-pattern grep (mc-model)
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-model/src/

# Verify locked surfaces
git diff main -- crates/mc-fixtures/ crates/mc-cli/ crates/mc-drivers/ crates/mc-recipe/ crates/mc-tessera/
# expected: zero output

# Demo still works
cargo run --release --bin mc -- demo
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml
```

---

## Per-milestone checklists

### Phase 3E checklist

- [ ] 17 new `ParsedRuleBody` variants added to `schema.rs`
- [ ] Corresponding `Expr` variants added to `mc-core/src/rule.rs`
- [ ] Parser extended with new precedence levels (or > and > not > comparison > additive > multiplicative > primary)
- [ ] Non-associative comparison parsing (MC1008 on chaining)
- [ ] Function table expanded: `if`, `min`, `max`, `abs`, `safe_div`, `clamp`, `coalesce`, `actual_ref`
- [ ] MC1004 narrowed to "unexpected token" only; MC1007 = unknown function
- [ ] Serializer handles all 17 new nodes with round-trip stability
- [ ] Evaluator handles all 17 new nodes with correct Null semantics
- [ ] `actual_ref` cross-coordinate read works (ScenarioShift)
- [ ] `CoordinateRead` enum introduced (shared infra for 3F/3G)
- [ ] `actuals_element` field added to `ParsedDimension` schema
- [ ] MC1007, MC1008, MC1009, MC1013, MC2037 diagnostics implemented
- [ ] Cross-coordinate nesting rejection (MC1013)
- [ ] Tests: parse, serialize, round-trip, eval for each new function
- [ ] Tests: Null propagation for comparisons, Null-condition in `if()`
- [ ] Tests: `actual_ref` correctness + dirty propagation
- [ ] All 524 existing tests still pass
- [ ] `cargo fmt --check --all` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] No `unwrap()`/`expect()` in `mc-core/src/` or `mc-model/src/`
- [ ] Locked crates: 0-line diff
- [ ] 10 consecutive `cargo test --workspace -q` identical

### Phase 3F checklist

- [ ] 5 new `ParsedRuleBody` variants: `Prev`, `Lag`, `Cumulative`, `RollingAvg`, `PeriodIndex`
- [ ] Corresponding `Expr` variants + eval arms in mc-core
- [ ] `kind: "Time"` added to legal dimension kinds
- [ ] `date:` optional field on `ParsedElement` (for future use / MC3010)
- [ ] Parser: `prev`, `lag`, `cumulative`, `rolling_avg`, `period_index` in function table
- [ ] Serializer: all 5 new nodes round-trip correctly
- [ ] Evaluator: boundary behavior (Null at out-of-range, partial windows)
- [ ] Evaluator: negative lag = lead
- [ ] Dirty propagation: time-offset reverse edges
- [ ] MC1010, MC1011, MC1012, MC2035, MC2036, MC3010, MC3012 implemented
- [ ] Cross-coordinate nesting rejection (MC1013) for time-series + actual_ref combos
- [ ] Tests: MoM growth pattern (`safe_div(Revenue - prev(Revenue), prev(Revenue), 0)`)
- [ ] Tests: cumulative at first period = self
- [ ] Tests: rolling_avg partial windows match Excel semantics
- [ ] Tests: boundary Null returns for prev/lag at edges
- [ ] Tests: dirty-propagation correctness for time writes
- [ ] Tests: MC3012 lint fires on large time dimension
- [ ] All prior tests (524 + 3E) still pass
- [ ] Format/clippy/determinism gates pass
- [ ] Locked crates: 0-line diff

### Phase 3G checklist

- [ ] `ParsedBenchmark`, `ParsedLookupTable`, `ParsedStatusThreshold`, `ParsedThresholdBand` types in schema.rs
- [ ] `ParsedModel` gains `benchmarks`, `lookup_tables`, `status_thresholds` fields (`#[serde(default)]`)
- [ ] 4 new `ParsedRuleBody` variants: `Benchmark`, `Lookup`, `Bucket`, `SumOver`
- [ ] Corresponding `Expr` variants + eval arms
- [ ] Parser: `benchmark`, `lookup`, `bucket`, `sum_over` in function table
- [ ] Serializer: all 4 new nodes round-trip correctly
- [ ] Evaluator: `benchmark` reads from model's benchmarks by key_dimension element
- [ ] Evaluator: `lookup` exact-match; returns Null on miss
- [ ] Evaluator: `bucket` returns 0-based band index; Null input -> Null
- [ ] Evaluator: `sum_over` sums leaf elements of named dimension
- [ ] Validator: exhaustive bands (MC5025 gaps, MC5026 overlaps)
- [ ] Validator: MC2037-MC2042 for reference-data block errors
- [ ] Lint: MC2030 (stale benchmark), MC2031 (unreferenced block), MC3011 (large sum_over), MC3013 (empty source), MC3014 (complex key expr), MC3015 (stale date)
- [ ] Dirty propagation: `sum_over` N-to-N fan-out within dimension
- [ ] Tests: YAML blocks parse correctly (including empty/missing)
- [ ] Tests: malformed YAML blocks rejected with correct codes
- [ ] Tests: `sum_over` share-of-total correctness
- [ ] Tests: `bucket` with various inputs covers all bands
- [ ] Tests: existing models without ref-data blocks parse unchanged
- [ ] All prior tests (524 + 3E + 3F) still pass
- [ ] Format/clippy/determinism gates pass
- [ ] Locked crates: 0-line diff

---

## Implementation sequence guidance

### Phase 3E order

1. Add `actuals_element` optional field to `ParsedDimension` in schema.rs
2. Add 17 new `ParsedRuleBody` variants (with body structs) to schema.rs
3. Add corresponding `Expr` variants to mc-core rule.rs
4. Extend `eval_expr` with all new eval arms (local ops first, then actual_ref)
5. Refactor parser: insert new precedence levels (`parse_or_expression` etc.)
6. Expand function table in `parse_identifier_or_call`
7. Extend serializer for all new nodes
8. Extend `prec()` function for new precedence levels
9. Update the compile bridge (ParsedRuleBody -> Expr translation)
10. Add diagnostic codes (MC1007, MC1008, MC1009, MC1013, MC2037)
11. Write tests (start with simple local ops, end with actual_ref)
12. Run all gates

### Phase 3F order

1. Add `"Time"` to legal dimension kinds in validator
2. Add optional `date:` field to `ParsedElement`
3. Add 5 new `ParsedRuleBody` variants
4. Add corresponding `Expr` variants
5. Extend eval with time-series semantics (needs time-dimension context)
6. Extend parser function table
7. Extend serializer
8. Add dirty-propagation logic for time-offset edges
9. Add diagnostic codes (MC1010-MC1012, MC2035-MC2036, MC3010, MC3012)
10. Write tests
11. Run all gates

### Phase 3G order

1. Add `ParsedBenchmark`, `ParsedLookupTable`, `ParsedStatusThreshold` to schema.rs
2. Add 3 new Vec fields to `ParsedModel`
3. Add validation rules for reference-data blocks
4. Add 4 new `ParsedRuleBody` variants
5. Add corresponding `Expr` variants
6. Extend eval (benchmark/lookup are map lookups; bucket is band search; sum_over is a loop)
7. Extend parser function table (handle string literal arguments)
8. Extend serializer
9. Add dirty-propagation logic for `sum_over` (DimensionScan)
10. Add all diagnostic codes
11. Write tests (YAML parsing first, then eval, then validation/lint)
12. Run all gates

---

## Critical semantic decisions (carry from ADRs)

1. **Booleans are f64-encoded.** No `ScalarValue::Bool`. Comparisons
   return 1.0/0.0. `if()` uses truthy/falsy semantics.

2. **Null in comparisons returns Null** (not 0.0). This preserves De
   Morgan's laws. `not(Null > 5)` = Null = `Null <= 5`.

3. **`if(Null, then, else)` returns else.** Null condition = unknown =
   take the safe path.

4. **Time-series functions operate on element INDEX order**, not
   calendar dates. "Previous" = element at index - 1.

5. **`rolling_avg` uses partial windows** (Excel-compatible). Period 1
   of a 3-period rolling avg returns just the period-1 value.

6. **`sum_over` sums LEAF elements only.** No double-counting via
   consolidated parents.

7. **`bucket()` returns zero-based band index as f64.** String labels
   are a display concern for Phase 6.

8. **`lookup()` is exact-match only.** Null on miss. No interpolation
   in Phase 3G.

9. **Cross-coordinate nesting is always MC1013.** Never nest
   `actual_ref`, `prev`, `lag`, `cumulative`, `rolling_avg`, `sum_over`
   inside each other. Users must use intermediate derived measures.

10. **Threshold bands are exhaustive.** First band starts at -infinity;
    last band ends at +infinity (no `max` field). Gaps and overlaps are
    hard errors (MC5025/MC5026).

---

## What NOT to do

- Do NOT add `ScalarValue::Bool` — f64-encoded booleans only.
- Do NOT add a parser library (`pest`, `nom`, `lalrpop`).
- Do NOT add new dependencies to any crate.
- Do NOT implement short-circuit evaluation for `and`/`or` (nice-to-have, not required).
- Do NOT implement `latest_actual_ref` (deferred beyond 3E).
- Do NOT implement calendar-math functions (`days_between`, `month_of`) — deferred to 3I.
- Do NOT implement interpolated lookups — Phase 3G.1 candidate.
- Do NOT modify `mc-fixtures`, `mc-cli`, `mc-drivers`, `mc-recipe`, `mc-tessera`.
- Do NOT bump `model_format_version` (optional blocks don't warrant it).
- Do NOT add `avg_over`, `min_over`, `max_over` — only `sum_over` is Phase 3G.
- Do NOT implement string-valued ScalarValue — deferred to Phase 3J+.
- Do NOT start Phase 3H (fitted models) after finishing 3G.

---

## Resolution order (when uncertain)

1. ADR-0011 / ADR-0012 / ADR-0013 (the binding contracts for each phase)
2. This handoff document
3. `docs/research-notes/formula-language-expansion.md`
4. `CLAUDE.md` (operating manual)
5. Phase 3D handoff (structural template / precedent)
6. The brief and `engine-semantics.md` (kernel semantics)
7. Your intuition (last — if you get here, write a SPEC QUESTION)

If the ADRs, this handoff, and the research note appear to conflict on
a semantic point, **stop and write a SPEC QUESTION** per CLAUDE.md section 11.
Do not silently pick one interpretation.

---

*Combined handoff drafted 2026-05-04 for the Phase 3E/3F/3G formula
language expansion sequence. ADR-0011, ADR-0012, and ADR-0013 are all
Accepted status. The implementing instance works on one branch and
reports DONE three times.*
