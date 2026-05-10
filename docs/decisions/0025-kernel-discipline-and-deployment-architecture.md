# ADR-0025: Mosaic Kernel Discipline and Deployment Shape Architecture

**Status:** Accepted
**Date:** 2026-05-07
**Accepted:** 2026-05-09
**Deciders:** project owner, with input from Claude Desktop and GPT
**Phase:** Cross-cutting (applies to all current and future phases)

> This ADR commits Mosaic to a set of kernel-discipline rules and a deployment-shape sequence. The kernel must remain deployment-agnostic so that all future deployment shapes (library, daemon, cloud service, semantic overlay) ride on the same engine without modification. Caching, persistence, and runtime decisions are pushed to deployment shells; the kernel stays pure. This ADR captures architectural commitments that span phases and prevents speculative deployment-specific work from contaminating the kernel.

---

## Context

Mosaic has shipped through Phase 3 (formula engine complete), Phase 4 (LLM-assisted authoring), Phase 5 (Tessera ingestion), Phase 6A (agent-ready CLI), and Phase 7A.1–7A.5 (narrative engine, ledger, cross-period analysis, benchmark aggregation, explanation chains). The architecture has held up: the kernel (`mc-core`) is sync, deployment-agnostic, and free of cloud-specific or server-specific assumptions.

Three forces are now putting pressure on the architecture:

1. **Phase 7 (narrative engine + ledger + benchmark aggregation)** is a presentation/interpretation layer that must NOT leak into the kernel. The temptation will be to add narrative-specific primitives to `mc-core`; this would compromise the engine's purity for a presentation concern.

2. **Future deployment shapes** (service daemon, cloud service, semantic overlay) are coming. Each has its own caching, persistence, networking, and concurrency requirements. If the kernel acquires deployment-specific code, future shapes become harder to build cleanly.

3. **DuckDB integration questions** are surfacing. DuckDB is excellent for analytical SQL but is not the cube engine. The temptation to use DuckDB as cube storage would compromise the cube semantics that make Mosaic differentiated.

Without explicit architectural commitments, these forces will gradually erode the kernel's deployment-agnostic property. Once that happens, future deployment shapes become rebuilds rather than extensions.

This ADR codifies the rules that prevent that erosion. It's a constitutional document — the rules apply to all current and future phases unless a subsequent ADR explicitly amends them.

---

## Decisions

### Decision 1: Kernel discipline rules (binding for all phases)

The Mosaic kernel (`mc-core`) MUST remain deployment-agnostic. The following rules are binding for all current and future work:

**1.1 — No async runtime in the kernel.**
- `mc-core` is sync-only. No `async`/`await` keywords, no tokio imports, no async traits.
- `tokio` is permitted ONLY in `mc-drivers` (as a transitive dependency of `postgres`) and in `mc-demo-server` and any future server crates. The kernel itself never sees async code.
- Future cloud or daemon work happens in deployment-shell crates, not in the kernel.

**1.2 — No cloud-specific assumptions.**
- The kernel does not know about S3, GCS, Azure Blob, or any specific cloud storage.
- The kernel does not know about HTTP, gRPC, REST, or any network protocol.
- The kernel does not know about authentication, authorization, multi-tenancy, or session management.
- All of these concerns live in deployment-shell crates that wrap the kernel.

**1.3 — No external database as cube storage.**
- DuckDB, Postgres, SQLite, and other databases are NOT cube storage backends.
- DuckDB is a Tessera driver (read source data) and may become an analytical query delegate (Phase 10+).
- Cube state lives in the kernel's own `HashMapStore` (current) or future binary serialization formats (Phase 8+).
- "Use Postgres as cube storage" or "use DuckDB as cube storage" is rejected as architecture; reject any PR that proposes it.

**1.4 — Explicit provenance, dependencies, and revisions.**
- Every cell value carries provenance (source: input, derived, consolidation, override).
- Rule dependencies are explicit and tracked in the dependency graph.
- The cube has a revision counter that bumps on any state change.
- Snapshots are first-class kernel operations.
- These properties must be preserved as new features are added; do not break them for performance shortcuts.

**1.5 — Import versioning.**
- Tessera imports are tracked with import IDs and audit logs.
- Post-hoc writes (via `mc model write`) are tracked separately in the writes log.
- These four sources of cell values (compiled YAML, canonical inputs, Tessera imports, post-hoc writes) compose deterministically; their interaction is documented in process-notes Rule 11.

