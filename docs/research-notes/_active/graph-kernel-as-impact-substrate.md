# Mosaic's Graph Kernel as a Deterministic Impact / Context Substrate

**Status:** ACTIVE — **engine spike returned 🟢 GREEN (2026-06-06)**; idea not yet resolved (edge-authoring-at-scale is the next gate). See [spike report](../../reports/spike-graph-impact-substrate-report.md).
**Date:** 2026-06-06
**Author:** Mosaic PM (Claude Opus 4.8, 1M context) + project owner (vibing session)
**The one-line thesis:** Mosaic isn't a numbers engine — it's a **blast-radius-and-trace engine** that happens to carry numbers. The agentic-dev world has no *deterministic* blast-radius-and-trace engine. That gap is the opportunity.

---

## The reframe that started this

Mosaic-the-product is "a multidimensional engine for numerical models." But `mc-core` underneath is not fundamentally about numbers. Strip the arithmetic and the kernel is:

- **declared edges** — "this node depends on those nodes"
- **dirty propagation** — change one node, get the *exact* transitive set of everything downstream (not a grep, not a guess — the precise closure)
- **lazy recompute** — only the affected set re-evaluates
- **trace** — every result chains back through the edges to the inputs that produced it

Numbers are just the payload. The transferable asset is the **graph + exact propagation + trace.**

The owner's intuition, stated raw: *"What if an LLM could understand instantly what areas/code would be impacted by a change, by math, because of Mosaic? A system that kills the pile of MCP/context7/skills/hooks you install per project."*

That intuition is grabbing at exactly what dirty propagation already computes: **"what is impacted by this change" = the transitive closure of dependents = blast radius.**

---

## The load-bearing source finding (verified 2026-06-06)

This is not speculative. The blast-radius primitive **already exists in the kernel:**

- `crates/mc-core/src/dirty.rs:171` — `mark_closure(root, graph)` calls
  **`graph.closure_of_dependents(root)`** → the exact transitive set of
  every node that (transitively) reads from `root`. That IS the blast
  radius of changing `root`.
- `crates/mc-core/src/dependency.rs:42-46` — the graph holds both
  `forward` (node → what it reads) and `reverse` (node → who reads it)
  edges. Reverse edges are how downstream is computed.
- `crates/mc-core/src/cube.rs:171` — `read_with_trace` returns the full
  dependency chain for any node (the "why is this value what it is"
  answer).
- `crates/mc-core/src/value.rs` — cells can already hold `Bool`,
  `Category`, `Str` (transient), not just `F64`. Node *state* doesn't
  have to be a number.

**The exact-impact engine ships today. It has only ever been pointed at revenue and bankrolls.** The experiment is: point it at project knowledge instead.

---

## The boundary that makes this real (where it works vs where it doesn't)

This is the load-bearing distinction. Get it wrong and the idea collapses into "rebuild a worse vector DB / a worse LSP."

### NOT this (already solved — do not rebuild)

- **Retrieval / "find content by similarity."** That's vector search
  (embeddings → ANN → cosine). Mature, you already use Vectorize. A cube
  is the wrong data structure for it. Finding the relevant paragraph in a
  10-Q is RAG's job, not Mosaic's.
- **Code-symbol dependencies.** "Find all references," "rename → what
  breaks," call hierarchy — LSP / rust-analyzer already do this as a
  deterministic graph over *syntax*. Mosaic would be a worse LSP.

### THIS (unsolved — has no engine today)

- **The INTENT layer above code.** LSP knows `foo` calls `bar`. Nothing
  knows:
  - "this CORS block exists because of ADR-012"
  - "this endpoint must stay wired to that handler — a *decision*, not a
    syntactic fact"
  - "changing this threshold violates the assumption module X was built on"
  - "this measure can't be Sum because of the weighted-average rule"
  These commitments/constraints/decisions form a **dependency graph with
  no engine.** They live as prose in ADRs, as hardcoded greps in bash
  drift-checks, as one-rule hooks, as CLAUDE.md sections nobody can query
  as a set. (The Continuity PRD says this verbatim: commitments are
  "implicit, per-repo, and unverifiable as a set.")

