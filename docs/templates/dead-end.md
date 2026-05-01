# Dead-end: [One-line description of the approach]

**Date:** YYYY-MM-DD
**Status:** `closed | re-opened`
**Phase context:** `1A | 1B | 2 | …`

---

## What we tried

Concrete description. What approach. What code shape. What configuration. Be specific — a future reader needs to know exactly what was attempted to judge whether their idea is the same thing or a different thing.

## Why we tried it

What we hoped it would unlock. What problem it would solve. What signal made us think this was worth attempting.

## What happened

The failure mode. **Show the actual error / measurement / contradiction**, not a paraphrase. If it was a benchmark regression, show the numbers. If it was a build error, show the error text. If it was a correctness violation, show the failing assertion.

## Exact conditions at failure

This is the bar. Without it, the dead-end is folklore.

- **Toolchain:** Rust X.Y.Z, OS, etc.
- **Cube size / fixture:** Acme (~25K cells)? Larger?
- **Code commit:** SHA at the time of failure.
- **Specific configuration:** any feature flags / env vars / build flags.
- **Anything else specific to this attempt** that would change a re-run's outcome.

## What would need to change for this to work

Explicit reopen conditions. Examples:

- Toolchain bumps past Rust 1.85.
- Cube size grows past 100K cells.
- A specific upstream library lands a fix or a feature.
- A different design choice (e.g., introducing the `CellStore` trait that Phase 2 owns) is made.

If none of those happen, **don't re-attempt this**.

## Cross-links

- Hypothesis answered: [`../hypotheses/H...`](../hypotheses/)
- Experiment that produced the failure (if any): [`../experiments/...`](../experiments/)
- Concept this affects: [`../concepts/...`](../concepts/)
- Source code touched / not-touched: [`../../crates/...`](../../crates/)

## If re-opened

(Filled in only if status changes to `re-opened`.)

- **Date:** YYYY-MM-DD
- **What changed:** explicit description of which condition above flipped.
- **New experiment:** [`../experiments/...`](../experiments/)
