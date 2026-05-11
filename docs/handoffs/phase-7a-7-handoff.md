# Phase 7A.7 Handoff — Narrative Evaluator Function Expansion

**Status:** Proposed (next to start)
**Date:** 2026-05-11
**Research note:** [narrative-evaluator-function-expansion.md](../research-notes/narrative-evaluator-function-expansion.md) (includes Desktop amendments)
**Estimated effort:** 2–3 sessions
**Crate:** `mc-narrative` only (evaluator.rs + context.rs)

---

## What this phase does

Adds ~15 missing functions and context variables to the narrative evaluator that eliminate all N/A issues in templates and enable Claude-tier narrative quality from deterministic templates.

---

## Part 1: Context variables (context.rs)

Add these alongside the existing `current.` and `prev.` insertion at ~line 80:

```rust
// After current/prev are set (line 83-84):
let first_val = sorted[0].value;
ctx.insert(format!("first.{measure}"), Val::Num(first_val));
ctx.insert(format!("last.{measure}"), Val::Num(current)); // alias for current

// First/last period names (set once, alongside current.period_name):
if !ctx.contains_key("first.period_name") {
    ctx.insert("first.period_name".into(), Val::Str(readable_name(&sorted[0].category)));
    ctx.insert("last.period_name".into(), Val::Str(readable_name(&sorted[n-1].category)));
}

// Min/max across all periods:
let min_val = entries.iter().map(|e| e.value).fold(f64::INFINITY, f64::min);
let max_val = entries.iter().map(|e| e.value).fold(f64::NEG_INFINITY, f64::max);
ctx.insert(format!("min.{measure}"), Val::Num(min_val));
ctx.insert(format!("max.{measure}"), Val::Num(max_val));

// Min/max period names:
if let Some(min_entry) = entries.iter().min_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(std::cmp::Ordering::Equal)) {
    ctx.insert(format!("min.{measure}.period"), Val::Str(readable_name(&min_entry.category)));
}
if let Some(max_entry) = entries.iter().max_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(std::cmp::Ordering::Equal)) {
    ctx.insert(format!("max.{measure}.period"), Val::Str(readable_name(&max_entry.category)));
}

// Total periods alias:
ctx.insert("total_periods".into(), Val::Num(n as f64));
```

For `days_in_campaign`: attempt to parse first and last period names as dates using the existing time format parsing. If both parse → compute day difference. If either fails → `Val::Null`.

---

## Part 2: Core functions (evaluator.rs)

Add to the `strip_func` chain (after the existing function blocks, ~line 955):

### `concat(arg1, arg2, ...)`
```rust
if let Some(inner) = strip_func(expr, "concat") {
    let args = split_top_level_commas(inner);
    let mut result = String::new();
    for arg in &args {
        let val = eval_expr_full(arg.trim(), ctx, cube, ledger, benchmark, context, scope_key);
        match val {
            Val::Null => {} // skip nulls silently
            Val::Str(s) => result.push_str(&s),
            Val::Num(n) => result.push_str(&format!("{n}")), // auto-coerce, no formatting
            Val::Bool(b) => result.push_str(if b { "true" } else { "false" }),
        }
    }
    return Val::Str(result);
}
```

### `format(number, spec)`
```rust
if let Some(inner) = strip_func(expr, "format") {
    let args = split_top_level_commas(inner);
    if args.len() != 2 { return Val::Null; }
    let num = eval_expr_full(args[0].trim(), ctx, cube, ledger, benchmark, context, scope_key);
    let spec_val = eval_expr_full(args[1].trim(), ctx, cube, ledger, benchmark, context, scope_key);
    match (num.as_num(), spec_val.as_str()) {
        (Some(n), Some(spec)) => Val::Str(format_number(n, spec)),
        _ => Val::Null,
    }
}
```

Format spec implementation (`format_number` helper):
```
Spec grammar: [+][,][.N][%]
  + → always show sign
  , → thousands separator (US comma)
  .N → N decimal places
  % → multiply by 100, append %

Special: NaN → "—", ±Inf → "∞"/"-∞"
```

### `round(value, decimals)`
```rust
if let Some(inner) = strip_func(expr, "round") {
    let args = split_top_level_commas(inner);
    // eval both, round to n decimals
    // (val * 10^n).round() / 10^n
}
```

