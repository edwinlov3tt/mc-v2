---
description: "Run `mc model inspect` on a Mosaic YAML file via MCP. Renders the model summary — dim counts (leaves vs consolidations), measures (Input vs Derived counts + aggregations), rules with bodies, hierarchies, golden test count, canonical inputs row count, and any open diagnostics."
---

# /mosaic-inspect — Inspect a Mosaic YAML model

Run `mosaic.model.inspect` via the MCP server. The output is a structured model summary suitable for review or sharing.

## Arguments

- **`[path]`** (optional) — the YAML file to inspect. Defaults to the open file or prompts.

## What this command does

1. **Resolve the path.**
2. **Invoke `mosaic.model.inspect <path>` via MCP.**
3. **Render the summary.** The text format (default) is:

```
Model: Acme_MarketingFinance (format v1)
  Description: Brief §4 reference cube — 6 dims × 11 measures × 5 rules × 2520 input cells
  Author: MarketingCubes V2
  Created: 2026-05-02

Dimensions: 6
  - Scenario (Scenario) — 3 elements (3 leaves, 0 consolidated)
  - Version (Version) — 3 elements (3 leaves, 0 consolidated)
  - Time (Standard) — 17 elements (12 leaves, 5 consolidated; default hierarchy 'Calendar' with 16 edges, depth 2)
  - Channel (Standard) — 8 elements (5 leaves, 3 consolidated; default hierarchy 'Grouping' with 7 edges, depth 2)
  - Market (Standard) — 15 elements (7 leaves, 8 consolidated; default hierarchy 'Geographic' with 14 edges, depth 3)
  - Measure (Measure) — 11 elements (11 leaves, 0 consolidated)

Measures: 11
  Input (6): Spend, CPC, CVR, Close_Rate, AOV, COGS_Rate
  Derived (5): Clicks, Leads, Customers, Revenue, Gross_Profit
  Aggregations: Sum (6), WeightedAverage (5)

Rules: 5
  - rule_clicks: Clicks = Spend / CPC
  - rule_leads: Leads = Clicks * CVR
  - rule_customers: Customers = Leads * Close_Rate
  - rule_revenue: Revenue = Customers * AOV
  - rule_gross_profit: Gross_Profit = Revenue * (1 - COGS_Rate)
  Longest rule chain depth: 5

Cardinality (Cartesian product across all dim elements): 201960
Golden tests: 9
Canonical inputs: 2520 cells from acme.inputs.csv
Test fixtures: (none declared)
Diagnostics: 0 errors, 0 warnings, 0 info
```

JSON format is also available via `--format json` for tooling integration.

## What inspect tells you

- **Cardinality** — full Cartesian product of all dim sizes. Acme is ~200K; large numbers are normal for this kind of model. Memory cost is per *populated* cell, not per Cartesian cell.
- **Rule chain depth** — the longest dependency chain among rules. Acme's chain (Spend → Clicks → Leads → Customers → Revenue → Gross_Profit) is depth 5. Longer chains compound floating-point rounding and rule-evaluation cost.
- **Diagnostics line** — Inspect runs lint internally (advisory) so you see warning counts. If non-zero, cross-reference with `/mosaic-lint` for details.
- **Aggregation counts** — verifies that ratios use WeightedAverage (5 in Acme). If you have `WeightedAverage: 0` in a marketing-mix model, you probably have ratios mis-aggregating as Sum (the canonical mistake — see `skills/schema-design/SKILL.md` aggregation section).

## Skills referenced

- `skills/authoring/SKILL.md` — the model layer's structural concepts (dims, hierarchies, measures, rules).
- `skills/domain-schemas/marketing-mix/SKILL.md` — how Acme's inspect output reads.

## Underlying CLI

```
mc model inspect <path> [--format text|json]
```

Default is text; pass `--format json` for the structured shape.

## What this command does NOT do

- **Does not run validate / lint / test** — use the dedicated commands.
- **Does not edit YAML.**
- **Does not show actual cell values** — inspect is metadata only. To read computed values, run `/mosaic-test` (which executes goldens) or use `mc demo --model <path>` for the Acme demo's sample reads.
