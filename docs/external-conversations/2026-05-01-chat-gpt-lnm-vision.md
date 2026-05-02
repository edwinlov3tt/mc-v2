# GPT-5: "Large Numbers Model" — LLM-as-model-architect framing

**Date:** 2026-05-01
**Source:** GPT-5 response, pasted into the project chat by the project owner.
**Context:** asked whether MarketingCubes could be the substrate for LLM-authored planning models in plain English, with the kernel acting as the validation/execution layer.
**Significance:** the response reframes Phase 3–7 of [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) from a "TM1 clone with a UI" to an **AI-native planning substrate**. ADR-0003 captures the strategic decision. This file preserves the primary source verbatim.

---

Yes. This is **very possible**, and honestly this should become the core vision:

> **MarketingCubes is a planning/modeling kernel that lets a good vibe coder describe a business in plain English, then an LLM turns that into validated cubes, dimensions, rules, scenarios, traces, and tests.**

But the key is this:

```txt
The LLM should not write custom engine code for every company.

The LLM should generate readable model definitions that the engine validates and runs.
```

That is the breakthrough.

## The right mental model

You do **not** want the LLM doing this:

```txt
User: Build me a finance model.
LLM: Writes random Rust / Python / SQL / formulas from scratch.
```

That becomes fragile fast.

You want this:

```txt
User: Build me a marketing and finance planning model for a home services company.

LLM outputs:
- dimensions
- hierarchies
- measures
- rules
- scenarios
- permissions
- data source mappings
- test cases
- explanation text

MarketingCubes validates it.
Rust kernel executes it.
Trace explains it.
```

So the LLM becomes a **model architect**, not the calculation engine.

## What this system should eventually look like

A company says:

```txt
We are a roofing company.
We spend money on Google Ads, Meta, SEO, and email.
We want to forecast leads, booked jobs, revenue, gross profit, and cash flow by market and month.
```

The LLM asks a few questions:

```txt
What markets do you serve?
What services do you sell?
What is your average job value?
What is your lead-to-sale close rate?
What marketing channels do you use?
Do you want scenarios like Baseline, Aggressive, Conservative?
Do you track actuals from Google Ads, Meta, QuickBooks, HubSpot, etc.?
```

Then it generates a readable model file:

```yaml
model: Roofing_Company_Planning

dimensions:
  scenario:
    elements: [Actual, Baseline, Aggressive, Conservative]

  time:
    hierarchy:
      FY2026:
        Q1_2026: [Jan_2026, Feb_2026, Mar_2026]
        Q2_2026: [Apr_2026, May_2026, Jun_2026]

  market:
    hierarchy:
      Florida:
        - Tampa
        - Orlando
        - Miami

  channel:
    hierarchy:
      Paid_Media:
        - Google_Ads
        - Meta_Ads
      Owned_Media:
        - SEO
        - Email

measures:
  inputs:
    - Spend
    - CPC
    - CVR
    - Close_Rate
    - Average_Job_Value
    - Gross_Margin_Rate

  derived:
    - Clicks
    - Leads
    - Customers
    - Revenue
    - Gross_Profit

rules:
  Clicks: Spend / CPC
  Leads: Clicks * CVR
  Customers: Leads * Close_Rate
  Revenue: Customers * Average_Job_Value
  Gross_Profit: Revenue * Gross_Margin_Rate
```

That is human-readable, LLM-readable, and engine-readable.

That is the lane.

## The big design principle

Make the system have **three layers**:

```txt
1. Rust kernel
   Fast, strict, boring, correct.

2. Model definition layer
   YAML / JSON / typed DSL.
   Human-readable.
   LLM-readable.
   Validated before execution.

3. LLM model builder
   Interviews the user.
   Generates model definitions.
   Explains assumptions.
   Creates tests.
   Suggests improvements.
```

The LLM should mostly live in layer 3.

It should not be trusted inside layer 1.

## Why this fits you perfectly

This makes the tool usable by someone like you:

```txt
Not a formal FP&A expert.
Not a data scientist.
Not a Rust systems engineer.
But technical enough to understand business logic, inspect outputs, and guide the AI.
```

That is the actual user persona.

Not "CFO with no technical skill."

Not "PhD quant."

The user is:

```txt
A business/marketing builder who knows what they want the model to do, and uses AI to assemble it safely.
```

That is a very real market.

## The LLM needs rails

The way you make this safe is by giving the LLM a strict schema.

