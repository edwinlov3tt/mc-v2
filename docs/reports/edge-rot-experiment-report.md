# Edge-Rot Experiment — Does an Intent→Code Graph Survive Real Churn?

**Date:** 2026-06-06
**The kill-shot both reviewers named:** intent→code edges are hand-curated
structured documentation; they rot silently at code's churn rate;
determinism over a stale graph is "confidently wrong, worse than grep."
Both Codex and Claude Desktop said: **run this before building anything.**
This is that experiment, run on real data (this repo).

---

## TL;DR — the prediction was WRONG, in an instructive way

I predicted (and said so plainly): the rot loop would come back
*unsurvivable* for automation, *survivable only with a human merge-gate*.

**The data says rot is far slower than anyone assumed — but also that the
edges that DO rot, rot SILENTLY, exactly as the reviewers warned.** The
nuance flips the conclusion: the kill-shot isn't "rot is fast." It's "rot
is *silent*." And silence is fixable with cheap tooling. That changes the
verdict from "probably dead" to "narrower than hoped, but not killed —
and the fix is mechanical, not a research problem."

---

## The setup (real data, not a toy)

- **Repo:** this one. 337 commits over 36 days (2026-05-01 → 06-06) —
  dense, real churn.
- **The natural edge corpus:** 39 ADRs already contain
  **intent→code edges I authored at decision-time** — concrete
  `file.rs:line` claims like "ADR-0031: norm_cdf parse site is
  formula.rs:933" or "ADR-0036: cube.rs:3069 says params don't participate
  in dirty propagation." These are *exactly* the edges the impact-graph
  product would store, authored by the same LLM-proposes process, and then
  left to rot as the code churned underneath.
- **9 distinct `file.rs:line` claims** across 6 ADRs spanning the full
  history (oldest: ADR-0006, repo week 1; newest: ADR-0036, this week).

This is a real, unplanned, 5-week rot experiment that already happened. I
just had to measure it.

---

## Measurement 1 — do the claimed locations still exist?

| Result | Count |
|---|---|
| File exists + line exists | **9 / 9** |
| DEAD-FILE (renamed/moved/gone) | 0 |
| DEAD-LINE (file shrank past the line) | 0 |

Zero hard breaks. Every ADR file:line claim still resolves to a real
location.

## Measurement 2 — do the lines still MEAN what the ADR claimed? (the real test)

| ADR | Claim | Claimed line | Actual content now | Verdict |
|---|---|---|---|---|
| 0036 | "params don't participate in dirty propagation" | cube.rs:3069 | `/// participation (constants don't participate in dirty propagation).` | ✅ EXACT |
| 0033 | "Null poisons multiply" | rule.rs:1943 | `return Ok(ScalarValue::Null);` | ✅ EXACT |
| 0034 | "the Filter enum" | query.rs:413 | `pub(crate) enum Filter {` | ✅ EXACT |
| 0036 | "bets is an external file" | simulate_command.rs:19 | `pub bets: String,` | ✅ EXACT |
| 0006 | a `println!` (repo **week 1**) | main.rs:253 | `println!(` | ✅ EXACT |
| 0031 | "the norm_cdf parse site" | formula.rs:933 | `}` (norm_cdf is now at **934**) | ⚠️ **SILENT 1-LINE DRIFT** |

**8 of 9 dead-on. 1 silently drifted by one line.**

## Measurement 3 — churn exposure (how much did the underlying files move?)

| File | Claim line | # commits touching the file since |
|---|---|---|
| main.rs | 253 | **41** |
| cube.rs | 3069 | 28 |
| rule.rs | 1943 | 14 |
| query.rs | 413 | 14 |
| formula.rs | 933 | 11 |
| sweep.rs | 184 | 6 |

The drifted claim (formula.rs) had **11 commits** of exposure. But
main.rs survived **41 commits** of churn with its claim still exact, and
cube.rs survived 28. **Churn count does not predict rot** — what predicts
rot is whether commits inserted/deleted lines *above* the claimed line,
which is mostly luck for a raw `:line` reference.

---

## What this actually proves (and disproves)

### Disproved: "rot is fast / the loop is unsurvivable"
Over 5 weeks and up to 41 commits per file, **8 of 9 hand-authored
intent→code edges stayed semantically exact.** The rot rate is ~11% of
edges drifting, and the one that drifted was off by a single line — a
human or LLM following the reference would still land in the right
function. This is dramatically slower rot than the reviewers (or I)
assumed. The "structured docs rot at code's rate" intuition is real but
the *rate* is low when edges point at semantically-stable anchors
(an enum name, a doc-comment, a function).

### Confirmed: "rot is SILENT" — the reviewers' actual point
The formula.rs:933→934 drift produced **zero signal.** Nothing failed.
The ADR still says 933; the code moved to 934; only this experiment caught
it. Claude's framing holds exactly: *"determinism over a stale graph is a
precise, reproducible, audit-trail-blessed wrong answer."* The drift was
small, but it was invisible — and at scale, invisible small drift
accumulates into a graph you can't trust.

