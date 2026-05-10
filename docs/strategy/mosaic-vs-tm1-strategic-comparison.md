# Mosaic vs TM1 — Strategic Comparison and Design Reference

**Status:** Reference document for project owner and PM instances
**Date:** 2026-05-07
**Compiled by:** Claude Desktop, synthesizing project owner conversation
**Scope:** How Mosaic compares to TM1 architecturally; what Mosaic does better; what TM1 still does better; what concepts to explore; strategic implications for positioning and roadmap

> Mosaic is positioned as the modern successor to TM1 — IBM's planning analytics engine with 35+ years of enterprise deployment. TM1's architecture made specific choices in the late 1980s that constrain its evolution; Mosaic makes different choices that align with modern infrastructure conventions. This document captures where Mosaic is structurally stronger, where TM1 retains operational advantages, what's at parity, and what concepts Mosaic should explore. Use this as the guiding reference for design decisions, marketing material, migration documentation, and strategic positioning.

---

## Part 1: The strategic framing

### The pattern this fits

Mosaic vs TM1 follows a pattern that's repeated several times in software history:

- **Postgres vs Oracle:** open-source steady improvement vs mature commercial feature set
- **Linux vs commercial Unix:** structural advantages vs operational polish
- **Git vs Subversion:** distributed model vs polished centralized model
- **Modern data stack (dbt, Snowflake) vs Cognos/MicroStrategy:** architectural simplicity vs enterprise breadth

In each case, the structurally-better newer thing eventually overtook the operationally-better older thing. Not overnight; over years. The pattern repeats because **structural advantages compound while operational polish has diminishing returns.**

### The core strategic bet

> Mosaic has structural advantages in design that compound over time. TM1 has operational advantages from maturity that diminish in marginal value. Bet on architecture; build the ecosystem deliberately; trust that time and execution close the operational gaps.

### What this document is for

**For internal design discipline:** When making architectural decisions, this document is the reference for "do we have a structural advantage here that we should protect, or are we drifting toward TM1's constraints?"

**For external positioning:** Marketing material, sales conversations, and migration documentation should lead with the structural advantages (text-based source, no FEEDERS, interpretation ledger, agent-readiness) and honestly acknowledge operational gaps.

**For roadmap prioritization:** Concepts TM1 has that Mosaic should explore become candidate work items. Some make the roadmap; others are deliberately not adopted because TM1's approach was wrong.

**For PM continuity:** When new PM instances pick up Mosaic work after compaction events, this document gives them the strategic comparison framework so they don't accidentally erode advantages or overlook gaps.

---

## Part 2: Genuine architectural advantages Mosaic has

These are places where Mosaic's design produces structurally better outcomes than TM1's. The advantages are foundational and unlikely to erode because they stem from base design choices rather than accumulated polish.

### Advantage 1: Text-based source of truth

**TM1's situation:** Source of truth is binary files (.cub for cubes, .dim for dimensions, .rux for rules). Inspection requires loading TM1. Diffing requires TM1-specific tools. Version control requires TM1-specific replication mechanisms.

**Mosaic's situation:** Source of truth is YAML, CSV, and JSONL. Everything is inspectable with `cat`, diffable with `git diff`, version-controllable with any git workflow, scriptable with any text-processing tool.

**Why this compounds:**
- LLMs can read your model and propose changes (because they read text)
- Code review on model changes works the same way as code review on application code
- CI/CD pipelines can validate models, run tests, deploy on merge
- Disaster recovery is "restore the git repo" rather than "restore TM1 backup files"
- Onboarding is reading source files, not navigating an admin UI

**Strategic value:** Every year that Mosaic stays text-based, the tooling ecosystem built around text gets stronger. Every year TM1 stays binary, the gap widens.

**Marketing framing:** "Your Mosaic models live in git, just like your code. Review changes in pull requests. Roll back deployments. Branch and merge. The text-based source of truth means modern engineering workflows work out of the box."

### Advantage 2: Deterministic evaluation guarantees

**TM1's situation:** Evaluation order is mostly deterministic but has historical edge cases — order of feeder firing, interaction of TI with rules during execution, sandbox-vs-base interactions. TM1 administrators learn idioms like "always commit sandboxes before running TI" because the underlying semantics aren't fully nailed down.

**Mosaic's situation:** Deterministic evaluation is a core kernel commitment. Same inputs, same model, same outputs — every time. The four cell-value sources (compiled YAML, Tessera imports, post-hoc writes, derived computations) compose deterministically.

