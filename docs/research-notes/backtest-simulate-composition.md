# backtest × simulate: Sweeping a Knob into a Bankroll Surface

**Status:** Research note (pre-ADR; the one clear next-phase signal from claw-core's EXP-056 backtest adoption)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.8, 1M context)
**Un-defers:** [ADR-0036](../decisions/0036-phase-10c-model-backtest.md) Amendment 4 (`--simulate` objective, deferred from 10C v1)
**Source:** claw-core EXP-056 (`claw-core/docs/reports/exp-056-backtest-adoption-and-wr-rebaseline.md`), Q2 + Q4
**Cross-links:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) (simulate), [evaluation-oracle-validation-push-bug.md](./evaluation-oracle-validation-push-bug.md) (the 38% catch this lineage started with)

---

## The demand, and why it's real (not nice-to-have)

`backtest` (10C) sweeps a knob and reports a metric surface built from
order-independent reductions (ROI = `ratio(pnl,stake)`, win rate, RMSE,
…). `simulate` (10F) replays bets chronologically and reports
path-dependent metrics (final bankroll, max drawdown, recovery). They are
the order-independent and path-dependent halves of evaluation.

claw-core's EXP-049 proved these two halves can **disagree about which
knob is best**:

> The per-line edge threshold won on per-bet ROI (`ratio(pnl,stake)`) but
> **lost 30-71% of bankroll** compounded vs the flat threshold. Per-bet-
> ROI-optimal ≠ bankroll-optimal.

So a `backtest` that optimizes a reduction metric can **recommend the
wrong knob** for anyone who actually compounds a bankroll. The fix is the
composition both reviewers and claw-core independently named: at each
grid point, run `simulate` and surface its path-dependent metrics
(final_bank, max_drawdown) as the backtest objective.

This was ADR-0036 Amendment 4's deferral ("`--simulate` objective doesn't
function as designed AND it's the only betting vocab in a domain-neutral
command"). claw-core's EXP-056 un-defers it with a concrete, evidenced
need — and ranks it **medium priority, not blocking** (they have a
workaround: sweep with backtest, then confirm the top candidates in
simulate manually).

---

## Why it wasn't built in v1 (the unsolved mechanism)

Amendment 4 deferred `--simulate` for a real reason, not just scope:

**`simulate` consumes an external bet-record file (`--bets`), not the
cube.** A `backtest` grid cell applies a transient *cube* override
(a swept `param`/`coef`/`input`). That cube change does not propagate to
an external parquet of bet records. So the naive composition — "run
simulate at each grid cell" — replays **identical records every cell** →
identical bankroll every cell → a flat, useless surface.

For the composition to mean anything, the swept knob has to change what
simulate replays. There are three candidate bridges, and choosing among
them is the ADR's central question:

### Bridge A — Swept value *filters* the records
The knob is a bet-inclusion threshold (EXP-033/049's case): sweeping
`edge_threshold` changes *which* records qualify, not their values.
simulate replays the filtered subset per grid cell. Clean for
threshold-style knobs; doesn't cover knobs that change *predictions*
(a coefficient sweep changes μ → changes p_bet_side → changes every
record's stake and outcome-probability, which a static record file can't
express).

### Bridge B — Regenerate records from the cube per grid cell
At each grid cell, the cube (with the swept override applied) *produces*
the bet records — predicted mean → p_over → edge → stake → outcome vs
actual — then simulate replays them. This handles coefficient/param
sweeps that change predictions, because the records are recomputed from
the overridden cube. But it requires the cube to carry the actuals +
the bet-construction logic as derived measures, and it's a much bigger
mechanism (the cube becomes a record generator, which is closer to
Phase 10E walk-forward than to a CLI composition).

### Bridge C — Hybrid: backtest emits per-cell record sets, simulate consumes
backtest, at each grid cell, materializes the cell's bet records (via
Bridge A filter or Bridge B regeneration) to a transient jsonl, then
invokes simulate's engine on it. The composition is explicit and
debuggable (you can inspect each cell's records), at the cost of I/O per
cell.

**The note's tentative lean: Bridge A for v1** (filter-style knobs cover
EXP-033/039/045/049 — the threshold family, which is the demonstrated
demand), with Bridge B explicitly deferred to (or merged with) Phase 10E
walk-forward, where cube-as-record-generator is the native shape. This
keeps the first version a true composition and avoids turning backtest
into a training/generation engine. But this is the open question the ADR
must settle — see below.

---

## The domain-neutrality tension (Amendment 4's other reason)

Amendment 4 also noted `--simulate <sizing-spec>` is the **only betting
vocabulary** in an otherwise domain-neutral command's surface. The
backtest engine ships zero domain metrics; bolting a Kelly-sizing string
onto it reintroduces betting-shape.

Two ways to keep the composition without the leak:
1. **Generic "external evaluator" seam.** backtest can pipe each cell's
   filtered/regenerated record set to *any* registered path-dependent
   evaluator, of which simulate (bankroll) is one. A forecasting domain
   could register a different sequential evaluator (e.g. a rolling-origin
   cumulative-error replay). The seam is "per-cell sequential
   evaluation," not "Kelly sizing."
