# Review Request — ADR-0036: Phase 10C `mc model backtest`

**For:** dual external review (Claude Desktop + GPT-5.1, high-effort thinking)
**ADR under review:** [`docs/decisions/0036-phase-10c-model-backtest.md`](../decisions/0036-phase-10c-model-backtest.md)
**Status:** Proposed (not yet accepted; this review gates implementation)
**Date:** 2026-05-27

---

## How to review

Read the full ADR (~350 lines). Respond **accept / accept-with-amendments
/ reject** + specifics, amendments as a numbered copy-pasteable block.
The prior five ADRs in this track shipped with 7/7/7/12/16/17 amendments
after this pass — real bugs every time (a wrong scipy fixture, a
push-default footgun that hid a 38% error). Be adversarial.

**The load-bearing concern this time is the MULTI-DOMAIN MANDATE.** The
project owner explicitly asked that backtest work across domains, not
just sports betting. grade and simulate were claw-core-driven and
simulate legitimately carries betting vocabulary. backtest must NOT. Push
hardest on: does any betting assumption leak into the engine? Is the
"swept knobs in, metric surface out" core truly domain-neutral? Would a
marketing/forecasting/finance cartridge use this command unchanged?

---

## Context

**Mosaic** = a Rust deployment-agnostic kernel that *evaluates* fitted
models over cubes (Python trains, Mosaic evaluates). The evaluation track
adds `mc model` subcommands replacing hand-rolled Python experiment
scripts.

**Shipped:**
- 10A metrics library — `std_over`/`var_over`/`count_over`/`wilson_ci_*`,
  sample variance (ddof=1).
- 10B `grade` — segmented holdout evaluation, grouped map-reduce,
  ORDER-INDEPENDENT. 9-reduction vocabulary (count/mean/sum/ratio/std/
  min/max/wilson_lower/wilson_upper), a `Filter` holdout grammar
  (dimension pins + measure predicates, float-`==` guarded).
- 10F `simulate` — chronological bankroll replay, PATH-DEPENDENT,
  consumes a bet-record file. Carries betting vocabulary (Kelly,
  win/loss/push). Caught a 38% error in claw-core's published numbers on
  first use.

**This ADR (10C backtest)** = parameter sweep × holdout evaluation. At
each value of a swept parameter, run the FULL grade-style evaluation;
report the metric surface; flag the best setting. Composes grade
(per-grid-point evaluation) + sweep (the axis mechanics). Replaces 7
claw-core scripts (EXP-021/026/032/033/039/042/044/045).

---

## The 8 decisions (read the ADR for rationale)

1. Command shape — `--sweep <axis> --holdout <filter> --metric <expr>
   --objective <m> --goal max|min`, reusing grade's Filter + 9-reduction
   vocab + group-by/bucket wholesale.
2. **Three domain-neutral axis kinds:** `param:` (a `parameters:` scalar —
   the primary, most-neutral knob), `coef:` (fitted-model coefficient,
   absolute or `Nx` multiplier), `input:` (an Input-measure value at a
   coord, transient). Multi-axis → cartesian grid, `--max-grid` cap.
3. At each grid cell: apply transient overrides, run the full holdout
   eval, record. Metric surface out. The domain-neutral core.
4. **Metrics: the 9-reduction vocabulary over author-named measures, OR
   an opt-in `--simulate` objective** for path-dependent quantities
   (bankroll/drawdown). The engine ships NO domain metric — betting
   writes `roi=ratio(pnl,stake)`, marketing writes
   `roas=ratio(revenue,spend)`.
5. `--objective`/`--goal` picks the best grid cell; omitted → full
   surface. Engine doesn't know good-high vs good-low; user declares it.
6. Output: surface table + JSON + `--emit-grid` jsonl.
7. CLI-only, mc-cli impl, zero kernel change (composes existing override
   mechanics).
8. Deterministic grid enumeration; seed required+threaded when
   `--simulate` + Monte Carlo.

---

## Pressure-test questions

### Q1 — Is the engine TRULY domain-neutral? (the mandate)
Walk the command surface for any betting assumption. The axis kinds
(param/coef/input), the metric vocabulary (generic reductions over author
measures), the objective (user-declared goal direction) — is any of it
sports-shaped? Would a marketing MMM cartridge (sweep adstock decay →
MAPE surface) or a forecasting cartridge (sweep smoothing α → RMSE) use
backtest UNCHANGED, or are there hidden betting-isms? The ADR claims AC
#15 (mandatory non-betting test) + AC #22 (dual cookbook example) are the
guardrails — are they sufficient, or does neutrality need more teeth?

### Q2 — Is the three-axis-kind taxonomy complete?
param / coefficient / input. Is there a fourth swept-quantity class a
non-betting domain needs? E.g.: a categorical/enum parameter (not a
numeric range)? A date-window sweep (rolling holdout)? A structural toggle
(feature on/off)? Or do those decompose into the three?

### Q3 — `--simulate` objective: v1 or defer?
Decision 4(b) lets backtest use simulate's path-dependent metrics
(bankroll/drawdown) as the per-grid-point objective. This is the ONLY
betting-flavored part of backtest. Options: (a) ship it v1 (claw-core's
EXP-033 threshold→ROI sweep wants it), (b) defer it — keep v1 purely
reduction-based + perfectly domain-neutral, add the simulate source
later. Does including it in v1 compromise the multi-domain spine, or is
opt-in enough isolation?

### Q4 — Lift grade's engine, or call it as a boundary?
backtest runs grade's evaluation N times. Decision/Step 0 proposes
lifting grade's reduction engine into shared code both call. Is that the
right refactor (vs backtest calling grade as a subprocess, or duplicating
the reduction logic)? Shared-code is cleanest but couples two CLI verbs
through a common module — acceptable?

### Q5 — backtest vs walk-forward boundary
backtest sweeps parameters of an ALREADY-FITTED model (no refit).
Walk-forward (10E) retrains per fold (Python). Alt 5 defers refit to 10E.
Is that boundary clean, or will users expect backtest to retrain (and be
surprised it sweeps a fixed model's parameters)? Is the naming
("backtest") misleading given it doesn't refit?

### Q6 — Grid explosion + cartesian-only
`--max-grid 1000` default; multi-axis is cartesian product only (no
"zip"/parallel-axes mode). Is 1000 right? Hard-error vs confirm-prompt on
explosion? Does any script need zip mode (sweep two axes in lockstep, not
their product)?

### Q7 — Anything missing for the 7-script family?
EXP-021 (α sweep), 026 (stacked variants), 032 (NB α robustness), 033
(optimal threshold), 039 (threshold × line), 042 (coef stress), 044 (OOS
coef), 045 (per-line × season). Walk each — can backtest express it? 026
(stacking multiple variant configs) is the one I'm least sure decomposes
into a sweep — does it?

---

## What NOT to relitigate
- The 10A primitives / reduction vocabulary (ADR-0033)
- grade's design + Filter grammar (ADR-0034)
- simulate's design + push-accuracy default (ADR-0035 incl. Amdt 18)
- Python-trains-Mosaic-evaluates / no-refit-in-Mosaic (ADR-0025)
- CLI-only for batch analytics (ADR-0034/0035 precedent)

---

## Output format requested
```
VERDICT: accept | accept-with-amendments | reject
[If amendments:] numbered, copy-pasteable.
Per-question: Q1...Q7
Multi-domain verdict: is the engine truly domain-neutral? (the mandate)
Biggest risk if shipped as-is: ...
What's well done: ...
```
