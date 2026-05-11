# Narrative Evaluator Function Expansion — Phase 7A.7

**Status:** Research note — ready for implementation
**Date:** 2026-05-11
**Scope:** Add missing evaluator functions + context variables to `mc-narrative` that enable rich conditional narratives
**Crate:** `mc-narrative` only (evaluator.rs + context.rs)
**Effort:** 2–3 sessions
**ADR needed:** No — this is a feature expansion within the existing evaluator framework, not an architectural decision. The evaluator's `strip_func` pattern is already established; this adds more functions to the same pattern.

---

## The problem

The narrative evaluator supports math (`+`, `-`, `*`, `/`), comparisons (`>`, `<`, `==`, `AND`, `OR`), and ~25 functions (`if`, `abs`, `count_where`, `names_where`, `benchmark_*`, `ledger_*`, `has_context_event`, etc.). But it's missing fundamental capabilities that templates need for rich output:

| Missing | What breaks | Example |
|---|---|---|
| `concat(a, b)` | Can't build conditional strings in bindings | "X impressions, Y clicks, **Z conversions**" where Z part is conditional |
| `format(num, spec)` | Can't format numbers inside conditional strings | Comma-separated numbers in `if()` branches |
| `first.<Measure>` | Can't reference the earliest period | "CTR moved from **0.85%** (first) to **2.25%** (last)" |
| `last.<Measure>` | Alias for `current.` — clearer intent | Same |
| `min.<Measure>` | Can't find minimum across all periods | "Lowest CTR was **0.07%** in March" |
| `max.<Measure>` | Can't find maximum across all periods | "Peak impressions of **192,698** in April" |
| `round(x, n)` | Can't control decimal places in bindings | Clean numbers in conditional text |
| `pct_change(a, b)` | Repeated verbose formula in every template | `(current.X - prev.X) / prev.X * 100` → `pct_change(current.X, prev.X)` |
| `plural(n, singular, plural)` | Can't pluralize conditionally | "1 month" vs "5 months" |
| `days_between(a, b)` | Can't compute campaign duration | "187 days of campaign data" |
| `rank_of(dim, measure)` | Can't say "2nd highest" | "Houston was the **2nd highest** performing city" |
| `trend_direction(measure)` | Can't describe multi-period trends | "CTR has been **steadily increasing** over 5 months" |

Every template that needs these falls back to "N/A" or requires splitting into multiple templates with duplicated logic.

---

## Proposed functions

### Tier 1: Must-have (fixes all current N/A issues)

#### `concat(expr1, expr2, ...)`
Concatenate string values. Returns a string.

```yaml
binding: "concat('delivered ', format(sum.Impressions, ','), ' impressions')"
# → "delivered 441,263 impressions"
```

Implementation: split args by top-level commas, eval each, coerce to string, join.

#### `format(number, spec)`
Format a number. Spec is a simplified format string.

```yaml
binding: "format(sum.Impressions, ',')"     # → "441,263"
binding: "format(campaign_avg.CTR, '.2')"   # → "0.14"
binding: "format(sum.Conversions, ',.0')"   # → "233"
binding: "format(pct_change, '+.0')"        # → "+165" or "-23"
```

Specs: `,` = comma separator, `.N` = N decimal places, `+` = always show sign. Combine: `+,.1` = signed, comma-separated, 1 decimal.

Implementation: parse spec string, apply Rust's format machinery.

#### `first.<Measure>` context variable
Value of `<Measure>` at the first (earliest) time period.

```yaml
when: "period_count >= 3"
binding: "first.CTR"  # → 0.85 (the oldest period's CTR)
```

Implementation: in `context.rs`, alongside where `current.` and `prev.` are set (line 83), add:
```rust
let first_val = sorted[0].value;
ctx.insert(format!("first.{measure}"), Val::Num(first_val));
```

Also add `first.period_name`:
```rust
ctx.insert("first.period_name".into(), Val::Str(readable_name(&sorted[0].category)));
```

