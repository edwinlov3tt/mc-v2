# Mosaic — Positioning + TM1 Scope Comparison

> **Mosaic** — a Large Numbers Model: an n-dimensional planning engine where every cell of your business is computed, traceable, and tied to the inputs that move it.

> **Framing.** Mosaic is **not a TM1 clone**. It is an AI-powered Large Numbers Model (LNM) system: a multidimensional engine for building, validating, testing, and operating large numerical models across finance, marketing, prospecting, sports betting, sales forecasting, and analytics. TM1 is the closest ancestor because it proved the cube/planning model — Mosaic keeps the multidimensional brain, replaces legacy ceremony with schema validation and AI-assisted authoring, and adds golden tests, traceability, and (eventually) model-backed cells with uncertainty.

**Last updated:** 2026-05-03 (project renamed from MarketingCubes V2 → Mosaic the same day; the `MC` / `mc-` naming convention in code stays — see [`../../CLAUDE.md`](../../CLAUDE.md) for the rename note)
**Maintained by:** project lead
**Companion docs:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) for sequencing; [`../process-notes.md`](../process-notes.md) for operational rules.

---

## What is a Large Numbers Model?

```
LLMs are for words.
LNMs are for structured numbers, assumptions, rules, forecasts, tests, and traceable decisions.
```

A Large Numbers Model is like a business spreadsheet, a TM1 cube, a forecasting model, and a validation test suite **fused into one AI-readable system**. Where an LLM holds language and reasoning, an LNM holds:

- **Structured numerical models** with multidimensional coordinates (time × market × scenario × measure × ...).
- **Deterministic formulas + assumptions** authored in YAML (today) or natural language via LLMs (Phase 4).
- **Inputs** loaded from CSVs, APIs, or model-backed cells.
- **Tests** (goldens) that pin specific cell values to known-good answers.
- **Traceability** — every result chains back to the rules + inputs that produced it.
- **Uncertainty** (eventually) — model-backed cells with confidence intervals and walk-forward validation.

What an LLM is to language, Mosaic is to the numbers that run a business: every cell predicted, every dependency tracked, every assumption auditable.

---

## The wedge: AI-powered Large Numbers Models

Mosaic is **not only a marketing planning tool**. Marketing is one proof domain. The broader wedge is the LNM substrate: a structured, AI-readable numerical model that combines dimensions, hierarchies, formulas, assumptions, inputs, tests, traces, and (eventually) predictive model-backed cells.

```
General engine.
Specific schemas.
One proof at a time.
```

Mosaic is the **general engine**. Domain schemas are the **products** that ride on top:

### First-wave proof domains (schemas, not separate codebases)

| Domain | What the LNM holds | First-wave priority |
|---|---|---|
| **Finance planning / FP&A** | Budgets, variances, plan-vs-actuals, scenarios, version comparison | High — closest to TM1's historical sweet spot |
| **Marketing mix + campaign planning** | Spend → Clicks → Leads → Customers → Revenue funnel; channel/market breakdown; what-if scenarios | High — Acme demo is here; fastest to validate internally |
| **Sports betting research / edge modeling** | Player/team metrics, situational adjustments, edge calculations, walk-forward backtest | Medium — strong differentiation case for model-backed cells |
| **Prospect scoring / lead modeling** | Lead attributes, scoring rules, conversion-likelihood scenarios | Medium — good fit for AI-authored rules |
| **Sales forecasting** | Pipeline, conversion rates, capacity constraints, period-over-period | Medium — overlap with FP&A |
| **Demand / inventory planning** | SKU × location × time × scenario; reorder rules; uncertainty bands | Lower priority for first wave; revisit when model-backed cells land |

**The first proof can still be marketing or finance** because that's the fastest domain to validate internally (Acme is already there; Phase 5 actuals + Phase 6 UI close the loop). But the **product identity is broader** — AI-powered numerical modeling, not marketing software.

---

## TM1 ancestry — what we keep, what we don't

TM1 is the historical proof point that the multidimensional-cube + rules + consolidations + dirty-tracking shape **works** for planning at enterprise scale. Mosaic borrows TM1's brain. It does NOT clone TM1's 40 years of accumulated enterprise surface area (TurboIntegrator, feeders, chores, Excel client, replication, cell-level security, etc.).

**Decisions legend:**

