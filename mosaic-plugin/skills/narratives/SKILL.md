---
name: mosaic-narratives
description: How to author, test, and extend YAML narrative templates for Mosaic's deterministic report engine. Covers the template schema (id, family, severity, when, template, bindings, format, deduplicate), expression syntax for predicates and bindings, format hints (currency, percent, count, delta_signed, etc.), aggregate functions (count_where, any_where, names_where, first_where), the severity ladder (info/success/warning/critical), and best practices for designing reusable narrative templates. Use when the user is writing narrative templates, debugging template output, or wants to know what expressions and format hints are available.
---

# Mosaic Narrative Templates — Authoring Guide

Mosaic's narrative engine evaluates YAML templates against populated cube data to produce structured report output. Templates are deterministic: same data always produces the same narrative. No LLM at runtime.

## Quick start

```yaml
narrative_format_version: 1

templates:
  - id: spend_mom
    family: [marketing]
    severity: info
    table_types: ["Monthly Performance"]
    when: "period_count >= 2"
    template: >
      {tactic_name} spend {direction} {abs_pct:.0f}% from
      {prev_period} to {current_period}.
    bindings:
      direction: "if(current.Spend >= prev.Spend, 'increased', 'decreased')"
      abs_pct: "abs((current.Spend - prev.Spend) / prev.Spend * 100)"
```

Save as `narratives/my-template.yaml` and run:

```bash
mc model narrate <model.yaml> --templates narratives/ --format json
```

---

## Template schema

Every template requires these fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique identifier. Used in output, evidence, dedup. |
| `family` | string[] | yes | Tactic families (e.g., `[display-like, search-like]`). |
| `severity` | enum | yes | One of: `info`, `success`, `warning`, `critical`. |
| `table_types` | string[] | yes | Which data tables this matches (substring match). |
| `when` | string | yes | Predicate expression. Template fires only when truthy. |
| `template` | string | yes | Output text with `{placeholder}` substitution. |

Optional fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bindings` | map | `{}` | Named expressions evaluated for substitution. |
| `sort_order` | int | `0` | Lower values fire first across all templates. |
| `deduplicate` | bool | `false` | If true, fires at most once per evaluation batch. |
| `format` | map | `{}` | Named format hints per binding (see below). |
| `notability_base` | float | none | Static notability score [0, 1]. |

---

## Severity ladder

Choose the severity that matches the TEMPLATE's intent, not the data:

| Severity | When to use | Example |
|----------|-------------|---------|
| `info` | Notable but not actionable | "CTR grew 3% month-over-month." |
| `success` | Positive outcome worth highlighting | "Hit industry benchmark for the first time." |
| `warning` | Action recommended | "Below benchmark by 15% — review targeting." |
| `critical` | Action required | "Zero conversions recorded — check pixel." |

A template's severity is fixed. Different conditions should use different templates with different severities, not one template that escalates.

---

## Expression syntax

`when:` predicates and `bindings:` values use the same expression language:

### Operators

- Arithmetic: `+`, `-`, `*`, `/`
- Comparison: `>`, `<`, `>=`, `<=`, `==`, `!=`
- Logical: `AND`, `OR`, `NOT` (also `not()` as function)
- Parentheses: `(a + b) * c`

### Functions

| Function | Example | Description |
|----------|---------|-------------|
| `abs(x)` | `abs(delta)` | Absolute value |
| `if(cond, then, else)` | `if(x > 0, 'up', 'down')` | Conditional |
| `not(x)` | `not(x > 0)` | Logical negation |
| `element_count(Dim)` | `element_count(City)` | Count elements in dimension |

### Aggregate functions

These iterate over dimension elements, evaluating predicates per-element:

| Function | Returns | Example |
|----------|---------|---------|
| `count_where(pred, dim)` | number | `count_where(Impressions < 500, City)` |
| `any_where(pred, dim)` | bool | `any_where(Clicks == 0 AND Impressions > 50, City)` |
| `names_where(pred, dim)` | string | `names_where(Impressions < 500, City)` |
| `first_where(pred, dim).name` | string | Name of first matching element |
| `first_where(pred, dim).Measure` | number | Measure value of first match |

The predicate can be ANY expression. Unlike Phase 6D's hardcoded conditions, these evaluate arbitrary predicates at runtime.

### Context variables

Pre-computed from cube data:

| Variable | Description |
|----------|-------------|
| `current.<Measure>` | Value at the latest time period |
| `prev.<Measure>` | Value at the previous time period |
| `sum.<Measure>` | Sum across all time periods |
| `campaign_avg.<Measure>` | Average across all time periods |
| `max_by.<Dim>.<Measure>.name` | Name of element with highest value |
| `max_by.<Dim>.<Measure>.value` | The highest value |
| `min_by.<Dim>.<Measure>.name` | Name of element with lowest value |
| `min_by.<Dim>.<Measure>.value` | The lowest value |
| `period_count` | Number of time periods |
| `tactic_name` | Subproduct / tactic name |
| `current_period` | Name of the current period |
| `prev_period` | Name of the previous period |

---

## Format hints

### Inline format specifiers

Used directly in the template string:

```yaml
template: "Impressions: {value:,.0f}, CTR: {ctr:.2f}%"
```

| Specifier | Result | Example |
|-----------|--------|---------|
| `:.0f` | Integer | `22` |
| `:.1f` | One decimal | `22.1` |
| `:.2f` | Two decimals | `22.13` |
| `:,.0f` | Integer with commas | `55,757` |
| `:+.0f` | With sign prefix | `+22` |

### Named format hints

Declared in the template's `format:` map. Override inline specs when present.

```yaml
bindings:
  spend: "current.Spend"
  clicks: "current.Clicks"
