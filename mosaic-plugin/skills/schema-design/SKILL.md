---
name: mosaic-schema-design
description: How to design a Mosaic cube schema — the canonical 6-dimension order [Scenario, Version, Time, Channel, Market, Measure] (binding), single-default-hierarchy-per-dim cap, MeasureRole {Input, Derived} (no Both in Phase 1), aggregation rules (Sum vs WeightedAverage with required weight_measure), rule-body well-typed-ness, and AllLeaves scope. Use whenever the user is designing a model from scratch, picking dimensions, classifying measures as input vs derived, choosing an aggregation method, or asking why a rule fires MC2002 / MC2007 / MC2011.
---

# Schema Design

A Mosaic cube has four structural pieces: **dimensions**, **hierarchies**, **measures**, **rules**. This skill teaches the design decisions and constraints that bind each.

## Rule 1: the dimension order is non-negotiable

Every Mosaic cube has its dims in this exact order:

```
[Scenario, Version, Time, Channel, Market, Measure]
```

**Why this is binding:** the kernel's `CellCoordinate` is positional against `cube.dimensions`. Element IDs at slot 0 are interpreted as Scenario IDs, slot 1 as Version IDs, etc. Reordering the dim list reorders the storage contract; any cube built from a reordered YAML produces wrong cells (or kernel errors at the next coordinate validation point). This is brief §3 + ADR-0001; the lint validates dim order with code MC2002.

**What if my domain doesn't have a real Channel?** Declare a dim with one element `All_Channels` (or whatever fits). Don't omit the dim — the dim count must be exactly 6.

```yaml
# Domain: pure financial planning, no channels
dimensions:
  - { name: "Scenario", kind: "Scenario", elements: [...] }
  - { name: "Version",  kind: "Version",  elements: [...] }
  - { name: "Time",     kind: "Standard", elements: [...] }
  - { name: "Channel",  kind: "Standard", elements: [{ name: "All_Channels" }] }
  - { name: "Market",   kind: "Standard", elements: [...] }
  - { name: "Measure",  kind: "Measure",  elements: [] }
```

The kernel doesn't care that Channel has one element; the dim still exists.

## Rule 2: each dim has at most one default hierarchy in Phase 1

A *hierarchy* is a tree of parent → child rollup edges over a dim's elements. The kernel auto-consolidates from leaves up to consolidated elements when you read a non-leaf coord.

```yaml
hierarchies:
  - dimension: "Time"
    name: "Calendar"
    default: true
    edges:
      - { parent: "Q1_2026", child: "Jan_2026", weight: 1.0 }
      - { parent: "Q1_2026", child: "Feb_2026", weight: 1.0 }
      - { parent: "Q1_2026", child: "Mar_2026", weight: 1.0 }
      - { parent: "FY_2026", child: "Q1_2026", weight: 1.0 }
      # ...
```

**Constraints:**

- Edge weights are F64 in `[0.0, 1.0]`. Outside that range fires MC2003.
- Edges form a DAG, no cycles. Cycles fire MC2004.
- An edge with `weight: 0.0` means "ignored at consolidation" — fires lint MC3007 advising removal.
- Phase 1 supports multiple hierarchies on a dim BUT the spec narrows Phase 1 to one default per dim. Future versions may add named alternates.

A consolidated element is **non-writable**: writeback against `Q1_2026 Spend` fails. Inputs only flow into leaves; rollups are computed.

## Rule 3: MeasureRole is `Input` OR `Derived` — there is no `Both`

```yaml
- name: "Spend"
  role: "Input"           # populated via canonical_inputs / fixtures / writebacks; no rule
  data_type: "F64"
  aggregation: "Sum"

- name: "Clicks"
  role: "Derived"         # value comes from a rule; never written directly
  data_type: "F64"
  aggregation: "Sum"
```

**The constraint:** Phase 1 supports `Input` and `Derived`. The brief change-log explicitly excludes `Both` (a "this is sometimes input, sometimes derived" hybrid). If your domain wants something like that, model it as **two separate measures** plus a rule that picks between them — never a single `Both`-role measure.

```yaml
# WRONG (would fire MC2007 if attempted):
- name: "Forecast_Or_Actual"
  role: "Both"            # not supported

# RIGHT — model as two measures + a rule:
- name: "Forecast"
  role: "Input"
  aggregation: "Sum"
- name: "Actual"
  role: "Input"
  aggregation: "Sum"
- name: "Best_Estimate"
  role: "Derived"
  aggregation: "Sum"
- name: "rule_best_estimate"
  target_measure: "Best_Estimate"
  body: "if_null(Actual, Forecast)"          # if Actual is present, use it; else use Forecast
  declared_dependencies: ["Actual", "Forecast"]
```

