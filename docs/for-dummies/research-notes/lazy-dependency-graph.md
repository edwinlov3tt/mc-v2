# Lazy Dependency Graph — For Dummies

> **In one line:** the engine doesn't write down "which cells depend on which" until somebody actually asks for a value.

[Technical version →](../../research-notes/lazy-dependency-graph.md)

---

## The analogy: the recipe book

Imagine you're a chef running a kitchen. You have hundreds of recipes, and each recipe lists its ingredients. Now imagine your boss walks in and says:

> *"We're running low on eggs. Tell me every recipe that uses eggs so I know what we can't make."*

There are two ways you could've prepared for this question:

**Option A (eager).** When you first opened the kitchen, you sat down and made a giant cross-reference index: *"eggs are used in pancakes, omelets, custard, mayonnaise, …"* You did this for every ingredient in every recipe. Now you can answer the boss instantly. But you spent a whole afternoon building the index, and most ingredients never get asked about.

**Option B (lazy).** You don't build any index up front. The first time someone *cooks* a pancake, you jot down on a sticky note: *"pancakes use: eggs, flour, milk."* When someone cooks an omelet, you jot down its ingredients. After a busy day of actual cooking, you've gradually built a partial index — but only for the recipes that were actually made.

Our engine does **Option B**. The "recipe book" is the cube full of derived cells (Clicks, Leads, Customers, Revenue, Gross_Profit). The "ingredients" are the underlying cells those recipes read (Spend, CPC, etc.). Until someone actually *reads* a derived cell — which makes the engine cook the recipe — it doesn't write down the ingredient list.

## What's actually happening

There's a thing called the **dependency graph**. Think of it as a notebook. Each page says: *"this cell reads from these other cells."*

When you call `Cube::build()` to set up the cube, the notebook is **empty**. There's a test that literally checks for this — if a future change accidentally pre-populates the notebook, that test screams.

The notebook gets filled in one entry at a time:

1. You ask for the value of, say, `(Q1, Florida, Paid_Search, Clicks)`.
2. The engine looks up the rule for Clicks: *"Spend ÷ CPC."*
3. It evaluates the rule, which means it reads `(Q1, Florida, Paid_Search, Spend)` and `(…, CPC)`.
4. *After* it returns the answer, it writes in the notebook: *"this Clicks cell read those two Spend and CPC cells."*

That last step is the lazy part. The notebook only ever knows about reads that have already happened.

## Why we care

There are two reasons this matters:

**1. The engine has to behave this way to pass its own tests.** The contract literally says "the dependency graph must be empty after build." If someone's instinct kicks in and they pre-populate it "just to be safe," the test fails and the build is broken.

**2. It changes how writes work.** When you write a new value to Spend, the engine has to figure out *what to invalidate* — what cached values are now stale. It does this by looking up the notebook. **If the notebook is empty (because nobody's done any reads yet), the invalidation does almost nothing.** That's not a bug — it's correct behavior, because if nobody's read anything, nothing is cached, so nothing needs invalidating.

This becomes important for the upcoming benchmark work: if you're trying to *measure* how long invalidation takes, you have to first force a bunch of reads to populate the notebook. Otherwise you're timing nothing.

## One thing that's easy to get wrong

Looking at the cube right after building it, you might think *"the dependency graph is broken — there are no entries!"* It's not broken. It's empty by design. Do a few reads and watch entries appear.

The flip side: if you're benchmarking write performance and forget to "warm up" the cube with reads first, your numbers will be artificially fast and meaningless.

---

*Tied to: [two-caching-layers](./two-caching-layers-in-read.md) (the notebook is what the cache uses to decide what's stale), [dirty-propagation](./dirty-propagation-as-per-write-delta.md) (the notebook is one of two ways the engine figures out what changed).*
