# EXP-[number]: [One-line title]

**Date:** YYYY-MM-DD
**Status:** `complete | superseded | in-progress`
**Phase:** `1A | 1B | 2 | …`
**Concepts:** `dependency-graph, dirty-propagation, consolidation, snapshot, …`
**Related hypotheses:** `H001, H003`
**Related dead-ends:** `2026-MM-DD-<slug>`

---

## Hypothesis

What specific claim are we testing? One or two sentences.

## Method

- **Fixture:** Acme cube? Custom mini-cube? Something else?
- **Baseline:** what we're comparing against (previous benchmark, naive impl, theoretical lower bound).
- **Variants tested:** list of code paths / config knobs / feature flags.
- **Metrics:** what numbers count (ns/op, ms/op, dirty-set delta size, allocation count, etc.).
- **Sample size / iterations:** enough to be stable; specify.
- **Environment:** Rust toolchain, machine summary if relevant.

## Results

Show **every number**. Don't summarize before the table.

| Variant | Metric A | Metric B | Notes |
|---|---:|---:|---|
| baseline | … | … | |
| variant 1 | … | … | |
| variant 2 | … | … | |

## Interpretation

What do the numbers mean? Be specific. If a variant beat baseline, by how much, and is the difference within noise?

## Decision

What did we ship / abandon / file as a follow-up? Link to the commit, the dead-end file, or the concept doc that documents the decision.

## Cross-links

- Concept: [`../concepts/...`](../concepts/)
- Hypothesis answered: [`../hypotheses/H...`](../hypotheses/)
- Dead-end created (if any): [`../dead-ends/...`](../dead-ends/)
- Source code touched: [`../../crates/...`](../../crates/)

## Reproducibility

Exact command to re-run:

```
cd /path/to/marketingcubes-v2
... commands ...
```