**Why this compounds:**
- Test fixtures pin known input/output pairs; tests stay green forever
- The interpretation ledger (Phase 7A.2) captures findings that are reproducible
- Snapshot/rollback semantics work cleanly because evaluation is deterministic
- Agent-readable contracts are stable because outputs are predictable
- Audit trails are defensible because results are reproducible

**Strategic value:** Every other Mosaic capability builds on determinism. TM1's lack of strict determinism means each downstream capability has edge cases; Mosaic's strict determinism means they don't.

**Marketing framing:** "Deterministic by construction. The same inputs always produce the same outputs. Test your models, prove your numbers, and audit your findings — because the engine guarantees it."

### Advantage 3: Automatic dependency graphs (no FEEDERS)

**TM1's situation:** Dependency graphs are manually maintained as FEEDERS — directives following the rules section that explicitly tell the engine "when this input cell changes, also recalculate these output cells." Two places to update when logic changes. Common bug source: forgotten or incorrect feeders cause stale results.

**Mosaic's situation:** Dependency graphs are extracted automatically from rule bodies. When you write `Total_Sales = Units * Price`, the parser extracts the dependency. No feeder section. No manual maintenance.

**Why this compounds:**
- Eliminates an entire class of bugs (stale results from missing feeders)
- Removes substantial maintenance burden for complex models
- Refactoring rules is safer (no separate feeder section to keep in sync)
- New developers don't need to learn the feeder discipline
- Models stay smaller (rules + feeders → just rules)

**Strategic value:** This is one of the genuinely delightful moments for TM1 users discovering Mosaic. "Wait, I don't have to write feeders?" The advantage is structural — TM1 cannot retrofit automatic dependency tracking without redesigning the rules engine.

**Marketing framing:** "No more FEEDERS. Mosaic extracts dependency graphs automatically from rule definitions. Refactor with confidence; your dependencies stay correct because the engine maintains them."

**Caveat to be honest about:** Mosaic currently has known cross-coordinate dependency-graph debt (cells referenced via `prev()`, `lag()`, `actual_ref` invalidate broadly). The fix is scoped for a future phase. Even with this debt, Mosaic's auto-tracked dependencies are dramatically better than TM1's manual feeders.

### Advantage 4: First-class snapshots

**TM1's situation:** Sandboxes are a way to make changes that aren't committed to the base cube. They work but are operationally fragile: sandbox conflicts when multiple users edit overlapping cells, sandbox-vs-base inconsistencies during certain operations, manual commit/discard workflows.

**Mosaic's situation:** Snapshots are kernel primitives. Take a snapshot, modify, roll back if needed. Snapshots compose with each other, with what-if scenarios, with backtesting workflows. The kernel guarantees snapshot consistency by construction.

**Why this compounds:**
- Walk-forward backtesting (snapshot-modify-evaluate-rollback in a loop) becomes natural
- Multi-scenario comparison (snapshot per scenario) is structurally clean
- Undo/redo workflows in the UI map to kernel operations
- What-if analysis is a first-class operation, not a workaround

**Strategic value:** Snapshot/rollback as a kernel primitive is the foundation for both planning use cases (what-if analysis) and forecasting use cases (backtesting). TM1 supports both but with operational friction.

**Marketing framing:** "Every change in Mosaic is reversible by construction. What-if analysis isn't a workaround; it's a first-class operation. Backtesting is a natural workflow, not a custom build."

### Advantage 5: Provenance tracking on every cell

**TM1's situation:** Cell values exist; their origin requires investigation. The TRACE function helps but it's diagnostic, not always accessible. Auditing "where did this number come from?" is real work.

**Mosaic's situation:** Every cell carries provenance (input/derived/consolidation/override) as a kernel property. Always queryable, always accurate, always part of the data model.

**Why this compounds:**
- Audit trails are queryable, not investigable
- Compliance reporting has structured evidence built in
- Debugging unexpected values shows their lineage immediately
- The interpretation ledger has rich evidence to draw from

**Strategic value:** Provenance tracking is the foundation for compliance positioning. TM1 has answers to provenance questions; they're just harder to get and less reliable.

**Marketing framing:** "Every cell knows where it came from. Audit trails are queryable, compliance is structured, and debugging is transparent. No more 'where did this number come from?' investigations."

### Advantage 6: Agent-readable from day one

**TM1's situation:** REST API designed for traditional client-server interactions. Functional but not designed for LLM agents. Wrapping TM1 for AI tooling requires substantial glue code.

**Mosaic's situation:** CLI/MCP surface designed agent-first. Stable JSON schemas, idempotent operations, clear exit codes, structured outputs. ADRs explicitly call out "agent-ready query layer" as a strategic pillar.

