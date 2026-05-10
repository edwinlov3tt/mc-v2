# Mosaic — Architecture, Deployment Shapes, and Product Vision

**Status:** Reference document — not binding over ADRs
**Date:** 2026-05-07
**Last updated:** 2026-05-09
**Compiled by:** Claude Desktop, synthesizing multi-turn conversation with project owner + GPT review
**Scope:** Cross-cutting architectural context, org/workspace structure, deployment shape narrative, database/storage strategy, and strategic product vision

> **This is a reference document, not a binding architectural specification.** When this document appears to conflict with an ADR, the ADR wins. Read this document to understand the "why" and overall shape of the system. Read ADRs to know the binding decisions.
>
> ADR hierarchy for this document's topics:
> - **ADR-0025** — binding for kernel discipline and deployment-shape sequencing
> - **ADR-0026** — binding for org/workspace/resource/grant architecture
> - **`docs/security/mosaic-security-posture.md`** — binding secure-development baseline
>
> For current phase status, see [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) and [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — those documents stay current; this one does not.

---

## Part 1: What Mosaic is becoming

### The strategic frame

Mosaic is a **portable semantic runtime for business numbers**. It turns raw data into modeled cells, formulas, scenarios, traces, judgments, narratives, benchmarks, and reusable cartridges.

This is meaningfully different from:
- **Spreadsheets** — no cube semantics, no traces, no determinism guarantees
- **BI tools** — relational, not multi-dimensional; no rules engine; no scenarios
- **Data warehouses** — storage, not modeling; no cube semantics; no narratives
- **Planning tools** — TM1 and Anaplan are the closest analogues; Mosaic is a modern successor targeting the same use cases with a radically more accessible architecture
- **AI report generators** — LLM-based, nondeterministic, token cost per report

### The five strategic pillars

Mosaic's differentiation comes from the combination of five capabilities. Many tools have one or two. Mosaic has all five. The combination is the moat.

**Pillar 1: Cube semantics.** Multi-dimensional coordinates, hierarchies, consolidation, scenarios, versions. Rules that reference cells across coordinates. Writeback as a first-class operation. TM1-equivalent capability with modern infrastructure.

**Pillar 2: Deterministic evaluation.** Same inputs produce same outputs, every time. No randomness in evaluation; no LLM-driven nondeterminism in core paths. Auditable and testable.

**Pillar 3: Snapshot/rollback.** First-class kernel primitive. Every cell change is reversible by construction. Enables what-if planning, backtesting, walk-forward validation. Other planning tools treat snapshots as backup; Mosaic treats them as a working primitive.

**Pillar 4: Agent-ready query layer.** CLI verbs, MCP tools, stable JSON schemas, stable exit codes, idempotent operations. LLMs and agents can use Mosaic natively without wrapping it in Python scripts.

**Pillar 5: Deterministic interpretation ledger.** Narrative engine produces structured findings (severity, evidence, template_id). Every narrative is logged. Cross-period analysis fires deterministically from ledger contents. Benchmark aggregation builds privacy-aware intelligence. LLM intelligence is amortized at design time (template authoring), not runtime (report generation).

### The strategic positioning

> Mosaic does not use AI to rewrite the same report every month. It uses AI to design reusable interpretation rules once, then runs those rules deterministically against live data forever. Every interpretation is logged as evidence, building a historical ledger that powers trend detection and benchmark aggregation over time.

The key insight: **LLM intelligence is amortized at design time, not runtime.** This produces:
- No token cost per report
- No hallucination risk on core reporting paths
- Auditable, defensible outputs
- Deterministic regression testing
- Cross-account intelligence via the interpretation ledger

### The compounding-knowledge thesis

An LLM-generated report is throwaway. Every report costs tokens; every analyst's expertise is captured only in their head; every account's history is locked in unstructured docs.

Mosaic inverts this. Every rule an analyst writes is captured forever. Every narrative is logged with structured evidence. Every benchmark comparison contributes to a growing library. **The agency or enterprise gets smarter with each report, not just faster.**

This is the strategic difference from LLM-based competitors:
- LLM tools: faster iteration on individual reports
- Mosaic: compounding institutional knowledge that becomes IP

Different products for different jobs. An LLM is a calculator; Mosaic is a knowledge base that produces reports. The reports are the output, but the knowledge base is the asset.

---

## Part 2: Kernel discipline (constitutional commitments)

These rules apply to all current and future phases. **The binding form of these rules is in ADR-0025.** This section provides the rationale narrative.

### Rule 1: Kernel stays sync-only

The Mosaic kernel (`mc-core`) is sync. No async/await, no tokio imports in the kernel. Async runtimes are permitted only in deployment-shell crates.

**Why:** async contaminates everything it touches. Sync APIs can be wrapped in async runtimes; the reverse is much harder. The kernel's sync discipline has held through multiple phases; breaking it now is a high-cost decision with unclear benefit.

### Rule 2: No cloud-specific or server-specific assumptions in the kernel

The kernel doesn't know about S3, HTTP, gRPC, authentication, multi-tenancy, or session management. All deployment concerns live in shell crates that wrap the kernel.

**Why:** the same kernel runs in every deployment shape. Deployment-specific assumptions in the kernel mean future shapes become rebuilds rather than extensions.

### Rule 3: No external database as cube storage

DuckDB, Postgres, SQLite, and other databases are NOT cube storage backends. Cube state lives in the kernel's `HashMapStore` (current) or future binary serialization formats.

**Why:** cube semantics (hierarchies, consolidation, rules, scenarios) don't map cleanly to relational tables. Forcing cubes into a relational engine produces a worse version of both engines.

### Rule 4: Explicit provenance, dependencies, revisions, snapshots

Every cell value carries provenance. Rule dependencies are tracked. The cube has a revision counter. Snapshots are first-class operations. These properties are the foundation for caching, agent-readiness, audit trails, and the interpretation ledger. Don't compromise them for performance shortcuts.

### Rule 5: Four cell-value sources compose deterministically

Cell values come from exactly four sources:
1. Compiled YAML model (definitions and canonical_inputs)
2. Tessera imports (recipe-driven data ingestion)
3. Post-hoc writes (via `mc model write`)
4. Rule-derived computations

The interaction between these sources is deterministic and documented in process-notes Rule 11.

### Rule 6: The five pillars are inviolable

Architectural decisions that compromise any of the five strategic pillars to optimize another are rejected unless explicitly ADR-documented and project-owner approved.

---

## Part 3: Organization and workspace architecture

**The binding form of this architecture is in ADR-0026.** This section provides the narrative context and worked examples. Read ADR-0026 for the decision rules, capability grant model, template inheritance chain, and resource scoping.

### Why this matters

The org/workspace architecture determines whether Mosaic can serve:
- A solo founder running multiple projects (personal use)
- An agency managing 50 client reporting workflows (small agency)
- An agency with enterprise clients who own their own data (agency + managed orgs)
- A vendor distributing domain cartridges to partner agencies (partner network)
- A holding company with acquired brands (enterprise)

Without this architecture, each of these shapes requires bespoke integrations. With it, the same kernel serves all of them.

### The shape

The four-entity model (full decision rules in ADR-0026):
```
Organization
└── Workspace
    └── Cube (model)
        └── Cell (coordinate × revision)

Plus: Managed Org Relationship
```

The decision rule that resolves 90% of ambiguity:
> Use a workspace when the parent org owns the operating environment.
> Use a separate org when the entity needs its own identity, users, billing, audit trail, data ownership, or downstream workspaces.

### The strategic pitch this enables

> Mosaic adapts to your business shape. Solo founder? One org with workspaces for each domain. Small agency? One org with workspaces for each client. Enterprise client? Their own org with workspaces for each department. Holding company? Parent org with child orgs for each acquired brand. Partner network? Capability grants without ownership transfer.
>
> Same architecture. Same engine. Different scales. You don't outgrow Mosaic; Mosaic grows with you.

This positioning spans the gap most B2B tools fail on — tools either feel too heavy for solo users or too limited for enterprise. The few that span both (Notion, Slack, Linear) became massively valuable companies precisely because their architecture didn't force a tradeoff.

---

## Part 4: Deployment shapes

**The binding sequencing rules are in ADR-0025 Decision 2.** This section provides the narrative rationale.

### The progression

**Shape 1 — Library + CLI (current):** File-based. Single-user. The foundation.

**Shape 2 — Local web UI (Phase 6B):** `mc serve` locally. Same file system. Single-user. Consumes existing CLI/MCP surface.

**Shape 3 — Demo server (Phase 6D):** `mc-demo-server` for the upload-to-narrative demo. In-memory. Single-purpose. Proves the narrative engine; not production architecture.

**Shape 4 — Service daemon (Phase 8):** Long-running. LRU cache. Snapshot persistence. Crash recovery. Multi-cube, org-aware. First real service deployment. Personal-use via Tailscale or Cloudflare Tunnel becomes viable here.

**Shape 5 — Topology-aware cache (Phase 9):** Builds on daemon. Predictive prefetch from cube structure. Cache by `(coordinate, revision)`. This is where Mosaic feels "alive" for interactive use.

**Shape 6 — Cloud service (Phase 9-10):** Multi-tenant. Customer cubes in managed object storage (S3/R2). Auth, billing, observability. Real infrastructure. Don't start until daemon has real users.

**Cloudflare-native option (Phase 9-10, speculative):** Mosaic compiled to WebAssembly running in Cloudflare Workers; Durable Objects for in-memory cube state; R2 for serialized cubes. Well-aligned with Cloudflare-heavy users. This is Phase 9-10 territory and depends on customer demand — do not build speculatively.

**Shape 7 — Semantic overlay (Phase 10+):** Mosaic in front of a customer's existing data warehouse. Tessera connects to source. DuckDB delegation for analytical queries. Most ambitious shape; build only when customer signal is real.

### The sequencing principle

Don't skip steps. Don't build deployment infrastructure speculatively before the previous shape is validated. Research notes and design sketches can happen early; production implementation cannot. The value of each shape is proving the architecture for the next one.

---

## Part 5: Database and storage strategy

### Cube state storage

Cube state lives in the kernel's `HashMapStore`. This is correct for current scale and deployment shape. Don't change it speculatively.

When scale or distribution needs surface, add a binary `.mosaic` snapshot format:
- YAML/CSV/logs remain the source of truth (authoring layer)
- `.mosaic` is a serialized snapshot for fast loading and distribution
- Build artifact, not primary persistence
- Phase 8+ work; trigger conditions: cube takes >5s to load, distribution use case surfaces, multi-tenant service needs replicas

### DuckDB's three legitimate roles

1. **Tessera driver (Phase 5C — shipped):** DuckDB databases as ingestion sources
2. **Staging engine (Phase 5D+ — planned):** SQL on raw CSV/Parquet/JSON before ingestion
3. **Analytical query delegate (Phase 10+ — future):** Non-cube-semantic queries routed to DuckDB

**Illegitimate role:** Cube storage. Reject any proposal to use DuckDB as cube state backend. Full rationale in ADR-0025 Decision 4.

### Cloudflare D1 specifically

D1 is a managed serverless SQLite-semantic database on Cloudflare's edge. **Important: D1 is not a raw SQLite file connection in production.** D1 exposes a Worker/HTTP REST API, not a file-based SQLite driver. The Phase 5C D1 REST driver handles this correctly by going through the REST API path, not the generic SQLite file driver.

The practical mapping for Tessera recipes:
- **Local development:** SQLite-like access (if using a local D1 emulator)
- **Production D1 access:** Through the D1 REST driver or a Worker proxy — not the generic SQLite driver

Tessera recipes that pull from D1 use watermark + cursor pagination patterns that respect D1's 100-bound-parameter limit, HTTP-based access, and eventual consistency model. Do not assume the generic SQLite driver works against production D1.

### Caching strategy

When caching is built (Phase 8+), the binding rules are in ADR-0025 Decision 3. The key principles:
- Cache by `(coordinate, revision)`, not by query string
- Revision encodes invalidation triggers automatically
- Budget-driven, not exhaustive — combinatorial explosion is the main risk
- Cartridges and cubes are separate caching units

---

## Part 6: The narrative engine and interpretation ledger

**Current status:** See [MASTER_PHASE_PLAN.md](../roadmap/MASTER_PHASE_PLAN.md) for which sub-phases have shipped. This section describes the design; the master plan has the accuracy on what's complete.

### The strategic centerpiece

Phase 7A productionizes the narrative engine from a demo into permanent Mosaic capability. Sub-phases (full status in master plan):

- **7A.1: Narrative Engine Productionization** — `mc-narrative` crate; `mc model narrate` CLI verb
- **7A.2: Interpretation Ledger** — append-only JSONL; every narrative durably logged
- **7A.3: Cross-Period Analysis** — trend detection from the ledger; "third consecutive month" capability
- **7A.4: Benchmark Aggregation** — workspace-local percentile library from own ledger data
- **7A.5: Explanation Chains + Context Events** — causal attribution; `finding_id` + `explanation_priority`; `context-events.yaml`
- **7B: Visual Template Editor** — UI for authoring; depends on Phase 6B maturity

### The explanation chain pattern

Templates follow a hierarchy from most-specific to most-generic explanation:
```
Finding: Impressions declined 30%
    1. Context event explanation (operational)
       "Budget change logged for this period"
    2. Correlated input change (data-driven)
       "Consistent with 28% budget reduction"
    3. Industry benchmark (comparative)
       "In line with industry trend (-25%)"
    4. Bare finding (no explanation)
       "Investigate causes"
```

First match wins. Templates declare `explanation_priority` (lower fires first) and `finding_id`. The engine evaluates templates with the same finding_id in priority order; first match suppresses the rest. This is what produces context-aware rather than robotic narratives.

### Cartridges as the distribution unit

A cartridge is a complete domain package:
- Cube schema YAML
- Tessera recipe(s) for ingestion
- Formula library (domain-specific calculations)
- Benchmark library (industry standards, sourced and dated)
- Narrative template library
- Report composition
- Plugin skill (LLM authoring guidance for that domain)
- Test fixtures (canonical inputs + expected narratives)

Cartridges are the natural distribution unit. They ship from vendor → installed in org → available to workspaces in that org. The first cartridge is the marketing/finance domain (from Phase 6D templates); other domains (sports betting, FP&A, prospect scoring) install on the same kernel with different schemas.

### Why this beats LLM-only tools

| LLM tools | Mosaic |
|---|---|
| Lower upfront cost, higher per-report token cost | Higher upfront cost (template authoring), zero per-report token cost |
| Flexible for one-off questions | Deterministic for recurring compliance-grade reports |
| Hallucination risk on numeric data | No inference at runtime; numbers from deterministic engine |
| No cross-account intelligence | Cross-account benchmarks from aggregated ledger |
| Throwaway reports | Compounding knowledge in cartridges |

The framing for leadership: "Compliance + audit + cross-account intelligence" — not "AI but cheaper." The cheaper framing puts Mosaic in direct comparison with LLMs; the compliance/audit framing positions it as something LLMs structurally cannot do.

---

## Part 7: Phase sequencing

**For current phase status, see [MASTER_PHASE_PLAN.md](../roadmap/MASTER_PHASE_PLAN.md) and [CURRENT_STATE.md](../CURRENT_STATE.md).** This section describes the ordering principles; those documents have the live accuracy.

### Sequencing principles

**Do not skip steps.** Each deployment shape builds on the previous one. Each narrative engine phase builds on the previous one. The compound value comes from building in order.

**Research may happen ahead of implementation.** Design notes, architecture sketches, and feasibility spikes for future phases are encouraged to happen early. Production implementation must wait until predecessors are validated.

**Don't bundle multiple architectural concerns in one phase.** If a phase grows beyond its scope, split it. Splitting keeps each phase reviewable, reversible, and testable.

**Phase 4C is critical-path for the org/workspace model.** Originally scoped as "multi-domain workspace primitive," Phase 4C should be rescoped to implement ADR-0026 (org/workspace/grants). It must ship before Phase 8 (daemon), because the daemon needs to be org-aware from the start.

---

## Part 8: Open architectural questions

These need project-owner decisions before formal ADRs draft.

### Q1: When does Phase 4C draft its ADR?

Phase 4C (org/workspace primitive, per ADR-0026) is planned but not started. It should draft after Phase 7A's narrative engine arc closes, so the org model inherits lessons from the narrative ledger's workspace scoping. It must be in place before Phase 8 (service daemon) starts, because the daemon needs to be org-aware.

### Q2: Is enterprise a real near-term target?

The org/workspace architecture pays off if Mosaic pursues enterprise users. If Mosaic stays personal/small-team, the org concept is over-engineering. Indicators that it's real: TM1 framing (TM1 is enterprise software), agency framing, cartridge distribution model, marketing finance use case. The architecture commitment should be made; Phase 4C implements it when the time comes.

### Q3: Cartridge ownership and partner attribution

When a partner uses your cartridge to serve their client, does the client see "Mosaic Vendor's cartridge" or "Partner Agency's reports"? This affects white-labeling rights, brand visibility, and partnership agreements. The grants framework needs "anonymous use" vs "attributed use" as a grant property. Decision deferred to Phase 10+ (when partners are real).

### Q4: Cross-org analytics and privacy

Within a holding company, can the parent query across child orgs? Within a partner network, can the vendor see aggregate usage? These are real features but privacy-sensitive. Phase 7A.4's privacy model for benchmark aggregation shapes the approach. Defer detailed design until that model is finalized.

### Q5: Cloudflare-native deployment timing

Workers + Durable Objects + R2 is a viable Phase 9-10 option for users heavily invested in Cloudflare infrastructure. The kernel's sync-only, no-cloud-assumptions discipline makes WASM compilation plausible. Build when customer signal is real; don't build speculatively.

### Q6: How does the first cartridge get authored?

Option 1: hand-write during Phase 7A.1 implementation. Option 2: LLM-authors with project owner review. Option 3: Phase 6D's templates evolve into the cartridge organically. Option 3 is preferred; it avoids cold-start authoring and builds on work already done.

### Q7: What goes in the first ledger schema?

The ledger schema is a load-bearing decision. GPT proposed: timestamps, model_hash, evidence objects, benchmarks, severity, notability_score. Lock this as the baseline; Phase 7A.2 ADR refines it. Don't reopen the basic shape.

### Q8: Personal workspace deployment story

Today: workspace directory + Tessera recipes + manual refresh. Phase 8 (daemon) + Tailscale: proper "anywhere access" personal deployment. Phase 9-10: Cloudflare-native if/when that shape ships. Don't build speculatively beyond the current phase.

---

## Part 9: What to communicate to next PM instance

When a new PM instance picks up Mosaic work, they need to understand:

1. **The five strategic pillars (Part 1)** — combination is the moat; architectural decisions verify against all five
2. **The kernel discipline rules (Part 2 / ADR-0025)** — sync only, no cloud assumptions, no external DB as cube storage, four cell-value sources, five pillars inviolable
3. **The org/workspace architecture (Part 3 / ADR-0026)** — four entities, decision rule for workspace vs org, capability grants, five-level inheritance, resource scoping
4. **The deployment shape sequence (Part 4 / ADR-0025 Decision 2)** — Library → local UI → demo → daemon → topology cache → cloud → semantic overlay; sequential; don't skip; research early, implement in order
5. **The database/storage strategy (Part 5)** — HashMapStore for cube state; D1 uses REST API in production (not a file driver); DuckDB has three legitimate roles, never cube storage
6. **The narrative engine plan (Part 6)** — 7A.1–7A.5 sub-phases, explanation chains, cartridges, LLMs amortized at design time
7. **Current phase state** — check MASTER_PHASE_PLAN.md and CURRENT_STATE.md; this doc does not stay current on phase status
8. **Open questions (Part 8)** — don't pre-resolve without project owner

---

## Part 10: Things to actively avoid

**Don't restart phases.** If a PM instance loses context, read this document and Phase X completion reports — not redesign Phase X from scratch.

**Don't speculate on deployment shapes.** Library + CLI is correct for the current phase. Phase 8 (daemon) is next after Phase 7A completes. Don't propose cloud or overlay work before daemon ships.

**Don't put presentation concerns in the kernel.** Narratives, templates, and UI rendering are presentation. The kernel exposes the data; presentation crates produce the output.

**Don't bundle multiple architectural concerns in one phase.** Split when scope grows; keep phases reviewable.

**Don't use enumerated relationship types in ADRs.** Use capability-based grants. Enumerated types are hard to extend; capabilities scale.

**Don't propose DuckDB as cube storage.** It's a Tessera source, a staging engine, a future analytical delegate. Not cube storage.

**Don't add async to the kernel.** Sync stays sync. Async lives in deployment shells.

**Don't precompute exhaustively.** Caching is budget-driven, model-driven. Combinatorial explosion is a real risk.

**Don't bypass the dual-review pattern.** Cross-cutting architectural ADRs benefit from Desktop + GPT cross-review. Single-perspective ADRs miss things.

**Don't assume D1 = local SQLite.** D1 uses a REST API in production. Use the D1 REST driver for production D1 access; don't assume the generic SQLite file driver works.

---

## Appendix A: Documents referenced

This document synthesizes content from:
- ADR-0025 (kernel discipline and deployment architecture)
- ADR-0026 (org/workspace/resource scope/capability grants)
- Phase 7A planning document
- Phase 6D ADR
- ADR-0014 (time representation)
- ADR-0010 (Tessera architecture)
- ADR-0009 (LNM substrate vision)
- ADR-0001 (Phase 1 scope)
- Master phase plan
- Multi-turn project owner + Claude Desktop + GPT conversation

When new ADRs draft, they should cross-link ADR-0025 and ADR-0026 as architectural foundations rather than this document, since ADRs are binding and this document is reference.

## Appendix B: The five-sentence elevator pitch

> Mosaic is a portable semantic runtime for business numbers. It turns raw data into modeled cells with deterministic rules, scenarios, snapshots, and audit trails — the same engine running in your laptop, your team's daemon, or our managed cloud. The narrative engine produces compliance-grade reports with structured evidence and zero AI inference cost; cartridges turn domain expertise into reusable, distributable IP that compounds across every workspace in your organization. Personal use scales to enterprise without rebuilding because the architecture is org/workspace from the start: solo founder, small agency, partner network, holding company, or large enterprise — same engine, same patterns, different scales. The differentiation isn't speed alone; it's that Mosaic is one of the only planning tools where calibration discipline, deterministic interpretation, and compounding institutional knowledge work together at every scale.

---

**End of document. Hand to next PM instance after compaction events. For current phase status, always check MASTER_PHASE_PLAN.md and CURRENT_STATE.md — this document does not stay current on phase specifics.**
