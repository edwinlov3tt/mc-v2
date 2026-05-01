# Weighted-Average Consolidation — For Dummies

> **In one line:** when you roll up a *ratio* (cost-per-click, conversion-rate, etc.) across markets, you can't just add them up or simple-average them. You weight by the underlying volume.

[Technical version →](../../research-notes/weighted-average-consolidation.md)

---

## The analogy: your GPA

In college, you took six classes one semester:

| Class | Grade | Credits |
|---|---|---:|
| Calculus | 4.0 (A) | 4 |
| Phys Ed | 4.0 (A) | 1 |
| Organic Chem | 2.0 (C) | 4 |
| Public Speaking | 3.0 (B) | 3 |
| Linear Algebra | 4.0 (A) | 4 |
| Modern Dance | 4.0 (A) | 1 |

What's your GPA?

**Wrong way #1: simple sum.** `4.0 + 4.0 + 2.0 + 3.0 + 4.0 + 4.0 = 21.0`. Your GPA is 21? That's not a number.

**Wrong way #2: simple average.** `21.0 ÷ 6 = 3.50`. Closer — but not how universities actually compute GPA.

**Right way: credit-weighted average.** You multiply each grade by its credits, sum that, and divide by total credits. That gives you `(4×4 + 4×1 + 2×4 + 4×3 + 4×4 + 4×1) ÷ (4+1+4+3+4+1) = 60 ÷ 17 = 3.53`. The C in Organic Chem hurts you more because it was a 4-credit class; the C in a 1-credit phys-ed wouldn't have hurt as much.

That weighting matters because **a ratio's "average" depends on the size of the things being averaged.** A 4.0 in a 1-credit phys-ed class doesn't deserve the same say as a 4.0 in a 4-credit calculus class.

## What's actually happening

In the cube, "Cost Per Click" (CPC) is a ratio. CPC at one market = `Spend ÷ Clicks` at that market. Now suppose Florida and Georgia both have CPCs of $1.50 — but Florida spent $90,000 last quarter and Georgia spent $1,000. What's the *combined* CPC for "Florida + Georgia"?

If you simple-sum: `1.50 + 1.50 = 3.00`. Meaningless.

If you simple-average: `(1.50 + 1.50) ÷ 2 = 1.50`. Hmm, that one *happens* to be right when the two CPCs are equal, but it's only coincidence. If Florida had CPC $1.40 and Georgia had CPC $5.00, the simple average is $3.20 — but that's wrong, because Florida did 99% of the spending, so the *real* combined CPC is going to be much closer to $1.40 than to $5.00.

The right answer is the **spend-weighted average**: `(1.40 × 90,000 + 5.00 × 1,000) ÷ (90,000 + 1,000) = 126,000 + 5,000 ÷ 91,000 ≈ $1.44`. That's what the engine computes.

## The funnel chain

The clever part is *which weight you use* for each ratio. The engine wires it up like a marketing funnel:

| Ratio | Weighted by | Why |
|---|---|---|
| **CPC** (cost per click) | **Spend** | the total dollars at the bottom of the cost layer |
| **CVR** (conversion rate, leads-per-click) | **Clicks** | the volume of clicks the rate is over |
| **Close_Rate** (customers per lead) | **Leads** | the volume of leads being closed |
| **AOV** (avg order value) | **Customers** | the count of orders contributing |
| **COGS_Rate** (cost of goods as a % of revenue) | **Revenue** | the revenue base the cost is a fraction of |

Each ratio is weighted by the *denominator* of the ratio at the leaf level. CPC = Spend÷Clicks, so when rolling up CPC, you weight by Spend. It's the volume of the underlying activity that the rate is over.

The other six measures in the cube — Spend, Clicks, Leads, Customers, Revenue, Gross_Profit — are *not* ratios. They're dollar amounts and counts. Those just **simple-sum** when rolled up: Florida + Georgia spend = `90,000 + 1,000 = 91,000`. Easy.

## Why we care

If you defaulted ratio measures to simple-sum (which is the obvious thing to do for *non*-ratio measures), every consolidated CPC / CVR / etc. in the cube would be wrong. And it would be wrong in a way that *looks like a normal number* — no error, no warning. The engine specifically forbids defaults. Every measure has its rollup rule explicitly stated when it's defined.

There's even a test that asserts the consolidated CPC is *not equal to* either the simple sum or the simple average — exactly to catch a refactor that reverts this discipline by mistake.

## One thing that's easy to get wrong

The "obvious" instinct when implementing rollup is "just sum everything up." Five of the eleven Acme measures break that pattern. Reading the spec carefully (or this note) before coding the rollup is the difference between a kernel that ships and one that produces nonsense ratios.

The other tricky bit: when CPC is being rolled up, the engine reads *both* the CPC value at each leaf *and* the Spend value at each leaf (the weight). That means changes to Spend invalidate the consolidated CPC, not just the consolidated Spend. Phase 2 invalidation work has to handle that.

---

*Tied to: [null-vs-zero-vs-nan](./null-vs-zero-vs-nan.md) (when the total weight is zero, the result is Null, not 0.0), [lazy-dependency-graph](./lazy-dependency-graph.md) (each weight read also creates a dependency edge).*