**1.6 — Deployment-shell crates may add concerns the kernel forbids.**
- `mc-demo-server` (Phase 6D) adds tokio, axum, multipart upload — all permissible because it's a deployment shell.
- Future `mc-daemon` (Phase 8) adds long-running processes, LRU caching, signal handling — permissible.
- Future `mc-cloud` (Phase 9) adds multi-tenancy, auth, billing — permissible.
- The kernel never sees any of these concerns.

**Why these rules matter.** The same kernel runs in every deployment shape. If the kernel acquires shape-specific assumptions, future shapes become rebuilds. The discipline that's held through Phases 1–7 is the strategic asset; protecting it is more valuable than any specific feature.

### Decision 2: Deployment shape sequence (binding for roadmap)

Mosaic supports multiple deployment shapes. They sequence in the following order.

> **Sequencing rule (amended 2026-05-09):** Do not promote implementation out of sequence. Research notes, design sketches, and small feasibility spikes may happen earlier. Production implementation of a deployment shape must wait until the predecessor shape has shipped and been validated. This prevents speculative infrastructure from accumulating before value is proven.

**Shape 1: Library + CLI (current state).**
- `mc-cli` binary; `mc-core` library
- File-based persistence (YAML + CSV + JSONL logs)
- Single-user, single-workspace
- This is the foundation; all other shapes wrap it

**Shape 2: Local web UI (Phase 6B).**
- `mc serve` runs locally; browser-based interaction
- Same file-based persistence
- Single-user (server is local, not multi-tenant)
- The web UI consumes the existing CLI/MCP surface

**Shape 3: Demo server (Phase 6D — in progress).**
- `mc-demo-server` for upload-to-narrative demo flow
- Single-user, single-purpose, in-memory cubes
- Proves the narrative engine concept; not production architecture

**Shape 4: Service daemon (Phase 8).**
- Long-running process with hot cube cache
- LRU eviction; snapshot persistence; crash recovery
- Loads cubes on first request; keeps hot cubes in memory
- API/MCP access; multi-cube, org-aware (see Decision 7 / ADR-0026)
- This is the first real "service" deployment

**Shape 5: Topology-aware runtime cache (Phase 9).**
- Builds on Shape 4
- Predictive prefetch driven by cube structure (hierarchies, dependencies, visible grids)
- Cache by cell coordinate + revision; bump revision on any state change
- High cache hit rates because adjacent queries share cells
- This is where Mosaic starts feeling "alive" for interactive use

**Shape 6: Cloud service (Phase 9 or 10, after Shape 4 ships).**
- Multi-tenant managed service
- Customer cubes in managed object storage (S3 or equivalent)
- Pool of service workers loading cubes on demand
- Auth, billing, multi-tenancy, observability
- This is real infrastructure work; do not start until Shape 4 has real users

**Shape 7: Semantic overlay (Phase 10+).**
- Mosaic sits in front of customer's existing data warehouse
- Tessera connects to source; Mosaic exposes cube semantics
- DuckDB delegation for analytical queries that don't need cube semantics
- Materialized views in customer's database matching cube aggregations
- This is the most ambitious shape; build only when customer signal is real

**Important: Do not build Shape 4 until Phase 6D + Phase 7A ship.** The narrative engine + interpretation ledger work on the existing library + CLI architecture. They don't need a daemon. Building a daemon before the narrative engine is done would distract from the strategic differentiator.

### Decision 3: Caching strategy (binding when caching is built)

Caching is NOT in Phase 7A or earlier. When caching is built (Phase 8+), it follows these rules:

**3.1 — Cache by cell coordinate + revision, not by query string.**
- Cache key: `(cube_id, coordinate, revision)`
- Cache value: cell value
- This produces high cache hit rates because adjacent queries share cells
- Generic SQL query-string caching is rejected as architecture

**3.2 — Revision encodes invalidation triggers.**
- Revision bumps when any of the following changes:
  - Input cells change (Tessera import, write, etc.)
  - Rule graph changes (model edit)
  - Benchmark referenced by a rule changes
  - Calibration map referenced by a rule changes
  - Fitted model referenced by `predict()` changes
- Revision bumps invalidate cached cells automatically (old revision entries become stale)
- Narrative templates have their own revision separate from cube cell revision

