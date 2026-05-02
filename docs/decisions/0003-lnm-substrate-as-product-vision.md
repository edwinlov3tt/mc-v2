# ADR-0003: LNM substrate — AI-native planning kernel as the product vision

**Status:** Accepted
**Date:** 2026-05-01
**Deciders:** Project owner + implementing instance
**Phase:** spans 3–7 (sets the strategic framing for every phase past Phase 2)

---

## Context

Phase 1A shipped the kernel. Phase 1B + Phase 2A closed the benchmark gates and produced a measurement baseline. The roadmap in [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) names Phases 3 (model definition layer), 4 (LLM-assisted authoring), 5 (data integration), 6 (UI / internal app proofs), and 7 (productization / customer-facing Media Partner App). Each phase is sketched but the **strategic intent that ties them together** is not yet captured anywhere durable. Without that, future planners will re-litigate the framing at every phase boundary, and the answer will drift.

Two recent external sources crystallized the framing.

**GPT-5's "Large Numbers Model" framing** ([`../external-conversations/2026-05-01-chat-gpt-lnm-vision.md`](../external-conversations/2026-05-01-chat-gpt-lnm-vision.md)) argued the project is not "TM1 clone" but **AI-native planning kernel**: the LLM is a *model architect* that emits validated config, the engine is the calculation authority, and the user is "a business builder who knows what they want the model to do, and uses AI to assemble it safely." Proposed a three-layer separation:

1. Rust kernel — fast, strict, boring, correct.
2. Model definition layer — YAML / typed DSL; human-readable, LLM-readable, validated before execution.
3. LLM model builder — interviews the user, generates definitions, explains assumptions, creates tests.

**Claude Code's TM1-LOC analysis** ([`../external-conversations/2026-05-01-claude-code-tm1-loc-analysis.md`](../external-conversations/2026-05-01-claude-code-tm1-loc-analysis.md)) sized the work: TM1's full enterprise suite is 3–5M LOC of accumulated 40-year compatibility surface. The **actual analytic engine work** in TM1 is 150–250K LOC. MarketingCubes targets 50–100K LOC at LNM-substrate maturity (Phase 5), which is 3–7% of TM1's full suite and matches the scale of SQLite / early DuckDB / Sled / TigerBeetle / Cube.dev core. *Smaller is the whole point*: the LOC gap is from consciously not building enterprise IT integration, multi-client SDKs, multiple UI surfaces, replication, MDX, TI language, and four decades of edge-case patches.

A separate exploration considered using the claw-core NBA totals pricing engine (`~/Projects/claw-core`) as a second fixture / planning workflow demonstrator. Captured in [`../research-notes/dual-fixture-claw-stress-test.md`](../research-notes/dual-fixture-claw-stress-test.md). Conclusion: claw could be a tenant of the LNM substrate (Phase 5+) only if the kernel's `Expr` grammar is extended (`NormCdf` and similar transcendentals), but it is not on the critical path. Decision-quality edge in claw — Kelly trajectories, strategy portfolio scenarios — does map onto MC's planning superpowers; model-quality edge (predictions, calibration) does not. ADR-0003 records the framing; the claw decision is downstream and ADR-worthy on its own when/if Phase 5+ tenants are scoped.

The strategic question this ADR answers: **what *is* MarketingCubes V2 as a product, beyond the Phase 1 kernel?** The answer determines what every subsequent phase has to preserve as an invariant.

## Decision

**MarketingCubes V2 is an AI-native planning substrate.** A user describes a business in plain English; an LLM emits a validated model definition; a strict Rust kernel executes it. The LLM is the *model architect*, never the calculation engine. The kernel is the *judge*, never the author. The model definition layer is the contract between the two.

**Three layers, named and bounded:**

