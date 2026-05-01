Yes, I can see both attached files. My honest read:

**This is a very strong foundation, but it is not ready to execute as-is.** It is a strong **strategy PRD** and an excellent **transfer inventory**, but it still needs to be turned into a stricter **engine specification** before you let Claude Code start building the Rust core.

I’d score it like this:

```txt
Vision:              9/10
Strategic direction: 8.5/10
Research discipline: 8.5/10
Execution readiness: 5.5/10
Risk control:        6.5/10
Scope control:       4.5/10
```

The idea is good. The layout is good. The weakness is that it is trying to be **TM1 + model registry + forecasting framework + experiment platform + WASM runtime + schema marketplace** all in one document.

That is inspiring, but dangerous.

## The biggest strength

The strongest thing in the PRD is that it frames MarketingCubes as a **substrate**, not a single app. The file explicitly says MarketingCubes V2 is a “multidimensional in-memory calc engine” with model-backed cells, calibration tracking, and audit-grade evaluation, while MMM, sports betting, sales forecasting, and prospect scoring are schemas built on top of the engine. That is exactly the right high-level product shape. 

That means you are not building:

```txt
A marketing dashboard
```

You are building:

```txt
A planning/modeling engine that marketing dashboards can sit on top of
```

That distinction is the whole game.

## The transfer inventory is genuinely good

The transfer file is probably the most useful part of the package. It documents the current repo, deployment surface, cron jobs, routes, model implementations, training scripts, simulator, experiment loop, known bugs, and missing concepts. That is exactly the kind of “bridge document” you need before rewriting or generalizing a system. 

The best part is that it does not just list code. It extracts lessons.

For example, it correctly identifies that the current claw-edge system has no abstract `Cell`, `Cube`, or `Dimension` types. Its types map directly to database tables and column sets, which means MarketingCubes V2 needs a richer vocabulary from the start. 

That is a very important insight. It means you cannot just “port claw-edge to Rust.” You need to create a new semantic layer.

## The PRD’s core architecture is directionally right

The PRD commits to a cube as the core data structure, where cells can be input cells, rule cells, or model-backed cells. That is a good abstraction. It also says reads should return the current value plus metadata without the caller caring whether the value came from user input, a formula, or a fitted model. 

That is the correct mental model.

The PRD also correctly identifies that dependency tracking, dirty recomputation, and lazy reads are central to the engine. The described flow is: input changes, downstream cells become dirty, dirty cells recompute on next read, and values are cached until invalidated. 

That is the right backbone for a TM1-inspired engine.

## The best product idea in the document

The best product idea is **schemas as products**.

The PRD maps launch use cases into schemas:

```txt
Marketing mix modeling
Sports betting research
Sales forecasting
Prospect scoring
```

Each schema has dimensions, hierarchies, measures, and differentiators. The PRD says each schema should ship as YAML plus a sample dataset and be installable, versionable, and forkable. 

That is excellent.

That turns the engine into a platform.

In plain English:

```txt
The Rust engine is the machine.
Schemas are the cartridges.
Apps are the user-facing products.
```

That is a strong business and architecture model.

## The main weakness: the scope is too big

The PRD is currently trying to launch with too many “first-class” features.

This list is too much for an early engine:

```txt
Rust core
Python bindings
WASM bindings
HTTP server
Sparse cube storage
Dependency DAG
Auto-feeder inference
Content-addressable artifacts
Model registry
Atomic blue-green model swaps
Point-in-time cube state
Calibration tracking
Walk-forward evaluation
OOD-vs-IS evaluation
Lasso
Ridge
ElasticNet
BayesianRidge
XGBoost
GLMs
ARIMA / ETS
Schema registry
CRDT editing later
LLM-native rules later
Hybrid ROLAP + MOLAP planner later
```

None of those ideas are bad. The problem is sequencing.

The PRD says Phase 2 adds Ridge, ElasticNet, BayesianRidge, XGBoost, GLMs, composition primitives, walk-forward, OOD-vs-IS, and calibration policy enforcement, with the acceptance criterion being that EXP-015’s OOD-vs-IS breakout is reproducible in three lines of cube code. 

That is an awesome target, but it is too much for Phase 2.

You should not add a “model-cell zoo” before the cube engine has proven:

```txt
1. writable cells
2. hierarchy rollups
3. rules
4. cross-cube references
5. persistence
6. trace
7. correctness tests
```

The model-backed layer is the exciting part, but it should sit on top of a boring, correct cube kernel.

## The most dangerous assumption

This line of thinking is dangerous:

```txt
Every cell value carries uncertainty.
```

