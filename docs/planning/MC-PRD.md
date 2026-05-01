Before I write this, two things I want to flag so the PRD I produce is actually useful and not just a 5,000-word document that sits in a folder.

**First: the answers I'm going to assume.** Earlier in this conversation I gave quick takes on the architectural-commit open questions. I'm going to assume those are your working answers (single-target regression v1, distributional output required, atomic blue-green hot-swap, OOD-vs-IS as a hard policy). If any of those don't sit right, push back before I commit them to the PRD — those four answers structure a lot of downstream decisions.

**Second: the shape of what I'm about to write.** A PRD outline at this stage shouldn't try to specify implementation details we haven't earned yet. It should be sharp on:

- *What the product is* (so you can tell when you're done)
- *Who it's for* (so you can tell when you're scope-creeping)
- *What the architecture commits to* (so you can tell when a decision conflicts)
- *What the phasing is* (so you can tell what to build next)

It should be vague on things that are still legitimately open (exact storage layout, exact DSL syntax, exact UI). Forcing specificity there will give you false confidence.

So this is a Product Requirements Document with the architecture decisions baked in but the implementation details deliberately held loose. You'll fill those in as you go.

Here it is.

---

# MarketingCubes V2 — Product Requirements Document (Outline)

**Version:** 0.1 (outline)
**Author:** Edwin Lovett
**Status:** Draft
**Last updated:** 2026-04-30

---

## 0. Foreword: how to read this document

This PRD describes a substrate, not a product. MarketingCubes V2 is a **multidimensional in-memory calc engine with native support for model-backed cells, calibration tracking, and audit-grade evaluation**. The eventual products built on it (marketing mix modeling, sports betting research, sales forecasting, prospect scoring) are addressed in Section 9 and treated as schemas-on-the-engine, not separate codebases.

The PRD is structured so that every architectural decision traces back to either (a) a documented lesson from claw-edge (cited via the transfer inventory), (b) a documented lesson from TM1 (cited via the developer guide), or (c) an explicit research bet that will be validated in Phase 1.

If a decision in this document is not backed by one of those three, it's a placeholder and should be flagged.

---

## 1. Product summary

### 1.1 The one-paragraph pitch

MarketingCubes is an embeddable analytical engine where users define multidimensional cubes (data shaped by hierarchies of dimensions like Channel × Market × Week), populate them with input data, and compose deterministic rules with model-backed cells (Lasso, BayesianRidge, XGBoost, etc.) that produce live, uncertainty-aware predictions. When inputs change, dependents recompute through a tracked dependency graph. Every model-cell exposes calibration metadata, walk-forward backtest results, and OOD-vs-IS evaluation as first-class metadata. The engine ships as a Rust core with Python and TypeScript bindings, deployable as a library, a server, or a WASM module in-browser.

### 1.2 Why this exists

Three categories of tool exist today and none of them solve the actual problem:

**Spreadsheets and planning tools** (Excel, TM1, Anaplan, Pigment) handle multidimensional data with hierarchies and live recalc, but cells are deterministic formulas. They cannot fit a model and have it update with new data.

**Notebooks and BI tools** (Jupyter, Hex, Tableau, Looker) handle modeling and visualization but lack persistent multidimensional state with live recalc. A user changing one input does not propagate through dozens of dependent calculations and forecasts in real time.

**Specialized forecasting tools** (Prophet, Robyn, Northbeam, Mutiny) handle one specific class of model on one specific schema. They cannot compose with each other or with deterministic logic in a single planning model.

MarketingCubes is the substrate that makes the third kind of product possible at scale: composable multidimensional models where some cells are formulas, some are fitted from data, all expose uncertainty, and all recompute when inputs change.

### 1.3 What MarketingCubes is *not*

To prevent scope creep:

- **Not a BI tool.** Visualization is a thin client over the engine; we do not compete with Tableau on dashboards.
- **Not a notebook.** Computation is declarative, not imperative-cell-execution.
- **Not a vertical app.** Marketing mix modeling, sports betting, sales forecasting are *schemas* shipped on top, not the product itself.
- **Not a deep learning platform.** Model cells target classical statistical and tabular ML (regression, GBMs, classification, time series). Neural networks are out of scope for V2.
- **Not a real-time streaming engine.** Recalc on input change is fast (sub-second for typical cubes) but the engine is not designed for sub-millisecond tick processing.

---

## 2. Target users and use cases

### 2.1 Primary user (V2 launch)

**The technical analyst-builder.** Someone who can write Python, understands what regression and cross-validation are, and is currently stitching together pandas, DuckDB, and ad-hoc model code to do work that should be a unified system.

Concrete personas:
- The agency analyst building marketing mix models in pandas + Stan + Tableau, frustrated that none of it composes.
- The independent quant building sports betting models in Python notebooks, frustrated that walk-forward backtests don't share state with live inference.
- The FP&A analyst who has outgrown spreadsheets and wants to add ML forecasting to their planning model.
- **Edwin.** This is your tool first.

### 2.2 Use cases at launch

1. **Marketing mix modeling.** Spend × Channel × Market × Week cube; Lasso-fitted attribution coefficients per channel; live what-if recalculation when a planner adjusts spend allocation.
2. **Sports betting research.** Game × Market × Date cube; the V1.6 Lasso (or successor) as a model-cell; walk-forward and OOD-vs-IS backtesting built into the engine.
3. **Sales forecasting.** Account × Quarter × Product cube; pipeline coverage and conversion rates as model-cells; live forecast recalc when deals move.
4. **Prospect scoring.** Prospect × Feature cube; XGBoost-fitted lead scores; ranked output that updates as new prospects enter.

These four are launch priorities. Others (stocks, prediction markets, demand forecasting) are post-launch expansions on the same engine.

### 2.3 Anti-personas

We are explicitly not designing for:
- **Non-technical business users.** They will use products *built on* MarketingCubes, not the engine itself.
- **Real-time trading firms.** Latency requirements are off-spec for our recalc model.
- **Deep learning practitioners.** PyTorch/TensorFlow are different tools.

---

## 3. Architectural commits

These are non-negotiable design decisions. Each cites the source.

### 3.1 The cube is the core data structure

A cube is an N-dimensional sparse array of cells. Dimensions have hierarchies. Cells have coordinates, values, and metadata. Everything else in the engine is built on this primitive.

**Source:** TM1 developer guide. Decades of validated design. The transfer inventory's Section 11 documents that claw-edge's flat per-game schema is the core thing that needs to change.

### 3.2 Cells can be inputs, deterministic rules, or model-backed

Three cell types, unified under a single read API:
- **Input cells:** values set by user or import.
- **Rule cells:** values computed from a declarative formula referencing other cells.
- **Model cells:** values computed from a fitted model, with the model itself as a versioned artifact.

A reader does not branch on cell type. Reading a cell returns its current value plus uncertainty metadata regardless of source.

**Source:** transfer inventory Section 9 (latent ModelCell pattern in claw-edge); model-backed cells were the breakthrough idea from earlier in this conversation.

### 3.3 Every cell value carries uncertainty

Reading any cell returns at minimum `(point, std)`. For deterministic rules, std propagates from upstream uncertainty. For model cells, std comes from the model's `predict_distribution()`. For input cells, std defaults to zero unless explicitly set.

**Source:** transfer inventory Section 8 (claw-edge already does this for V1.6 with `residual_std`); deep research report on BayesianRidge + alpha panel decomposition; this is the most-distinguishing feature versus existing analytics tools.

### 3.4 Dependencies are declarative and tracked

Cells declare their dependencies (which other cells they read from). The engine builds a DAG. When an input changes, the engine identifies dirty downstream cells and recomputes them lazily on next read. No cron-driven recalc.

**Source:** TM1's feeders + dirty-tracking model; transfer inventory Section 7 documents the pain of the cron-driven alternative; "Build Systems à la Carte" (Mokhov et al.) is the canonical reference.

### 3.5 Calibration and evaluation are first-class

