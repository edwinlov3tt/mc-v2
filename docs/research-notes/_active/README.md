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
| [`graph-kernel-as-impact-substrate.md`](./graph-kernel-as-impact-substrate.md) | Can Mosaic's dependency-graph kernel (blast-radius + trace) power deterministic impact analysis / agentic context, not just numbers? | **The spike** — model a tiny project's intent→code edges as a Mosaic graph, change one node, show exact blast radius |
| [`evidence-fusion-decision-substrate.md`](./evidence-fusion-decision-substrate.md) | Can Mosaic fuse scored LLM judgments (news, earnings, reports) + hard numbers into auditable, uncertainty-aware decisions? | Depends on distribution-valued cells (Phase 11); validated conceptually, awaiting that foundation |