2. **Keep it betting-explicit but isolated.** Accept that bankroll
   simulation IS a betting concept, gate it behind `--simulate`, and
   document that backtest's *core* is domain-neutral while `--simulate`
   is a domain-specific objective plugin. (simulate itself already
   legitimately carries betting vocab per ADR-0035.)

The generic seam (1) is more principled and preserves the multi-domain
mandate; the explicit version (2) is simpler and ships faster. Another
question for the ADR.

---

## What a v1 might look like (sketch, not commitment)

```
mc model backtest mlb-totals.yaml \
  --unit games \
  --sweep "param:edge_threshold=0.0:0.20:0.01" \
  --holdout "Time == 2025" \
  --simulate "quarter_kelly:cap=0.025,shrink=0.02" \
  --bets-from filter:Abs_Edge_NB \     # Bridge A: swept threshold filters records
  --objective final_bank --goal maximize \
  --emit-grid bankroll_surface.jsonl
```

The surface: edge_threshold × {final_bank, max_drawdown, roi_per_bet} —
so you can SEE that the per-bet-ROI-optimal threshold and the
bankroll-optimal threshold differ (the EXP-049 lesson, now a one-call
artifact instead of a two-step manual confirm).

---

## Scope estimate (rough)

If Bridge A (filter-style): ~2-3 sessions. It composes two shipped
engines (backtest grid + simulate replay) with a per-cell record-filter
bridge. The new code is the bridge + threading simulate's config through
backtest's cell loop. Bridge B (regeneration) is larger and overlaps
Phase 10E — defer.

The MC-objective rule from ADR-0036 Amendment 8 carries forward: the
objective is the deterministic single-path metric; Monte Carlo bands are
reportorial only (picking "best" on a resampled band overfits the seed).

---

## Open questions for the eventual ADR
1. **Which bridge?** A (filter), B (regenerate), or C (hybrid)? The lean
   is A for v1, B → Phase 10E. Confirm A covers enough of the demand
   (it covers the threshold family; does claw-core need coefficient-sweep
   → bankroll, which needs B?).
2. **Domain-neutral seam or betting-explicit plugin?** Generic
   "per-cell sequential evaluator" vs an explicit `--simulate` betting
   objective. The mandate favors the seam; speed favors explicit.
3. **Relationship to Phase 10E.** Bridge B (cube regenerates records) is
   basically walk-forward's per-fold evaluation. Should backtest×simulate
   and 10E be designed together so the record-generation format is shared?
4. **The push-accuracy default (ADR-0035 Amdt 18) must flow through** —
   per-cell simulated bankroll must auto-derive pushes, or every cell
   re-inherits the 38% error this whole lineage started by catching.

---

## Recommendation: capture now, ADR when claw-core pulls

claw-core called this **medium priority, not blocking** — they have a
workaround (sweep then confirm in simulate). And it has a genuine
unsolved mechanism (the records bridge) that deserves a dual review, not
a rushed build. So: this note captures the design space; the ADR gets
written when claw-core actually hits the wall (i.e., when the
sweep-then-manually-confirm loop becomes painful enough that they ask for
the one-call version).

That's the demand-driven discipline holding: a strong, evidenced signal
that's explicitly non-urgent becomes a *filed design*, not an immediate
phase. The evaluation track is at ~90% of claw-core's workflow (their
number); this is the last 10%, and it's correctly the next thing to build
— just not necessarily now.

---

## The smaller rough edges (cheap, can batch into a 10C.2 patch)

claw-core's EXP-056 Q5 flagged four, all minor:

1. **Better diagnostic for a non-measure objective.** Using a parquet
   column name (`abs_edge_pp`) instead of the cartridge measure
   (`Abs_Edge_NB`) errors as "objective Null in every cell" — masks the
   root cause. A "`abs_edge_pp` is not a measure in this cartridge; did
   you mean `Abs_Edge_NB`?" diagnostic (Levenshtein-nearest measure
   name) would save a round-trip. **Real fix, small.**
2. **Cookbook: the threshold-as-Null-gated-measure pattern.** EXP-033/
   039/045 (the threshold family) need a cartridge pattern where
   `direction_correct` is gated to Null for non-bets so count/mean
   exclude sub-threshold games — because a bet-inclusion cutoff isn't a
   model param. claw-core figured it out; document it so the next
   consumer doesn't have to. **Cookbook section.**
3. **Cookbook: a backtest JSON schema example.** The shape is clean
   (`grid[]` + per-cell `sweep_values`/`metrics` + top-level `best`); one
   worked schema example in the cookbook helps codegen consumers.
4. **`--dry-run` praised** — no change, just noted it landed well.

Items 1-3 are a tidy ~half-session 10C.2 polish patch (one diagnostic +
two cookbook sections), independent of the backtest×simulate work. Worth
doing before the next big phase since they're friction claw-core hit on
real adoption.

---

## Notes
- This note un-defers ADR-0036 Amendment 4 but does NOT supersede it —
  the deferral was correct for 10C v1; this records that the deferred
  thing now has demand + a design space, pending the wall.
- The lineage is worth seeing whole: the oracle caught a 38% bankroll
  error (push bug) → forced a WR re-baseline (59.68%→57.55%) → and the
  same EXP-049 path-dependence lesson now drives the one remaining
  feature. Every step is the deterministic-evaluation thesis compounding.
