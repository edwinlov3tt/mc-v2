# Mosaic as an Evidence-Fusion / Decision Substrate for Agentic Systems

**Status:** ACTIVE — conceptually validated, foundation-gated (needs distribution-valued cells, Phase 11)
**Date:** 2026-06-06
**Author:** Mosaic PM (Claude Opus 4.8, 1M context) + project owner (vibing session)
**The one-line thesis:** Mosaic turns **stochastic LLM judgments into deterministic, auditable decisions** — by fusing scored signals (news, earnings, reports) + hard numbers through fixed rules into an uncertainty-aware, traceable output that recomputes as evidence changes.

---

## The reframe that started this

The owner asked whether Mosaic could power context/document management —
"sports betting and news, stocks and news and earnings reports" — finding
value across a lot of information.

The key disentangling: **that is not a retrieval problem, it's a
signal-fusion problem.** The hard part isn't *finding* the earnings
report (RAG/Vectorize does that). It's *combining* it with everything else
into a decision:

```
model edge (from the cube)            ← a number
+ news sentiment score                ← an LLM read the article, scored it 0.7 bullish
+ injury-report flag                  ← an agent extracted it, scored it
+ line movement                       ← a number
+ earnings surprise magnitude         ← scored from the 10-Q
──────────────────────────────────────
→ "bet / don't bet, and how confident"  ← traceable; recomputes when any signal updates
```

**That fusion is exactly Mosaic's shape:** multiple scored inputs →
deterministic rules → a derived decision → every value traceable to its
inputs → change one signal and only the affected decisions recompute.

The news never goes *into* Mosaic. The pipeline:
- **Retrieval (RAG/Vectorize)** finds the relevant news/earnings/reports
- **An LLM scores** the extracted content into numbers ("+0.7 bullish, 60% confident")
- **Mosaic fuses** those scores deterministically into an auditable decision

**Retrieval finds; the LLM judges; Mosaic makes the judgment auditable.**

---

## Why distribution-valued cells make this the killer app

A score from a messy source is *never* a clean number. "This news is
bullish" is `0.7 ± a lot`; an LLM's confidence IS a distribution. This is
the exact thing distribution-valued cells (the Phase 11 Bayesian
foundation) were designed for:

- the news score enters as a distribution, not a point
- it propagates through the fusion rules (the dependency graph already does this)
- the decision comes out as `P(this bet wins) = 0.55 [0.48, 0.63]`, with
  the band *honestly reflecting how uncertain the inputs were*
- `prob_above(edge, threshold)` decides the action on **confidence**, not
  point value

Mix hard numbers (line movement) with fuzzy LLM judgments (news
sentiment) and the output's uncertainty correctly widens for the fuzzy
inputs. **No spreadsheet, no RAG system, no vector DB does that.** That's
the differentiated capability — and it's why this note is gated on
distribution-valued cells: without distributions, fusion is just weighted
averaging of point estimates; *with* them, it's principled
uncertainty-aware decision-making.

---

## The breakthrough framing

Step all the way back:

> **Mosaic turns stochastic LLM judgments into deterministic, auditable
> decisions.**

The deep problem in *all* agentic systems: LLMs are non-deterministic, so
decisions built on them aren't reproducible or auditable. The fix is the
pattern Mosaic already embodies — the LLM produces structured scores; a
deterministic engine combines them by fixed rules; the decision is
traceable even though its inputs were LLM-generated. The agent supplies
*judgment*; Mosaic supplies the *auditable arithmetic*.

Same lesson as the 38% bug catch: a deterministic oracle catches what the
stochastic layer gets wrong, and lets you trace exactly which signal moved
the outcome. Already proven on betting math. Generalize it: **"evidence
fusion engine" is a new Mosaic domain** — neither marketing nor sports —
the universal shape of "fuse many scored signals into an auditable,
uncertainty-aware decision that recomputes as evidence changes."

---

## The boundary (what it is NOT)

- **NOT retrieval.** Finding the document is RAG/Vectorize. Mosaic sits
  *downstream* of retrieval.
- **NOT the scorer.** The LLM does the judging (article → sentiment
  score). Mosaic consumes the score.
- **NOT a vector DB competitor.** It's the deterministic decision layer
  that sits downstream of vector DBs — the category nothing occupies.

The clean stack: `retrieve (vector DB) → score (LLM) → fuse + decide +
audit (Mosaic)`.

---

## Dual-review finding (2026-06-06) — the differentiation is gated on TWO things, one unsolved