#### `last.<Measure>` context variable
Alias for `current.<Measure>` — semantic clarity.

```yaml
binding: "last.CTR"  # same as current.CTR
```

Implementation: insert alongside `current.`:
```rust
ctx.insert(format!("last.{measure}"), Val::Num(current));
```

### Tier 2: High-value (enables richer analysis)

#### `min.<Measure>` / `max.<Measure>` context variables
Minimum and maximum value of a measure across all time periods.

```yaml
binding: "min.CTR"   # → 0.07 (worst month)
binding: "max.CTR"   # → 1.11 (best month)
```

Implementation: in `context.rs`, compute from the sorted entries:
```rust
let min_val = entries.iter().map(|e| e.value).fold(f64::INFINITY, f64::min);
let max_val = entries.iter().map(|e| e.value).fold(f64::NEG_INFINITY, f64::max);
ctx.insert(format!("min.{measure}"), Val::Num(min_val));
ctx.insert(format!("max.{measure}"), Val::Num(max_val));
```

Also add the period name for min/max:
```rust
ctx.insert(format!("min.{measure}.period"), Val::Str(readable_name(&min_entry.category)));
ctx.insert(format!("max.{measure}.period"), Val::Str(readable_name(&max_entry.category)));
```

Enables: "Peak impressions of **192,698** in **April** — lowest was **65,212** in **May**."

#### `pct_change(current, previous)` function
Percentage change. Returns `(current - previous) / previous * 100`.

```yaml
binding: "pct_change(current.CTR, prev.CTR)"  # → 16.0
binding: "pct_change(current.Clicks, prev.Clicks)"  # → -74.0
```

Implementation: two-arg function, eval both, compute. Returns `Val::Null` if previous is 0.

Eliminates the repeated `(current.X - prev.X) / prev.X * 100` pattern in every MoM template.

#### `round(value, decimals)` function
Round a number.

```yaml
binding: "round(campaign_avg.CTR, 2)"  # → 0.14
binding: "round(sum.Impressions / 1000, 1)"  # → 441.3
```

Implementation: two-arg function, `(val * 10^n).round() / 10^n`.

#### `plural(count, singular, plural_form)` function
Conditional pluralization.

```yaml
binding: "plural(period_count, 'month', 'months')"  # → "months" if > 1
```

Implementation: three-arg function, return singular if count == 1, else plural_form.

#### `coalesce(a, b, ...)` function
Return first non-null/non-N/A value.

```yaml
binding: "coalesce(campaign_avg.CTR, campaign_avg.VCR, 0)"
# → CTR if it exists, else VCR if it exists, else 0
```

Enables templates that work across tactic types without separate when-guards for every metric variant.

### Tier 3: Advanced (narrative intelligence)

#### `trend_direction(measure)` function
Analyze multi-period trend and return a description string.

```yaml
binding: "trend_direction(CTR)"
# → "steadily increasing" | "steadily decreasing" | "fluctuating"
#    | "stable" | "spiking" | "recovering"
```

Implementation: examine all period values for the measure. Compute:
- Count of consecutive increases/decreases
- Coefficient of variation (stddev / mean)
- Direction of most recent 2 periods vs overall

Return one of: `'steadily increasing'`, `'steadily decreasing'`, `'fluctuating'`, `'stable'`, `'spiking'`, `'recovering'`, `'declining then recovering'`.

This is the function that makes narratives feel human-authored — a human analyst wouldn't say "CTR went from 0.85 to 2.25." They'd say "CTR has been **steadily increasing** over 5 months."

#### `best_period(measure)` / `worst_period(measure)` functions
Return the period name with the highest/lowest value.

```yaml
binding: "best_period(CTR)"   # → "May 2026"
binding: "worst_period(CTR)"  # → "February 2026"
```

Enables: "Best-performing month was **May 2026** with **2.25%** CTR."

#### `streak(measure, direction)` function
Count consecutive periods of increase/decrease.