1. **Layer 1 — Kernel (`mc-core`).** Phase 1 ships this. Rust, deterministic, single-threaded, no `unsafe`, no `serde` in `mc-core`, no LLM exposure. Performance gates per [`../PERF.md`](../PERF.md). Adds capabilities through Phase 2 (optimization), Phase 5 (data ingest mechanics), and any later kernel work. **The LLM never writes Rust here.**

2. **Layer 2 — Model definition layer (`mc-model` or equivalent, Phase 3).** Declarative format (TOML / YAML / typed DSL — choice pending Phase 3A's parser-dep ADR). Schema-validated before execution. Compiles to the existing `CubeBuilder` / `Dimension::builder` / `Hierarchy::builder` / `Rule { … }` constructors. Round-trips byte-identical against `build_acme_cube()`. **The LLM emits this; the engine validates this; the kernel runs the result.** Includes lint, golden-test runner, explain-mode trace-to-prose, and a fixed schema vocabulary (Dimension, Hierarchy, Measure, Rule, Scenario, Version, Permission, DataSourceMapping, GoldenTest — the LLM cannot invent new top-level concepts).

3. **Layer 3 — LLM authoring (`mc-author` or equivalent, Phase 4).** Prompting layer. Interviews the user, emits Layer 2 definitions, surfaces validation errors back in plain language, generates golden tests, suggests improvements. Has *no direct path to Layer 1*. Output passes through Layer 2's parser before anything reaches the kernel.

**Non-negotiables, locked in by this decision:**

- The LLM never bypasses Layer 2. There is no LLM-emits-Rust path. There is no LLM-mutates-cube-internals path.
- Layer 2's schema is the rail. New top-level concepts require an ADR + spec amendment, not an LLM patch.
- Layer 1 stays the calculation authority. Floating-point arithmetic, dirty propagation, consolidation, snapshot semantics — all decided by Rust code, not by prompts.
- The kernel must be **generic**: measures, dimensions, rules, hierarchies, scenarios are all *data*. No business concept (Spend, Revenue, Marketing, Finance) is hard-coded into `mc-core`. The Acme demo is a fixture, not a kernel feature. (Already true at Phase 1A; this ADR locks it as a Phase 3+ invariant.)
- The product framing is **AI-native TM1 for marketing and finance planning**, not "TM1 clone." The differentiator is Layer 2 + Layer 3, not Layer 1's feature parity with TM1's 40-year accumulation.
- The user persona is the **vibe coder business builder** — technical enough to inspect outputs and guide the AI, not a formal FP&A architect or data scientist. Authoring should not require Rust, SQL, or pandas knowledge by Phase 4.

## Consequences

**Positive:**

- Phases 3, 4, 5, 6, 7 inherit a single shared framing. Phase 3A's parser-dep choice, Phase 4's prompting design, Phase 6's UI authoring surface, Phase 7's customer-onboarding story all land against the same three-layer principle. No re-litigation per phase.
- The product story is sharper than "TM1 clone." That matters for hiring, fundraising, customer pitching, and the scope discipline GPT-5 named: *the LLM should not write custom engine code for every company; the LLM should generate readable model definitions that the engine validates and runs*.
- Layer 2's schema becomes the **competitive moat**. TM1 doesn't have one (TI is a programming language, not a typed config). Anaplan has Modelscript but it's proprietary. A clean, LLM-emittable, lintable, golden-testable schema is a real differentiator.
- Layer 3's LLM is grounded by Layer 2's validator. Hallucinations are caught structurally, not at runtime. The "vibe coder" promise is safe to make because the engine is the judge.
- Phase 1's design discipline (kernel-as-data — measures/dimensions/rules/scenarios are all data, not hard-coded business concepts) was already correct for this decision. ADR-0001's "what we are NOT doing" list aligned with this without naming it.
- Tractable scope. 50–100K LOC at Phase 5 maturity is a 1–3 person team for 12–24 months — within reach. Comparable to SQLite / early DuckDB / Sled / TigerBeetle / Cube.dev core.

**Negative / accepted trade-offs:**

- Phase 3A's parser dep choice (TOML+serde vs custom DSL vs alternative) becomes load-bearing. A bad schema design here costs more than a bad kernel optimization in Phase 2 because Layer 3's LLM will be trained against it. **Phase 3A's brief must spend disproportionate care on schema vocabulary, error messages, and round-trip semantics.**
- The "vibe coder" user persona is narrower than "any planning user." Formal FP&A architects will find Layer 2 too restrictive (they want TI's programming-language flexibility). Pure non-technical users will find it too structured (they want a UI). The persona is the middle: a technical builder who wants AI assistance with rails. This is a deliberate market choice and limits the addressable market.
- The LLM-as-architect framing depends on LLM quality continuing to improve. If LLMs hit a quality ceiling that prevents reliable schema emission, Phase 4's value prop weakens. Mitigation: Phase 3 (model definition layer) is independently valuable — it's a TM1-quality declarative authoring tool — even if Phase 4's LLM never lands.
- Phase 7's customer-facing Media Partner App must respect the three-layer principle, which means no per-customer Rust forks. Customer-specific behavior lives in the customer's *model definition file* (Layer 2), not in `mc-core`. This is a hiring signal: anyone who joins thinking "we just write a feature for that customer" needs to be re-oriented to Layer 2 before they touch code.
- Several "TM1 features" deliberately don't ship: TI / TurboIntegrator (a full programming language), MDX query language, multi-client SDKs (C / Java / .NET / OData), Excel add-in (Perspectives), replication / clustering / hot-standby, LDAP / Kerberos / SAML / Cognos integration. Per the Claude Code analysis, these are 80%+ of TM1's LOC and are accommodations for a market we're not targeting. Customers who need them will not be customers; that's accepted.

**Reversal cost:**

- **Reversing the three-layer principle is one-way past Phase 3 ship.** Once Layer 2's schema lands and the LLM is trained against it, walking back to "LLM writes Rust directly" or "LLM mutates cube internals" invalidates everything Phase 3 built. The decision is therefore one-way at Phase 3A's commit; it is fully reversible before then.
- **Reversing the "AI-native" framing in favor of "TM1 clone" is reversible at any time** but means competing with IBM on their 40-year-accumulated home turf. Cheap to reverse strategically, expensive to reverse competitively.
- **The LLM-never-writes-Rust rule is permanent.** Even if some future operation seems to call for it (e.g., generating custom kernel optimizations per customer), the right path is a `mc-core` extension exposed via Layer 2, not an LLM-to-Rust escape hatch. Phase 7 productization MUST NOT cross this line; ADR-0003 supersedes any later decision that tries to.

## Alternatives considered

1. **TM1 clone (no LLM authoring layer).** Build a faithful Rust reimplementation of TM1's core, ship it as an open-source TM1 alternative. *Rejected:* IBM owns the addressable market, has 40 years of feature accumulation, and the differentiator (Rust + open-source) is not enough on its own. The Claude Code LOC analysis is dispositive: outbuilding TM1 in scope is impossible at small team scale; out-focusing them is the only viable path.

2. **Direct LLM-to-Rust code generation.** Each company's planning model becomes an LLM-generated Rust crate that compiles and links against `mc-core`. *Rejected:* fragile (every model requires a Rust toolchain), validation is at compile time only (no schema-level lint), the LLM has free rein to invent concepts the kernel doesn't support, and the resulting per-customer code is unauditable. GPT-5's framing is explicit about this trap.

3. **Spreadsheet replacement.** Position MC as a replacement for Google Sheets / Excel formulas with multidimensional rollups. *Rejected:* too narrow (the kernel is overkill for what spreadsheet users actually need), too crowded (Sheets / Excel / Airtable / Notion / Coda all already serve this), and loses the planning superpowers (writeback, scenarios, snapshot/rollback, derived rules) that the kernel's design was justified by.

4. **Single LLM-authored DSL with no separate model definition layer.** Skip Layer 2 entirely — let the LLM emit a typed Rust struct that the kernel consumes directly. *Rejected:* this is "LLM-to-Rust" with extra steps. Layer 2's value is precisely that it's a *human-readable, LLM-readable, version-controllable, lintable* artifact. Removing it removes the rails.

5. **Extend `Expr` with transcendentals (`NormCdf`, `Erf`, isotonic-map lookup) immediately to enable claw-core as a first tenant.** *Rejected for now, deferrable to Phase 5+:* the dual-fixture analysis ([`../research-notes/dual-fixture-claw-stress-test.md`](../research-notes/dual-fixture-claw-stress-test.md)) found claw's planning fit is partial — Kelly + bankroll + scenario composition map onto MC, but the prediction edge does not. Extending `Expr` is a real Phase 2+ ADR (kernel grammar change); coupling it to claw before Phase 3 ships is premature. Revisit in Phase 5 when data integration scope is set.

6. **Defer the strategic framing entirely; let it emerge phase by phase.** *Rejected:* the framing has *already* emerged (the master phase plan names Phases 3-7 and GPT's response named the LNM principle); the choice is whether to lock it in an ADR or let each phase re-litigate it. Locking it now is cheaper.

## Cross-links

- **Specs that govern Phase 1 (the foundation this builds on):** [`../specs/engine-semantics.md`](../specs/engine-semantics.md), [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md). Both lock during their phase; this ADR sets framing for *future* briefs (Phase 3A onward), not these.
- **Master phase plan:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phases 3 / 4 / 5 / 6 / 7 are the LNM substrate; this ADR records the strategic intent across all of them.
- **Operating manual:** [`../../CLAUDE.md`](../../CLAUDE.md). §0 (hierarchy of authority), §1 (allowed deps), §1.1 (current deferrals).
- **Prior ADR:** [`./0001-phase-1-scope.md`](./0001-phase-1-scope.md) — Phase 1 kept the kernel generic *as a precondition* for this decision. The "what we are NOT doing" list there (no model cells, no DuckDB, no `serde` in `mc-core`, no `CellStore` trait) is what makes the LNM substrate buildable; do not relax those constraints in `mc-core` even when Phase 3+ wants them in surrounding crates.
- **External conversations (verbatim primary sources):**
  - [`../external-conversations/2026-05-01-chat-gpt-lnm-vision.md`](../external-conversations/2026-05-01-chat-gpt-lnm-vision.md) — GPT-5's three-layer framing.
  - [`../external-conversations/2026-05-01-claude-code-tm1-loc-analysis.md`](../external-conversations/2026-05-01-claude-code-tm1-loc-analysis.md) — TM1-scale comparison and 50-100K LOC sizing.
  - [`../external-conversations/chat-gpt-response-1.md`](../external-conversations/chat-gpt-response-1.md), [`../external-conversations/claude-response-2.md`](../external-conversations/claude-response-2.md) — earlier Phase 1 scope-discipline argument that produced ADR-0001.
- **Research notes:**
  - [`../research-notes/dual-fixture-claw-stress-test.md`](../research-notes/dual-fixture-claw-stress-test.md) — claw-core as a candidate Phase 5+ tenant; analysis of where MC fits and doesn't fit a real betting workflow.
- **Future deliverables this ADR shapes (none exist yet):**
  - `../specs/phase-3-model-definition-brief.md` — must respect three-layer principle.
  - `../decisions/<NNNN>-phase-3-parser-dep-choice.md` — schema vocabulary + parser dep, with the keep-out-of-`mc-core` invariant.
  - `../decisions/<NNNN>-expr-extension-for-tenants.md` — when (or whether) to extend `Expr` with `NormCdf` / `Pow` / etc. for non-marketing tenants.

## Implementation sketch — Phase 2 / Phase 3 acceptance signals for the LNM vision

Five concrete signals across the next phases that the LNM substrate is being built correctly. **None of these are scope changes to currently-`proposed` or currently-`planned` phases** — they are the framing every phase is checked against. If a phase ships and one of these signals is missed, the next phase's brief should explicitly call it out.

1. **Phase 2 (optimization) preserves kernel-as-data.** Phase 2B (consolidation fast path) and any subsequent Phase 2C+ optimizations must not introduce business-concept hard-coding. If `Arc<Hierarchy>` ships in Phase 2B, the change is to *how hierarchies are stored*, not *which hierarchies the kernel knows about*. Hierarchies remain data. (Existing Phase 2B handoff already respects this; flagging here as the framing constraint.)

2. **Phase 3A (model definition layer) ships a vocabulary the LLM can target without seeing kernel internals.** The schema's top-level vocabulary (Dimension / Hierarchy / Measure / Rule / Scenario / Version / Permission / DataSourceMapping / GoldenTest) is fixed. The Acme demo round-trips byte-identical. Acceptance gate: an LLM given only the schema doc (not `mc-core/src/`) can emit a syntactically valid roofing-company model on first try. (Aspirational; the formal Phase 3A acceptance gate is round-trip-correctness.)

3. **Phase 3A's parser dep stays out of `mc-core`.** `serde` / `toml` / `yaml-rust` / whatever Phase 3A chooses lands in a new crate (likely `mc-model`). `mc-core/Cargo.toml` does not change. The Phase 3 parser-dep ADR captures this as an explicit invariant.

4. **Phase 3A ships `mc lint <model.yaml>` and `mc test <model.yaml>` from day one.** GPT-5 named these as the most important features for the AI-native promise. Lint catches schema violations; test runs the model's golden tests. Both must be standard CLI commands shipped with Phase 3A — not a "Phase 3B follow-up." Otherwise the LLM has no validation rail to push errors back through.

5. **Phase 4 (LLM authoring) does not have read access to `mc-core/src/` or `mc-model/src/`.** The LLM sees Layer 2's schema documentation, the validator's error format, the golden-test runner's output format, and the explain-mode trace format. Nothing else. If Phase 4's prompting layer needs internal knowledge to function, that knowledge gets promoted to Layer 2 documentation, not embedded in the prompt.

These are guard rails, not acceptance criteria for any single phase. Each phase's brief defines its own gates; these signals are what the *cross-phase invariant* looks like.

## Notes

The "claw-core as second tenant" question (covered in [`../research-notes/dual-fixture-claw-stress-test.md`](../research-notes/dual-fixture-claw-stress-test.md)) re-opens at Phase 5 when data integration is scoped. The path: extend `Expr` with `NormCdf` + `Pow`, write a strategy-portfolio cube that consumes claw's `wagers` table via Phase 5's CSV importer or D1 adapter, demo it as one tenant alongside the roofing-company example. **Not on the critical path; revisit when Phase 5 begins.**

The "vibe coder" persona naming is GPT-5's — the project owner uses it explicitly. The persona description (technical enough to inspect outputs and guide AI, not a formal FP&A architect or data scientist) is now the canonical user persona for any product surface decision past Phase 6. If a UI / CLI / DSL choice doesn't fit this persona, it's the wrong choice.

The Phase 1 invariants in ADR-0001 (no `serde` in `mc-core`, no `CellStore` trait, concrete `HashMapStore`, no `MeasureRole::Both`, single hierarchy per dim) are what made this ADR possible. Phase 3+ may want to add `serde` *to a sibling crate* (`mc-model`), introduce `CellStore` as a trait *for storage backend swap*, and add `MeasureRole::Both` *if a real planning model needs it* — but each of those is a separate ADR, not a relaxation of Phase 1's discipline. The pattern is: Phase 1's "no" was a precondition; Phase N's "yes" requires its own justification.