Codex + Claude Desktop both reviewed this note. Both verdicts: **MIXED,
leaning wishful** — and both landed the same sharpening:

- **As point-score fusion, this is "a spreadsheet/rule-engine with LLM
  inputs."** Not a category. The note already concedes this (the
  weighted-averaging admission below). Confirmed.
- **The differentiation is staked entirely on distribution-valued cells
  (Phase 11, unbuilt) — AND on a second thing the note undersold: LLM
  score CALIBRATION, which is nobody's solved problem.** Propagating an
  uncalibrated "0.7 bullish, 60% confident" vibes-number through flawless
  Bayesian math yields a "precisely-computed, beautifully-traced garbage
  interval" (Claude). The honest-uncertainty-band is the whole pitch, and
  it's honest about uncertainty it can't actually measure.
- **The uncertainty-propagation math isn't novel or Mosaic's** — PyMC,
  Stan, the `uncertainties` package do automatic error propagation through
  arbitrary expressions. And the note concedes Python still
  scores/fits, so Mosaic again contributes the *trivial* eval step.

**The strongest version both reviewers reached (sharper than this note):**
the differentiator is NOT "Mosaic fuses scores." It's that **deterministic
recompute + trace enables honest calibration/backtesting** — replay
historical scored decisions, check whether your 60%-confidence calls
actually hit 60%. The value is the *calibration/audit loop*, not the
fusion arithmetic. **And that loop is evaluation-track DNA** (deterministic
replay + trace — the same thing that caught the 38% bug). So even fusion's
real edge routes back to Mosaic's actual moat: deterministic-replay-with-
trace, which the evaluation track already ships.

**Status implication:** keep this ACTIVE but reframe the eventual ADR
around the *calibration/backtest loop*, not "uncertainty-aware fusion."
And it stays double-gated: needs distribution-valued cells (buildable) AND
good-enough LLM calibration (open research). Until both, it demos as a
spreadsheet. Don't build the fusion layer first; build the calibration
loop (which is closer to the evaluation track you already have).

## The honest constraints

1. **Garbage in, garbage out — worse, not better.** A confidence interval
   is only as honest as the input scores. Mosaic propagates uncertainty
   perfectly and can still be confidently wrong if the LLM's scores are
   bad. Calibration tooling (does my 95% actually cover 95%?) is
   load-bearing, not optional.
2. **Gated on distribution-valued cells.** Without them this is just
   point-estimate weighted averaging — useful but not differentiated. The
   differentiation IS the uncertainty propagation. So this waits on
   Phase 11.
3. **Python still scores / fits.** Mosaic fuses and evaluates; it doesn't
   run the LLM or fit the distributions. The Python-trains/scores-Mosaic-
   evaluates split holds.

---

## Relationship to the sibling note

This note and [`./graph-kernel-as-impact-substrate.md`](./graph-kernel-as-impact-substrate.md)
are two reframes of the same underlying realization — **Mosaic's kernel is
a general-purpose deterministic substrate, not a numbers tool.** The
impact-substrate note is about *the graph* (blast radius, trace) for
agentic dev/context. This note is about *the values* (scored signals,
fused with uncertainty) for agentic decisions. They share a spine:
deterministic recompute + traceability applied to LLM-era problems.

The impact-substrate note has a cheap spike that can run NOW (the blast-
radius primitive already exists). This one is foundation-gated (needs
distribution-valued cells). So: **spike the graph idea first; this fusion
idea matures alongside the Phase 11 Bayesian work.**

---

## Cross-links
- [`../distribution-valued-cells.md`](../distribution-valued-cells.md) — the hard dependency (Phase 11 foundation)
- [`../pymc-marketing-pattern-extraction.md`](../pymc-marketing-pattern-extraction.md) — the Bayesian primitives (`prob_above`, `mc model optimize`) this would use
- [`../evaluation-oracle-validation-push-bug.md`](../evaluation-oracle-validation-push-bug.md) — the deterministic-oracle-catches-stochastic-error lesson, generalized here
- [`./graph-kernel-as-impact-substrate.md`](./graph-kernel-as-impact-substrate.md) — the sibling reframe (graph, not values)
- owner's Ignite PRD — the retrieval layer (Vectorize) this sits downstream of

## Notes
- The differentiation lives entirely in the uncertainty. If this ever
  ships as point-estimate fusion, it's a worse spreadsheet. Distribution-
  valued cells are the whole point — don't build the fusion layer until
  the distribution foundation exists, or it'll demo as "weighted average
  with extra steps."