**3.3 — Use cube structure as the caching hint, not query history.**
- Pre-fetch decisions are driven by the cube model: hierarchies, dependencies, scoped scopes (FutureLeaves, etc.)
- Query-history learning may be layered on later (boost cache priority for frequently-accessed cells)
- The foundation is model-driven; query-pattern learning is enhancement

**3.4 — Budget-driven, not exhaustive.**
- Do NOT precompute "all aggregated slices" by default — this explodes combinatorially
- Cache memory has explicit budget; LRU eviction when budget exceeded
- Prefetch decisions are cost-bounded (max prefetch cells per query, max prefetch time)
- Priority scoring: visible grid > recent navigation > hierarchical neighbors > rule dependents

**3.5 — Cartridges are a separate caching unit from cubes.**
- Cartridge components (templates, benchmarks, fitted models) are immutable within a version
- Cache cartridge components once per cartridge_version; share across workspaces using that cartridge
- Cube state caches per-workspace; cartridge components cache globally
- This separation matters because cartridge components are heavy (compiled templates, large benchmark tables) but stable

**3.6 — Narrative ledger has its own caching surface.**
- Ledger entries are queried by template_id + scope + period_range
- This is a different access pattern than cube cells (queried by coordinate)
- The ledger needs its own indexing strategy; design in Phase 7A.2 ADR specifically
- Ledger cache is separate from cube cell cache

### Decision 4: DuckDB role (binding)

DuckDB has three legitimate roles in Mosaic; one illegitimate role.

**Legitimate role 1: Tessera driver (current — Phase 5C).**
- DuckDB databases as ingestion sources
- Mosaic reads from DuckDB tables via Tessera recipes
- DuckDB is the source; cube state is in Mosaic's kernel

**Legitimate role 2: Staging engine (future — Phase 5D+).**
- Run SQL on raw CSV/Parquet/JSON before ingestion
- Use DuckDB to join, filter, aggregate source data; pipe results to Tessera
- DuckDB is the data preparation layer; cube state is in Mosaic's kernel

**Legitimate role 3: Analytical query delegate (future — Phase 10+).**
- For queries that don't need cube semantics (raw analytical SQL against source data)
- Mosaic routes: cube-semantic queries to kernel; analytical-SQL queries to DuckDB
- Transparent to user; Mosaic decides which engine handles each query
- DuckDB is the analytical engine; cube semantics still in Mosaic's kernel

**Illegitimate role: Cube storage.**
- DuckDB is NOT a cube storage backend
- Cube state lives in `HashMapStore` (or future binary serialization), not in DuckDB tables
- Multi-dimensional cube semantics (hierarchies, consolidation, rules, scenarios) are not relational
- Forcing them into DuckDB tables produces a worse version of both engines
- Reject any PR that proposes "use DuckDB as cube storage"

**Why this matters.** DuckDB is genuinely excellent at what it does. Using it for what it's good at (analytical SQL on flat data) is leverage. Using it for what it's bad at (multi-dimensional cube semantics) is a mistake that compromises Mosaic's differentiation.

### Decision 5: The five-pillar differentiation (strategic centerpiece)

Mosaic's strategic differentiation comes from the combination of five pillars. Most planning tools have one or two; Mosaic has all five. Protecting all five is more valuable than optimizing any one of them.

**Pillar 1: Cube semantics.**
- Multi-dimensional coordinates, hierarchies, consolidation, scenarios, versions
- Rules that reference cells across coordinates (cross-coordinate reads via `prev`, `lag`, `actual_ref`, `scenario_ref`)
- Writeback as first-class operation
- TM1-equivalent capability with modern infrastructure

**Pillar 2: Deterministic evaluation.**
- Same inputs produce same outputs, every time
- No randomness in evaluation; no LLM-driven nondeterminism in core paths
- Auditable: every computed value traces back to its inputs
- Testable: golden tests pin known input/output pairs

**Pillar 3: Snapshot/rollback.**
- First-class kernel primitive (Phase 1B)
- Every cell change is reversible by construction
- Enables what-if planning, backtesting, walk-forward validation
- Other planning tools treat snapshots as backup; Mosaic treats them as a working primitive

**Pillar 4: Agent-ready query layer.**
- CLI verbs (`query`, `whatif`, `trace`, `sweep`, `diff`, `write`, `transform`, `narrate`)
- MCP tools mirroring CLI
- Stable JSON schemas, stable exit codes, idempotent operations
- LLMs and agents can use Mosaic natively without wrapping it in Python scripts

