# Mosaic as a Number-Validation Harness for Agentic Analytics

**Status:** ACTIVE — quick test run 2026-06-06, **positive result on real data**
**Date:** 2026-06-06
**Author:** Mosaic PM (Claude Opus 4.8) + project owner
**The one-line thesis:** When an agent writes an analytics report, the
numbers are the part most likely to be plausibly-wrong and least likely
to be caught. Mosaic recomputes them deterministically + traces them —
catching the clean-looking wrong number a human skims past. This is the
38% bug catch generalized into a development discipline.

---

## Why this one (and not the rest of the AI-eng list)

The owner asked which agentic-development approaches Mosaic could aid,
listing the full modern AI-engineering surface (KV cache, quantization,
batching, RAG retrieval, routing, guardrails, evals, observability…).

**Of that entire list, exactly ONE bucket is where Mosaic has a
*demonstrated, differentiated* edge: evals / grounding / "validate the
agent's numbers."** Everything else is inference-stack or retrieval
territory Mosaic doesn't touch (see the triage table at bottom). So this
note tests the one with real signal and explicitly bounds the rest.

The connection to what's already proven: claw-core's 38% bug catch was
*exactly this* — an LLM (their Python) produced a plausible bankroll
number, Mosaic's deterministic recompute caught it was wrong, and the
trace showed why. That wasn't a betting fluke; it's the general shape of
"agent does analysis → Mosaic validates the arithmetic."

---

## The test (run on the real Acme cube, 2026-06-06)

**The trap class:** weighted-average consolidation. Per CLAUDE.md §2.10,
Acme's `CPC` does NOT simple-average up a hierarchy — it spend-weights
(`sum(spend)/sum(clicks)`). This is the canonical number an LLM gets
wrong: asked for "the Q1 Paid_Search Florida CPC," an agent reads the CPC
column and averages it, or divides totals naively.

**What Mosaic computes (ground truth, deterministic):**
```
(Baseline, Working, Q1_2026, Paid_Search, Florida, CPC) = 1.5202381
```

**The tell that proves it's the trap:** that value has a non-terminating
tail (`1.5202381…`). A *simple* average of the input CPCs (which are all
of the clean form `1.50 + 0.05c + 0.02m`) would terminate cleanly
(`1.52`, `1.535`). The messy tail is the signature of spend-weighting
(`Σspend / Σclicks`). **The correct number is the "ugly" one; the LLM's
clean simple-average is the wrong one — and it looks MORE trustworthy
because it's round.** That's the danger: the wrong answer is the
plausible-looking one.

**What an agent would plausibly report:** ~1.52–1.55 (simple mean of the
CPC column) — close enough to the right answer that no human skimming the
report catches it. Off by method, not by a flagrant amount. The worst
kind of wrong: invisible.