### `pct_change(current, previous)`
```rust
if let Some(inner) = strip_func(expr, "pct_change") {
    let args = split_top_level_commas(inner);
    // (current - previous) / previous * 100
    // If previous == 0 → Val::Null
}
```

### `ratio(numerator, denominator)`
```rust
if let Some(inner) = strip_func(expr, "ratio") {
    let args = split_top_level_commas(inner);
    // numerator / denominator
    // If denominator == 0 or null → Val::Null
}
```

### `plural(count, singular, plural_form)`
```rust
if let Some(inner) = strip_func(expr, "plural") {
    let args = split_top_level_commas(inner);
    // If count == 1 → singular, else → plural_form
}
```

### `coalesce(arg1, arg2, ...)`
```rust
if let Some(inner) = strip_func(expr, "coalesce") {
    let args = split_top_level_commas(inner);
    // Eval left to right, return first non-Null
    // Nonexistent variables → Null (not error)
}
```

---

## Part 3: Analysis functions (evaluator.rs)

### `trend_slope(measure)` → `'up'` / `'down'` / `'flat'`
```
total_change_pct = (last - first) / first * 100
if |total_change_pct| < 5: "flat"
if total_change_pct > 0: "up"
else: "down"

Requires period_count >= 2. Returns Null if < 2.
```

### `trend_consistency(measure)` → `'steady'` / `'variable'` / `'volatile'`
```
cv = stddev(all_period_values) / mean(all_period_values)
if cv < 0.15: "steady"
if cv < 0.50: "variable"
else: "volatile"

Requires period_count >= 3. Returns Null if < 3.
```

### `streak(measure)` → signed integer
```
Walk from latest period backwards
Count consecutive same-direction changes
+3 = three consecutive increases; -2 = two consecutive decreases
Minimum 2 periods. Returns 0 if < 2 or if latest change is flat.
```

### `best_period(measure)` / `worst_period(measure)` → period name string
```
Return the period name with the highest/lowest value.
On ties: return the earliest period (deterministic).
Requires period_count >= 1.
```

### `volatility(measure)` → number
```
Coefficient of variation: stddev / mean.
Returns a number [0, ∞). Low (<0.15) = stable. High (>0.50) = volatile.
Requires period_count >= 2. Returns Null if < 2 or if mean == 0.
```

### `days_in_campaign` context variable
```
Parse first.period_name and last.period_name as dates (using ADR-0014 time parser if available).
Compute calendar day difference.
If either name can't be parsed → Null.
```

---

## Part 4: Helper function needed — `split_top_level_commas`

Several functions take variadic args. You need a helper that splits by commas but respects nested parentheses:

```rust
fn split_top_level_commas(s: &str) -> Vec<&str> {
    // Split by commas at paren depth 0
    // "concat('hello, world', format(x, ','))" → ["'hello, world'", "format(x, ',')"]
}
```

Check if this already exists in the evaluator (it might — `eval_if` already splits ternary args).

---

## Part 5: Tests

### Unit tests for each function:
```rust
#[test] fn test_concat_strings()          // concat('a', 'b', 'c') → "abc"
#[test] fn test_concat_with_null()        // concat('a', null, 'b') → "ab"
#[test] fn test_concat_with_number()      // concat('Total: ', 42) → "Total: 42"
#[test] fn test_format_comma()            // format(441263, ',') → "441,263"
#[test] fn test_format_decimal()          // format(1.456, '.2') → "1.46"
#[test] fn test_format_signed()           // format(5.0, '+.0') → "+5"
#[test] fn test_format_percent()          // format(0.0146, '.2%') → "1.46%"
#[test] fn test_format_nan()              // format(NaN, '.2') → "—"
#[test] fn test_round()                   // round(1.456, 2) → 1.46
#[test] fn test_pct_change()              // pct_change(120, 100) → 20.0
#[test] fn test_pct_change_zero_denom()   // pct_change(5, 0) → Null
#[test] fn test_ratio_zero_denom()        // ratio(100, 0) → Null
#[test] fn test_plural_singular()         // plural(1, 'month', 'months') → "month"
#[test] fn test_plural_multiple()         // plural(5, 'month', 'months') → "months"
#[test] fn test_coalesce_first_wins()     // coalesce(42, 99) → 42
#[test] fn test_coalesce_skip_null()      // coalesce(null, 99) → 99
#[test] fn test_coalesce_zero_not_null()  // coalesce(0, 99) → 0
#[test] fn test_trend_slope_up()          // values [1,2,3,4,5] → "up"
#[test] fn test_trend_slope_flat()        // values [10,10.2,9.8,10.1] → "flat"
#[test] fn test_trend_consistency_steady()    // low CV → "steady"
#[test] fn test_trend_consistency_volatile()  // high CV → "volatile"
#[test] fn test_streak_positive()         // values [1,2,3,4] → 3 (three up-moves)
#[test] fn test_streak_negative()         // values [4,3,2,1] → -3
#[test] fn test_streak_mixed()            // values [1,3,2,4] → 1 (only last move is up)
#[test] fn test_best_period()             // max value period
#[test] fn test_worst_period()            // min value period
#[test] fn test_best_period_tie()         // earliest on tie
#[test] fn test_first_last_context_vars() // first.CTR, last.CTR populated correctly
#[test] fn test_min_max_context_vars()    // min.CTR, max.CTR + period names
```