**Pillar 5: Deterministic interpretation ledger.**
- Narrative engine produces structured findings (severity, evidence, template_id)
- Every narrative is logged in the interpretation ledger (Phase 7A.2)
- Cross-period analysis fires deterministically from ledger contents (Phase 7A.3)
- Benchmark aggregation builds privacy-aware industry intelligence (Phase 7A.4)
- LLM intelligence is amortized at design time (template authoring), not runtime (report generation)

**The strategic position:** other tools have one or two pillars. Mosaic has all five. The combination is the moat, not any individual pillar. Architectural decisions that compromise any pillar to optimize another are rejected.

### Decision 6: Process commitments (binding)

**6.1 — This ADR is referenced by future ADRs.**
- ADRs that propose deployment-shape work (daemon, cloud, semantic overlay) MUST cross-link this ADR
- ADRs that propose kernel changes MUST verify against Decision 1 rules
- ADRs that propose new external dependencies MUST verify against Decision 4 (DuckDB role)
- The kernel-discipline self-test from process-notes Rule 1 is extended to include the rules in Decision 1

**6.2 — Architecture review before implementation.**
- Phase 8 (service daemon) requires its own ADR; this ADR is the architectural foundation it references
- Phase 9 (cloud service) requires its own ADR with multi-tenancy, auth, billing design
- Phase 10+ (semantic overlay) requires its own ADR with DuckDB delegation specifics
- Each future shape ADR must verify it preserves Decisions 1–5

**6.3 — Research notes for future shapes.**
- Three research notes filed alongside this ADR:
  - `docs/research-notes/mosaic-service-daemon.md` (Phase 8 placeholder)
  - `docs/research-notes/topology-aware-runtime-cache.md` (Phase 9 placeholder)
  - `docs/research-notes/semantic-overlay-mode.md` (Phase 10+ placeholder)
- These capture design space without committing to implementation
- When the time comes to draft the formal ADR, the research note becomes the starting point

**6.4 — No speculative implementation of deployment shapes.**
- Research notes, design sketches, and feasibility spikes for future shapes may happen early
- Production implementation of any deployment shape must wait until its predecessor shape has shipped and been validated
- The sequencing is: prove value at current shape; then build the next shape
- Do NOT build daemon infrastructure before Phase 7A ships
- Do NOT build cloud infrastructure before Phase 8 has real users
- Do NOT build semantic overlay before Phase 9 has customer signal

### Decision 7: Organization and workspace architecture (pointer)

Mosaic uses a four-entity container model: **Organization → Workspace → Cube → Cell**. This enables the full range of deployments from personal use to enterprise agencies, partner networks, holding companies, and resellers — on the same kernel and deployment infrastructure.

**The full architecture — entity model, capability-based grants, template inheritance chain, resource scoping, and cross-workspace correctness guardrails — lives in ADR-0026.** This decision is the binding pointer; ADR-0026 is the binding specification.

**Kernel is unaware of orgs and workspaces.** These concepts live in workspace manifests, service layers, Phase 4C primitives, and future cloud shells. The kernel exposes cells, rules, and snapshots; workspace and org concepts are a shell concern.

**Phase 4C** is the implementation vehicle for this architecture. Its scope (currently described as "multi-domain workspace primitive") should be rescoped to "organization and workspace primitive" per ADR-0026 when its ADR drafts.

---

## Out of scope

Explicitly NOT in this ADR (deferred to future ADRs or research notes):

- Specific implementation of Phase 8 service daemon (own ADR when phase begins)
- Specific implementation of Phase 9 cloud service (own ADR when phase begins)
- Specific implementation of Phase 10+ semantic overlay (own ADR when phase begins)
- Detailed cache eviction policies and prefetch algorithms (Phase 9 ADR)
- Multi-tenancy design, auth model, billing infrastructure (Phase 9 ADR)
- Customer database schema for materialized views (Phase 10 ADR)
- Cross-region replication, disaster recovery, SLAs (Phase 9+ ADRs)
- Specific cartridge format and distribution model (Phase 7A ADRs)
- Full org/workspace architecture (ADR-0026)
- Application-layer integrity / Grout (research note at `docs/research-notes/grout-security-architecture-vision.md`; future ADR at Phase 8.5+)

