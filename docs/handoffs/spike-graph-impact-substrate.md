# Spike Handoff — Graph-Kernel-as-Impact-Substrate (the blast-radius proof)

**Status:** Ready to run — highest-leverage spike in the project
**Date:** 2026-06-06
**Research note:** [`../research-notes/_active/graph-kernel-as-impact-substrate.md`](../research-notes/_active/graph-kernel-as-impact-substrate.md)
**Estimated effort:** 1 session (it's a proof + a verdict, not a product)
**Crate:** investigation in `mc-core` (no product code; a throwaway example/test is fine)
**Branch:** `spike/graph-impact-substrate`

---

## The one question this answers

**Can Mosaic's dependency-graph kernel compute the exact blast radius of
changing a non-numeric "intent" node, and produce a legible trace of WHY
each downstream node is affected?**

If yes → "Mosaic's kernel as a deterministic impact/context substrate for
agentic dev" is real, and we know the smallest next build.
If no / too painful → we've learned cheaply where the idea is bounded.

This is NOT building a product. It's proving (or killing) a thesis with
the smallest possible experiment.

---

## Why this is cheap: the primitive already exists

Verified at source 2026-06-06 — the blast-radius engine ships today:

- `crates/mc-core/src/dirty.rs:171` — `mark_closure(root, graph)` calls
  **`graph.closure_of_dependents(root)`** → the exact transitive set of
  every node that (transitively) reads from `root`. **That is the blast
  radius.**
- `crates/mc-core/src/dependency.rs:42-46` — `forward` (node → what it
  reads) + `reverse` (node → who reads it) edges.
- `crates/mc-core/src/cube.rs:171` — `read_with_trace` → the dependency
  chain for any node (the "why" answer).
- `crates/mc-core/src/value.rs` — cells already hold `Bool`/`Category`/
  `Str`, so a node can carry non-numeric "state."

The spike is mostly: **can we MODEL a project's intent as a cube, and does
`closure_of_dependents` give the answer we want when we change a node.**

---

## Step 0: Preflight (confirm the primitive is callable as described)
```
cd /Users/edwinlovettiii/Projects/mc-v2
git worktree add ../mc-v2-spike-impact -b spike/graph-impact-substrate main
cd ../mc-v2-spike-impact

# Confirm closure_of_dependents exists + signature
grep -nE "fn closure_of_dependents|pub fn mark_closure|reverse|fn closure" crates/mc-core/src/dependency.rs crates/mc-core/src/dirty.rs

# Confirm read_with_trace returns a walkable chain
grep -nE "pub fn read_with_trace|struct TraceNode|struct Trace\b" crates/mc-core/src/cube.rs crates/mc-core/src/trace.rs

# How does a cube get built from a model today? (the path the spike mimics)
grep -nE "pub fn compile|build_acme|fn build" crates/mc-model/src/compile.rs crates/mc-fixtures/src/*.rs | head
```
Report: is `closure_of_dependents` pub/callable from a test? Does
`read_with_trace` give a chain you can print? If either is private,
note it — the spike can still construct a graph + call the closure via
whatever the public surface is (write→read drives dirty marking).

---

## Step 1: Model a tiny project's intent as a cube

The mapping (this is the creative core of the spike — keep it MINIMAL):

| Project-knowledge concept | Cube representation |
|---|---|
| A decision / commitment / constraint | a cell (Input — you set its "state") |
| A code-fact / file / endpoint | a cell |
| "B exists because of decision A" | a rule: B depends on A (a declared edge) |
| Changing a decision | writing the A cell |
| Blast radius of that change | `closure_of_dependents(A)` |
| "Why is X impacted?" | `read_with_trace(X)` chain back to A |

**Concrete tiny fixture (≈8-12 nodes), e.g. a web-app slice:**
- `decision_cors_policy` (Input) — a CORS decision (ADR-012)
- `decision_no_hardcoded_assets` (Input) — a UI commitment
- `endpoint_api_chat` (rule: depends on `decision_cors_policy`)
- `middleware_cors` (rule: depends on `decision_cors_policy`)
- `component_folder_view` (rule: depends on `decision_no_hardcoded_assets`)
- `component_sidebar` (rule: depends on `decision_no_hardcoded_assets`)
- `test_cors_smoke` (rule: depends on `endpoint_api_chat` + `middleware_cors`)
- `deploy_worker` (rule: depends on `endpoint_api_chat`)

Author this as inline YAML (single braces — §4.5) OR build the
DependencyGraph directly in a test if YAML's measure/dimension framing is
too numeric-shaped for intent nodes. **Either is fine — the spike's job is
to find out which framing is natural, that's a finding.**

Dimensions don't have to be sport/marketing — a single "Node" dimension
whose elements are the node names, one Input measure ("present"/state) and
the rules expressing edges, may be the cleanest. Discover the right shape;
report it.

---

## Step 2: The blast-radius test (the core assertion)

```
1. Build the graph / cube.
2. Change decision_cors_policy (write a new state).
3. Call closure_of_dependents(decision_cors_policy)  [via mark_closure or
   the dirty set after write].
4. ASSERT the dirty/affected set is EXACTLY:
   { endpoint_api_chat, middleware_cors, test_cors_smoke, deploy_worker }
   and NOT { component_folder_view, component_sidebar }  (different decision).
```

This proves: changing one intent node yields the *exact* downstream set,
including transitive (test + deploy are 2 hops from the decision), and
correctly EXCLUDES unrelated nodes. That's deterministic blast radius over
intent.

---

## Step 3: The trace test (the "why" assertion)

```
For one affected node (e.g. test_cors_smoke):
  read_with_trace(test_cors_smoke)
  ASSERT the trace chains: test_cors_smoke → endpoint_api_chat +
  middleware_cors → decision_cors_policy.
  (i.e. you can answer "why is the cors smoke test in the blast radius?"
   → "because it depends on the endpoint and middleware, which depend on
   the CORS decision.")
```

Legible trace = an LLM (or human) can be TOLD not just *what's* affected
but *why*, deterministically. That's the auditability property nothing in
the MCP/hooks world has.

---

## Step 4: The honest-wall probe (edge authoring)

While building Step 1, keep notes on the ONE thing that decides the
product's viability: **how painful was authoring the edges?**

- Did expressing "B exists because of A" feel natural, or fought against
  the numeric framing?
- For a real project (not the toy), who/what authors these edges? (LLM
  proposes + human confirms is the expected answer — does the toy make
  that plausible or absurd?)
- Is the cube/measure/dimension model a good fit for intent nodes, or
  does it want a thinner "just a DAG of named nodes" API?

This is the make-or-break finding. The engine works (Steps 2-3 will likely
pass — the primitive exists). The question is whether *modeling intent as
this graph* is tolerable enough to be worth it.

---

## Step 5: The verdict report

`docs/reports/spike-graph-impact-substrate-report.md` — lead with one of:

- **GREEN** — blast radius exact + trace legible + edge authoring
  tolerable. The thesis holds. Recommend the smallest next build (likely:
  a thin "intent graph" API or a `mc impact <node>` command sketch) and
  which of the two products (impact-analysis engine vs context substrate)
  to aim at first.
- **YELLOW** — engine works but the cube framing fights intent modeling.
  Recommend what kernel/API shape would fix it (e.g. a `DependencyGraph`-
  direct API decoupled from cube measures) + estimate.
- **RED** — something fundamental doesn't transfer (e.g. closure is
  cube-coupled in a way that can't express arbitrary intent DAGs; or edge
  authoring is hopeless). Document why, bound the idea to where it works.

