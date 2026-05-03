---
name: mosaic-marketing-mix
description: The marketing-mix domain pattern for Mosaic — Acme as the canonical reference (6 dims × 11 measures × 5 rules × 2,520 input cells). Use when designing a marketing-mix model, deciding which measures are inputs vs derived, choosing the right channel/market hierarchy shape, or asking "what does a typical marketing-mix model look like?". Documents Acme's structure verbatim so a new marketing-mix model can mirror it. The ONLY domain schema shipped in Phase 4A.
---

# Marketing-Mix Domain Schema

Marketing-mix planning is the canonical Mosaic use case. The Acme reference model — `examples/models/acme-marketing.yaml` — captures the typical shape: marketing dollars allocated across channels and markets over time produce clicks, leads, customers, revenue, and gross profit through a deterministic ratio chain.

**This is the only domain schema Phase 4A ships** (per ADR-0008 amendment F). Any new marketing-mix model should mirror Acme's structure unless the user has a specific reason to differ.

## The Acme reference at a glance

| | Count |
|---|---:|
| Dimensions | 6 (canonical order) |
| Hierarchies | 3 (Time, Channel, Market) |
| Measures | 11 (6 Input, 5 Derived) |
| Rules | 5 (depth-5 chain) |
| Canonical input cells | 2,520 (1 × 1 × 12 × 5 × 7 × 6) |
| Cube cardinality | 201,960 (full Cartesian) |
| Goldens | 9 |

## Dimensions

Six dims in canonical order. The Scenario / Version / Measure dims have specialized `kind:`; Time / Channel / Market are `Standard`.

### Scenario (3 elements)

```yaml
- { name: "Baseline",      scenario_meta: "Default" }
- { name: "Aggressive",    scenario_meta: "NonDefault" }
- { name: "Conservative",  scenario_meta: "NonDefault" }
```

The marketing-mix pattern: one default scenario plus alternates for what-if planning ("what if we shift 30% of spend to social?"). The kernel doesn't enforce a one-default rule — that's enforced at the application layer.

### Version (3 elements)

```yaml
- { name: "Working",   version_state: "Draft" }
- { name: "Submitted", version_state: "Submitted" }
- { name: "Approved",  version_state: "Approved" }
```

Workflow states. The version dim lets the planner branch a draft, submit it for review, and lock an approved copy without losing the draft.

### Time (17 elements, 12 leaves + 5 consolidations)

```yaml
elements:
  - { name: "Jan_2026" } - { name: "Feb_2026" } - { name: "Mar_2026" }
  - { name: "Apr_2026" } - { name: "May_2026" } - { name: "Jun_2026" }
  - { name: "Jul_2026" } - { name: "Aug_2026" } - { name: "Sep_2026" }
  - { name: "Oct_2026" } - { name: "Nov_2026" } - { name: "Dec_2026" }
  - { name: "Q1_2026" }  - { name: "Q2_2026" }  - { name: "Q3_2026" }  - { name: "Q4_2026" }
  - { name: "FY_2026" }
```

Hierarchy: months → quarters → year (depth 2). Adapt to your planning horizon — semesters / weeks / fiscal-quarters / etc. — but keep the leaf grain consistent.

### Channel (8 elements, 5 leaves + 3 consolidations)

```yaml
elements:
  - { name: "Paid_Search" } - { name: "Paid_Social" } - { name: "Display" }
  - { name: "Email" }       - { name: "Organic" }
  - { name: "Paid_Media" }  - { name: "Owned_Earned" }
  - { name: "All_Channels" }
```

Hierarchy: channels → channel families (Paid_Media, Owned_Earned) → All_Channels.

### Market (15 elements, 7 leaves + 8 consolidations)

```yaml
elements:
  # Cities (leaves)
  - { name: "Tampa" } - { name: "Orlando" } - { name: "Miami" }
  - { name: "Atlanta" } - { name: "Charlotte" }
  - { name: "New_York_City" } - { name: "Boston" }
  # States
  - { name: "Florida" } - { name: "Georgia" } - { name: "North_Carolina" }
  - { name: "New_York_State" } - { name: "Massachusetts" }
  # Regions
  - { name: "Southeast" } - { name: "Northeast" }
  # Country
  - { name: "USA" }
```