```yaml
binding: "streak(CTR, 'up')"    # → 3 (3 consecutive months of CTR increase)
binding: "streak(Clicks, 'down')"  # → 2
```

Enables: "CTR has **increased for 3 consecutive months** — sustained positive trajectory."

#### `volatility(measure)` function
Coefficient of variation (stddev/mean) across all periods. Returns a number [0, ∞).

```yaml
when: "volatility(CTR) > 0.5"
template: "CTR is **highly volatile** — ranging from **{min.CTR:.2f}%** to **{max.CTR:.2f}%**."
```

Low volatility (<0.15) = stable. Medium (0.15–0.5) = some variance. High (>0.5) = volatile.

#### `days_in_campaign` context variable
Number of calendar days from first period to last period.

```yaml
template: "**{days_in_campaign}** days of campaign data across **{period_count}** reporting periods."
```

Implementation: parse first and last period names as dates, compute difference. If period names aren't parseable as dates, return `Val::Null`.

#### `spend_efficiency(cost_measure, result_measure)` function
Cost per result. `sum.cost / sum.result`.

```yaml
binding: "spend_efficiency(Spend, Conversions)"  # → 45.23 (cost per conversion)
binding: "spend_efficiency(Spend, Clicks)"        # → 2.15 (cost per click)
```

Only useful when Spend is available. Returns `Val::Null` if result is 0.

---

## Context variables to add (in context.rs)

These are pre-computed values inserted into the evaluation context alongside `current.`, `prev.`, `sum.`, `campaign_avg.`:

| Variable | Value | Example |
|---|---|---|
| `first.<Measure>` | Earliest period value | `first.CTR` → 0.85 |
| `last.<Measure>` | Latest period value (= current) | `last.CTR` → 2.25 |
| `first.period_name` | Name of earliest period | "Jan 2026" |
| `last.period_name` | Name of latest period | "May 2026" |
| `min.<Measure>` | Minimum value across all periods | `min.CTR` → 0.07 |
| `max.<Measure>` | Maximum value across all periods | `max.CTR` → 2.25 |
| `min.<Measure>.period` | Period name of minimum value | "March 2026" |
| `max.<Measure>.period` | Period name of maximum value | "May 2026" |
| `days_in_campaign` | Calendar days from first to last period | 150 |
| `total_periods` | Alias for period_count (clearer) | 5 |

---

## What this enables (example templates after expansion)

**Before (current, with N/A workarounds):**
```yaml
# Can't do this — concat and format don't exist:
template: "**{period_count} months** — **{impressions:,.0f}** impressions{note}."
bindings:
  note: "if(sum.Conversions > 0, concat(', **', format(sum.Conversions, ','), '** conversions'), '')"
  # ↑ produces N/A

# Workaround: split into two templates, duplicate logic
```

**After (with new functions):**
```yaml
template: >
  **{period_count} {period_word}** of campaign data — **{impressions:,.0f}** impressions,
  **{clicks:,.0f}** clicks{conversion_note}. CTR has been **{trend}** from
  **{first_ctr:.2f}%** to **{last_ctr:.2f}%**. Peak performance was **{best_period}**
  at **{max_ctr:.2f}%** CTR.
bindings:
  period_word: "plural(period_count, 'month', 'months')"
  impressions: "sum.Impressions"
  clicks: "sum.Clicks"
  conversion_note: >
    if(sum.Conversions > 0,
      concat(', generating **', format(sum.Conversions, ','), '** conversions'),
      '')
  trend: "trend_direction(CTR)"
  first_ctr: "first.CTR"
  last_ctr: "last.CTR"
  best_period: "best_period(CTR)"
  max_ctr: "max.CTR"
```

**Output:** "**5 months** of campaign data — **441,263** impressions, **4,412** clicks, generating **233** conversions. CTR has been **steadily increasing** from **0.85%** to **2.25%**. Peak performance was **May 2026** at **2.25%** CTR."

That's Claude-tier narrative quality from deterministic templates.

---

## Implementation plan