Every model cell exposes:
- `.calibration()` returning a PIT histogram and calibration ratio
- `.walk_forward(strategy)` returning cross-validated metrics
- `.evaluate(fold_strategy, breakout='ood'|'is'|'all')` returning per-segment metrics

It is impossible to query a model cell's prediction without being able to also query its calibration metadata. Backtests with OOD breakout are a built-in operation, not a per-experiment script.

**Source:** transfer inventory Section 3; EXP-015's discovery that all-seasons backtests systematically inflated performance is the canonical motivating bug. This commit ensures that bug is impossible to repeat.

### 3.6 The engine is a Rust core with multiple bindings

The engine is implemented in Rust. It exposes:
- A native Rust API (for embedding in Rust applications)
- A Python binding via PyO3 (for the FastAPI / Jupyter / pandas use cases)
- A TypeScript binding via WASM (for in-browser local-first and Cloudflare Workers)
- An optional HTTP server (for traditional client-server deployments)

All four bindings call into the same compiled engine. There is no separate "frontend version" or "lite version" — the WASM build is the engine compiled to a different target.

**Source:** earlier conversation on Rust-as-substrate; transfer inventory Section 11 confirms this is missing in claw-edge and would require a complete rewrite (which is the point — we're writing it).

### 3.7 Model artifacts are content-addressable and registry-backed

Every model cell's fitted artifact (weights, calibration map, training metadata) is stored content-addressably (hash of contents → artifact). Cells reference artifacts by hash. The artifact registry supports atomic blue-green swaps: a cell can hot-swap from `artifact_hash_old` to `artifact_hash_new` without downtime, and rollback by referencing the prior hash.

**Source:** transfer inventory Section 2 (claw-edge's KV-loaded weights pattern is a working example, but lacks atomic swap and rollback); the answer to open question #5.

### 3.8 Imputation is part of the cell contract, not a runtime concern

Every model cell declares its imputation policy at fit time and persists it in the artifact. At inference time, the cell imputes missing features using only the persisted policy. There is no runtime override, no "production median" separate from the "training median," no `TRAINING_MEDIANS` constant in code.

**Source:** transfer inventory Section 10 (codex finding #6 — TRAINING_MEDIANS vs column_means mismatch). This is the most-cited production bug class in the inventory and the substrate-level fix is a strict contract.

### 3.9 Versioned cube state with point-in-time queries

The cube store supports point-in-time queries: `cube.at(timestamp).read(coord)` returns the cell's value as it was at that timestamp. The implementation may use snapshot isolation, write-ahead logging with replay, or full content-addressable cube history — the exact mechanism is open, but the API is committed.

**Source:** transfer inventory Section 11 (versioning is missing); audit and reproducibility require this; the CRDT/Git-style versioning differentiator from earlier conversation depends on it.

### 3.10 The DSL is an expression-tree, not a string

Rules and dependencies are declared via a typed expression API, not parsed from strings. A YAML/text representation can be derived from the expression tree, but the tree is the source of truth. This avoids parser complexity and makes static analysis (auto-feeder inference, type checking) tractable.

**Source:** earlier conversation on DSL design; the answer to open question #7 — we punt on Excel-vs-MDX-vs-Python syntax for V2 and expose the AST directly. A string-parsing layer can be added in V3 if user research demands it.

---

## 4. The model-cell layer (the differentiating piece)

### 4.1 Required interface

Every model cell implements:

```
predict(input) → (point, std)
predict_distribution(input) → Distribution  # optional but encouraged
fit(training_data, config) → ModelArtifact
impute_missing(partial_input) → input
calibration() → PITHistogram
walk_forward(strategy) → metrics_per_fold
evaluate(fold_strategy, breakout) → segmented_metrics
describe() → { feature_contract, non_zero_features, hyperparameters, training_metadata }
```

### 4.2 Required model types at launch

V2 ships with these fitter types implemented:

- **Lasso** (linear with L1)
- **Ridge** (linear with L2)
- **ElasticNet** (linear with L1+L2)
- **BayesianRidge** (linear with proper posterior)
- **GLMs** (logistic, Poisson, negative binomial)
- **XGBoost** (gradient-boosted trees, via bindings)
- **ARIMA / ETS** (time series)
- **Constant / cold-start** (degenerate cell that returns a fixed value, used as fallback)

Each fitter implements the full ModelCell interface. Adding a new fitter type is documented as a 1-day task; the framework provides scaffolding.

**Source:** transfer inventory Section 2 (Lasso, XGBoost, panel-Ridge already exist in claw-edge); deep research reports on BayesianRidge and per-game σ; the MLB chat-gpt research on negative binomial.

### 4.3 Composition primitives

Cells can be composed:
- **Ensemble:** `ensemble([cell_a, cell_b], weights=[0.9, 0.1])` — weighted average; std combines correctly.
- **Calibrated:** `calibrate(cell, calibration_map)` — passes the cell's output through a post-cell.
- **Conditional:** `conditional(predicate, cell_a, cell_b)` — switches between cells based on a runtime check (replaces claw-edge's cold-start fallback pattern).
- **Stacked:** the output of one model cell can be a feature input to another.

Composition produces a new cell that itself implements the full ModelCell interface. Compositions are tracked in the dependency graph.

**Source:** transfer inventory Section 9 (composition patterns are latent in claw-edge but not first-class).

### 4.4 The calibration discipline

Calibration is enforced as engine policy:

- Every model cell exposes a calibration ratio and a PIT histogram on demand.
- A backtest run produces an OOD-vs-IS breakdown unless explicitly opted out (and the opt-out is logged).
- Cells can be flagged as "calibrated" only if their PIT calibration ratio is in a configurable band (default: 0.95-1.05) and walk-forward variance is bounded.
- Uncalibrated cells produce predictions, but downstream consumers are warned.

**Source:** transfer inventory Section 3, Section 8, Section 10; EXP-015 is the canonical motivating example; this is the most-defensible product moat.

### 4.5 Feature contract

A model cell's feature contract is a structured type, not a string list:

```
feature_contract: {
  name: string,
  dtype: f64 | i64 | bool | category,
  valid_range: (min, max) | None,
  imputation: { strategy: 'median' | 'mean' | 'constant' | 'predicted', value: any },
  source: cube_coordinate_or_input_marker,
}
```

The cell validates inputs against the contract at runtime. Mismatched dtypes, out-of-range values, or missing fields without imputation policies produce errors, not silent NaN propagation.

**Source:** ISS-005 (transfer inventory Section 10); the richer-featureContract gap I flagged in the inventory review.

---

## 5. The cube and dependency graph

### 5.1 Cube definition

A cube is defined by:
- A name and namespace
- A list of dimensions, each with a name, member set, and hierarchy
- A measure or measures (the cells)
- Cell-level rules and model attachments

Cubes are defined in code (Python/TypeScript) or in YAML. The YAML form is a derived projection of the in-code form.

**Source:** TM1 dev guide chapters 1-2; the schema-as-code pattern Edwin already validated for MarketingCubes V1.

### 5.2 Storage

Cells are stored sparsely. The exact layout is open for Phase 1 research; candidates include:
- HashMap<Coord, Cell> (naive baseline)
- Apache Arrow with custom indexing
- Block-compressed with Roaring bitmaps for member presence
- LSM tree for write-heavy workloads

The choice will be benchmarked in Phase 1 and locked in Phase 2. The API is designed to make the underlying choice swappable.

**Source:** earlier conversation on storage; DuckDB papers; TM1 dev guide on dimension order optimization.

### 5.3 Dependency graph

The engine maintains a DAG of cell-level dependencies. Edges are inferred from rule definitions and model-cell feature contracts. Cycles are detected at definition time, not runtime.

When an input cell changes:
1. The engine marks all transitively-dependent cells as dirty.
2. On next read of a dirty cell, the engine recomputes it (and recursively any dirty upstream cells).
3. Computed values are cached until the next dirty mark.

This is lazy evaluation with dirty tracking — TM1's model with modern naming.

**Source:** TM1 dev guide on feeders; "Build Systems à la Carte"; transfer inventory Section 7.

### 5.4 Auto-feeder inference (the differentiator)

For model cells, the engine performs static analysis on the feature contract: any cube cell that appears in the contract becomes an automatic dependency of the model cell. There is no manual `FEEDERS` declaration as in TM1.

This eliminates the single biggest pain point of TM1 administration (manual feeder management) and is the most-cited differentiator versus TM1.

**Source:** earlier conversation; TM1 dev guide on feeders' difficulty.

### 5.5 Hierarchies and consolidations

Each dimension can have one or more hierarchies. A hierarchy is a tree of parent-child relationships with weights. Consolidated cells (cells whose coordinate includes a non-leaf member) are computed by walking the hierarchy and aggregating leaf cells.

Aggregation rules per dimension level (sum, weighted sum, average, max, custom) are configurable per measure.

**Source:** TM1 dev guide on consolidations; transfer inventory Section 11 on what's missing.

---

## 6. Evaluation and audit

### 6.1 The experiment loop, built in

The engine ships with a built-in experiment runner that takes:
- A model cell or composition
- An evaluation strategy (walk-forward / k-fold / holdout)
- A breakout policy (`ood`, `is`, `all`, custom segmentation)
- A metric set (MAE, RMSE, CRPS, log-likelihood, calibration ratio, edge-bucket monotonicity, ROI)

And produces a structured experiment report — the same shape as the experiment markdown files in claw-edge, but emitted as data, not prose.

**Source:** transfer inventory Section 3; the four-move audit pattern.

### 6.2 The audit chain

Every experiment report is content-addressable. When a later experiment supersedes or corrects an earlier one, the engine maintains a linked chain. Reports never disappear; they are amended with banners pointing to successors.

**Source:** transfer inventory Section 3 audit pattern; EXP-013 → EXP-015 → CURRENT_STATE chain.

### 6.3 Doctrine retirement

Configurable thresholds and heuristics in the engine (default calibration band, default OOD policy, default edge-bucket boundaries) are tagged with provenance: which experiment established them, which experiments validate or contradict them. When an experiment contradicts a doctrine, the doctrine is auto-flagged for review.

**Source:** transfer inventory Section 10 (CLAUDE.md edge-tier doctrine empirically wrong); answer to open question #12.

---

## 7. Storage, deployment, and operations

### 7.1 Storage substrate

The engine's storage is pluggable:
- **Embedded:** the engine includes its own sparse store (Phase 1 default).
- **DuckDB-backed:** for users who already have a DuckDB warehouse (Phase 2).
- **External:** Postgres / SQLite / cloud KV adapters (Phase 3).

The cube API does not change across substrates.

### 7.2 Deployment models

Three supported:
- **Library:** `pip install marketingcubes` or `cargo add marketingcubes` — runs in-process.
- **Server:** standalone binary exposing HTTP/gRPC — runs as a service.
- **WASM:** browser-side via `npm install @marketingcubes/wasm` — runs locally for collaborative editing or offline planning.

The same cube definition runs on all three.

### 7.3 Persistence

Cube state persists as:
- Snapshot files (binary, memory-mappable, fast load)
- Write-ahead log (append-only, replayable)
- Content-addressable artifact registry for model cells

This combination supports point-in-time queries, crash recovery, and atomic rollback.

### 7.4 Concurrency model

Phase 1: single-writer, multi-reader with reader-writer lock and copy-on-write read snapshots.
Phase 2: MVCC for true multi-user concurrent editing.
Phase 3: CRDT-based offline sync for local-first collaborative editing.

The differentiator (collaborative planning models) lives in Phase 3 but the architecture is designed to support it from day one.

---

## 8. Phasing and milestones

Phasing follows the original plan from earlier conversations, refined with everything we've learned.

### Phase -1: Foundation (current — 2 weeks)

- Finalize this PRD
- Read the prior art (TM1 papers, DuckDB papers, salsa, automerge, "Build Systems à la Carte")
- Decide on storage layout for Phase 1
- Decide on DSL surface (expression tree only; no string parsing)

### Phase 0: Reading and reference designs (2 weeks)

- Implement a toy cube engine in Rust (no models, no rules, just sparse storage)
- Build the dependency-graph primitive in isolation
- Build a parser-free expression tree as the rules layer
- All three exercises produce throwaway code; the goal is learning, not deliverables

### Phase 1: Minimum viable cube (4-6 weeks)

- Real sparse storage (chosen layout from Phase -1)
- Real dependency graph with auto-feeder inference for model cells
- One model-cell type working end-to-end: Lasso
- One full integration test: load V1.6 weights, define a cube matching claw-edge's schema, reproduce production V1.6 inference exactly
- **Acceptance criterion:** for a given game, the cube engine returns the same predicted total as production claw-edge, byte-for-byte.

### Phase 2: The model-cell zoo (4-6 weeks)

- Add Ridge, ElasticNet, BayesianRidge, XGBoost, GLMs
- Composition primitives (ensemble, calibrate, conditional, stacked)
- Walk-forward and OOD-vs-IS evaluation built in
- Calibration policy enforcement
- **Acceptance criterion:** reproduce EXP-015's OOD-vs-IS breakout in three lines of cube code.

### Phase 3: Persistence and versioning (4-6 weeks)

- Snapshot + WAL persistence
- Content-addressable artifact registry
- Point-in-time queries
- Atomic blue-green model-cell swaps
- **Acceptance criterion:** roll back a cube to a prior state and observe identical predictions.

### Phase 4: Bindings and deployment (4 weeks)

- PyO3 Python bindings
- WASM TypeScript bindings
- HTTP server mode
- **Acceptance criterion:** the same cube definition runs in Python, TypeScript browser, and standalone server with identical results.

### Phase 5: First production schema (4 weeks)

- Migrate one full claw-edge workflow into MarketingCubes
- Marketing mix model template
- Documentation, examples, getting-started guide
- **Acceptance criterion:** Edwin replaces a piece of claw-edge production with the cube engine and observes equivalent or better behavior.

### Phase 6: Differentiator features (ongoing)

- CRDT-based collaborative editing
- LLM-native rules authoring
- Template marketplace
- Hybrid ROLAP+MOLAP query planner

---

## 9. Schemas as products

Each launch use case maps to a documented schema:

### 9.1 Marketing mix modeling schema

- Dimensions: Channel × Market × Week × Scenario
- Hierarchies: Channel (Paid Search → Search → Marketing), Market (DMA → Region → Country)
- Measures: Spend (input), Conversions (input), Attributed Lift (Lasso model cell), Predicted Q4 Conversions (Lasso forecast cell)
- Differentiator: every coefficient is interpretable; live slider for spend reallocation; calibration ratio visible per channel

### 9.2 Sports betting research schema

- Dimensions: Game × Market × Date × Bookmaker
- Hierarchies: Date (Day → Week → Season), Bookmaker (Tier → Bookmaker)
- Measures: Predicted Total (Lasso V1.6 cell), Edge (rule), Recommendation (rule)
- Differentiator: the entire claw-edge research program runs as cube cells; OOD-vs-IS breakouts are one method call

### 9.3 Sales forecasting schema

- Dimensions: Account × Quarter × Product × Stage
- Hierarchies: Account (Rep → Region → Org), Quarter (Month → Quarter → Year)
- Measures: Pipeline (input), Coverage (rule), Predicted Closed Won (BayesianRidge cell)
- Differentiator: uncertainty bounds on every forecast; live recalc when deals move stages

### 9.4 Prospect scoring schema

- Dimensions: Prospect × Feature × ScoringModel
- Hierarchies: Feature (Feature → Category → Type)
- Measures: Score (XGBoost cell), Rank (rule)
- Differentiator: live re-ranking; explainable feature contributions per prospect

Each schema ships as a YAML file and a sample dataset. Schemas are first-class artifacts in the engine — installable via a registry, versionable, forkable.

---

## 10. Open questions deferred to Phase 1

These are explicitly punted past PRD drafting because committing without research would be premature:

1. Exact sparse storage layout (HashMap vs Arrow vs LSM vs hybrid)
2. Exact serialization format for cube snapshots
3. Whether Python bindings expose pandas DataFrames or a thin native type
4. WASM binding's memory model (linear memory limits affect cube size)
5. Specific Rust crates for parser-free expression trees
6. The auto-feeder inference algorithm's soundness/precision tradeoff

Each of these gets a Phase 1 research bet with explicit acceptance criteria.

---

## 11. Risks and mitigations

**Risk:** the engine takes longer than 6 months and Edwin loses focus.
**Mitigation:** Phase 1 has a single hard acceptance criterion (reproduce V1.6 inference). If that's not achievable in 6 weeks, the project is paused. No phase rolls into the next without its acceptance test passing.

**Risk:** the model-cell abstraction is too opinionated and doesn't fit real-world models.
**Mitigation:** Phase 2 adds 7 fitter types. If 7 is hard, the abstraction is wrong and we revise.

**Risk:** auto-feeder inference is unsound and produces wrong calibration.
**Mitigation:** the engine compares auto-inferred feeders against full-scan ground truth in Phase 1 testing. If they diverge, the algorithm is fixed before any production schema runs on it.

**Risk:** the WASM binding is too slow for browser-side use.
**Mitigation:** Phase 4 benchmarks browser-side recalc on a 100K-cell cube. If sub-second is impossible, the WASM target is descoped to read-only and editing requires server-side.

**Risk:** Edwin builds the engine but no one else uses it.
**Mitigation:** Edwin is the first user. If the engine doesn't measurably improve Edwin's own daily work (claw-edge research velocity, MarketingCubes V1 capabilities), the strategy is wrong regardless of external interest.

---

## 12. Naming

Codename: MarketingCubes V2.

Real names to consider (in order of preference):

- **Atlas** — multidimensional, planetary, the right scale
- **Cubist** — multidimensional, references the art movement, memorable
- **Lattice** — mathematical, clean, suggests structure
- **Foundry** — emphasizes templates-as-craft
- **Tensor** — mathematically correct but taken in ML

Naming is a Phase 4 task (when there's something to name). Until then: MarketingCubes V2.

---

## 13. Success metrics

V2 is successful if, by 6 months post-Phase-5:

1. Edwin's claw-edge research velocity has measurably increased (more experiments per week, faster turnaround on new model variants)
2. At least one external user (agency, analyst, indie quant) has shipped a real model on the engine
3. The engine has reproduced the V1.6 production inference, the EXP-015 OOD-vs-IS breakout, and at least one MMM model — all with bit-exact or near-bit-exact agreement against the reference implementation
4. The transfer inventory's "what's missing" list is reduced from 8 items to 0

---

*End of outline. This is a 60-70% draft — sharp on commits, deliberately loose on implementation details that should emerge from Phase 1 research. Next pass should add: detailed acceptance criteria for each phase, a one-page summary suitable for sharing with potential collaborators, and a glossary mapping TM1/claw-edge terms to MarketingCubes terms.*

---

That's the outline. Three things I want to call out before you read it:

**The phasing is ambitious.** Phase 1's "reproduce V1.6 inference exactly" acceptance criterion is the right gate but it's harder than it sounds — you need the cube primitive, the dependency graph, the model-cell layer, *and* the integration test all working in 6 weeks. If you've never written serious Rust before, double the timeline. The honest read is Phase 1 is 6-12 weeks, not 4-6.

**Section 9 (schemas as products) is where your business model lives.** Notice that I made each launch use case a *schema*, not a *product*. That's the key strategic move — you build one engine, you ship many schemas, each schema is a 2-3 day effort once the engine exists. The marketplace conversation we had earlier is exactly this.

**The success metric (#3) about "bit-exact or near-bit-exact" reproduction of V1.6 is load-bearing.** It's the single best test of whether the engine actually works. If MarketingCubes V2 can take the production V1.6 weights JSON and spit out the same predicted total for a given game, you've proven the entire model-cell layer end-to-end. Don't soften that test.