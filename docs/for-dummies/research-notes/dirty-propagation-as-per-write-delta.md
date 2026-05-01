# Dirty Propagation as a Per-Write Delta — For Dummies

> **In one line:** "what changed *because of this single edit*" is a different question from "what's currently stale in the whole cube." The test bound (≤ 215) is asking the first question, not the second.

[Technical version →](../../research-notes/dirty-propagation-as-per-write-delta.md)

---

## The analogy: your inbox

Imagine your spouse comes home and asks:

> *"Hey, did anything happen at work today?"*

You have two ways to answer:

**Wrong answer.** You read out every single email you've ever received, ever. Including the 12,000 newsletters from 2019. *"Well, in 2019 I got an email from LinkedIn, and then in 2020 I got…"*

**Right answer.** *"Yeah, three things came in *today*: the budget got approved, Sarah is out next week, and the printer broke."*

Both are technically *true* lists of things in your inbox. But the question was about *what's new today*, not the lifetime archive.

That's exactly the situation the engine's "dirty list" is in.

## What's actually happening

The engine keeps a **dirty list** — a list of cells whose cached values are stale and need recomputing. Every time you write to a cell, that cell *and everything that depended on it* get added to the dirty list.

Now, the test contract says:

> *"After one Spend write at one cell, the dirty list should grow by no more than 215 entries."*

That's a sensible bound. A single Spend write at one place can plausibly affect: the 5 derived measures at the same place (Clicks, Leads, Customers, Revenue, Gross_Profit), plus those measures rolled up across various consolidation levels (Q1 totals, Florida totals, USA totals, etc.). The math works out to about 215.

**Here's the gotcha.** Before the test runs that single write, the test fixture *first* does a setup step where it writes all 2,520 input cells. Each of *those* writes also dirtied a bunch of stuff. By the time the test's "one write" happens, the dirty list already has about 17,820 entries from the setup.

So if you ask "how big is the dirty list right now?" after the test write, the answer is around 18,035. Not ≤ 215.

The fix is to phrase the assertion as a **delta**: *"how many entries did the dirty list grow by, just from this one write?"* That number really is ≤ 215, because *that's the question the spec was actually asking.* You snapshot the dirty list before the write, do the write, snapshot again, and compare.

## Why we care

This is one of the most common shapes of test-misreading. The spec says "after one write, ≤ 215." Read literally, that fails. Read in spirit ("the marginal effect of one write is ≤ 215"), it's fine. The discipline is: **never just loosen the bound to match what you measured** — check whether the question itself was the marginal one or the absolute one.

For the upcoming benchmark work, the same discipline applies: when timing dirty propagation, you have to time the *delta cost* of one write — snapshot before, write, snapshot after, time the difference — not the absolute state of the cube.

## One thing that's easy to get wrong

The temptation when this test fails is to bump the number from 215 up to 18,000-something to "make the test pass." That's the wrong move — the bound 215 is load-bearing. It's how the engine catches over-marking bugs. If a future change starts marking 250 cells per write instead of 215, that bound is what flags it.

The other temptation is to *clear the dirty list* after fixture setup so the absolute interpretation works. Don't. The fixture setup mandate is part of the spec; clearing dirty after setup would mask propagation bugs in the setup itself.

---

*Tied to: [lazy-dependency-graph](./lazy-dependency-graph.md) (one of the two paths that fills the dirty list), [two-caching-layers](./two-caching-layers-in-read.md) (the dirty list is the cache's "this is stale" signal).*
