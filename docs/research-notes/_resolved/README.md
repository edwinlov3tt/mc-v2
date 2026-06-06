# research-notes/_resolved/

**Ideas we explored and settled.** Each note here started in
[`../_active/`](../_active/) with an open question, got an experiment /
spike / review, and reached a verdict. The verdict is stamped at the top
of each note; the body preserves the original exploration for the audit
trail.

"Resolved" does NOT mean "built." It means *the open question is
answered* — which is just as often "killed / rerouted / proven commodity"
as "green-lit." A cheap kill is a successful outcome: it's a product not
built on sand.

## Resolved

| Note | Verdict | Outcome |
|---|---|---|
| [`graph-kernel-as-impact-substrate.md`](./graph-kernel-as-impact-substrate.md) | **Rerouted, not Mosaic's** (2026-06-06) | Engine spike GREEN, but dual external review (Codex + Claude Desktop) proved the blast-radius engine is commodity (bazel/Neo4j/SQLite-CTE/petgraph all do it; `closure_of_dependents` is ~15 lines). An edge-rot experiment on real repo data overturned the "rot is fatal" fear (8/9 un-maintained 5-week-old edges stayed exact) but confirmed line-anchored edges rot *silently* — the fix is symbol-anchoring + LSP resolve. The survivable product is narrow and belongs to **Continuity, not Mosaic**. Mosaic's real edge is its deterministic numeric evaluation substrate. |

## The takeaway that came out of this batch

Across the spike + two reviews + the rot experiment, one conclusion kept
recurring from every angle: **every idea that had a genuine edge traced
back to deterministic-replay-with-trace** — which is exactly what the
evaluation track (grade / simulate / backtest) already ships, and what
the 38% bug catch validated. The "Mosaic's graph kernel as an agentic
substrate" detour was real engineering but a commodity moat; the numeric
evaluation substrate is the actual product. Build that; pursue the graph
idea, if at all, as standalone Continuity.