**Why this compounds:**
- AI agents become primary consumers of business systems over the next decade
- Tools designed for agents will outcompete tools designed for humans-only
- The plugin/skill ecosystem (Phase 4) leverages this naturally
- Customer integration with AI workflows is built-in, not retrofitted

**Strategic value:** Generational advantage. TM1 will eventually retrofit better agent support, but the foundation is harder to retrofit than to build in from the start. Mosaic is positioned for the AI-native enterprise.

**Marketing framing:** "Built for agents from day one. LLMs use Mosaic natively through MCP tools. AI workflows integrate without glue code. Your AI strategy doesn't have to wait for vendor catch-up."

### Advantage 7: The interpretation ledger

**TM1's situation:** Produces values; downstream tools (reporting, dashboards) interpret them. Interpretation is throwaway — generated fresh each time, in different tools, with different semantics. No structural memory of "what was significant when."

**Mosaic's situation:** Narrative engine + interpretation ledger means interpretations are first-class persistent artifacts. Every "this is significant" finding is logged with structured evidence in queryable form. Trends emerge from the ledger. Cross-period analysis is deterministic.

**Why this compounds:**
- Reports become smarter over time as the ledger grows
- Trend detection is deterministic, not LLM-driven
- Benchmark aggregation builds privacy-aware industry intelligence
- Cross-period analysis ("third consecutive month") works without LLM at runtime
- Agency operational reporting ("which clients have repeated warnings") becomes possible

**Strategic value:** The strongest single advantage. TM1 has no equivalent. Anaplan has no equivalent. None of the modern BI tools have an equivalent. It's genuinely novel positioning.

**Marketing framing:** "Reports that compound, not just generate. Every interpretation is logged with structured evidence. Trends emerge from history. Your analytical knowledge becomes IP, not throwaway output."

### Advantage 8: Cartridge model for distribution

**TM1's situation:** Implementations — patterns customers and consultants build for specific industries. Valuable but packaged as TM1 backups + documentation + tribal knowledge. Distribution is "send a backup file, hope it loads."

**Mosaic's situation:** Cartridges are structured packages — cube schema + Tessera recipes + formula library + benchmark library + narrative templates + tests. Versioned, signable, distributable. Cartridges become marketplace assets.

**Why this compounds:**
- Vendors sell cartridges; consultants productize patterns
- Communities maintain open-source cartridges for common domains
- Cartridge marketplace becomes a network effect (more cartridges → more users → more cartridges)
- Domain expertise becomes leverageable IP

**Strategic value:** TM1 implementations are bespoke; Mosaic cartridges are products. Different category, different economics, different scaling dynamics.

**Marketing framing:** "Domain expertise as a product. Marketing cartridge, FP&A cartridge, sports analytics cartridge — install, configure, ship. Your industry expertise becomes leverageable IP, not consulting hours."

### Advantage 9: Pure-Rust safety and performance

**TM1's situation:** Core is C++. Fast but with well-known safety issues — buffer overflows, use-after-free, dangling pointers. Mature enough that obvious bugs are caught, but security posture is structurally weaker.

**Mosaic's situation:** Rust foundation eliminates entire CVE categories by design. No memory safety bugs in safe code. No data races (the borrow checker prevents them). Performance comparable to C++ without the safety tradeoffs.

**Why this compounds:**
- Security review focuses on logic bugs, not memory bugs
- CVE rates structurally lower than C++ projects
- Modern toolchain (cargo, clippy, fuzz testing) is built into the language
- Future Grout primitives layer on a safe foundation

**Strategic value:** Not unique to Mosaic — many modern data tools are Rust now. But it's a genuine advantage over TM1's C++ legacy. Combined with the deployment-shell isolation pattern, this produces a structurally hardened security posture.

**Marketing framing:** "Memory-safe by construction. Built in Rust to eliminate entire CVE categories that haunt C++ codebases. Your planning analytics shouldn't be your weakest security link."

---

## Part 3: More subtle advantages worth knowing

These are real advantages but more nuanced than "Mosaic better in every case." Worth understanding for honest positioning.

### Schema as code

**TM1:** Dimensions managed through UI or TI scripts. Changes are operations on the server; history is the operations log.

**Mosaic:** Dimensions declared in YAML alongside cube models. Changes are git commits.

**The advantage:** Schemas are version-controlled, reviewable, testable.

**The nuance:** TM1's mutable dimensions handle slowly-changing-dimension cases more naturally — declaring full element history in YAML is awkward. Mosaic handles this through Tessera-driven dimension updates, which works but is less elegant in this specific case.

### Rule debugging

**TM1:** Functional but limited debugging tools. RULESTRACE, manual evaluation tracing, watching cell values change. Real debugging often by careful reading of rule logic.