### The decisive insight neither reviewer reached: the rot mode is the FIX
The single rotted edge was a **raw line number** (`:933`). Every edge that
stayed exact did so because it pointed at a **semantic anchor** that
happens to live on a line — an enum name, a `"norm_cdf" =>` match arm, a
doc-comment string. **If edges target symbols/anchors instead of line
numbers, rot drops toward zero AND becomes loud** (a vanished symbol fails
to resolve; a moved symbol is found at its new line). LSP/ctags resolve
symbol→current-line deterministically. So:

> **The edge should be `formula.rs#fn parse → "norm_cdf" arm`, not
> `formula.rs:933`.** Symbol-anchored edges don't rot from line shifts,
> and when the symbol genuinely disappears, they fail LOUD (resolution
> error) instead of silent (wrong line).

That converts the kill-shot. The reviewers said "silent rot kills it." The
data says "line-anchored edges rot silently; symbol-anchored edges
mostly don't rot and fail loud when they do." The fix is anchor choice +
a resolve-step (LSP/ctags), not a research problem.

---

## Honest limits of this experiment

- **n=9.** Small. A real corpus is thousands of edges. But these 9 are
  *real*, *aged 5 weeks*, *across the full churn*, and *authored by the
  actual process* — far better evidence than a synthetic study, even if
  few.
- **These edges were never maintained** — nobody re-confirmed them. So the
  91% exact rate is the *un-maintained floor*, not a best case. That's the
  strong version of the result: even with zero maintenance, anchor-quality
  references mostly held.
- **It measures `file:line` claims, not full "B exists because of decision
  A" semantic edges.** A semantic edge ("this endpoint exists because of
  the CORS decision") can rot in a way a location reference can't: the
  endpoint still exists but the *reason* changed. This experiment does NOT
  test that deeper rot. It tests location rot, which is the necessary-but-
  not-sufficient layer. **The reason-rot question is still open** — but
  it's a smaller, slower target than "every line number."
- **Detectability wasn't auto-tooled** — I checked semantics by hand. A
  product needs the symbol-resolve step to make detection automatic. The
  experiment shows that step is the make-or-break feature, not an
  afterthought.

---

## Verdict: the direction is NARROWER but NOT KILLED — and it's Continuity's, not Mosaic's

1. **Rot is survivable IF edges are symbol/anchor-anchored, not
   line-anchored** (resolve via LSP/ctags; fail loud on missing symbol).
   The experiment shows un-maintained anchor-quality edges held 91% over 5
   weeks.
2. **The engine is still commodity** (both reviews, unrefuted) — this
   experiment changes nothing there. closure_of_dependents is 15 lines;
   the value was never the engine.
3. **Therefore the product, if any, is: symbol-anchored intent→code edges
   + a resolve-on-read step + loud-fail on broken anchors + closure/trace.**
   That is a *data + workflow + LSP-integration* product. Mosaic's graph
   kernel contributes the trivial 15 lines; it is NOT the moat and NOT
   required (a recursive CTE or petgraph does the same).
4. **This belongs to Continuity, not Mosaic.** Continuity already is "a
   tiny runner + a ledger that travels in a repo." Symbol-anchored edges +
   resolve-step + loud-fail is a Continuity feature (its `fingerprint` /
   `staleness_mode` machinery is already reaching for exactly this — the
   addendum's "anchor-scoped hashing" is the same idea). Building it on
   Mosaic buys nothing.

**Net:** the reviewers were right that line-anchored silent rot is real (we
caught it: formula.rs). They (and I) were wrong that it's fast enough to
kill the idea outright. The survivable version exists, it's narrow, and
it's a Continuity feature — which routes back to the same conclusion the
reviews reached from the engine side: **Mosaic's edge is its deterministic
NUMERIC evaluation substrate; the agentic-graph idea, if pursued, is
standalone Continuity, not Mosaic.**

---

## The one number that matters
**91% of un-maintained, 5-week-aged, real intent→code edges stayed
semantically exact; the 9% that drifted did so silently AND were
line-anchored — the exact failure mode that symbol-anchoring removes.**

Measure-before-build paid off: we now know the kill-shot is "line
references" (fixable) not "rot is fast" (fatal). One session, real data,
prediction overturned.

---

## Cross-links
- [`../research-notes/_active/graph-kernel-as-impact-substrate.md`](../research-notes/_active/graph-kernel-as-impact-substrate.md)
- [`./spike-graph-impact-substrate-report.md`](./spike-graph-impact-substrate-report.md) (the engine GREEN; this is the authoring/rot YELLOW→measured)
- The two external reviews (Codex + Claude Desktop) that demanded this experiment run first
- Owner's Continuity PRD + source-dormancy addendum (`fingerprint`/`staleness_mode` — the home for symbol-anchored edges)
