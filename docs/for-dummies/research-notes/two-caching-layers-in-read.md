# Two Caching Layers in `read` — For Dummies

> **In one line:** the engine keeps two scratch-pads of "answers I've already worked out," so when you ask the same question twice it doesn't redo all the math.

[Technical version →](../../research-notes/two-caching-layers-in-read.md)

---

## The analogy: the desk calculator

You're an accountant working out a complicated month-end report. Someone asks:

> *"What's the total Q1 spend in Florida?"*

You pull out your calculator, walk through 35 line items, add them all up, and tell them: *"$329,400."*

A minute later, the same person asks the same question. Do you redo all 35 line items? Of course not — you already worked it out. You glance at the sticky note on your desk where you wrote *"Q1 Florida spend = $329,400"* and just say it again.

Now, imagine you also have a second sticky note nearby. This one is for a different *kind* of question — something like *"how much did Tampa spend in March on Paid Search?"* You wrote *"Tampa-Mar-Paid_Search Spend = $11,500"* on it, with a note that says *"this is a freshly-entered input, came directly from the user."* Different shape of answer, different sticky note.

Two scratch-pads. Same idea. Different kinds of answers.

The engine has exactly this setup.

## What's actually happening

When you ask the engine *"what's the value of cell X?"*, there are three possibilities:

**Possibility 1.** X is an **input cell** (someone typed it in). The engine just looks it up directly. No math needed. No cache needed. Done.

**Possibility 2.** X is a **derived leaf** (a formula at one specific location, like Tampa-Mar-Paid_Search Clicks). To get the answer, the engine has to evaluate the formula, which means reading other cells (Spend, CPC) and dividing them. That's expensive-ish. So after computing the answer, the engine stores it on the **first scratch-pad**: *"derived-leaf cache."* Next time someone asks for the same cell, the engine reads it off the pad and skips the formula evaluation.

**Possibility 3.** X is a **consolidated cell** (a rolled-up answer, like Q1-Florida-Spend). To get this, the engine has to walk through every leaf cell underneath — for the worst Acme rollup that's 420 leaves. That's *very* expensive. So after computing the answer, the engine stores it on the **second scratch-pad**: *"consolidated cache."* Next time someone asks the same rollup, glance at the pad, return the answer.

## How the engine knows when a sticky note is stale

Each sticky note has two markers on it:

1. **A revision number** — what version of the cube was current when this answer was written. If the cube has been edited since (the revision has bumped), this sticky note might be stale.
2. **A "still good?" flag** — managed separately. When you write a new value to any cell, the engine ALSO adds the cells that depended on it to a "stale list" (the *dirty list* — see the [dirty-propagation note](./dirty-propagation-as-per-write-delta.md)). If a sticky note's cell is on the stale list, that note is bad.

So when reading a cached answer, the engine checks:

> *"Is this sticky note's revision still current AND is its cell NOT on the stale list?"*

If both hold: cache hit, return the stored answer instantly.
If either fails: cache miss, redo the math, write a fresh sticky note.

There's one more rule: **if you ask for a value WITH a trace** (i.e., "show me the work, every step of the way"), the engine bypasses both caches. The trace requires walking the entire computation tree, which is the same cost as recomputing — there's no shortcut, so caching wouldn't help.

## Why we care

The cube has thousands of cells. Without caching, every question hits the formulas again. The consolidated cache in particular is critical: there's a test that explicitly demands the second read of a consolidated value be **at least 10× faster** than the first. That bound forced caching to ship in Phase 1.

For Phase 1B benchmarking, this matters because *"warm" reads (cache-hit) and "cold" reads (cache-miss) are wildly different costs.* When the benchmark engineer sets up timings, they have to be deliberate about which case they're measuring. A "warm consolidated read" should be sub-microsecond; a cold one might take milliseconds.

## One thing that's easy to get wrong

You might assume the engine has one big cache. It has two — one for derived-leaf answers, one for consolidated answers — sharing the same underlying storage but distinguished by the *kind* of answer (the "provenance" stamp).

Another easy slip: the "rollback" feature drops every entry from the derived-leaf scratch-pad (because rule definitions might have changed since the snapshot) but *keeps* entries from the consolidated scratch-pad (those are pure math over hierarchy structure and stay valid). It's an asymmetry you'd miss if you assumed the two caches were the same thing.

---

*Tied to: [lazy-dependency-graph](./lazy-dependency-graph.md) (the dependency graph is what tells the engine which cached cells to mark stale on a write), [dirty-propagation](./dirty-propagation-as-per-write-delta.md) (the "stale list" is what gates the cache).*