### Part 1: Context variables (context.rs)
Add `first.`, `last.`, `min.`, `max.`, `min.*.period`, `max.*.period`, `days_in_campaign` to the context builder. ~30 lines of code alongside the existing `current.` and `prev.` insertion.

### Part 2: Core functions (evaluator.rs)
Add to the `strip_func` chain:
- `concat(...)` — variadic string join
- `format(num, spec)` — number formatting
- `round(val, n)` — numeric rounding
- `pct_change(a, b)` — percentage change
- `plural(n, sing, plur)` — pluralization
- `coalesce(a, b, ...)` — first non-null

Each is ~10-20 lines following the existing pattern.

### Part 3: Analysis functions (evaluator.rs)
- `trend_direction(measure)` — multi-period trend classifier (~40 lines)
- `best_period(measure)` / `worst_period(measure)` — period name lookup (~15 lines each)
- `streak(measure, direction)` — consecutive period counter (~20 lines)
- `volatility(measure)` — coefficient of variation (~15 lines)

### Part 4: Tests
- Unit tests for each new function (in evaluator.rs test module)
- Integration tests with real template YAML files
- Verify existing templates still produce identical output

### Part 5: Upgrade templates
Rewrite `display-like.yaml` templates to use the new functions. Eliminate the template-splitting workarounds. Richer, more natural output.

---

## What this does NOT change

- No changes to `mc-core`
- No changes to the template YAML schema (same `when:`, `bindings:`, `template:` fields)
- No new crates or dependencies
- No API surface changes
- Existing templates that don't use new functions work identically

---

## Priority order for the demo

If time is short, ship in this order:

1. **`first.` / `last.` / `min.` / `max.` context vars** — fixes the "CTR moved from N/A% to X%" issue immediately
2. **`concat()` + `format()`** — fixes conditional string building
3. **`pct_change()` + `round()` + `plural()`** — cleaner template authoring
4. **`trend_direction()` + `best_period()` + `streak()`** — the "wow" factor
5. **`volatility()` + `days_in_campaign` + `coalesce()`** — polish

Items 1-3 are the must-haves. Items 4-5 are what make it Claude-tier.

---

---

## Amendments from Claude Desktop review (2026-05-11)

### Amendment 1: `trend_direction()` → split into composable functions

The single `trend_direction()` function with string return values is too vague for deterministic evaluation. Two implementations could classify the same data differently.

**Replace with three composable functions:**

- `trend_slope(measure)` → returns `'up'`, `'down'`, or `'flat'` based on overall direction (first vs last value, with ±5% dead zone for flat)
- `trend_consistency(measure)` → returns `'steady'`, `'variable'`, or `'volatile'` based on coefficient of variation (CV < 0.15 = steady, 0.15–0.50 = variable, > 0.50 = volatile)
- `streak(measure)` → returns signed integer (positive = consecutive up periods, negative = consecutive down, zero = flat/mixed). NOT stringly-typed.

**Explicit classification thresholds:**
```
trend_slope:
  total_change_pct = (last - first) / first * 100
  if abs(total_change_pct) < 5: return "flat"
  if total_change_pct > 0: return "up"
  return "down"

trend_consistency:
  cv = stddev(values) / mean(values)
  if cv < 0.15: return "steady"
  if cv < 0.50: return "variable"  
  return "volatile"

streak:
  Walk from latest period backwards
  Count consecutive same-direction changes
  Return positive for up streak, negative for down streak
  Minimum 2 periods of data; return 0 if < 2
```

**Templates compose:** `"CTR has been **{trend_slope(CTR)}** and **{trend_consistency(CTR)}**"` → "CTR has been **up** and **steady**."

### Amendment 2: `format()` spec grammar (complete)

