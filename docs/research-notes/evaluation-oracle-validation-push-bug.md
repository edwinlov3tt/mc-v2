# The Evaluation Oracle Caught a 38% Error On Its First Production Use

**Status:** Validation evidence (strategic — records a concrete payoff of the evaluation-primitives track)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.8, 1M context)
**Cross-links:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) (the command that caught it), [built-in-evaluation-primitives.md](./built-in-evaluation-primitives.md) (the track's thesis), [ADR-0009](../decisions/0009-lnm-substrate-as-product-vision.md) (the LNM-substrate strategic framing this validates)
**Source incident:** claw-core EXP-055 (`claw-core/docs/reports/exp-055-simulate-adoption-and-push-correction.md`)

---

## One-sentence version

On the very first production use of `mc model simulate`, the deterministic
evaluation oracle caught a **38% overstatement** in claw-core's published
sports-betting bankroll numbers — an error that had silently propagated
through eight experiments — and the correction was **~500× larger** than
the subtle batch-vs-sequential timing issue we had spent five careful
amendments getting right.

This note records the incident as concrete evidence for *why the
evaluation-primitives track exists*. If anyone ever asks "was building
deterministic evaluation commands into Mosaic worth it," this is the
answer with a number attached.

---

## The thesis being validated

The evaluation-primitives track ([research note](./built-in-evaluation-primitives.md))
was built on a claim:

> Every new hypothesis currently requires a ~300-line Python script that's
> 80% boilerplate. The scripts are valuable but not composable, and — the
> deeper argument — **a hand-rolled Python script silently encodes
> assumptions that a deterministic, spec-reviewed oracle would surface.**
> Moving evaluation into Mosaic isn't just faster; it's *more correct*,
> because the math lives in one reviewed place instead of being
> re-implemented per experiment.

That last clause was the strategic bet. It was plausible but unproven —
"more correct" is easy to assert and hard to demonstrate. EXP-055
demonstrated it.

This also validates the broader LNM-substrate vision ([ADR-0009](../decisions/0009-lnm-substrate-as-product-vision.md)):
Mosaic as the deterministic, auditable brain that downstream consumers
trust *because* its math is reviewed once and reproducible. A consumer's
own Python had a bug; the substrate they pointed it at found the bug.

---

## What happened

**Context.** claw-core's MLB totals model publishes bankroll headlines —
e.g. "$1k → $2,962, +196% on 2025." Those numbers gate model-version
decisions (ship V1.1 or not). They were computed by a Python script that
walked a bet-record dataframe and tallied wins/losses from a `won` column
(0/1).

**The bug.** In totals betting, a game whose actual total lands *exactly*
on an integer line (e.g. line = 9.0, final = 9 runs) is a **push** — the
stake is returned, the bet is neutral, neither win nor loss. claw-core's
`won` column scored those pushes as **wins** for the UNDER side (24 of 26
such games in 2025). Because the model is UNDER-heavy, those phantom
push-wins compounded through the entire season's bankroll replay.

**The catch.** When claw-core ran the newly-shipped `mc model simulate`
on their real bet-record file, simulate's `--derive-pushes` flag
(reconstruct pushes from `actual_total == line`) surfaced the discrepancy
immediately:

| 2025 V1.0 bankroll | final | vs published |
|---|---|---|
| sequential, pushes-as-wins (the **published** headline) | $2,962 | — |
| batch, pushes-as-wins | $2,964 | +0.07% |
| **batch, push-accurate (correct)** | **$1,829** | **−38%** |

The error wasn't in the model. It wasn't in Mosaic. It was in the
hand-rolled scoring assumption baked into the Python `won` column — the
exact class of silent, per-script assumption the oracle thesis predicted
would bite.

---

## Why this is the track's payoff moment

**The correction dwarfed everything we'd been carefully amending.** ADR-0035
shipped with 17 amendments before this. Several were about getting
same-timestamp bet ordering exactly right (batch vs sequential replay) —
genuinely load-bearing, 45% of bets share a timestamp, and we spent
amendments A1, A17 and a SPEC-question round on it. That correction was
worth **~$2 (0.07%)** on this data.

The push correction — which fell out of a single flag the consumer ran
on a whim — was worth **−$1,133 (−38%)**. ~500× larger. The thing that
mattered most was not the thing we'd been most careful about; it was the
thing a deterministic oracle surfaced *by existing*.

That asymmetry is the lesson. Careful spec review catches the subtle
known unknowns. A deterministic oracle that re-derives the math from
first principles catches the **unknown unknowns** — the assumptions so
baked-in that nobody thought to question them. The Python script's author
never doubted the `won` column; it took a second, independent,
spec-reviewed implementation to expose it.

**It propagated through 8 experiments.** Because the `won` column was
upstream of everything, the same overstatement contaminated EXP-049/050/053
and the "59.68% WR / +13.93% ROI" headline (EXP-026/028). One bad
assumption, eight wrong numbers. A single reviewed oracle would have
prevented all eight; eight hand-rolled scripts reproduced the bug eight
times.

---

## What survived, and why that matters too

Crucially, claw-core's **relative conclusions survived the correction**:

- V1.1 still fails 2 of 3 walk-forward folds (the ship-blocker holds — in
  fact strengthens).
- NB > Gaussian by +5.78pp (a *difference* between two arms).
- Edge monotonicity holds.

These survived because **both arms of every comparison hit the same
pushes** — the error was a common-mode bias that cancelled in
differences. Only the *absolute* numbers were wrong.

This is the honest, important nuance: the oracle didn't invalidate
claw-core's modeling work, it corrected the *headline magnitudes*. The
model decisions were robust; the marketing numbers were inflated. For a
tool whose entire value proposition is "reproducible, defensible claims,"
correcting an inflated headline before it's published externally is
exactly the failure mode you want caught — and caught *internally, by your
own substrate*, not by a skeptical counterparty after publication.

---

## The design lesson that fed back into the product

The catch also exposed a *product* flaw, not just a consumer-data flaw:
simulate made push-accuracy **opt-in** (`--derive-pushes`) when it should
have been the **default** wherever derivable. The user had to ask for the
correct number. That's the silently-wrong-default footgun the whole
ADR-0035 amendment set guards against — and we'd left one in.

The fix ([ADR-0035 Amendment 18](../decisions/0035-phase-10f-model-simulate.md),
Phase 10F.1): push-accuracy becomes the default whenever the score columns
are present; `--no-derive-pushes` is the explicit opt-out for reproducing
historical published numbers. The oracle caught the consumer's bug *and*
the experience of catching it improved the oracle.

That feedback loop — consumer runs the tool → tool surfaces a bug → bug
exposes a default that should change → default changes → next consumer is
correct-by-default — is the track working as designed.

---

## The number to remember

> **38%.** First production use. One flag. Eight experiments corrected.
> ~500× the magnitude of the timing subtlety we'd spent five amendments
> on. The deterministic oracle earned its existence on day one.

---

## Caveats (honesty about scope)

- **One incident, one consumer, one domain.** This is a single strong data
  point, not a statistical claim about how often the oracle catches bugs.
  It is, however, the *first* production use — n=1 with a 38% hit rate is
  a remarkable first data point, even if the long-run rate is much lower.
- **The oracle didn't "know" pushes were mis-scored.** It surfaced the
  discrepancy because it implemented a *different, independently-specified*
  scoring path (4-state outcome with push derivation). The catch was a
  consequence of having two independent implementations disagree — which
  is exactly why a reviewed second implementation is worth more than a
  faster copy of the first.
- **The consumer did the right thing.** claw-core ran `--derive-pushes`,
  investigated the discrepancy, traced it to their `won` column, corrected
  8 experiments, and filed EXP-055. The oracle surfaces; the human
  resolves. Credit where due.

---

## For the strategy file

When the question is "why does Mosaic evaluate models instead of just
storing them" — or "why build CLI evaluation commands when Python already
works" — the answer is now concrete: *because on its first production use,
the deterministic evaluation path caught a 38% error in a real consumer's
published numbers that their hand-rolled Python had silently carried
through eight experiments.* Speed was the pitch; correctness was the
payoff.