format:
  spend: "currency"
  clicks: "count"
template: "Spent {spend}, got {clicks} clicks."
# Output: "Spent $11,500, got 8,420 clicks."
```

| Hint | Output | Example |
|------|--------|---------|
| `currency` | Dollar-formatted with commas | `$11,500` |
| `percent_0` | Integer percent | `23%` |
| `percent_1` | One-decimal percent | `23.4%` |
| `percent_2` | Two-decimal percent | `23.41%` |
| `count` | Comma-separated integer | `8,420` |
| `count_short` | Abbreviated | `8.4K`, `1.2M` |
| `delta_signed` | With explicit sign | `+47`, `-312` |
| `decimal_2` | Two-decimal number | `0.42` |
| `date_short` | Pass-through (string) | `Mar 2026` |

---

## Binding resolution

Bindings resolve in **dependency order** (DAG topological sort). A binding can reference another binding:

```yaml
bindings:
  abs_pct: "abs((current.Clicks - prev.Clicks) / prev.Clicks * 100)"
  verb: >
    if(abs_pct > 100, 'more than doubled',
      if(abs_pct > 50, 'surged',
        if(current.Clicks >= prev.Clicks, 'increased', 'declined')))
```

Here `verb` references `abs_pct`. The engine resolves `abs_pct` first, then `verb` can use its value. Chains of any depth work. Cycles are detected (cyclic bindings evaluate to Null).

---

## Deduplication

Templates with `deduplicate: true` fire at most once per evaluation batch (across all cubes):

```yaml
- id: conversion_alarm
  deduplicate: true
  severity: critical
  when: "sum.Conversions == 0 AND sum.Impressions > 1000"
  template: "Zero conversions recorded across {total:,.0f} impressions."
```

Use for templates that apply globally (data sufficiency warnings, conversion tracking alerts).

---

## Best practices

1. **One template, one insight.** Don't pack multiple insights into one template. Split into separate templates with appropriate severities.

2. **Threshold-based `when:`.** Always include a meaningful predicate. `when: "true"` fires every time — only use for guaranteed-relevant templates like data sufficiency notes.

3. **Use `sort_order` for priority.** Data quality warnings (`sort_order: -10`) should fire before performance insights (`sort_order: 0`).

4. **Match `table_types` precisely.** Use substring matching: `"Monthly Performance"` matches both "Monthly Performance" and "Ad Group Monthly Performance".

5. **Binding chains for complex logic.** Compute intermediate values in bindings; keep the template string readable.

6. **Named format hints for consistency.** Use the `format:` map when the same binding should always render the same way. Use inline specs for one-off formatting.

7. **Test with `mc model narrate`.** Run `mc model narrate <model> --templates <dir> --format text` to preview output before committing.

---

## Template families

Templates are organized by tactic family. The `family` field tags which families a template applies to. Common families:

- `display-like` — Targeted Display, Addressable Display, Social Display, Native
- `video-like` — STV, OTT, Pre-Roll, Video
- `search-like` — Paid Search, Local Search, SEO
- `social-like` — Social Media, Social Advertising

A template can belong to multiple families. The engine does not currently filter by family (all templates are evaluated), but the field enables future per-family report sections.

---

## Extending with new templates

To add a new template:

1. Create or edit a `.yaml` file in the narratives directory.
2. Add the template definition with a unique `id`.
3. Test with `mc model narrate <model> --templates <dir>`.
4. No Rust code changes needed.

To add a new template family (e.g., for a new tactic type):

1. Add templates with the new family tag.
2. Use `table_types` that match the new data shape.
3. Context variables (`current.X`, `max_by.Dim.X.name`, etc.) work automatically for any measure and dimension names in the cube data.

---

## Diagnostic codes

The narrative engine uses `MC7xxx` codes:

| Code | Meaning |
|------|---------|
| MC7001 | Template references unknown measure |
| MC7002 | Template references unknown dimension |
| MC7003 | `when:` predicate has invalid syntax |
| MC7004 | Format hint references undefined formatter |
| MC7005 | Template body has unresolved `{placeholder}` |
| MC7006 | Invalid severity value |
| MC7007 | Unknown template family |
| MC7008 | Duplicate template ID |
| MC7009 | Undefined section reference |
| MC7010 | `notability_base` outside [0, 1] |

Run `mc_narrative::validate_templates()` to check templates at load time.