Hierarchy: cities → states → regions → USA (depth 3).

### Measure (11 elements, all leaves)

The Measure dim's elements come from the top-level `measures:` block; declare it with `kind: "Measure"` and `elements: []`.

## Measures (11 total: 6 Input, 5 Derived)

### Inputs (6 — populated via canonical_inputs)

| Measure | Aggregation | Weight | Note |
|---|---|---|---|
| `Spend` | `Sum` | — | Marketing dollars allocated (USD). |
| `CPC` | `WeightedAverage` | `Spend` | Cost per click (USD/click). |
| `CVR` | `WeightedAverage` | `Clicks` | Click-to-lead conversion rate. |
| `Close_Rate` | `WeightedAverage` | `Leads` | Lead-to-customer close rate. |
| `AOV` | `WeightedAverage` | `Customers` | Average order value (USD/customer). |
| `COGS_Rate` | `WeightedAverage` | `Revenue` | Cost-of-goods-sold rate (fraction). |

**Notice the WeightedAverage pattern:** every ratio measure is weighted by the quantity that drives it. CPC weighted by Spend (we spend dollars). CVR by Clicks (clicks become leads). Close_Rate by Leads. AOV by Customers. COGS_Rate by Revenue. The weight measure is always a `Sum`-aggregated quantity that the ratio "rides on." See `skills/schema-design/SKILL.md` aggregation section.

### Derived (5 — computed via rules)

All `Sum`-aggregated:

| Measure | Aggregation | Computed by |
|---|---|---|
| `Clicks` | `Sum` | rule_clicks |
| `Leads` | `Sum` | rule_leads |
| `Customers` | `Sum` | rule_customers |
| `Revenue` | `Sum` | rule_revenue |
| `Gross_Profit` | `Sum` | rule_gross_profit |

## Rules (5 — chain depth 5)

The rule chain models marketing's attribution funnel. Each rule fires at AllLeaves (every leaf coord across the 5 non-Measure dims):

```yaml
- name: "rule_clicks"
  description: "Clicks = Spend / CPC — translate marketing dollars into click volume."
  target_measure: "Clicks"
  scope: "AllLeaves"
  body: "Spend / CPC"
  declared_dependencies: ["Spend", "CPC"]

- name: "rule_leads"
  description: "Leads = Clicks * CVR — apply the click-to-lead conversion rate."
  target_measure: "Leads"
  scope: "AllLeaves"
  body: "Clicks * CVR"
  declared_dependencies: ["Clicks", "CVR"]

- name: "rule_customers"
  description: "Customers = Leads * Close_Rate — apply the lead-to-customer close rate."
  target_measure: "Customers"
  scope: "AllLeaves"
  body: "Leads * Close_Rate"
  declared_dependencies: ["Leads", "Close_Rate"]

- name: "rule_revenue"
  description: "Revenue = Customers * AOV — top-line revenue."
  target_measure: "Revenue"
  scope: "AllLeaves"
  body: "Customers * AOV"
  declared_dependencies: ["Customers", "AOV"]

- name: "rule_gross_profit"
  description: "Gross_Profit = Revenue * (1 - COGS_Rate) — revenue net of cost of goods sold."
  target_measure: "Gross_Profit"
  scope: "AllLeaves"
  body: "Revenue * (1 - COGS_Rate)"
  declared_dependencies: ["Revenue", "COGS_Rate"]
```

## Hierarchies

Acme has three default hierarchies:

- **Time / Calendar:** months → quarters → FY_2026 (16 edges).
- **Channel / Grouping:** channels → channel families → All_Channels (7 edges).
- **Market / Geographic:** cities → states → regions → USA (14 edges).

All edges have `weight: 1.0`. Adjust weights only if your rollup has a non-uniform contribution (e.g., sales territories splitting a city across two regions).

## Canonical inputs

Acme's canonical_inputs sources from a sibling CSV (2,520 rows, 1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures):

```yaml
canonical_inputs:
  source: "acme.inputs.csv"
  columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
```

The CSV is generated from a deterministic formula (see `mc_fixtures::canonical_inputs_for(time_idx, channel_idx, market_idx)` for the math); inspecting individual values:

```
Mar_2026 / Paid_Search / Tampa (t=3, c=0, m=0):
  Spend = 10_000 + 500*3 = 11_500
  CPC = 1.50
  CVR = 0.020
  Close_Rate = 0.10
  AOV = 200.0
  COGS_Rate = 0.30
```

Through the rule chain:

```
Clicks       = 11_500 / 1.50          = 7_666.667
Leads        = 7_666.667 * 0.020      = 153.333
Customers    = 153.333 * 0.10         = 15.333
Revenue      = 15.333 * 200.0         = 3_066.667
Gross_Profit = 3_066.667 * (1 - 0.30) = 2_146.667
```

These are the values the Acme golden suite anchors on.

## Variations to expect

A new marketing-mix model often differs from Acme along one or two axes:

### Different time grain

Replace `Jan_2026..Dec_2026` (months) with `Wk1..Wk52` (weeks) or `2025-Q1..2030-Q4` (quarters across multiple years). Hierarchy edges adjust accordingly.

### Different channel mix

Replace Paid_Search / Paid_Social / Display / Email / Organic with whatever channels matter (e.g., Affiliate / Influencer / Podcast / Connected_TV). Keep the family-level rollup (Paid vs Owned/Earned, or whatever family structure makes sense).

### Different market mix

The geographic hierarchy is the most variable. B2C might use cities → states → regions → country (Acme's shape); B2B might use accounts → segments → industries; Telco might use store-locations → DMA → state.

### Multiple scenarios

Acme declares Baseline / Aggressive / Conservative but only loads inputs for Baseline. A model that exercises multiple scenarios populates each with its own data set (typically via a fixture per scenario).

### Additional derived measures

Common extensions beyond Acme's 5-rule chain:

- **CAC (Cost of Acquisition):** `Spend / Customers`. WeightedAverage by Customers.
- **LTV (Lifetime Value):** `Revenue * Retention_Months / Customers`. Often modeled as input.
- **ROAS:** `Revenue / Spend`. WeightedAverage by Spend.
- **Profit_Margin:** `Gross_Profit / Revenue`. WeightedAverage by Revenue.

Each addition needs a measure declaration + (if derived) a rule + (if WeightedAverage) a `weight_measure:`.

## Building a new marketing-mix model

1. **Start by copying `examples/models/acme-marketing.yaml`** to your new file.
2. **Edit the metadata block** — change `name`, `description`, `created`.
3. **Adapt the dimensions** — change Time grain, Channel mix, Market mix to fit. **Keep the dim order** (`[Scenario, Version, Time, Channel, Market, Measure]`).
4. **Adapt the hierarchies** — match your dim shape. Edges need to be DAGs; weights in `[0.0, 1.0]`.
5. **Adapt the measures** — keep Spend + the 5 Acme ratios as a baseline, add domain-specific measures.
6. **Adapt the rules** — extend the chain or add parallel rules per added measure.
7. **Generate canonical_inputs** — either inline tabular for a small fixture, or sibling CSV for production data.
8. **Write goldens** — at minimum: 1 input anchor, 1 end-of-chain derived anchor, 1 consolidation rollup. Mirror Acme's pattern.
9. **Run the loop:** validate → fix → lint (zero warnings target) → test (goldens green).

## Anti-patterns (DON'T)

- **Don't omit hierarchies** — the kernel still works without them, but the planner can't see rollups (no Q1, no FY, no Florida totals). Almost all marketing-mix models have at least Time + Geographic hierarchies.
- **Don't make every measure `Sum`** — ratios MUST be `WeightedAverage` with the right `weight_measure`. CPC, CVR, AOV, etc. — every ratio gets a weight (see schema-design).
- **Don't skip Spend** — it's the foundation. Even if your domain calls it "Investment" or "Budget", model it as a `Sum`-aggregated input measure named appropriately.
- **Don't rename Acme rule names without updating goldens.** If you change `rule_clicks` to `rule_leads_calculation`, update everywhere — the rules registry tracks rules by name.
- **Don't fork the example file in place.** Copy it, edit the copy, commit the copy with a unique name. The plugin's `examples/models/acme-marketing.yaml` is a reference; users should keep it byte-identical to the source.