**The opportunity: give the intent layer a dependency graph, powered by
Mosaic's propagation kernel.** Change any node — a decision, a constraint,
a value, a code-fact — and compute the exact blast radius across
commitments, docs, AND their declared links to code. An LLM asking "if I
change this flag, what breaks, what decisions does it touch, what must I
re-read" gets a deterministic, traceable answer instead of grepping and
guessing.

---

## The "kills the meta-tool pile" claim — steelmanned and bounded

What MCP / context7 / skills / hooks / per-project bash-checks actually
are, underneath: **five uncoordinated answers to one question — "get the
agent the right context and enforce the right behavior."** Point solutions
that don't know about each other.

The honest cut:

- **Does NOT kill RAG / context7 / unstructured retrieval.** Similarity
  search over arbitrary prose stays. You can't graph your way to "the
  relevant paragraph in a library's docs."
- **DOES subsume the home-grown per-project pile** — the bash drift
  scripts, the one-rule hooks, the unqueryable CLAUDE.md rules, the
  "is this still wired correctly" checks. Every one of those is secretly
  computing "given this change/state, what context matters and what rule
  applies" — which is a graph query with a blast radius. One declared
  project-knowledge graph + one propagate/query/trace engine replaces the
  heap.

**Defensible headline:** *One deterministic project-knowledge graph that
replaces the uncoordinated pile of per-project enforcement-and-context
point-tools — by making "what's relevant to this action" and "what's the
blast radius of this change" a single traceable query instead of N
installed gadgets.* (NOT "kills everything including RAG" — that's the
version that doesn't survive contact.)

---

## Why deterministic beats the probabilistic ecosystem

Every context tool today is either **probabilistic** (RAG retrieves by
similarity — sometimes wrong, never reproducible) or **static** (a
hook/skill fires identically regardless of what *this specific change*
actually touches). The agentic ecosystem has no **deterministic,
change-aware, traceable** context layer.

Mosaic's kernel is exactly that: same graph + same change → same blast
radius → same relevant-set, every time, with a trace. An agent that gets
its context from a deterministic graph can be *audited*: "why did you
touch this file?" → "because the graph said this decision depended on it."
Nothing in the MCP/skills/hooks world has that property. (Same lesson as
the 38% bug catch: a deterministic oracle catches what the stochastic
layer gets wrong, and you can trace exactly which signal moved the
outcome.)

---

## The honest wall: edge authoring

One blocker, named plainly so the spike tests it:

**The graph is only as good as its declared edges, and intent→code edges
are NOT automatically derivable.** LSP gives syntax edges for free; nobody
gives "this code exists because of that decision" for free. Someone — or
an LLM pass with human confirmation at decision boundaries (the Continuity
human-anchor loop) — must assert those edges.

Consequence: **authoring the edges is the hard problem; running the engine
is not** (the engine exists). The realistic v1 models the *decision/
constraint* layer + its declared links to code-facts (the layer with no
engine), and lets LSP own the syntax layer it already owns. Two graphs,
one boundary, don't merge.

Split that matters: **edge authoring is LLM-assisted and
non-deterministic; edge *querying* (blast radius, trace) is
deterministic.** That split is the right shape — the fuzzy part is
human/LLM, the auditable part is the engine.

---

## Two products hide in here (the spike tells you which)

1. **Impact-analysis engine** — "what does this change touch." Narrower,
   very defensible, LSP-adjacent-but-for-intent. The safe real core.
2. **Context-assembly substrate** — "what does the agent need to know,
   deterministically." Bigger, fuzzier, the "kills the meta-tool pile"
   version. The vision (1) earns its way into.

Build toward (1); let it prove (2).

---

## SPIKE RESULT (2026-06-06): 🟢 GREEN on the engine

Ran `crates/mc-core/tests/spike_impact_substrate.rs` — 5/5 pass. Modeled
an 8-node toy intent graph (2 decisions, 4 code-facts, 2 artifacts, 8
edges) directly on `DependencyGraph`. Proven:
- changing the CORS decision → EXACTLY {endpoint, middleware, smoke-test,
  deploy} including 2-hop transitive; the unrelated decision's subgraph is
  correctly excluded.
- the "why is X affected" chain reconstructs back to the changed decision.
- `closure_of_dependents` is the blast-radius primitive; sub-ms; deterministic.

**Key discovery:** the reusable asset is the **bare `DependencyGraph`**, NOT
the cube. Intent nodes are not measures-at-coordinates; forcing them into a
Scenario×Version×Measure cube is awkward and unnecessary. The graph kernel
is more general than the cube built on it. Edge authoring on the bare graph
was trivial and natural (`add_edge(reader, edge(dep))` reads as "reader
depends on dep").

**What's still open (why this note stays ACTIVE):** *who authors the
intent→code edges for a REAL repo at scale.* The spike hand-authored them
to isolate the engine. The LLM-proposes / human-confirms authoring loop
(the Continuity human-anchor pattern) is unproven — that's the next gate.

Full verdict: [`../../reports/spike-graph-impact-substrate-report.md`](../../reports/spike-graph-impact-substrate-report.md).
Resolution of the two-products question: **Product 1 (impact-analysis
engine) is real and ~1-2 sessions from a usable v0; Product 2 (context
substrate) is gated on the edge-authoring experiment.**

## THE SPIKE (original plan — now executed; see result above)

**Goal:** prove the blast-radius-over-intent idea is real, cheaply, before
committing to anything.

**The experiment:** model one tiny project's commitments + a handful of
code-facts as a Mosaic-style dependency graph. Change one input node
(flip a flag, alter a threshold, supersede a decision). Show it produce
the **exact** blast radius across code + docs + decisions, and show the
trace ("why is this in the blast radius") is legible.

**Pass:** blast radius is exact + trace is legible + edge authoring was
tolerable → the pattern generalizes; we have the seed.
**Fail (also valuable):** edge authoring is so painful it's not worth it →
learned cheaply, the idea is bounded to where edges are free.

Full experiment design in the companion handoff:
[`../../handoffs/spike-graph-impact-substrate.md`](../../handoffs/spike-graph-impact-substrate.md).

---

## Cross-links
- `crates/mc-core/src/dirty.rs:171` (`mark_closure` / `closure_of_dependents` — the blast-radius primitive)
- `crates/mc-core/src/dependency.rs` (forward + reverse edges)
- `crates/mc-core/src/cube.rs:171` (`read_with_trace`)
- [`./evidence-fusion-decision-substrate.md`](./evidence-fusion-decision-substrate.md) — the sibling reframe (Mosaic as decision substrate for *scored signals*)
- [`../distribution-valued-cells.md`](../distribution-valued-cells.md) — the foundation the fusion sibling needs
- owner's PRDs that triggered this: Continuity ledger (the intent-layer-has-no-engine problem stated independently), Ignite agent (Vectorize for retrieval — the boundary)
- [`../lazy-dependency-graph.md`](../lazy-dependency-graph.md), [`../dirty-propagation-as-per-write-delta.md`](../dirty-propagation-as-per-write-delta.md) — the kernel mechanics this rests on

## Notes
- This is the most "new category" idea the project has produced — it's not
  marketing/sports/forecasting Mosaic, it's *Mosaic's kernel as a
  substrate for agentic context*. Treat the spike as the gate: it's the
  cheapest way to learn if the new category is real.
- The transferable insight, stated once: **you didn't build a numbers
  engine, you built a blast-radius-and-trace engine, and the agentic-dev
  world has no deterministic blast-radius-and-trace engine.**