**Mosaic:** `mc model trace` produces structured trace output: which rule fired, which inputs were read, intermediate values, final result. JSON, machine-readable, agent-consumable.

**The advantage:** More transparent and toolable.

**The nuance:** TM1's tooling has 30 years of polish. Specific debugging workflows (interactive cell-by-cell trace) are smoother in TM1 even if Mosaic's trace output is more structured. Mosaic will catch up here over time.

### Audit and compliance

**TM1:** Audit logs work; capture user actions, value changes, process executions. Generally accepted for regulated reporting.

**Mosaic:** More granular — every cell change, every Tessera import, every narrative generation logged. Future Grout primitives produce cryptographically verifiable audit trails.

**The advantage:** More detailed, more queryable, more cryptographically defensible.

**The nuance:** TM1's audit is sufficient for most compliance regimes today. Mosaic's marginal value depends on the specific compliance requirement. Worth more in regulated industries; less in ones where TM1's audit already passes.

### Multi-tenancy

**TM1:** Through separate server instances or careful workspace partitioning. Engine itself isn't designed multi-tenant-first.

**Mosaic:** Org/workspace architecture is multi-tenant by design. Resource scoping is explicit; cross-tenant access is intentionally constrained.

**The advantage:** Cleaner SaaS-style deployment story.

**The nuance:** Most TM1 deployments aren't multi-tenant SaaS; they're single-tenant enterprise. The advantage is real but addresses a deployment shape TM1 wasn't optimized for.

---

## Part 4: Where TM1 retains real advantages

Honest acknowledgment of where TM1 is currently ahead. These gaps will close over time but the lead is real.

### Performance at extreme scale

**TM1:** 35+ years of performance tuning. Handles enormous cubes (billions of cells, hundreds of dimensions) on reasonable hardware. Proprietary in-memory storage, custom compression, decades of optimization.

**Mosaic:** Younger. HashMapStore is appropriate for current scale but hasn't been stress-tested at TM1-scale workloads. Phase 8's binary serialization, Phase 9's caching, future optimization work — these will close the gap, but TM1 has a long head start.

**Honest framing for marketing/sales:**
- **Don't claim** Mosaic is faster than TM1 at scale.
- **Do claim** Mosaic's architecture supports superior scaling characteristics (no SKIPCHECK overhead, deterministic evaluation, modern caching) and let benchmarks prove the point when available.
- **Do claim** Mosaic handles current workloads (millions of cells) with sub-second response times.

### Enterprise feature breadth

**TM1 ships with:**
- SSO integration (multiple providers)
- Multi-server replication
- Hot standby
- Comprehensive admin console
- Mature backup/restore tooling
- Integration with IBM's broader BI stack
- Certified deployment patterns for regulated industries
- Professional services ecosystem
- Decades of consulting expertise

**Mosaic has:**
- A kernel
- A CLI
- MCP tools
- Planned phases for the rest

**Honest framing:**
- **Don't claim** Mosaic is enterprise-ready today.
- **Do claim** Mosaic is architecturally enterprise-capable, with specific enterprise features shipping in specific phases.
- **Do** be specific about what's shipped, what's planned, and when (when timeline is clear).

### TI ecosystem maturity

TM1 customers have built thousands of TI processes that solve real problems. That accumulated knowledge is a real asset — not in the language itself, but in the patterns and solutions captured in TI processes.

Mosaic's equivalent capabilities (Tessera, write APIs, host scheduling) are individually better, but the ecosystem of solutions built on top hasn't accumulated yet. A TM1 user migrating to Mosaic loses access to community-shared TI processes.

**Honest framing:**
- **Don't claim** TM1 customers can migrate easily.
- **Do claim** migration produces a structurally better system worth the work.
- **Do invest** in migration tooling and pattern documentation (Phase 10+ priority).

### Mature consulting and training

TM1 has IBM-certified consultants, training courses, certifications, conferences. Customers can hire experts. Mosaic has none of this.

This is a network-effect gap. TM1's lead here will persist for years even if Mosaic's architecture is superior. Customers worry about "who will help us if something goes wrong."

**Strategic implication:** Build the ecosystem deliberately. Documentation, tutorials, certifications, partnerships with consulting firms. Operational work, not engineering work, but gating for enterprise adoption.

---

## Part 5: At parity (same concept, different names)

Things both systems do roughly equivalently. Worth knowing the terminology mapping but no architectural advantage either way.