For example, the LLM is only allowed to output:

```txt
Dimension
Hierarchy
Measure
Rule
Scenario
Version
Permission
DataSourceMapping
GoldenTest
```

It cannot invent random concepts.

Then the engine validates:

```txt
Does every rule reference real measures?
Are there cycles?
Are the dimensions valid?
Are consolidated cells being written to?
Are derived cells being overwritten?
Are ratios being summed incorrectly?
Are all required assumptions present?
Do golden test numbers match?
```

That is how you get "vibe coder friendly" without making it a hallucination machine.

## The most important feature: model linting

You should eventually have a command like:

```bash
mc lint roofing-model.yaml
```

And it returns:

```txt
PASS: all dimensions valid
PASS: no duplicate elements
PASS: no rule cycles
PASS: all derived measures have rules
PASS: all input assumptions exist
WARNING: CPC is weighted by Spend. Confirm this is intended.
WARNING: Close_Rate is weighted by Leads. Confirm this is intended.
FAIL: Revenue rule references Average_Deal_Size, but measure is named Average_Job_Value.
```

This is huge.

The LLM can generate the first draft, but the engine must be the judge.

## The second most important feature: golden tests

When the LLM builds a model, it should also generate tiny example tests.

Example:

```yaml
golden_tests:
  - name: basic_paid_search_revenue
    inputs:
      Spend: 10000
      CPC: 5
      CVR: 0.10
      Close_Rate: 0.20
      Average_Job_Value: 5000
      Gross_Margin_Rate: 0.45
    expected:
      Clicks: 2000
      Leads: 200
      Customers: 40
      Revenue: 200000
      Gross_Profit: 90000
```

Then MarketingCubes runs:

```bash
mc test roofing-model.yaml
```

That is how a non-expert can trust the model.

## The third most important feature: explain mode

The user should be able to ask:

```txt
Why is Q1 revenue $1.2M?
```

And the engine says:

```txt
Q1 Revenue = Jan Revenue + Feb Revenue + Mar Revenue

Jan Revenue = Customers × Average Job Value
Customers = Leads × Close Rate
Leads = Clicks × CVR
Clicks = Spend / CPC

For Tampa / Google Ads / Jan:
Spend = $10,000
CPC = $5
Clicks = 2,000
CVR = 10%
Leads = 200
Close Rate = 20%
Customers = 40
Average Job Value = $5,000
Revenue = $200,000
```

This is where the LLM can shine. The Rust engine creates the trace. The LLM turns the trace into plain English.

## The product vision gets sharper

I would phrase the core vision like this:

> **MarketingCubes lets AI build and validate company-specific planning models from plain English, while a strict Rust calculation engine guarantees the math, dependencies, rollups, writeback rules, and traceability.**

That is much stronger than "TM1 clone."

It becomes:

```txt
AI-native TM1 for marketing and finance planning.
```

But the "AI-native" part is not the engine hallucinating calculations. It is the model-authoring layer.

## What to add later, not now

Do **not** put this into Phase 1 yet.

But add it as a later roadmap section:

```txt
Phase 4 or Phase 5: Model Authoring Layer

- YAML schema format
- model linter
- golden test runner
- plain-English model generator prompt
- LLM interview flow
- generated documentation
- trace-to-explanation summaries
- company model templates
```

Phase 1 should still stay focused on the Rust kernel.

But yes, design the kernel so this becomes easy later.

## The key design rule now

As you build Phase 1, avoid hard-coding anything that would make LLM-generated models impossible later.

Good:

```txt
Measures are data.
Dimensions are data.
Rules are expression trees.
Hierarchies are data.
Scenarios are data.
```

Bad:

```txt
Spend is a special engine concept.
Revenue is a special engine concept.
Marketing is hard-coded.
Finance is hard-coded.
Only the Acme demo works.
```

The Acme demo can be hard-coded as a fixture.

The engine must stay generic.

## My honest take

Yes, this is absolutely the right bigger vision.

The thing you are really building is not just a calc engine. It is:

```txt
A safe modeling language for business planning,
with a Rust execution engine,
and an LLM copilot that can assemble models from business intent.
```

That is a legit innovation path.

And it fits your "vibe coder" angle perfectly because the product is not saying:

```txt
You never need to understand anything.
```

It is saying:

```txt
You do not need to be a formal FP&A architect or data scientist to build a useful planning model. The system will guide you, validate you, and explain the math.
```

That is the right promise.