---

## Alternatives considered

### Alt 1: Allow async in the kernel for future deployment flexibility

Considered. Adding async to the kernel would give future deployment shapes more flexibility — they could call kernel APIs from async contexts without bridging.

**Rejected because:**
- Async contaminates everything it touches (function coloring problem)
- Async APIs are harder to use from sync contexts than vice versa
- The kernel's sync-only discipline has held through 7 phases; breaking it now is a high-cost decision
- Deployment shells can wrap sync kernel APIs in async runtime calls easily; the reverse is much harder
- Performance benefits are unclear; the kernel is already fast

The kernel stays sync. Deployment shells own async concerns.

### Alt 2: Use DuckDB as cube storage backend

Considered. Using DuckDB tables to store cube cells would let Mosaic ride on DuckDB's columnar storage and analytical performance.

**Rejected because:**
- DuckDB is relational; Mosaic is multi-dimensional. The mapping is awkward and lossy.
- Hierarchies, consolidation, rules, and scenarios don't map cleanly to relational tables
- Mosaic's writeback semantics (small frequent updates) don't fit DuckDB's bulk-analytical optimization
- The kernel's discipline (`HashMapStore` is replaceable but the semantics matter) would be compromised
- DuckDB has a real legitimate role (Tessera driver, analytical delegate); using it as cube storage muddies that

DuckDB stays in the staging/delegation layer. Cube state stays in the kernel.

### Alt 3: Cache by query string (generic SQL-cache pattern)

Considered. A simpler caching model where cache keys are hashed query strings.

**Rejected because:**
- Cache hit rates are poor (queries rarely repeat exactly)
- Cache invalidation is complex (which queries become stale when state changes?)
- Doesn't leverage Mosaic's structural knowledge (hierarchies, dependencies)
- Generic; nothing differentiated about it

Cache by `(coordinate, revision)`; let revision encode invalidation. High hit rates because adjacent queries share cells.

### Alt 4: Build the service daemon in Phase 7

Considered. Building Phase 8 (service daemon) before or alongside Phase 7 (narrative engine).

**Rejected because:**
- Phase 7's narrative engine works fine on the existing library + CLI architecture
- A daemon is real infrastructure work that distracts from the narrative engine
- The narrative engine is the strategic differentiator; the daemon is a delivery mechanism
- Sequencing: prove the differentiator first; then build the delivery mechanism

Phase 7 ships first. Phase 8 (daemon) sequences after.

### Alt 5: Skip the daemon; go straight to cloud

Considered. Cloud-hosted Mosaic without a daemon-deployment intermediate step.

**Rejected because:**
- Cloud infrastructure is the most expensive thing to build
- Without the daemon, every cloud request is cold-start
- The daemon proves the architecture; cloud productionizes it
- Skipping daemon means cloud is being built without architectural validation

Daemon first. Cloud after. Don't skip steps.

### Alt 6: Allow narrative-specific primitives in the kernel

Considered. Putting narrative-related primitives (template evaluation, ledger queries) directly in `mc-core`.

**Rejected because:**
- Narratives are presentation/interpretation, not cube semantics
- `mc-core` should have no notion of "what humans want to read about this cell"
- The narrative engine is its own crate (`mc-narrative` per Phase 7A.1); it consumes kernel APIs
- Putting narrative concerns in the kernel would compromise the kernel's purity

Narratives stay in `mc-narrative`. Kernel exposes the data; narrative crate produces the interpretation.

### Alt 7: Hard-code relationship types for org/workspace model

Considered. Enumerate inter-org relationships as typed constants (`parent_of`, `partner_of`, `reseller_of`, etc.) in ADR-0025 itself.

**Rejected because:**
- Enumerated relationship types are hard to extend (adding a new type requires migrating all dependent code)
- Real business relationships don't fit tidy categories — a franchise may be parent_of AND reseller_of simultaneously
- Once types are baked into the constitution, future code starts depending on them and the blast radius of adding a new type grows
- Capability-based grants (`use`, `view`, `fork`, `contribute`, `admin`) scale to any business shape without schema changes

Full rationale in ADR-0026.

---

## Cross-links