I understand why the PRD says it. It is a differentiator. But as an engine rule, it is too absolute.

For a sports betting model, uncertainty metadata is essential.

For a finance planning cube, a lot of cells are just deterministic:

```txt
Gross Profit = Revenue - COGS
Total Spend = Paid Search + Meta + Display
Q1 = Jan + Feb + Mar
```

You can support uncertainty, but I would not make every cell return `(point, std)` as the base contract. I would make it:

```txt
CellValue {
  value
  value_type
  provenance
  uncertainty: optional
  trace: optional
}
```

Then model cells and probabilistic rules can attach uncertainty. Normal planning cells do not need fake `std = 0` everywhere.

That small change will make the engine easier to use outside predictive modeling.

## Auto-feeder inference is being over-promised

The PRD calls auto-feeder inference a differentiator and says model-cell feature contracts can automatically generate dependencies, avoiding manual feeders. 

That is a great idea, but it should not be positioned as solved.

This is one of the hardest parts.

A safe v1 should say:

```txt
Rules must declare dependency scope explicitly.
The engine can suggest/infer dependencies later.
Full-scan validation compares declared dependencies to observed dependencies.
```

In other words, do not start with:

```txt
The engine magically infers all feeders.
```

Start with:

```txt
The engine makes dependencies explicit, testable, and auditable.
```

That is still a huge improvement.

## The transfer inventory exposes the real gaps well

The “what’s not here” section is very useful. It correctly calls out that claw-edge lacks:

```txt
multidimensional cube structure
hierarchies / consolidations
cross-cube references
planning model versioning
multi-user concurrent editing
WASM-deployable engine
```

The notes specifically say the current system is a flat per-game record keyed by things like `game_id` and `model_version`, and moving to real cubes requires arbitrary-rank dimensions, compositional coordinates, and efficient sparse storage. 

That is the right gap analysis.

The same file also correctly says point-in-time state does not exist today. You can snapshot a prediction, but not “the entire cube as it was on April 28.” It recommends write timestamps, previous-value pointers, audit tables, snapshot isolation, or explicit versioning. 

That is exactly the kind of thinking you need.

## What I would change immediately

I would split this into **four separate documents**.

### 1. Product PRD

This should answer:

```txt
Who is this for?
What pain does it solve?
What does the first user experience look like?
What is out of scope?
What makes it different?
```

Keep the current PRD’s vision, target users, anti-personas, schemas-as-products, and success metrics.

### 2. Engine Semantics Spec

This is the missing document.

It should define:

```txt
Cube
Dimension
Hierarchy
Element
Measure
CellCoordinate
CellValue
Scenario
Version
Rule
Consolidation
Slice
Writeback
Trace
Security scope
Persistence model
```

This spec should be boring and precise. No ML zoo. No marketplace. No WASM. Just the rules of the world.

### 3. Rust Kernel Build Plan

This should be the actual Claude Code execution doc.

It should say:

```txt
Project structure
Crates
Data types
Trait boundaries
Test files
Benchmarks
Golden datasets
What not to build yet
```

This is what you hand to Claude Code.

### 4. Research Notebook / Prior Art Log

This is where you put:

```txt
TM1 lessons
DuckDB lessons
DataFusion lessons
Arrow lessons
Kylin / MOLAP lessons
Build Systems à la Carte notes
Sparse storage experiments
```

Do not mix this into the product PRD.

## The first working demo should be simpler

Right now, the PRD’s acceptance criteria are too tied to claw-edge V1.6 and EXP-015. That is useful, but it makes the first milestone overly ML-heavy.

Your first demo should be this:

```txt
A user edits Paid Search Spend for March.

The engine recalculates:
Clicks = Spend / CPC
Leads = Clicks × CVR
Customers = Leads × Close Rate
Revenue = Customers × AOV
Gross Profit = Revenue - COGS

Then it rolls March into Q1.
Then it rolls Tampa into Florida.
Then it shows a trace explaining the final Revenue number.
```

That one demo proves more of the TM1-style planning engine than reproducing a sports model prediction does.

The V1.6 reproduction test should still exist, but I’d move it to the **model-cell layer**, not the kernel’s first gate.

## Revised execution order

I would change the build order to this:

### Phase 0: Engine semantics

Deliverable:

```txt
docs/engine-semantics.md
```

Acceptance:

```txt
Every core object has a clear definition, JSON shape, invariants, and examples.
```

### Phase 1: Rust cube kernel

Build:

```txt
dimensions
elements
hierarchies
coordinates
sparse cell storage
write cell
read cell
consolidated rollup
```

Acceptance:

```txt
Jan + Feb + Mar = Q1
Tampa + Orlando + Miami = Florida
Weighted rollups work.
Missing cells behave predictably.
```

### Phase 2: Rules and trace

Build:

```txt
rule cells
dependency declarations
dirty marking
lazy recompute
trace output
cycle detection
```

Acceptance:

```txt
Changing Spend updates Clicks, Leads, Revenue, Profit.
Trace explains every value.
Cycles fail at definition time.
```

### Phase 3: Persistence and versions

Build:

```txt
snapshot
write-ahead log
scenario/version branches
rollback
point-in-time read
```

Acceptance:

```txt
Change a model, save it, branch it, roll it back, and get identical prior values.
```

### Phase 4: Model cells

Build:

```txt
Lasso only
feature contract
artifact load
imputation contract
predict
calibration metadata
```

Acceptance:

```txt
Reproduce V1.6 inference near-bit-exact.
```

### Phase 5: DuckDB bridge

Build:

```txt
load actuals from DuckDB
map rows into cube coordinates
compare Actual vs Forecast
variance rules
```

Acceptance:

```txt
Historical actuals live in DuckDB.
Writable forecast lives in the Rust cube.
Variance works across both.
```

### Phase 6: Bindings

Build:

```txt
Python binding
TypeScript/WASM binding
HTTP server
```

Acceptance:

```txt
Same cube returns same values in Rust, Python, browser, and server mode.
```

That order is more robust.

## The PRD should be less sports-betting centered

The transfer inventory is from claw-edge, so naturally the PRD inherits a lot of sports model thinking: OOD-vs-IS, calibration ratios, PIT histograms, edge buckets, betting simulator, Lasso/XGBoost, etc.

That is useful, but be careful.

If MarketingCubes is supposed to be a marketing + finance + planning engine, the PRD needs more examples like:

```txt
marketing budget allocation
campaign forecast
pipeline forecast
P&L impact
headcount planning
gross margin
cash forecast
scenario comparison
budget lock / approval
actuals vs forecast
```

Right now, the ML/research story is stronger than the finance planning story.

That is fixable, but important.

## What is missing from the PRD

Here are the gaps I would add before coding heavily.

### 1. A concrete finance cube example

Add a full example:

```txt
Cube: Finance_PnL
Dimensions:
  Scenario
  Version
  Entity
  Department
  Account
  Time

Measures:
  Amount

Rules:
  Gross Profit = Revenue - COGS
  EBITDA = Gross Profit - OpEx
  CAC = SalesAndMarketingSpend / NewCustomers
```

### 2. A concrete marketing-to-finance bridge

Add:

```txt
Marketing Spend
→ Clicks
→ Leads
→ Opportunities
→ Closed Won
→ Revenue
→ Gross Profit
→ Cash
```

This is the “wow” demo.

### 3. A security/writeback model

TM1 has data reservations for controlling write access to cube regions, and the IBM guide describes data reservations as exclusive write access to regions of a cube for individual users. 

Your PRD mentions multi-user editing later, but writeback permissions should appear earlier as a semantic concern.

Even a simple v1 needs:

```txt
Who can edit this slice?
Who can approve this version?
Who can lock this forecast?
Who can publish this scenario?
```

### 4. A testing doctrine

Add a section called:

```txt
Correctness Doctrine
```

Include tests for:

```txt
rollup correctness
weighted consolidation
missing cell behavior
rule recompute
cycle detection
cross-cube references
version rollback
trace accuracy
model artifact reproducibility
imputation consistency
```

The transfer inventory already proves why this matters. It documents the training/inference imputation mismatch where training used column means but production used hand-coded medians. The lesson is clear: the training pipeline must export the values it used, and inference must consume those exact values. 

### 5. A “do not build yet” list

This is crucial.

For v0, do not build:

```txt
CRDT collaboration
schema marketplace
LLM rules
ARIMA / ETS
XGBoost
all bindings at once
hybrid query planner
auto-feeder magic
full UI
```

Park them deliberately.

## My final verdict

The layout is good. The direction is correct. Claude did not point you in a bad direction.

But I would push back hard on one thing:

**This PRD is currently more ambitious than it is executable.**

That is not a reason to stop. It is a reason to sharpen it.

The right next move is not “start coding the whole thing.” The right next move is:

```txt
Turn this into an Engine Semantics Spec + Rust Kernel Phase 1 build brief.
```

The trophy path is real, but the first trophy is not model-backed cells or WASM. The first trophy is:

```txt
A tiny Rust cube engine that can write cells, roll up hierarchies, apply rules, recalculate dirty dependents, and explain the result.
```

Once you have that, everything else becomes believable.