### Regression: existing templates produce identical output
Run the existing display-like.yaml templates against the Acme cube data with both old and new evaluator. Byte-for-byte comparison.

---

## Part 6: Upgrade templates (after functions ship)

Rewrite `demo/narratives/display-like.yaml` to use the new functions. Eliminate the template-splitting workarounds from the N/A fix. Examples:

```yaml
- id: campaign_overview
  template: >
    **{period_count} {period_word}** of campaign data —
    **{total_impressions}** impressions, **{total_clicks}** clicks,
    **{avg_ctr:.2f}%** average CTR{conversion_note}.
  bindings:
    period_word: "plural(period_count, 'month', 'months')"
    total_impressions: "format(sum.Impressions, ',')"
    total_clicks: "format(sum.Clicks, ',')"
    avg_ctr: "campaign_avg.CTR"
    conversion_note: >
      if(sum.Conversions > 0,
        concat(', generating **', format(sum.Conversions, ','), '** conversions'),
        '')

- id: full_campaign_trend
  when: "period_count >= 3"
  template: >
    Over **{period_count}** months, CTR moved from **{first_ctr:.2f}%** to
    **{last_ctr:.2f}%** — a **{trend_slope(CTR)}** and **{trend_consistency(CTR)}**
    trajectory. {streak_note}
  bindings:
    first_ctr: "first.CTR"
    last_ctr: "last.CTR"
    streak_note: >
      if(streak(CTR) >= 3,
        concat('CTR has increased for **', streak(CTR), '** consecutive months.'),
        if(streak(CTR) <= -3,
          concat('CTR has declined for **', abs(streak(CTR)), '** consecutive months.'),
          ''))
```

---

## Acceptance criteria

1. All Tier 1 context variables populated (`first.`, `last.`, `min.`, `max.`, period names)
2. `concat()` works with strings, numbers (auto-coerce), nulls (skip)
3. `format()` handles `,` `.N` `+` `%` specs correctly
4. `format()` produces "—" for NaN, not "NaN"
5. `pct_change()` and `ratio()` return Null on zero denominator
6. `trend_slope()` returns deterministic results per threshold spec
7. `trend_consistency()` returns deterministic results per CV thresholds
8. `streak()` returns signed integer
9. `best_period()` / `worst_period()` resolve ties to earliest
10. `coalesce()` treats zero as non-null; skips Null
11. All existing templates produce identical output (regression test)
12. `cargo test --workspace` passes
13. `cargo clippy --all-targets --workspace -- -D warnings` passes
14. No changes to `mc-core`
15. Templates rewritten to use new functions (no more template-splitting workarounds)

---

## Files to modify

| File | Change |
|---|---|
| `crates/mc-narrative/src/context.rs` | Add first/last/min/max/days_in_campaign context vars |
| `crates/mc-narrative/src/evaluator.rs` | Add ~12 functions to strip_func chain + format_number helper |
| `crates/mc-narrative/src/evaluator.rs` (tests) | ~30 unit tests |
| `demo/narratives/display-like.yaml` | Rewrite templates to use new functions |

---

**End of handoff. Ship Tier 1+2 first (fixes all N/A), then Tier 3 (trend analysis wow factor), then upgrade templates.**