- **build** — Mosaic builds its own version (kernel-level concept, can't be outsourced).
- **simplify** — Mosaic ships a smaller, modern equivalent that does 80%+ of the value with ~20% of the surface area.
- **integrate** — Mosaic does NOT rebuild this; integrates with a modern third-party tool when needed (dbt / Airbyte / DuckDB / Snowflake / Datadog / etc.).
- **ignore** — Out of scope. Never built. The wedge doesn't need it.

### Capability matrix

| TM1 capability | Mosaic equivalent | Decision |
|---|---|---|
| **Multidimensional cube engine** (dimensions, hierarchies, sparse storage) | `mc-core` kernel — N dims (currently 6 in Acme; the engine is dim-count-agnostic), hierarchies, `HashMapStore` sparse, `Cube` API | **build** ✓ shipped (Phase 1A) |
| **Rules / TM1 Rules language** (deterministic per-cell formulas) | `Rule` + `Expr` + structured-tree YAML; friendly formula syntax via Phase 3D (`Revenue = Customers * AOV` strings compile to the existing AST) | **build** ✓ shipped (Phase 1A + 3D) |
| **Consolidations (Sum, WeightedAverage, etc.)** | `AggregationRule` enum + `Consolidator` | **build** ✓ shipped (Phase 1A) |
| **Dirty propagation / dependency tracking** | `DirtyTracker` (bitset-backed since Phase 2D) + `DependencyGraph` (lazy) | **build** ✓ shipped (Phase 1A + 2D) |
| **Feeders** (manual sparse-cube hints) | Automatic dependency graph; *may* need explicit hints later for multi-cube / model cells / very large production cubes | **simplify** — defer feeders; revisit if real workload data forces it (Phase 5+) |
| **Snapshots / Sandboxes** | `Cube::snapshot` + `rollback_to`; deep-clone today, COW deferred per ADR-0003 | **build** ✓ shipped (Phase 1A) |
| **Versions / Scenarios** | `Version` + `Scenario` dimensions with `VersionState` (Working / Submitted / Approved / Archived) | **build** ✓ shipped (Phase 1A) |
| **Cube definition (model authoring)** | YAML model files via `mc-model` crate; structured-tree OR formula-string rule bodies (Phase 3D) | **simplify** — YAML > custom DSL; ✓ shipped (Phase 3A + 3D) |
| **Model validation / diagnostics** | `mc-model::validate` + `mc-model::lint` + 4 CLI verbs (`validate / inspect / lint / test`) + stable diagnostic codes (MC1xxx parse / MC2xxx validate / MC3xxx lint / MC4xxx reserved) + JSON envelope for LLM/UI consumption | **build** ✓ shipped (Phase 3B) — modern equivalent of TM1's "rule check" with structured codes for AI iteration |
| **Test fixtures / golden values** | `canonical_inputs:` + `test_fixtures:` blocks in YAML; sibling CSV; `mc model test` runs goldens | **build** ✓ shipped (Phase 3C) |
| **Friendly formula syntax** | Phase 3D: `body: "Revenue * (1 - COGS_Rate)"` compiles to the existing AST | **build** ✓ shipped (Phase 3D) |
| **TurboIntegrator** (ETL: source connections, transformations, scheduled jobs, retries, idempotency, lineage, audit) | Clean import contract (CSV / DuckDB / one real platform feed); no clone. Real ETL via dbt / Airbyte / Fivetran / similar when needed | **integrate** — *do NOT clone TI*. AI authoring helps with import mapping, not with scheduling/retries/audit/lineage. Phase 5 = small import contract; production ETL stays external |
| **Chores** (scheduled jobs / cube-side cron) | None internal; defer to external scheduler (cron, Airflow, GitHub Actions, dbt Cloud) | **integrate** — never build internal scheduling |
| **Excel client / Perspectives / TM1Web** | Web UI with spreadsheet-native ergonomics: copy/paste, CSV import/export, Excel export, grid keyboard controls, formula-like authoring | **simplify** — modern web grid that *feels* spreadsheet-native, no Excel add-in (Phase 6) |
| **LLM-assisted authoring** | Phase 4 — LLMs emit YAML against the schema; iterate on stable diagnostic codes; structured re-prompting | **build** — this is the wedge differentiator (TM1 doesn't have it; the formula-syntax foundation in Phase 3D is what makes LLM authoring tractable) |
| **Model-backed cells / uncertainty / calibration** | Probabilistic cells, walk-forward testing, uncertainty metadata (per PRD) | **build** — but **defer** until basic planning product is usable. Phase 6B+ research track. *Easy way to blow up scope if started early* |
| **Cell-level security** (TM1 cell security cubes) | Role + scenario + model permissions; not per-cell ACLs | **simplify** — coarser model-shape security in Phase 6; per-cell only if a real customer needs it |
| **Data Reservation** (TM1's lock-out for in-progress edits) | Scenario / version locks + audit trail; soft-locks already in `mc-core` | **build minimal** — already partial in kernel; surface in UI layer |
| **Aliases / Subsets / Attributes** (alternate element names; named element subsets) | Element `name`/`id`/`description`; YAML element refs by name; subsets deferred | **simplify** — descriptions cover most alias use cases; subsets if real demand surfaces |
| **Replication / multi-server cubes** | Single-cube engine; no replication semantics in scope | **ignore** — Phase 7+ at earliest |
| **Drill processes** (cell-to-source detail navigation) | Trace API (`Cube::read_with_trace`) shows the rule chain; cell-to-actuals-source is Phase 5+ | **build minimal** — trace already exists; source-detail drill = Phase 5+ |
| **Process / TI scripting language** | None. AI-assisted authoring + import contract instead | **ignore** as a language; **integrate** the ETL layer |
| **Authentication / SSO / multi-user concurrency** | Auth + audit trail + multi-user concurrency for small internal team (≤ 10 users) | **build minimal** — Phase 6 first usable product gate; *the architecture needs a transaction/audit story before the UI becomes serious* |
| **Migration tooling / cube versioning** | Model `format_version: 1` + future migration semantics when v2 is needed | **build minimal** — defer until a real user has authored a cube the format authors can't reflexively rewrite |
| **Admin monitoring / health dashboards / metrics** | Operational telemetry, query-cost diagnostics, cache-hit rates | **integrate** — Datadog / Grafana / OpenTelemetry, not internal admin UI |
| **Long-tail enterprise surface** (40+ years of accumulated features) | Deliberately not in scope; will not be matched | **ignore** — by design |

---

## Schemas as products

The **product** is the LNM substrate: a system for creating trustworthy numerical models. The **schemas** are the use cases that ride on it.

```
Product (substrate):
  Mosaic — the Large Numbers Model engine

Schemas (use cases that install on the substrate):
  Marketing Mix Model
  Sports Betting Edge Model
  Prospect Scoring Model
  Sales Forecast Model
  FP&A Planning Model
  Inventory Demand Model
```

Each schema is an installable, versionable, forkable artifact: a model file (or family of files) that defines dimensions, measures, rules, fixtures, and goldens for that domain. The Phase 3A schema (YAML) + Phase 3B diagnostics + Phase 3C test fixtures + Phase 3D formula syntax + Phase 4 LLM authoring (when it lands) all support this pattern.

**This positions Mosaic as a platform, not a vertical product.** First customers may use it for marketing planning (Acme is already there). Second-wave customers may use the same engine for sports-betting research, with a different schema. Third-wave for FP&A. Same engine, different installable schemas.

---

## Honest scale (with the broader framing)

| Target | % there |
|---|---|
| **LNM kernel (multidim engine + rules + consolidation + dirty tracking + snapshots + diagnostics + tests + formula syntax)** | **75–85%** today (post-Phase 3D). Solid foundation; the structural pieces are in place. |
| **First usable internal planning product** (auth + UI + actuals + persistence + multi-user) for ONE schema (e.g., marketing mix) | **35–45%** — kernel is solid; product surface (UI, auth, actuals, persistence) is mostly Phase 5 / 6 work |
| **Second proof schema** (e.g., sports-betting research) demonstrably runs on the same kernel without code changes | **20–30%** — once Phase 3D's formula syntax shipped, schema authoring is purely YAML. The blocker is having a domain expert author the schema; the kernel is ready |
| **General LNM platform with multiple shipped schemas** | **15–25%** — depends on Phase 4 LLM (lowers schema-authoring barrier dramatically) + Phase 5 actuals + Phase 6 UI |
| **Enterprise TM1 replacement** (everything in the table above, at enterprise scale) | **8–12%** — deliberately not the target |

---

## What I'd push back on (corrections to common framings)

**1. AI does not replace TurboIntegrator.** AI helps with import mapping, schema explanation, transformation drafting, error debugging. ETL itself is scheduled jobs, retries, idempotency, lineage, source authentication, schema-drift handling, partial-failure handling, audit logs, data reconciliation, and access controls. Don't pretend AI replaces ETL — *integrate* with modern ETL tools instead.

**2. Feeders aren't "solved" yet.** The automatic dependency graph handles the current deterministic shape, but feeder-like problems may resurface with multi-cube references, conditional rules, model cells, external actuals, sparse derived cells, dynamic dimensions, or large production cubes. Don't say "we don't need feeders" — say "we will avoid manual feeders as long as possible, but we may eventually need explicit dependency or materialization hints for advanced models."

**3. Don't dismiss Excel too early.** Excel is still the planning user's comfort zone. Don't build an Excel add-in soon, but DO plan for copy/paste from spreadsheets, CSV import/export, Excel export, grid keyboard controls, and formula-like authoring. The UI should *feel* spreadsheet-native even if it's not an Excel client. (Phase 3D's formula syntax is the first concrete step toward this.)

**4. Security and concurrency cannot wait until "enterprise later."** Even for 5 internal users you need a clear answer to: what happens if two people edit the same cell? What if one changes the model while another is planning? Who changed Spend from $10k to $15k? Can I roll back just my scenario? The architecture needs a transaction/audit story *before the UI becomes serious*, not after.

**5. Model cells are a moat, but also a trap.** Probabilistic / model-backed cells are exciting and differentiated, but they're the easiest way to blow up scope. **Don't let model cells become Phase 4** if the basic planning product isn't usable yet. Sequence: Phase 3D (formulas — ✓) → Phase 4 (LLM YAML) → Phase 5 (actuals) → Phase 6 (UI). Model cells are Phase 6B / 7 research track.

**6. "Large Numbers Model" needs a one-line explanation every time.** People will not instantly know what LNM means. Default explanation: *"a business spreadsheet, TM1 cube, forecasting model, and validation test suite fused into one AI-readable system."*

**7. Don't let "general engine" mean "build everything."** The structure is: general engine, specific schemas, one proof at a time. The substrate is general; the wedge is one schema at a time.

---

## The narrow path (sequencing)

The first usable shipped product (Phase 6 exit gate) needs:

- A model an analyst can author in YAML — **shipped (Phase 3A)**
- A linter that catches mistakes before they become wrong numbers — **shipped (Phase 3B)**
- Test fixtures so the model is reproducible — **shipped (Phase 3C)**
- Friendly formula syntax so authors don't write `body: { mul: [...] }` by hand — **shipped (Phase 3D)**
- LLM authoring so non-engineers can describe a model in plain English — **Phase 4**
- Real actuals from one external source — **Phase 5**
- A web UI that feels spreadsheet-native — **Phase 6**
- Auth + audit + multi-user for a small internal team — **Phase 6**
- One shipped proof-of-value internal use case — **Phase 6 exit gate**

Then second-wave: a second schema demonstrating the engine isn't tied to the first proof's domain.

Everything in the **integrate** and **ignore** rows above is deliberately not on this path.

---

## What this doc is for

- **Stops accidental TM1-cloning.** When a future phase considers building feeders / TurboIntegrator / chores / cell security / replication / Excel-add-in, this doc is the artifact that says "no, we decided to integrate / simplify / ignore — here's why."
- **Stops accidental scope explosion.** When someone proposes "let's add model cells / multi-cube / advanced security in Phase 4," this doc points at the wedge sequencing and the moat-but-trap warning on model cells.
- **Anchors the strategic positioning.** When asked "is this TM1?" the answer is *"It's an AI-powered Large Numbers Model platform. TM1's multidimensional brain in a modern body, with schema validation + AI-assisted authoring + golden tests + traceability + (eventually) model-backed cells."*
- **Stops accidental vertical-product framing.** When someone proposes "let's brand this as a marketing tool," this doc points at the schemas-as-products pattern and the broader LNM positioning.

---

## Open strategic questions (revisit periodically)

These don't need answers today; they need to *not be silently decided* by accumulated implementation choices.

1. **What's the first real customer wedge?** Internal proof (per the master plan's Phase 6 exit gate) using which schema? Marketing? Finance? Both? An external pilot? Self-use? The wedge shapes which "integrate" rows actually need partner integrations vs which can wait.
2. **When do model-backed cells stop being a research track and become a feature?** Tied to wedge identity. If the customer schema is "media planner who wants probabilistic spend forecasting" or "sports-betting researcher who wants edge calculations with confidence intervals," model cells move forward. If the customer schema is "FP&A team migrating off Hyperion / TM1," model cells stay deferred.
3. **Where does the audit story live in the kernel?** Today: `Provenance` + revision numbers + soft locks. Multi-user concurrency at scale needs a clearer transaction model — write-conflict resolution, optimistic concurrency, audit-log persistence — and the architecture decision belongs *before* the UI lands, not after.
4. **What's the schema-evolution / migration story?** Current `format_version: 1` (single integer) holds for now. When v2 ships, what does the migration look like? Hand-rolled in `mc-model`? Generic transformer? This is post-Phase-3D ADR territory.
5. **Where does "deterministic planning" stop and "ML planning" start?** The PRD frames them on a continuum (deterministic rules + model-backed cells). The wedge identity from #1 determines how aggressively to blur the line.
6. **Schema marketplace / forkability.** If schemas are first-class artifacts, do they install via a registry? Are they versioned? Forked? Phase 6+ question; doesn't need an answer until a second schema ships.

---

*This is a living doc. Revise when the wedge shifts, when a new TM1 capability becomes relevant, when an "integrate" decision needs to flip to "build" because the integration layer didn't exist or didn't work, or when a new schema family reveals a gap in the LNM substrate.*