| TM1 term | Mosaic term | Notes |
|---|---|---|
| Cube | Cube | Identical concept |
| Dimension | Dimension | Identical concept |
| Element | Coordinate value / dimension member | Same concept; Mosaic uses different vocabulary |
| Consolidation | Parent coordinate / rollup | Same concept; both support hierarchical aggregation |
| N-level / C-level | Leaf cell / consolidated cell | Mosaic doesn't use this terminology but the concept is identical |
| Subset | Filter / scope | Both support defined slices of dimensions |
| View | Query | Mosaic queries via CLI/MCP fill this role |
| Sandbox | Snapshot + scenario | Mosaic's snapshot/rollback is cleaner; TM1 sandboxes do similar work |
| Persistence | Writeback log + snapshot | Both commit changes to durable storage |
| Replication | Future capability (Phase 9+) | TM1 has it today; Mosaic plans for it |
| Chore | Scheduled task | Mosaic defers scheduling to host environments (cron, GitHub Actions, systemd) |

---

## Part 6: TM1 concepts Mosaic deliberately doesn't replicate

These are TM1 features Mosaic avoids on purpose. The reasoning matters because future PMs might propose adding them; this section is the documented "no, here's why."

### SKIPCHECK and zero suppression

**What TM1 has:** Performance optimization that skips evaluation of cells where all inputs are zero. Produces wrong answers in some cases (constant rules, calculations producing non-zero from zero inputs). The SKIPCHECK directive disables this optimization at the cost of performance.

**Why Mosaic doesn't need it:** Mosaic's evaluator uses explicit dependency graphs, not heuristic zero-suppression. The engine evaluates cells based on what rules actually reference, not based on whether inputs happen to be zero. Correctness is guaranteed by construction.

**Strategic positioning:** This is one of those places where TM1's age shows. SKIPCHECK is a workaround for a 1980s performance optimization. Modern engines don't need this hack because they have better data structures.

### TurboIntegrator (TI)

**What TM1 has:** Custom scripting language for data import, cube manipulation, scheduled processes, integration with external systems. Widely considered TM1's biggest design wart — weird syntax, limited debugging, no version control story, tight coupling to TM1's internals.

**Why Mosaic doesn't need it:** The underlying jobs are split across cleaner purpose-fit primitives:

| TM1 TI use case | Mosaic primitive |
|---|---|
| Data import | Tessera recipes (declarative, versioned, testable) |
| Cube manipulation | `mc model write` and write batches (explicit, audited) |
| Scheduling | Host environment (cron, systemd, GitHub Actions) |
| Transformations | Formula engine (declarative rules in YAML) |
| Integration | CLI verbs + MCP tools (agent-readable) |

**Strategic positioning:** TM1 made TI a single language that does everything; Mosaic uses purpose-fit primitives for each job. This is a deliberate architectural choice and one of Mosaic's design advantages.

**Marketing framing:** "No proprietary scripting language to learn. Tessera handles imports, the formula engine handles transformations, your host environment handles scheduling. Each primitive is best-of-breed and uses standard tooling."

### Manual feeders

Already covered in Part 2 (Advantage 3). Mosaic's automatic dependency graphs replace this entirely.

### Server-side vs client-side rules distinction

**What TM1 has:** Distinguishes rules that run on the server (always evaluated) from logic that runs client-side (in reports, in clients). Affects performance and consistency.

**Why Mosaic doesn't need it:** Evaluation is unified — there's no client/server split because evaluation is always in the kernel. Simpler and better.

### Binary file format as primary persistence

**What TM1 has:** .cub files as cube storage; .dim files as dimensions; .rux as rules. Loading TM1 means loading these binary files.

**Why Mosaic doesn't replicate (but does adopt the spirit):** Mosaic's primary source of truth is text (YAML/CSV/JSONL) for inspectability, version control, and tooling reasons. The future `.mosaic` binary format (Phase 8+) is a build artifact for fast loading and distribution, not the primary source of truth.

The distinction matters: TM1's binary format IS the model; Mosaic's binary format is a cached compilation of the model. You can always rebuild Mosaic's binary from text source; TM1's text exports are derivative of the binary.

---

## Part 7: TM1 concepts worth exploring for Mosaic

These are TM1 features that might inform Mosaic's design. Not commitments to build; candidates for future research notes and ADRs.

### Dimension element types

**What TM1 has:** Distinguishes "Numeric" elements (can hold values) from "String" elements (text labels) from "Consolidation" elements (rolled-up parents). Creates a typed dimension system with built-in validation.

**Mosaic's current state:** Dimensions are just coordinate values without explicit typing. The implicit assumption is "dimensions are labels; values live in measures."

**Worth considering:** Does Mosaic want explicit dimension element typing? Probably yes for validation. Probably as a Phase 4C work item alongside the workspace primitive.

