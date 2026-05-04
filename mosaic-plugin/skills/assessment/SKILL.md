---
name: mosaic-assessment
description: How to assess a user's data sources and propose a Mosaic model that fits their business. Use when the user has data (spreadsheets, databases, APIs) and wants to understand what Mosaic can do for them — before committing to building anything. The assessment is a "solutions architect consultation" that discovers data, proposes a model, explains the value, and offers to build it.
---

# Mosaic Assessment — Data Discovery & Model Proposal

When a user runs `/mosaic-assess` or asks "what could Mosaic do with my data?", you are acting as a solutions architect. Your job is to look at what they HAVE and propose what they COULD BUILD — then explain the value in business terms they immediately understand.

## The Assessment Flow (5 steps)

### Step 1: Discover Data Sources

Look for data in the user's context:

- **Files:** Excel (.xlsx), CSV, JSON in their working directory or Downloads
- **Databases:** connection strings in .env files, docker-compose.yml, config files
- **APIs:** REST endpoints in their codebase (look for fetch/axios/requests calls)
- **Existing models:** any YAML, SQL schemas, or data dictionaries

For each source found, sample the schema:
- Spreadsheets: read headers + first 5-10 rows
- Databases: `\dt` + `SELECT * FROM <table> LIMIT 5`
- CSVs: `head -5 <file>`

Report what you found: "I see [N] data sources with [M] columns of data covering [domain]."

### Step 2: Identify Dimensions and Measures

From the sampled data, classify columns:

**Dimensions** (categorical; form the cube's axes):
- Time/Date columns → Time dimension (monthly, weekly, daily granularity)
- Geographic columns → Market/Region dimension
- Category columns (channel, campaign, segment, product) → their own dimensions
- Status/type columns → potentially Scenario or Version dimensions

**Measures** (numeric; the values in the cube):
- Dollar amounts (spend, revenue, cost) → Input measures with aggregation: Sum
- Counts (customers, clicks, sends, orders) → Input measures with aggregation: Sum
- Rates/ratios (CPC, CTR, conversion rate, ROI) → likely Derived measures (calculated from other measures)
- Averages (AOV, ACV) → likely Derived or WeightedAverage aggregation

**The key insight to surface:** ratios and rates should almost NEVER be imported as inputs. They should be DERIVED by Mosaic from their component measures. This is what makes the cube recalculate correctly when you change an input.

Classification heuristic:
- Can this value be calculated from two other columns in the same row? → Derived
- Is this value a ratio of two other measures? → Derived (body: "A / B")
- Is this value a difference or margin? → Derived (body: "A - B" or "(A - B) / A")
- Otherwise → Input

### Step 3: Propose a Model

Present the proposed model in plain language FIRST, then show the YAML:

```
I propose a Mosaic model with:

DIMENSIONS (the axes of your data cube):
  • Scenario: Actual vs Forecast (lets you plan against reality)
  • Version: Working vs Approved (lets you draft without affecting the approved plan)
  • [Time dim]: [granularity] (your time axis)
  • [Categorical dim 1]: [element list]
  • [Categorical dim 2]: [element list]
  • Measure (built-in)

MEASURES (what's in the cells):
  INPUT (from your data):
    • [measure 1] — [description] — aggregation: Sum
    • [measure 2] — ...
  DERIVED (Mosaic calculates):
    • [derived 1] = [formula] — "change [input], this updates instantly"
    • [derived 2] = [formula]

THE VALUE (what this gives you):
  • Change next month's [input] → see [derived 1], [derived 2] recalculate instantly
  • Compare Actual vs Forecast on the same grid
  • Roll up by [dim] to see totals with correct weighted averages
  • Snapshot a plan, edit it, compare versions
```

### Step 4: Explain the Business Value

This is the most important step. Don't just list features — show SPECIFIC examples from THEIR data:

**Template:**
> "For [specific context from their data], Mosaic shows:
> - **[Derived measure]:** [calculated value from their actual data]
> - **[Another derived]:** [value]
>
> **The forecasting power:** change [specific input] from $X to $Y →
> Mosaic instantly shows [derived] changes from [old] to [new].
> You see the impact BEFORE spending the money."

Use THEIR actual numbers. "ROI = 1.75×" means more than "Mosaic can calculate ROI."

**Common value propositions by domain:**

| Domain | Key value prop |
|---|---|
| Marketing/campaigns | "Change ad spend → see CAC and ROI recalculate instantly" |
| Sales | "Change close rate → see revenue forecast update across all reps" |
| Finance/FP&A | "Change revenue assumptions → see P&L flow through to bottom line" |
| E-commerce | "Change conversion rate → see required traffic to hit revenue target" |
| Media buying | "Change CPM → see reach and frequency adjust for budget" |

### Step 5: Offer to Build (with approval gates)

On user approval, execute the pipeline:

1. **Build the model:** invoke `/mosaic-author` with the proposed schema
2. **Write the recipe:** invoke `/mosaic-import` to propose the data mapping
3. **Load data:** if `mc tessera apply` is available, offer to ingest the actual data
4. **Show it working:** query a specific coordinate and demonstrate a derived value calculating

Gate: ALWAYS ask for approval before each step. Never auto-execute.

```
Shall I:
  1. ✓ Build this model? (generates a .yaml file)
  2. ✓ Write the import recipe? (maps your data to the cube)
  3. ✓ Load your actual data? (ingests via Tessera)

Type 1, 2, 3, or "all" to proceed.
```

## What NOT to do

- Don't propose a model without sampling the actual data first
- Don't classify a ratio as an Input measure (it should be Derived)
- Don't propose more than 6-8 dimensions (cubes get sparse fast)
- Don't promise database connectivity if `mc tessera` isn't available
- Don't modify any existing data — assessment is READ-ONLY until the user approves building
- Don't propose Derived measures that reference other Derived measures in Phase 5A (rule chains are fine, but keep it simple for the assessment)

## Domain templates (reference patterns)

When you recognize the domain, use these as starting points:

**Marketing/Campaign Performance:**
- Dims: Scenario, Version, Time, Channel/Campaign, Market/Region, Measure
- Input: Spend, Impressions, Clicks, Conversions, Revenue
- Derived: CPC (Spend/Clicks), CTR (Clicks/Impressions), Conv_Rate (Conversions/Clicks), CPA (Spend/Conversions), ROAS (Revenue/Spend)

**Email Marketing / Direct Mail:**
- Dims: Scenario, Version, Drop_Date, Audience/Segment, Market, Measure
- Input: Ad_Spend, Sends, Opens, Clicks, Conversions, Revenue, New_Customers
- Derived: Open_Rate (Opens/Sends), CTR (Clicks/Opens), Conv_Rate (Conversions/Clicks), CAC (Spend/New_Customers), ROI ((Revenue-Spend)/Spend)

**SaaS Metrics:**
- Dims: Scenario, Version, Month, Product/Plan, Segment, Measure
- Input: MRR, New_MRR, Churned_MRR, Expansion_MRR, Customers, New_Customers, Churned_Customers
- Derived: Net_MRR_Growth (New_MRR + Expansion_MRR - Churned_MRR), Churn_Rate (Churned_Customers/Customers), ARPU (MRR/Customers), LTV (ARPU/Churn_Rate)

**Sales Pipeline:**
- Dims: Scenario, Version, Quarter, Rep/Team, Stage, Measure
- Input: Opportunities, Pipeline_Value, Closed_Won, Closed_Lost, Days_In_Stage
- Derived: Win_Rate (Closed_Won/(Closed_Won+Closed_Lost)), Avg_Deal_Size (Pipeline_Value/Opportunities), Velocity (Pipeline_Value*Win_Rate/Days_In_Stage)

## The "aha moment" formula

The assessment succeeds when the user says "oh, that's useful." The formula:

1. Show them a number they already know from their data (e.g., "your October Houston CAC was $8.88")
2. Show them that Mosaic DERIVED it from their raw inputs (not hardcoded)
3. Show them what happens when they change an input ("if you spend $30K instead of $22K, CAC jumps to $12.10")
4. They realize: "I can plan next month's budget by trying different spend levels and seeing the impact instantly"

That's the assessment. Discovery → Proposal → Value demonstration → Build (on approval).