- **Org/Workspace Architecture:** ADR-0026 (the binding complement to Decision 7)
- **Process notes:** [`../process-notes.md`](../process-notes.md) (Rule 1 self-test references this ADR)
- **Master phase plan:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) (deployment shape sequence aligns with phase numbering)
- **Phase 7A planning document:** [`./0020-phase-7a-narrative-engine-plan.md`](./0020-phase-7a-narrative-engine-plan.md) (narrative engine is the strategic centerpiece; this ADR protects the kernel that makes it possible)
- **ADR-0001 (kernel scope):** [`./0001-phase-1-scope.md`](./0001-phase-1-scope.md) (original kernel discipline; this ADR extends it to deployment-shape concerns)
- **ADR-0009 (LNM substrate vision):** [`./0009-lnm-substrate-as-product-vision.md`](./0009-lnm-substrate-as-product-vision.md) (strategic framing; this ADR codifies the architectural commitments that enable the vision)
- **ADR-0010 (Tessera architecture):** [`./0010-phase-5-tessera-architecture.md`](./0010-phase-5-tessera-architecture.md) (Tessera is the ingestion shell; kernel discipline applies)
- **Phase 6D ADR:** [`./0019-phase-6d-marketing-report-demo-mvp.md`](./0019-phase-6d-marketing-report-demo-mvp.md) (`mc-demo-server` is the first deployment shell with relaxed rules)
- **Strategic positioning:** [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md)
- **Mosaic architecture and vision:** [`../strategy/mosaic-architecture-and-vision.md`](../strategy/mosaic-architecture-and-vision.md) (reference document; not binding over ADRs)
- **Grout security research:** [`../research-notes/grout-security-architecture-vision.md`](../research-notes/grout-security-architecture-vision.md) (application-layer integrity; pre-ADR research)
- **Security posture:** [`../security/mosaic-security-posture.md`](../security/mosaic-security-posture.md) (secure-development baseline; prerequisite to Grout implementation)
- **Research notes (filed alongside this ADR):**
  - `docs/research-notes/mosaic-service-daemon.md`
  - `docs/research-notes/topology-aware-runtime-cache.md`
  - `docs/research-notes/semantic-overlay-mode.md`

---

## Notes

**Why this ADR exists.** Most architectural commitments live in individual phase ADRs. This one is different: it's a constitutional document that applies to all phases. It codifies the rules that have been implicit in the project from Phase 1 but were never written down.

The trigger for writing it now: Phase 7 (narrative engine) and Phase 8 (service daemon) both put pressure on the kernel's purity in different ways. Without explicit rules, the wrong commitments could get made silently. This ADR makes the rules explicit so future phases can be evaluated against them.

**The five pillars are the strategic story.** Mosaic's value isn't any individual capability; it's the combination of cube semantics + determinism + snapshots + agent-readiness + interpretation ledger. Each pillar is independently valuable; the combination is what makes Mosaic different from every other tool in this space.

When making architectural decisions, the test is: does this preserve all five pillars? If a proposal compromises one pillar to optimize another, it's probably wrong. The exception is when the compromise is explicit, ADR-documented, and the project owner approves the trade.

**On sequencing vs. parallelizing.** The sequencing rule (Decision 6.4) is about production implementation, not research. Design sketches, research notes, and feasibility spikes for later deployment shapes are encouraged to happen early — that's how the design space stays informed. The restriction is on shipping production infrastructure before its predecessor is validated. Parallel research; sequential production implementation.

**On the GPT/Desktop dual-review pattern.** This ADR was drafted by Claude Desktop after a multi-turn conversation including GPT review of an earlier framing. The corrections from that review (cache by coordinate not query, don't precompute exhaustively, framing of cells as live queries was wrong) are reflected in this ADR's decisions. The dual-review pattern is working and should be repeated for future cross-cutting ADRs.

---

## Acceptance amendments

**Amendment 1 (2026-05-09):** Softened "do not parallelize deployment shapes" to "do not promote implementation out of sequence" in Decision 2 and 6.4. Research notes and feasibility spikes may happen early; only production implementation is sequenced. Preserves discipline without blocking useful research.

**Amendment 2 (2026-05-09):** Added Decision 7 (org/workspace pointer to ADR-0026). The full org/workspace architecture is in ADR-0026; Decision 7 establishes the cross-link and binds the kernel to remain org/workspace-unaware.

**Amendment 3 (2026-05-09):** Added Alt 7 (hard-coded relationship types rejected in favor of capability-based grants). Architecture rationale is now in the Alternatives Considered section with pointer to ADR-0026 for the full model.