**Recommendation:** Add to research notes when Phase 4C begins. Specifically: typed dimensions for validation (e.g., "Time dimension elements must parse as dates," "Numeric measures cannot accept string values").

### Dimension element aliases

**What TM1 has:** Dimension elements can have multiple aliases — "January" might also be "Jan", "Q1M1", "Period 1". Different views show different aliases. Useful for internationalization and multi-perspective reporting.

**Mosaic's current state:** Dimension members have one canonical name.

**Worth considering:** Aliases as a future feature. Real TM1 capability that Mosaic users might miss when they encounter localization or multi-perspective reporting needs.

**Recommendation:** Defer until a real customer use case surfaces. Capture in research notes; don't build speculatively.

### Element attributes (dimension metadata)

**What TM1 has:** Dimension elements can have attributes attached. A "Product" dimension might have attributes like "Category", "Supplier", "Launch Date" attached to each product element. Attributes are queryable and usable in rules.

**Mosaic's current state:** Reference data tables (Phase 3G) cover similar use cases. Separate tables with foreign-key-style joins.

**Worth considering:** How Mosaic's reference data tables compare to TM1 attributes in user experience. Functionality overlaps but authoring experience differs.

**Recommendation:** Capture comparison in user-experience research note. Possible Phase 4+ enhancement to make reference data feel more like TM1 attributes when that's the natural model.

### Personal sandboxes for multi-user environments

**What TM1 has:** Each user has a private sandbox — workspace where they can experiment without affecting the base cube. Commit when ready, discard otherwise.

**Mosaic's current state:** Snapshots cover this conceptually but the multi-user workflow isn't fully designed.

**Worth considering:** What's the multi-user collaboration model? TM1 has answers (sandboxes, commit semantics, conflict resolution). Mosaic needs equivalent answers when multi-user deployment matters.

**Recommendation:** Phase 9+ design work. Sandboxes per user, conflict resolution semantics, commit/merge workflows. Capture in research notes for the cloud service phase.

### Replication and high availability

**What TM1 has:** Multi-server replication for high availability. Hot standby instances. Read replicas for query distribution.

**Mosaic's current state:** Single-instance deployment. Phase 8 daemon will be single-instance. Phase 9 cloud service will be multi-tenant but not multi-replica.

**Worth considering:** Replication for HA, read scaling, geographic distribution.

**Recommendation:** Phase 10+ work. Real engineering project. Capture in research notes when customer demand surfaces.

### Mature import/export tooling

**What TM1 has:** Decades of polish on TM1's import/export tools. Handles edge cases, large datasets, error recovery, partial loads.

**Mosaic's current state:** Tessera (Phase 5) is well-designed but younger. Less battle-tested on edge cases.

**Worth considering:** Operational hardening of Tessera over time. Error recovery patterns, large-dataset optimization, partial-load handling.

**Recommendation:** Continuous improvement priority. Each Tessera-related production issue should produce hardening work. Consider a dedicated "Tessera operational maturity" phase eventually.

### Rich admin tooling

**What TM1 has:** Comprehensive admin console — server management, user administration, performance monitoring, log analysis, model deployment.

**Mosaic's current state:** CLI-driven. No admin UI.

**Worth considering:** Admin tooling for the cloud service deployment (Phase 9+).

**Recommendation:** Part of Phase 9 cloud service scope. Don't build speculatively for personal/single-tenant use; build when multi-tenant cloud deployment justifies it.

### Workflow approval chains

**What TM1 has:** Some TM1 implementations support approval workflows — submit a forecast, manager reviews, approves or sends back for revision.

**Mosaic's current state:** No equivalent.

**Worth considering:** Workflow primitive for planning use cases where approval is required (budget submissions, forecast reviews).

**Recommendation:** Domain-specific feature. Probably best handled as a cartridge concern (specific cartridges include workflow primitives) rather than core kernel feature. Capture as research note when planning use cases drive demand.

---

## Part 8: Strategic implications

### Positioning rules of thumb

**Lead with structural advantages:**
- Text-based source of truth (modern dev workflow)
- Deterministic evaluation (no SKIPCHECK-style hacks)
- Automatic dependency graphs (no FEEDERS pain)
- First-class snapshots (kernel primitive vs sandbox approximation)
- Provenance tracking (every cell knows its origin)
- Agent-readable (CLI/MCP first, not retrofitted)
- Interpretation ledger (no TM1 equivalent exists)
- Cartridge distribution (productized vs bespoke)
- Rust safety (no C++ memory bug class)

