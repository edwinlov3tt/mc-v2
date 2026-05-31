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
- [`evaluation-oracle-validation-push-bug.md`](./evaluation-oracle-validation-push-bug.md) — validation evidence: `mc model simulate` caught a 38% overstatement in claw-core's published numbers on first production use. The concrete payoff of the evaluation-primitives track (and the LNM-substrate thesis): a deterministic, reviewed oracle catches the unknown unknowns a hand-rolled script silently carries.
- [`binary-size-and-deployment-split.md`](./binary-size-and-deployment-split.md) — the kernel+model layer is already 885 KB; the 55 MB `mc` monolith is ~40 MB bundled DuckDB. A capability-split (`mc` lite / `mc-data` / `mc-server`) along existing crate seams gets a ~5 MB evaluation binary. Pre-ADR; captured for when distribution actually matters (it doesn't yet).
- [`backtest-simulate-composition.md`](./backtest-simulate-composition.md) — the one clear next-phase signal after claw-core adopted backtest (EXP-056): sweep a knob → bankroll/drawdown surface, because per-bet-ROI-optimal ≠ bankroll-optimal (EXP-049). Un-defers ADR-0036 Amdt 4. Has a real unsolved mechanism (simulate reads an external record file a cube sweep can't change — 3 candidate bridges). Medium priority, not blocking; ADR when claw-core hits the wall.

## Expected next notes

Phase 1B benchmarks are likely to produce two more research notes:

- **Criterion compatibility under Rust 1.78.** What we tried (specific version pins), what worked, what didn't, and what the closure conditions are.
- **Hierarchy clone overhead in consolidated reads.** Whether the per-read clone in `cube.rs::read_consolidated` is meaningful at Acme scale.

## Proposals (not adopted; seeking validation)

These are exploratory shapes — drafted, not decided. Each one names the question it asks a future reader (or another model instance) to pressure-test before commitment. They deliberately sit outside the "Active notes" index so adopting one is an explicit move, not a drift.

- [`dual-fixture-claw-stress-test.md`](./dual-fixture-claw-stress-test.md) — *2026-05-01.* Should we use claw-core's NBA totals dataset as MC's second fixture (parallel to Acme), both as kernel stress-test and as a planning-workflow dogfood? Concrete cube sketch (7 dims, 13 measures, 5 rules) + ingest path + 4 candidate planning use cases + 6 honest red flags. **Status:** awaiting second-instance review.

## How to write a research note

1. Copy [`../templates/research-note.md`](../templates/research-note.md).
2. One topic per file. Don't combine.
3. Lead with the conclusion in one sentence, then unpack the *why*.
4. Cross-link source code, ADRs, primary-source conversations, and reports.
5. Status: `active | superseded`. If superseded, link to the replacement note or ADR.
