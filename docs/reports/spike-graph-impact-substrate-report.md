# Spike Report — Graph-Kernel-as-Impact-Substrate

**Date:** 2026-06-06
**Spike:** [`../handoffs/spike-graph-impact-substrate.md`](../handoffs/spike-graph-impact-substrate.md)
**Research note:** [`../research-notes/_active/graph-kernel-as-impact-substrate.md`](../research-notes/_active/graph-kernel-as-impact-substrate.md)
**Test:** `crates/mc-core/tests/spike_impact_substrate.rs` (5 tests, all pass)

---

## VERDICT: 🟢 GREEN (engine) / 🟡 YELLOW (the product shape)

**The engine thesis is PROVEN.** Mosaic's dependency-graph kernel computes
the exact blast radius of changing a non-numeric "intent" node, transitively,
and the "why is this affected" chain is reconstructable — deterministically,
no grep. 5/5 tests pass, first run, fmt+clippy clean.

**The nuance (why also YELLOW):** the spike deliberately modeled intent on
the `DependencyGraph` API *directly*, bypassing the cube/measure/dimension
framing. That was the right call to isolate the engine question — and it
surfaced the real product finding: **the reusable asset is `DependencyGraph`
itself, not "Mosaic the cube."** The path to a product is exposing the graph
kernel as a thin standalone surface, not authoring intent as numeric cubes.

---

## What was proven (the engine)

Modeled an 8-node toy project intent graph directly on `DependencyGraph`:
2 decisions (CORS policy / no-hardcoded-assets), 4 code-facts (endpoint,
middleware, 2 components), 2 dependent artifacts (smoke test, deploy),
8 declared edges. "B exists because of A" = "B reads A" (forward edge).

| Test | Asserts | Result |
|---|---|---|
| `changing_cors_decision_yields_exact_blast_radius` | changing the CORS decision → EXACTLY {endpoint, middleware, smoke-test, deploy} (incl. 2-hop transitive) | ✅ |
| `cors_blast_radius_excludes_unrelated_decision_subgraph` | the hardcoded-assets subgraph is ABSENT; the root isn't in its own radius | ✅ |
| `changing_other_decision_is_isolated` | the two decisions have disjoint blast radii | ✅ |
| `trace_explains_why_a_node_is_affected` | smoke-test traces back through endpoint+middleware to the CORS decision; NOT to the unrelated decision | ✅ |
| `deploy_worker_traces_to_cors_decision_two_hops` | transitive "why" chain works at 2 hops | ✅ |

**The load-bearing primitive** (`DependencyGraph::closure_of_dependents`,
dependency.rs:102) returned the exact transitive dependent set every time.
The "why" chain (walking forward edges via `dependencies_of`) reconstructs
the explanation. Both are pure-graph, deterministic, sub-millisecond.

This is the proof that **"what's impacted by this change, exactly, by math"
= the transitive closure of dependents** — and Mosaic already ships it.

---

## The edge-authoring assessment (Step 4 — the make-or-break)

The handoff flagged edge authoring as the thing that decides viability.
Findings:

1. **Authoring edges directly on `DependencyGraph` was trivial and natural.**
   `add_edge(reader, edge(dep))` reads exactly like "reader depends on dep."
   No numeric framing fought it. A `depends_on(g, reader, dep, id)` helper
   made the toy graph 8 readable lines.

2. **The cube/measure/dimension framing was NOT used — and shouldn't be.**
   The spike's key discovery: intent nodes are not measures-at-coordinates.
   Forcing "decision_cors_policy" into a Scenario×Version×Measure cube would
   be awkward and pointless. The right substrate is the bare
   `DependencyGraph` (+ `CellCoordinate` as an opaque node id). The graph
   kernel is **more general than the cube that's been built on top of it.**

3. **The unsolved part remains unsolved (as expected):** *who authors the
   intent→code edges for a real project.* The spike authored them by hand to
   isolate the engine. For a real repo, the edges come from: an LLM proposing
   "this commit relates to ADR-12" + human confirmation at decision
   boundaries (the Continuity human-anchor loop). The spike does NOT prove
   that authoring loop is tolerable at scale — it proves the engine consumes
   the edges correctly once they exist. **That authoring loop is the next
   real risk, and it's a separate experiment.**

---

## What this means for the product (the two-products question, answered)

The research note posed two products. The spike resolves which is reachable:

- **Product 1 — impact-analysis engine** ("what does this change touch"):
  **REAL and close.** The engine works today. The build is: a thin API /
  CLI (`mc impact <node>`) over `DependencyGraph` + a way to load an intent
  graph from a declarative file (a `.graph.jsonl` of nodes + edges, NOT a
  cube YAML). ~1-2 sessions for a usable v0 over a hand-authored graph.

- **Product 2 — context-assembly substrate** ("what the agent needs to know,
  deterministically"): **gated on edge authoring at scale.** The engine is
  ready; the blocker is the LLM-proposes-human-confirms edge loop. That's the
  next spike, not the next build.

**Recommended next move:** build Product 1's thin surface (a standalone
`DependencyGraph`-backed impact/trace API decoupled from the cube) over a
hand-authored intent graph file. That's the smallest thing that turns the
proven engine into something usable — and it's the platform the edge-
authoring experiment (Product 2) would then plug into.

---

## What the spike did NOT prove (honest boundaries)

- **Not** that auto-deriving edges from code works (deferred; LSP owns syntax
  edges, intent edges need authoring).
- **Not** that the LLM-proposes / human-confirms authoring loop is tolerable
  at real-repo scale (the next experiment).
- **Not** that this beats LSP for *code-symbol* impact (it doesn't, and
  shouldn't try — the value is the *intent* layer above code).
- **Not** retrieval (still RAG/Vectorize's job — unchanged).
- `read_with_trace` (the Cube-level trace API) was NOT used — at the pure-
  graph level the forward-edge walk is the equivalent "why" mechanism, and
  it's simpler. A product would likely expose graph-level trace, not the
  cube's.

---

## Recommended next steps (in order)

1. **Thin impact API spike/build** — expose `DependencyGraph` blast-radius +
   trace as a standalone surface (`mc impact`, or a tiny library) loading a
   `.graph.jsonl`. Proves Product 1 is usable, not just provable. ~1-2 sessions.
2. **Edge-authoring experiment** — can an LLM propose intent→code edges from
   a real repo's ADRs+commits, with human confirmation, tolerably? This is
   the gate on Product 2. A separate spike.
3. **Only then**, if both land: an ADR for "Mosaic graph kernel as an
   agentic impact/context substrate" as a real product line distinct from the
   numeric-modeling product.

---

## Cross-links
- Research note: [`../research-notes/_active/graph-kernel-as-impact-substrate.md`](../research-notes/_active/graph-kernel-as-impact-substrate.md)
- Test: `crates/mc-core/tests/spike_impact_substrate.rs`
- `crates/mc-core/src/dependency.rs:102` (`closure_of_dependents` — the proven primitive)
- `crates/mc-core/src/dependency.rs:63` (`add_edge` — trivial edge authoring)

## One-line takeaway
**The blast-radius engine is real and ships today; the reusable asset is the
bare `DependencyGraph`, not the cube; the product is "thin impact/trace API
over the graph"; the remaining risk is edge authoring at scale, which is the
next experiment — not the next build.**