`if_null` is the only conditional Phase 3D supports; for richer logic, restructure the model so the case distinction happens in the input pipeline.

## Rule 4: pick the right aggregation rule per measure

This is the **single most common LLM mistake**: defaulting to `Sum` for everything. CPC, CVR, Close_Rate, AOV, COGS_Rate, and most ratio measures consolidate as **weighted averages**, NOT simple sums.

| Aggregation | When to use | Example |
|---|---|---|
| `Sum` | Quantities (USD, counts, units) that add up across rollup children | Spend, Clicks, Leads, Revenue, Gross_Profit |
| `WeightedAverage` | Ratios, rates, prices that need a quantity to weight against | CPC (weighted by Spend), CVR (by Clicks), Close_Rate (by Leads), AOV (by Customers), COGS_Rate (by Revenue) |
| `Min` | "Best across" (e.g., earliest time, lowest cost) | Lead_Time_Days, Min_Latency |
| `Max` | "Worst across" (e.g., latest time, highest cost) | Peak_Demand, Max_Latency |

**WeightedAverage REQUIRES a `weight_measure` field.** Without it, validation fails with MC2011:

```yaml
# WRONG (fires MC2011):
- name: "CPC"
  aggregation: "WeightedAverage"

# RIGHT:
- name: "CPC"
  aggregation: "WeightedAverage"
  weight_measure: "Spend"
```

**The right weight measure is usually the quantity that "drives" the ratio.** CPC = Spend / Clicks at the leaf level, but at consolidation, the weighted-average roll-up uses Spend (not Clicks) because that's what we're spending. The pairings:

- **CPC** weighted by **Spend** (we spend, we get clicks; CPC is dollars per click weighted by dollars).
- **CVR** weighted by **Clicks** (clicks become leads; CVR is leads per click weighted by clicks).
- **Close_Rate** weighted by **Leads** (leads become customers).
- **AOV** weighted by **Customers** (customers buy; AOV is revenue per customer weighted by customers).
- **COGS_Rate** weighted by **Revenue** (revenue has cost; COGS_Rate is cost per revenue weighted by revenue).

The MC3006 lint rule flags suspicious-looking ratio-named measures that are still `aggregation: Sum`; if your model has `Conversion_Rate: Sum`, the lint catches it. Don't suppress — fix.

### The binding rule for DERIVED ratio measures

**If your rule body matches `A / B` (a division), the derived measure's aggregation MUST be `WeightedAverage` with `weight_measure` set to the DENOMINATOR (`B`).**

This is NOT optional. Getting it wrong produces silently catastrophic numbers — consolidation of a ratio via `Sum` adds leaf-level ratios together, which is mathematically meaningless. Example:

```
Houston Oct CAC = $22,015 / 442 = $49.81  (leaf)
Austin Oct CAC  = $6,000 / 30  = $200.00  (leaf)

WRONG (Sum aggregation):  $49.81 + $200.00 = $249.81  ← meaningless
RIGHT (WeightedAverage by Matched_New_Customers):
    ($22,015 + $6,000) / (442 + 30) = $28,015 / 472 = $59.35  ← correct blended CAC
```

**The algebra:** `WeightedAverage` of `A/B` weighted by `B` gives `SUM(A) / SUM(B)` at consolidation — which IS the correct aggregate ratio. This is why the weight measure is always the denominator.

**Quick reference for common patterns:**

| Rule body | Aggregation | weight_measure | Why |
|---|---|---|---|
| `Spend / Clicks` (CPC) | WeightedAverage | Clicks | SUM(Spend)/SUM(Clicks) = blended CPC |
| `Spend / New_Customers` (CAC) | WeightedAverage | New_Customers | SUM(Spend)/SUM(New) = blended CAC |
| `(Revenue - Spend) / Spend` (ROI) | WeightedAverage | Spend | correct weighted margin |
| `Revenue / Customers` (AOV) | WeightedAverage | Customers | SUM(Rev)/SUM(Cust) = blended AOV |
| `Opens / Sends` (Open_Rate) | WeightedAverage | Sends | SUM(Opens)/SUM(Sends) = blended rate |

**The heuristic: if the rule body contains `/`, use WeightedAverage. The weight is the denominator.** There are no exceptions for Mosaic models. MC3007 lint warns you if you get this wrong; `mc model test` golden assertions catch the numeric impact.

## Rule 5: rule bodies must be well-typed and have declared deps

A rule has six fields:

```yaml
- name: "rule_revenue"
  description: "Revenue = Customers * AOV — top-line revenue."
  target_measure: "Revenue"        # required; must be a Derived measure
  scope: "AllLeaves"               # only Phase 1 scope
  body: "Customers * AOV"          # formula or structured tree
  declared_dependencies: ["Customers", "AOV"]   # MUST list every measure read by body
```

**Constraints (each is a separate MC2xxx code if violated):**

- `target_measure` must exist (MC2005) and be `role: Derived` (MC2007 fires on Input).
- `target_measure` must have at most one rule (MC2006 fires on a Derived measure with no rule).
- `scope: "AllLeaves"` is the only legal value in Phase 1.
- `body` is either a formula string (Phase 3D) or a structured tree (Phase 3A). Both forms are accepted indefinitely.
- `declared_dependencies` MUST list every measure name read by `body`. Missing a dep doesn't fire MC2xxx — it fires `EngineError::UndeclaredDependency` at runtime, which surfaces as MC0002 during `mc model test`. **List them.**
- The rule graph (rule_X depends on rule_Y depends on rule_X) must be acyclic. Cycles fire MC2008.

For body grammar see `skills/formulas/SKILL.md`.

## Rule 6: name everything consistently

The lint rule MC3001 catches inconsistent naming within a dim. Pick a style and stick with it:

```yaml
# CONSISTENT (snake_case throughout — Acme convention):
elements:
  - { name: "Paid_Search" }
  - { name: "Paid_Social" }
  - { name: "Display" }
  - { name: "Email" }

# INCONSISTENT (mixes styles — fires MC3001):
elements:
  - { name: "PaidSearch" }       # PascalCase
  - { name: "paid_social" }      # lower_snake
  - { name: "Display" }
  - { name: "Email" }
```

The Acme convention is `Title_Case_With_Underscores` for elements, `snake_case` for rule names (`rule_clicks`, `rule_revenue`), and `Title_Case` for measure names. Mosaic doesn't enforce one specific style — it enforces *consistency within each dim*.

## Common design patterns

### Pattern: mostly-flat dim with a few rollup tiers

The Time dim in Acme:
- 12 leaf months (`Jan_2026` … `Dec_2026`)
- 4 quarter consolidations (`Q1_2026` … `Q4_2026`)
- 1 year (`FY_2026`)

Hierarchy edges: months → quarters → year. Rollup depth = 2.

### Pattern: parent-child rollup tree

The Market dim in Acme:
- 7 leaf cities (Tampa, Orlando, Miami, Atlanta, Charlotte, NYC, Boston)
- 4 states (Florida, Georgia, NC, NY State, Massachusetts)
- 2 regions (Southeast, Northeast)
- 1 country (USA)

Edges: cities → states → regions → USA. Rollup depth = 3.

### Pattern: ratio + driver pair

Whenever you have a ratio, add the driver:

```yaml
- name: "CTR"                        # ratio: clicks per impression
  role: "Input"
  aggregation: "WeightedAverage"
  weight_measure: "Impressions"

- name: "Impressions"                # driver
  role: "Input"
  aggregation: "Sum"
```

Without the driver as a measure, you can't weight the ratio's rollup; the consolidation produces meaningless averages.

### Pattern: derived measure chain

The Acme rule chain has depth 5: `Spend → Clicks → Leads → Customers → Revenue → Gross_Profit`. Each step:

```yaml
- name: "rule_clicks"
  body: "Spend / CPC"
  declared_dependencies: ["Spend", "CPC"]
- name: "rule_leads"
  body: "Clicks * CVR"
  declared_dependencies: ["Clicks", "CVR"]
# ... etc.
```

The kernel handles the chain via on-demand evaluation — when a user reads `Gross_Profit`, the rules fire bottom-up. You don't write the chain explicitly; the kernel does.

## Anti-patterns (DON'T)

- **Don't reorder dimensions.** Even if your domain "naturally" fits a different order, Mosaic's storage contract is positional. Use `[Scenario, Version, Time, Channel, Market, Measure]` always.
- **Don't omit a dim because it's "just one element."** Include it with one element. The kernel still works; the cube cardinality stays consistent.
- **Don't use `Sum` for ratio measures.** `WeightedAverage` is the right choice; the LLM keeps making this mistake. CPC, CVR, AOV, etc. — every ratio gets a weight.
- **Don't try to use `MeasureRole::Both`.** Phase 1 doesn't have it. Use two measures + a rule.
- **Don't write rules without declared_dependencies.** Phase 1 enforces dep declarations at compile time.
- **Don't put consolidation logic in rules.** Rules compute leaf values; consolidation is automatic via the hierarchy. A rule with body `Spend_Q1 + Spend_Q2 + Spend_Q3 + Spend_Q4` is wrong — model the time hierarchy and let the kernel sum.
