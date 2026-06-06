# research-notes/_active/

**What we are actively figuring out right now.** Live exploration — not yet a decision, not yet shelved.

A note lives here while there is an open question we are running an experiment or building a spike to answer. When the question is settled (the spike returns a verdict, the idea is adopted into an ADR, or it's killed), the note moves to [`../_resolved/`](../_resolved/) with a one-line outcome stamped at the top.

This is distinct from the main [`../`](../) research-notes folder, which holds settled, durable reference notes (the "here's a fact about the world / the engine" kind). `_active/` is the working set — the things on the bench.

## Lifecycle

```
_active/   →  experiment / spike / dual-review  →  _resolved/
(open question)                                    (verdict stamped, outcome known)
```

When you move a note to `_resolved/`, prepend a status line:
`**RESOLVED (YYYY-MM-DD):** <one-line outcome — adopted into ADR-NNNN / killed because X / spiked GREEN, building Y>`

## Currently active

| Note | The open question | Next move |
|---|---|---|
| [`number-validation-harness.md`](./number-validation-harness.md) | Can Mosaic catch an agent's plausible-but-wrong analytics numbers (deterministic recompute + trace)? | **POSITIVE — tested 2026-06-06 on real Acme data.** Caught the weighted-avg CPC trap (correct 1.5202381 vs an LLM's clean-but-wrong simple average). The 38% bug catch as a discipline. Next (demand-gated): a thin `mc report-check` skill, not a kernel phase. The one bucket of the AI-eng list where Mosaic has a *demonstrated* edge. |
| [`evidence-fusion-decision-substrate.md`](./evidence-fusion-decision-substrate.md) | Can Mosaic fuse scored LLM judgments + hard numbers into auditable, uncertainty-aware decisions? | **Double-gated** (dual review 2026-06-06): needs distribution-valued cells (Phase 11, buildable) AND LLM calibration (open research). Reframe the eventual ADR around the *calibration/backtest loop* (evaluation-track DNA), not "fusion." Until both gates clear, it's a spreadsheet — don't build yet. |

## Recently resolved (moved to `_resolved/`)

- **graph-kernel-as-impact-substrate** (2026-06-06) — engine spike GREEN,
  but dual review + edge-rot experiment concluded: engine is commodity
  (15-line BFS, not a moat), survivable product is symbol-anchored edges
  and belongs to **Continuity, not Mosaic**. Mosaic's real edge is its
  deterministic numeric evaluation substrate. See [`../_resolved/graph-kernel-as-impact-substrate.md`](../_resolved/graph-kernel-as-impact-substrate.md).