**What Mosaic adds beyond the number — the trace** (the audit an LLM
can't fake):
```
CPC 1.5202381
  └─ Revenue = 3066.67 (Rule: Mul)
       ├─ Customers = 15.33 (Rule: Mul)
       │    ├─ CPC = 1.50 (Input)
       │    └─ CVR = 0.02 (Input)
       └─ AOV = 200.00 (Input)
```
Every derived number chains back to the inputs and the rule that produced
it. An agent can be *told* not just "your CPC is wrong" but "the correct
one is spend-weighted, here's the input chain."

**Result: POSITIVE.** Mosaic catches the plausible-wrong analytics number
on real data, deterministically, with a trace. The edge is demonstrated,
not hypothesized.

---

## What this could become (the development discipline)

The owner's framing — "validating the agent's numbers when doing analysis
for generating analytics reports" — is the real product shape:

**Agent-writes-report, Mosaic-checks-arithmetic loop:**
1. Agent drafts an analytics narrative ("CPC rose to 1.52 in Q1 driven
   by Paid Search…")
2. Each quantitative claim is a coordinate query against the cube
3. Mosaic recomputes it deterministically; a mismatch beyond tolerance is
   a flagged hallucinated number
4. The trace tells the agent *why* the real number differs → it self-
   corrects with the correct method, not a reworded guess

This is **"harness-driven development" applied to analytics** (the owner's
phrase): the harness isn't unit tests, it's a deterministic numeric oracle
the agent's prose is checked against. It generalizes past betting/
marketing to any domain where an agent reports numbers it derived: finance
summaries, forecast write-ups, KPI dashboards, earnings analysis.

The Mosaic primitives this needs ALL EXIST today: `query` (recompute a
coord), `read_with_trace` (the why), golden tests (pin known-good),
`grade`/`backtest` (segment-level checks). The "report validator" is a
thin orchestration over them — closer to a plugin/skill than a kernel
phase.

---

## Honest bounds (where this stops)

- **Mosaic validates numbers it can COMPUTE** — i.e. numbers that are
  cells/derivations in a cube. It can't validate a number the agent
  pulled from outside the model (a cited stat from a PDF). That's
  retrieval-grounding (RAG's job), not Mosaic's.
- **The cube has to exist.** Validating "the agent's analytics" presumes
  the analytics live in a Mosaic model. For ad-hoc "agent did some math in
  its head," there's nothing to check against. The win is for *modeled*
  domains — which is exactly where Mosaic already is.
- **Not novel as "recompute a number"** — a spreadsheet recomputes too.
  The edge is the *combination*: deterministic recompute + dependency
  trace + weighted/hierarchical correctness (the §2.10 traps) + it's
  driveable by an agent via the CLI/MCP surface. The trace is the part a
  spreadsheet doesn't give an agent.

---

## Triage of the rest of the owner's AI-engineering list

So the breadth question is settled — where Mosaic helps, where it doesn't:

| Area | Mosaic relevance |
|---|---|
| **Evals: golden sets, regression, LLM-as-judge** | ✅ **STRONG** — golden tests + deterministic recompute IS this for numbers; the demonstrated edge |
| **"Validate the agent's numbers" / grounding numerics** | ✅ **STRONG** — this note; the report-validator loop |
| **Harness-driven development** | ✅ **REAL** — the deterministic oracle as the harness; agent self-corrects against it |
| **Drift from an ADR (the owner's example)** | 🟡 **resolved elsewhere** — that's the graph-impact idea → Continuity, not Mosaic (see `_resolved/`) |
| Structured-output validation, schema repair | 🟡 adjacent — Mosaic's model-schema + diagnostics validate *cube* YAML, not arbitrary LLM JSON |
| RAG: chunking, embeddings, rerank, freshness | ❌ not Mosaic — vector DB territory (Vectorize), boundary already drawn |
| KV cache, paged attention, continuous batching | ❌ not Mosaic — inference-stack, no surface area |
| Quantization (INT8/4, FP8, AWQ, GPTQ), distillation | ❌ not Mosaic — except the *narrow* future tie: compressing sample-valued cells (see distribution-valued-cells note), not the kernel |
| Speculative decoding, prefill/decode latency | ❌ not Mosaic |
| Model routing, fallback, degraded-mode UX | ❌ not Mosaic |
| Prompt vs semantic caching, cache safety | ❌ not Mosaic (Mosaic has a *value* cache, unrelated to LLM caches) |
| Observability: traces/spans/tokens/cost/drift | 🟡 partial — Mosaic *traces numeric derivations*; not LLM-call observability |
| Safety: prompt injection, data leakage, multi-tenant isolation | 🟡 adjacent — ADR-0026 capability grants / Grout are about *cube* access, not LLM safety |
| Cost attribution per feature/tenant | ❌ not Mosaic |

**The pattern:** Mosaic's surface in the AI-eng stack is exactly
"deterministic numeric eval + trace." That's evals/grounding/harness for
NUMBERS. It has zero surface in inference-stack, retrieval, or LLM-runtime
concerns — and pretending otherwise would be the same "engine is
commodity" mistake the graph idea hit.

---

## Next move (if pursued)

Cheap: a `mc report-check` skill/plugin (NOT a kernel phase) — give it an
agent's analytics claims + a cube, it recomputes each claim, flags
mismatches, returns the trace for the agent to self-correct. Thin
orchestration over existing `query`/`read_with_trace`. A 1-session spike
would prove the loop end-to-end (agent claims a wrong CPC → harness
catches it → agent corrects with the traced method).

But per the demand-driven discipline: **this stays a filed positive
result until a real consumer wants agent-written analytics validated.**
The test proved the edge is real; building the harness waits for demand —
same as backtest×simulate.

---

## Cross-links
- [`../evaluation-oracle-validation-push-bug.md`](../evaluation-oracle-validation-push-bug.md) — the 38% catch this generalizes
- CLAUDE.md §2.10 (weighted-average consolidation — the trap class tested here)
- `crates/mc-fixtures/src/lib.rs` (the Acme CPC measure)
- `mc demo` (the running proof: consolidated CPC = 1.5202381, with trace)
- [`./evidence-fusion-decision-substrate.md`](./evidence-fusion-decision-substrate.md) — sibling (fusion is the *decision* version; this is the *validation* version)
- [`../_resolved/graph-kernel-as-impact-substrate.md`](../_resolved/graph-kernel-as-impact-substrate.md) — the ADR-drift idea (rerouted to Continuity)

## Note
This is the third reframe tested in the 2026-06-06 batch (graph-impact:
rerouted; evidence-fusion: double-gated; number-validation: positive).
It's the one that came back clean — because it's the LEAST novel and the
MOST grounded: it's just the 38% bug catch named as a discipline. The
unglamorous, already-validated thing keeps being the real edge.
