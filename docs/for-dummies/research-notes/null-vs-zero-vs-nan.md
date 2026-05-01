# Null vs Zero vs NaN — For Dummies

> **In one line:** these three things look similar to a programmer but mean three completely different things, and the engine refuses to mix them up.

[Technical version →](../../research-notes/null-vs-zero-vs-nan.md)

---

## The analogy: a tax form

You're filing your taxes. There's a line that says *"Other income."* Three things could be on that line:

1. **Blank.** You haven't gotten around to filling it in yet, or you genuinely don't know. → This is **Null**. It's not "no income." It's "no answer yet."
2. **`$0.00`.** You filled it in on purpose. You actually had zero other income. You're claiming this as a fact. → This is **Zero**.
3. **`ERROR`** scrawled across the line because you tried to compute it as `$5,000 ÷ 0` and your calculator broke. → This is **NaN** (and its cousins: `+Infinity`, `-Infinity`).

A tax accountant treats these three states *very* differently:

- "Blank" means "ask the client."
- "Zero" means "fact, move on."
- "ERROR" means "your math is wrong, fix it."

Substituting any of these for any other is a serious problem. Zero ≠ Blank ≠ Error.

## What's actually happening

The engine has the same three states, with the same rules:

- **Null** is its own first-class value. *"No answer / unknown / not applicable here."* It's not zero. It's not stored as a special number. It's a different shape entirely.
- **Zero** is a regular number. It's a fact: "the value here is zero."
- **NaN / Infinity** are what you get from broken floating-point math (dividing by zero, overflowing, etc.). The engine treats them as **errors**.

The discipline is in three places:

**At the front door (writeback).** When you try to write a NaN or Infinity into a cell, the engine rejects the write with an error. *"NaN is not a valid cell value."* Storage never sees NaN. Ever. Period. This is enforced inside `Cube::write` before anything is committed.

**During formula evaluation.** When the engine is computing a derived cell — say, Clicks = Spend ÷ CPC — and the math overflows or hits a divide-by-zero, the result gets converted to **Null**, not stored as Infinity, not propagated as NaN. The thinking is: *"the computation was undefined, so the answer is 'unknown.'"*

**At division specifically.** Divide-by-zero (or divide-by-anything-tiny-enough-to-round-to-zero) returns **Null**, not Infinity, not an error. So if some market has zero clicks and you ask "what's the cost-per-click?" you get Null — *"unknown / undefined"* — not infinity, not zero.

## The Null arithmetic table

Here's how Null behaves in math:

| Operation | What happens |
|---|---|
| `Null + 5` | Result is `5`. (Null acts like an empty/missing value, not zero.) |
| `Null + Null` | Result is `Null`. |
| `5 - Null` | Result is `5`. |
| `Null - 5` | Result is `-5`. (Sub treats Null on the left as if it were 0.) |
| `Null × anything` | Result is `Null`. (Null poisons multiplication.) |
| `Null ÷ anything` | Result is `Null`. |
| `anything ÷ 0` | Result is `Null`. (Div-by-zero never produces Infinity here.) |

Add and Subtract treat Null as "the missing value, just skip it." Multiply and Divide treat Null as "I don't know — so the answer is also I-don't-know."

## Why we care

If you conflate any of these, you get nonsense numbers that *look* legitimate:

- If "no Spend recorded yet" gets stored as `0.0` instead of Null, then the consolidated Spend across all markets is artificially low — but it looks valid. A finance person presenting that to the board would have no way to tell.
- If divide-by-zero gave Infinity instead of Null, that Infinity would propagate through every downstream rule and every cell that depended on it would also be Infinity. One bad cell poisons the whole report.
- If a NaN ever reached storage, every comparison involving that cell would behave weirdly (NaN doesn't even equal itself in floating-point math), and bug reports would start pouring in from people seeing inconsistent rollups.

The engine's discipline is: **catch these states at the boundary**, never let them mix, always have one canonical "I don't know" value (Null).

## One thing that's easy to get wrong

In Rust (or pretty much any language), `0.0` and `NaN` are valid `f64` values, so it's tempting to use `0.0` as a default when no value has been entered. **Don't.** Use `Null`. If you assign `0.0` to mean "not entered," you've just invented a fourth state and broken the whole discipline.

The other classic trap is float comparison: `if value == 0.0 { … }`. Floating-point math is fuzzy, so direct equality is unreliable. The engine uses small epsilons (like `< 1e-300` for "this is basically zero, treat as zero for division") and never compares floats with `==` outside of tests.

---

*Tied to: [weighted-average-consolidation](./weighted-average-consolidation.md) (zero total weight → Null result, not zero, not NaN), [two-caching-layers](./two-caching-layers-in-read.md) (a cached cell holding Null is meaningfully different from one holding 0.0).*