**Acknowledge operational gaps honestly:**
- Performance at extreme scale (TM1's lead is real; Mosaic will close it)
- Enterprise feature breadth (gap closes over phases)
- Migration tooling (real work; not trivial)
- Consulting/training ecosystem (years to build)

**Don't overclaim:**
- Don't say "faster than TM1" without benchmarks
- Don't say "enterprise-ready" before specific enterprise features ship
- Don't say "easy migration" — it's real work
- Don't say "drop-in replacement" — it's a different paradigm

### Marketing material structure

When writing marketing copy comparing Mosaic to TM1:

1. **Open with the structural advantage that matters most for the audience.** For developers: text-based source. For analysts: no FEEDERS. For executives: interpretation ledger. For compliance: provenance + Grout.

2. **Acknowledge what TM1 does well.** "TM1 is a mature, capable system with decades of enterprise deployment. Mosaic is a structurally newer approach to the same problem space."

3. **Make the structural argument.** "Mosaic's design choices align with modern infrastructure conventions (text-based source, automatic dependencies, agent-readable APIs). These produce compounding advantages over time."

4. **Be honest about gaps.** "TM1 has more enterprise tooling today. Mosaic's roadmap addresses this in [specific phases]. Choose based on your timeline and priorities."

5. **Close with the trajectory.** "The pattern of structurally-better newer tools overtaking operationally-mature older tools has played out repeatedly in software (Postgres vs Oracle, Linux vs Unix, Git vs Subversion). Mosaic vs TM1 fits the same pattern."

### Roadmap implications

This comparison informs roadmap priorities:

**Protect the structural advantages:**
- Text-based source (don't add binary primary formats)
- Deterministic evaluation (don't add nondeterministic shortcuts)
- Automatic dependencies (don't add manual feeders)
- Provenance tracking (don't compromise for performance)
- Agent-readable contracts (don't add unstable APIs)

**Close the operational gaps deliberately:**
- Performance optimization (Phase 8+ caching, Phase 9+ scaling)
- Enterprise features (Phase 9+ cloud service includes auth/billing/admin)
- Migration tooling (Phase 10+ when customers ask)
- Documentation/training (continuous priority)

**Selectively explore TM1 concepts:**
- Dimension element types (Phase 4C candidate)
- Sandboxes for multi-user (Phase 9 design)
- Replication (Phase 10+ when needed)

**Reject TM1 concepts that don't fit:**
- Manual feeders (would compromise advantage 3)
- TurboIntegrator language (would muddy purpose-fit primitives)
- SKIPCHECK heuristics (would compromise determinism)
- Binary primary persistence (would compromise text-based advantage)

### Internal design discipline

When making architectural decisions, ask:

1. **Does this preserve the structural advantages?** If a proposal weakens text-based source, determinism, automatic dependencies, provenance tracking, or agent-readability, push back hard.

2. **Are we drifting toward TM1's constraints?** Watch for proposals that would re-introduce manual dependency tracking, opaque evaluation order, binary-only formats, or non-deterministic shortcuts.

3. **Is this an operational gap we should close?** Performance, enterprise features, migration tooling, ecosystem development — these are the right places to invest.

4. **Is this a TM1 concept worth borrowing?** Element typing, aliases, attributes, sandboxes — adopt selectively when they serve Mosaic's goals.

---

## Part 9: Talking points for specific audiences

### For TM1 administrators

**Lead with:**
- "No more FEEDERS." (genuine relief)
- "No SKIPCHECK debugging." (eliminates a class of bugs)
- "Your model lives in git." (modern workflow)

**Acknowledge:**
- Migration is real work (TI processes need to be rebuilt)
- Some TM1 conveniences (interactive trace, mature admin UI) take time to match
- Enterprise tooling gap exists today

**Close with:**
- "The day-to-day experience is dramatically better. The migration cost is real but the ongoing benefit compounds."

### For developers

**Lead with:**
- Text-based source of truth (everything in git)
- Agent-readable from day one (LLM workflows native)
- Rust foundation (memory-safe, modern toolchain)
- Deterministic evaluation (testable, reproducible)

**Acknowledge:**
- Younger ecosystem (fewer pre-built integrations)
- Less mature tooling polish (CLI-first, web UI in Phase 6B)

**Close with:**
- "If you've used dbt or Cube.dev, this feels familiar. If you've used TM1, the modern dev experience is the major upgrade."

### For executives

**Lead with:**
- Compounding institutional knowledge (interpretation ledger as IP)
- Cryptographic provenance (audit-grade evidence)
- Cartridge marketplace (domain expertise as product)
- Future-proof architecture (designed for AI-native enterprise)

**Acknowledge:**
- Younger product (less battle-tested at extreme scale)
- Smaller ecosystem (fewer certified consultants today)

**Close with:**
- "The strategic bet is on architecture compounding faster than TM1 can refactor. The pattern of structurally-better newer tools overtaking older ones is well-established."

### For compliance/audit teams

**Lead with:**
- Cryptographic provenance on every cell
- Hash-chained audit logs (tamper-evident)
- Signed exports (verifiable independently of Mosaic)
- Deterministic evaluation (reproducible findings)

**Acknowledge:**
- Specific regulatory certifications (SOC 2, etc.) take time to acquire
- Compliance teams need to evaluate Mosaic for their specific regime

**Close with:**
- "Mosaic is designed compliance-first. The cryptographic foundation is structurally stronger than retrofit security. We're building for regulated industries from day one."

---

## Part 10: Maintenance of this document

### When to update

This document should be updated when:

- A new architectural decision changes the comparison (advantage gained or lost)
- A new TM1 capability is identified that's worth exploring or rejecting
- Marketing positioning evolves based on customer conversations
- A phase ships that closes one of the operational gaps
- New evidence emerges about TM1's behavior or limitations

### How to update

Updates should preserve the structure:
- Part 2 advantages: don't add speculative ones; only add when shipped
- Part 4 gaps: update as they close (move items from "gap" to "at parity" or "advantage")
- Part 7 worth-exploring: add new candidates; mark when explored and decided
- Part 8 strategic implications: refine as positioning evolves

### Cross-links

When other documents reference TM1 comparisons, link here as the authoritative source. This prevents the comparison logic from being duplicated and inconsistent across multiple documents.

---

## Cross-links

- **Architecture and vision:** [`mosaic-architecture-and-vision.md`](./mosaic-architecture-and-vision.md) (reference doc)
- **Grout security architecture:** [`../research-notes/grout-security-architecture-vision.md`](../research-notes/grout-security-architecture-vision.md) (research/pre-ADR)
- **Phase 7A planning:** [`../decisions/0020-phase-7a-narrative-engine-plan.md`](../decisions/0020-phase-7a-narrative-engine-plan.md)
- **Kernel discipline:** [ADR-0025](../decisions/0025-kernel-discipline-and-deployment-architecture.md)
- **Org/workspace architecture:** [ADR-0026](../decisions/0026-org-workspace-resource-scope-capability-grants.md)
- **Master phase plan:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md)

### External references

- TM1 documentation (IBM Knowledge Center)
- TM1 community resources (Cubewise blog, TM1 forums)
- Comparable architectural transitions (Postgres history, Linux kernel evolution, Git design)

---

## Appendix A: One-paragraph summary

Mosaic is positioned as the modern successor to IBM TM1, with structural architectural advantages that compound over time: text-based source of truth (vs binary), deterministic evaluation (no SKIPCHECK hacks), automatic dependency graphs (no FEEDERS pain), first-class snapshots (vs fragile sandboxes), built-in provenance tracking, agent-readable contracts (CLI/MCP first), the interpretation ledger (no TM1 equivalent), cartridge distribution model (productized vs bespoke), and Rust safety foundation. TM1 retains operational advantages from 35+ years of maturity: extreme-scale performance tuning, enterprise feature breadth, ecosystem of consultants and patterns, and battle-tested deployment tooling. The strategic bet is that structural advantages compound faster than operational polish — a pattern that's repeated with Postgres vs Oracle, Linux vs Unix, and Git vs Subversion. Marketing should lead with structural advantages, acknowledge operational gaps honestly, and trust that time and execution close the gaps. Internal design discipline uses this comparison to protect the structural advantages and selectively close the operational gaps. Several TM1 concepts (element typing, aliases, attributes, sandboxes) are worth exploring; others (FEEDERS, TurboIntegrator, SKIPCHECK, binary persistence) are deliberately rejected.

## Appendix B: One-sentence pitches

**For developers:** Mosaic gives you TM1's planning power with Git-native workflows, zero feeders, and agent-ready APIs.

**For analysts:** Mosaic does what TM1 does, without the FEEDERS pain or the SKIPCHECK debugging.

**For executives:** Mosaic is the planning analytics platform built for the AI-native enterprise — compounding knowledge, cryptographic provenance, distributable cartridges.

**For compliance:** Mosaic produces cryptographically verifiable audit trails by design, not by retrofit.

**The umbrella pitch:** Mosaic is to TM1 what Postgres was to Oracle — structurally newer, architecturally better, operationally catching up faster than the incumbent can refactor.

---

**End of comparison document. Update as architectural advantages ship and operational gaps close. Use as the authoritative reference for TM1 comparison logic across all Mosaic documentation.**
