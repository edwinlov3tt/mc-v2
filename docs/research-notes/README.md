# research-notes/

**Distilled lessons from research, spikes, benchmarks, and surprise findings.**

Where [`../external-conversations/`](../external-conversations/) holds verbatim primary sources (LLM responses, vendor threads), this folder holds the *takeaways* — short, opinionated notes written for a future reader who needs the conclusion, not the transcript.

A research note is the right shape when:

- You investigated something (a benchmark, a library, a TM1 manual, a thread of advice from another model) and the conclusion is worth keeping.
- The conclusion will inform an ADR or report later but isn't itself a decision.
- The conclusion is broader than a single phase — it's a fact about the world or about the engine that future phases need to know.

If the finding is itself a decision (we chose X over Y), write it as an ADR in [`../decisions/`](../decisions/) instead.
If the finding is one phase's report (we shipped X with these gates), write it in [`../reports/`](../reports/) instead.

## Status

Six Phase 1A notes shipped 2026-05-01. Phase 1B is expected to add two more (criterion compatibility, hierarchy-clone overhead).

## Active notes

Phase 1A rationale that was previously scattered across the brief, the completion report, CLAUDE.md, and source comments — promoted here as self-contained notes for future phases:

- [`lazy-dependency-graph.md`](./lazy-dependency-graph.md) — why edges materialize on first read rather than at construction.
- [`dirty-propagation-as-per-write-delta.md`](./dirty-propagation-as-per-write-delta.md) — why §10.1 dirty-set assertions are framed as deltas, not absolutes.
- [`null-vs-zero-vs-nan.md`](./null-vs-zero-vs-nan.md) — the §7 rules, where they're enforced, what bugs they prevent.
- [`weighted-average-consolidation.md`](./weighted-average-consolidation.md) — why CPC / CVR / Close_Rate / AOV / COGS_Rate don't simple-sum, and the funnel-position weight chain.
- [`two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — derived-leaf cache + consolidated cache, dirty bit as invalidator, trace-bypass rule.
- [`snapshot-as-deep-clone.md`](./snapshot-as-deep-clone.md) — why Phase 1 snapshots are full clones rather than COW.

## Expected next notes

Phase 1B benchmarks are likely to produce two more research notes:

- **Criterion compatibility under Rust 1.78.** What we tried (specific version pins), what worked, what didn't, and what the closure conditions are.
- **Hierarchy clone overhead in consolidated reads.** Whether the per-read clone in `cube.rs::read_consolidated` is meaningful at Acme scale.

## How to write a research note

1. Copy [`../templates/research-note.md`](../templates/research-note.md).
2. One topic per file. Don't combine.
3. Lead with the conclusion in one sentence, then unpack the *why*.
4. Cross-link source code, ADRs, primary-source conversations, and reports.
5. Status: `active | superseded`. If superseded, link to the replacement note or ADR.