Include: the test code, the exact source lines that made it work (or not),
the edge-authoring assessment (Step 4), and the recommended next move.

---

## What NOT to do
- Don't build a product, a CLI command, or an MCP server. This is a proof.
- Don't model a real 50-file project — 8-12 toy nodes is the point.
- Don't try to auto-derive edges from code (that's the deferred hard
  problem; the spike authors edges by hand to isolate the *engine*
  question from the *authoring* question).
- Don't touch the betting/marketing cubes or any product code.

---

## Acceptance gate (the spike)
- [ ] Tiny intent graph built (8-12 nodes, mixed decisions + code-facts)
- [ ] Blast-radius test: changing one node yields the EXACT transitive
  affected set, excludes unrelated nodes (Step 2)
- [ ] Trace test: an affected node's trace chains back to the changed
  decision, legibly (Step 3)
- [ ] Edge-authoring assessment written (Step 4 — the make-or-break)
- [ ] Verdict report GREEN/YELLOW/RED + recommended next move + the
  source lines that decided it
- [ ] `cargo test` for the spike test passes; fmt/clippy clean (quote it)
- [ ] Branch pushed (spike/graph-impact-substrate); PM reviews verdict

---

## Cross-links
- Research note (the full thesis): [`../research-notes/_active/graph-kernel-as-impact-substrate.md`](../research-notes/_active/graph-kernel-as-impact-substrate.md)
- `crates/mc-core/src/dirty.rs:171` (mark_closure / closure_of_dependents)
- `crates/mc-core/src/dependency.rs` (forward + reverse edges)
- `crates/mc-core/src/cube.rs:171` (read_with_trace)
- CLAUDE.md §4.5 (single-brace YAML), §6.7 (quote real runs)

## Note for the spike runner
The likely outcome is GREEN on the engine (the primitive demonstrably
exists) and the real signal is in Step 4 (edge authoring) + whether the
cube framing is natural for intent. Spend your judgment there — that's
where the product lives or dies, not in whether `closure_of_dependents`
returns the right set (it will).