```
Format spec grammar:
  [SIGN][SEPARATOR][.PRECISION][TYPE]

Components (all optional):
  SIGN:       '+' = always show sign for positive numbers
  SEPARATOR:  ',' = thousands separator (US locale, always comma — no locale switching)
  PRECISION:  '.N' = N decimal places (round, don't truncate)
  TYPE:       '%' = multiply by 100, append '%' suffix
              'f' = fixed-point (default when omitted)

Examples:
  '.2'    → 1234.567  → "1234.57"
  ',.2'   → 1234.567  → "1,234.57"
  '+,.1'  → 1234.567  → "+1,234.6"
  '+,.1'  → -123.45   → "-123.5"
  '.2%'   → 0.0146    → "1.46%"
  ','     → 441263     → "441,263"
  '.0'    → 233.7     → "234"

Special values:
  NaN  → "—" (em-dash, not "NaN")
  +Inf → "∞"
  -Inf → "−∞"
  Null → "" (empty string)

Errors:
  Invalid spec   → MC8001
  Non-numeric input → MC8002
```

US locale always. No scientific notation. No accounting-style negative formatting (parentheses). Percentage type multiplies by 100.

### Amendment 3: Null/coercion semantics (binding for all new functions)

```
Null semantics:
  Val::Null is the only null. Zero (0.0) is NOT null. Empty string is NOT null.

  When a binding references a nonexistent variable (e.g., campaign_avg.NonExistent):
    → evaluates to Val::Null (NOT a runtime error)

  concat() with null arguments: skip null args silently
    concat("a", null, "b") → "ab"

  concat() with numeric arguments: auto-coerce to string (no separator, full precision)
    concat("Total: ", 441263) → "Total: 441263"
    For formatted output, use format(): concat("Total: ", format(441263, ','))

  coalesce() semantics:
    Evaluate args left to right
    Return first non-Null value
    Zero is NOT null (coalesce(0, 5) → 0)
    If arg fails to evaluate → treat as Null, continue (NOT a runtime error)
    If all args null → return Null

  pct_change() with zero denominator → Null (not error, not Infinity)
  ratio() with zero denominator → Null (not error, not Infinity)
```

### Amendment 4: Replace `spend_efficiency()` with `ratio()`

`spend_efficiency(cost, result)` is too specialized — it's just division with a domain-specific name. Replace with:

```yaml
ratio(numerator, denominator)
  Returns numerator / denominator
  If denominator is 0 or Null → returns Null
  Handles divide-by-zero cleanly without producing NaN or Infinity
```

Templates write `ratio(sum.Spend, sum.Conversions)` instead of `spend_efficiency(Spend, Conversions)`. More reusable, less vocabulary to learn.

### Amendment 5: `best_period()` / `worst_period()` tie-breaking

On ties (two periods with identical max/min value), return the **earliest** period. This is deterministic and documented.

### Amendment 6: `days_in_campaign` uses ADR-0014 time parser

Don't roll a custom date parser. Use the same time format parsing path as ADR-0014. If the workspace declares `time_format`, use it. If period names can't be parsed as dates, return Null.

### Amendment 7: Diagnostic codes (pre-allocated)

| Code | Fires when |
|---|---|
| MC8001 | Invalid format spec in `format()` |
| MC8002 | Type error (e.g., non-numeric input to `format()`) |
| MC8003 | Division by zero in `pct_change()` or `ratio()` (returns Null, logs warning) |
| MC8004 | Insufficient data for trend functions (< 3 periods) |

### Amendment 8: Non-goals (explicit)

NOT being added in this phase:
- Locale-aware formatting beyond US conventions
- Scientific notation
- Regex matching or string manipulation beyond concat
- Date arithmetic beyond `days_in_campaign`
- Custom user-defined functions
- Template-level macros or includes

---

## Cross-links

- **Evaluator:** `crates/mc-narrative/src/evaluator.rs` (function dispatch at line ~858)
- **Context builder:** `crates/mc-narrative/src/context.rs` (variable insertion at line ~80)
- **Templates:** `demo/narratives/display-like.yaml` (consumer of these functions)
- **Phase 7A.1:** Original narrative engine (established the evaluator pattern)
- **ADR-0020:** Phase 7A planning document
- **ADR-0014:** Time representation (for `days_in_campaign` date parsing)
