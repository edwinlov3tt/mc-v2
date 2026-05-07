# Phase 7A — Narrative Engine + Interpretation Ledger (Planning Document)

**Status:** Accepted as planning doc (PM-reviewed 2026-05-07; open questions Q1-Q8 answered below; ready for ADR-0020 formal draft after Phase 6D audit findings are incorporated)
**Date:** 2026-05-07
**Filed by:** Claude Desktop, synthesizing GPT recommendations + Phase 6D demo learnings + project owner direction
**Targets:** ADR-0020 (Phase 7A.1), ADR-0021 (Phase 7A.2), ADR-0022 (Phase 7A.3), ADR-0023 (Phase 7A.4), ADR-0024 (Phase 7B)
**Roadmap slot:** Phase 7A.1 through 7A.4 + Phase 7B per `MASTER_PHASE_PLAN.md`

> Phase 7A productionizes Phase 6D's demo narrative engine into a permanent Mosaic capability. The vision: Mosaic ships domain cartridges with built-in industry knowledge that produce reports without LLM cost. LLM intelligence is amortized at cartridge design time; reports run deterministically forever after. Every narrative output is durably logged as structured analysis events, creating an interpretation ledger that powers cross-period analysis, benchmark aggregation, and account health scoring.

---

## Strategic frame

**The product statement (binding):**

> Mosaic does not use AI to rewrite the same report every month. It uses AI to design reusable interpretation rules once, then runs those rules deterministically against live data forever. Every interpretation is logged as evidence, building a historical ledger that powers trend detection and benchmark aggregation over time.

**Why this matters strategically:**

1. **Token economics.** A monthly report has ~50 narrative statements. At 100 tokens per statement via LLM, that's 5,000 tokens per report. Across 1,000 customers, that's 5M tokens/month for output that doesn't need LLM intelligence at runtime. Capturing the LLM's judgment once at template-design time and replaying deterministically saves real dollars and real latency.

2. **Determinism = trust.** Same numbers always produce the same explanation. Reports can be tested. Outputs are auditable. No hallucination risk. This is the difference between "AI-assisted reporting" (slow, expensive, occasionally wrong) and "deterministic reporting with AI-designed rules" (fast, cheap, always right).

3. **Cartridge moat.** Domain expertise captured as YAML templates + benchmark libraries becomes distributable IP. A marketing-mix cartridge ships with HVAC benchmarks, B2B SaaS benchmarks, e-commerce benchmarks — built once, sold/shared/used forever. The cartridge format is the unit of value.

4. **Ledger as IP.** Every report run contributes anonymized evidence to a benchmark database. Customers benefit from aggregate intelligence without exposing their data. The ledger turns reporting from a cost center into a knowledge accumulator.

5. **Interpretation engine, not narrative generator.** The output is structured findings (severity, evidence, template_id, benchmarks fired, notability score) — readable by humans, parseable by agents, queryable by future reports. The rendered sentence is one view of the data, not the data itself.

---

## What Phase 6D taught (carry-forward)

Phase 6D's demo proved the core architecture works. The narrative engine evaluates templates against populated cubes in <2ms and produces structured output. Key learnings:

| Phase 6D decision | Carry-forward to Phase 7A |
|---|---|
| `narrative.rs` self-contained, designed for extraction | Phase 7A.1 extracts to `crates/mc-narrative/` as mechanical refactor |
| Templates use formula engine for `when:` and `bindings:` | Same in 7A; no parallel expression language |
| Format hints Rust-side, not JS-side | Same; API returns rendered text + structured evidence |
| Skip silently when `when:` predicate fails | Same; "no change worth mentioning" produces no output |
| Evidence objects include all binding values | Extended in 7A.2 with benchmark refs, ledger metadata |
| 4 template families (display/video/search/social) | Phase 7A ships these as the marketing cartridge starter library |
| Template families are extensible (new tactic = new template family) | Cartridge pattern formalizes this |
| Sub-200ms performance contract | Same target; 7A.1 hardens it as a benchmark gate |
| `mc start` banner + browser flow | Migrates to `mc serve` in Phase 6B; 7A.1 adds `mc model narrate` CLI verb |

**What Phase 6D DOESN'T have that 7A adds:**
- Persistent ledger of every narrative ever produced
- Cross-period analysis (trend detection from prior ledger entries)
- Benchmark library with versioning, sourcing, refresh metadata
- Notability scoring based on historical context
- Composable report structure (executive summary + sections)
- User-built templates via UI (deferred to Phase 7B)

---

## Phase 6D Audit Findings → Binding 7A.1 Requirements

The Phase 6D YAML-driven template engine refactor shipped successfully
but the self-audit (Section F — "where did you struggle?") surfaced
6 specific limitations that 7A.1 MUST address to make the engine
production-grade. These are NOT theoretical — they're gaps the Phase
6D implementer documented honestly.

### Finding 1: `count_where` / `any_where` / `names_where` / `first_where` are pre-computed, not generic

**What 6D ships:** The YAML header claims these are generic functions.
In reality, the context builder pre-computes specific conditions
(`Impressions < 500`, `Clicks == 0 AND Impressions > 50`) as literal
HashMap keys. An analyst writing `count_where(Impressions < 200, City)`
gets nothing — that specific key doesn't exist.

**What 7A.1 must ship:** A truly generic expression evaluator that
can evaluate arbitrary predicates against dimension elements at
runtime. Two paths:

- **Path A (preferred):** Integrate with `mc_model::parse_expression`
  (shipped in Phase 3I) + `mc_core::eval_expr`. The formula engine
  already evaluates arbitrary expressions — the narrative engine
  should use it rather than re-implementing. This requires building
  an eval context from the cube's populated data that the formula
  engine can consume.

- **Path B (fallback):** Build a proper mini-evaluator with a real
  tokenizer + parser that handles arbitrary conditions. More work
  than Path A; less reuse.

**Binding choice: Path A.** The formula engine exists, is tested
(912+ tests), and supports every operator the narrative templates
need. 7A.1 bridges the gap between `mc-narrative`'s context and
`mc-core`'s eval. Don't build a second expression language.

### Finding 2: Dedup is hardcoded by template ID in Rust

**What 6D ships:** `matches!()` on specific template IDs determines
which templates fire once only. Adding a new "fire once" template
requires editing Rust code.

**What 7A.1 must ship:** A `deduplicate: true` field in the template
YAML schema. The engine checks: if `deduplicate: true` and this
template already fired for any cube in this upload, skip.

### Finding 3: Two-pass binding resolution is shallow (max 1 level of binding→binding reference)

**What 6D ships:** Bindings resolve in two passes. A binding that
references another binding works (e.g., `verb` references `abs_pct`).
A binding that references a binding that references a binding would
fail.

**What 7A.1 must ship:** Dependency-ordered binding resolution.
Build a DAG of binding references; resolve in topological order.
No arbitrary depth limit — if a binding chain is 5 deep, that's
fine (cycle detection prevents infinite loops).

### Finding 4: `NOT` operator missing from the expression evaluator

**What 6D ships:** No `NOT` support. Templates work around this with
`== 0` or inverted comparisons.

**What 7A.1 must ship:** `NOT` operator in the evaluator. Trivial
addition if using Path A (formula engine already has `not()`).

### Finding 5: Dimension name mapping is heuristic (inferred from table name)

**What 6D ships:** The context builder guesses dimension names from
the CSV table type ("Performance by City" → dimension is "geo").
If someone adds a template for an unmapped table type, `max_by.*`
keys won't match.

**What 7A.1 must ship:** Dimension names come from the cube schema
itself (which is already compiled from the registry). No guessing
— the cube knows its dimensions. Wire the context builder to read
dimension names from `Cube::dimensions()`.

### Finding 6: UTF-8 byte-vs-character indexing bugs in the evaluator

**What 6D ships:** Fixed for the specific cases that crashed, but
the underlying issue (char indices vs byte indices in string
operations) could recur if the evaluator is extended.

**What 7A.1 must ship:** If taking Path A (use mc_model's formula
parser), this is moot — the formula parser already handles UTF-8
correctly. If extending the mini-evaluator, use a proper tokenizer
that operates on byte positions from the start (the `logos` crate
or hand-rolled with `char_indices()` everywhere).

---

## Open Questions — PM Answers (binding for ADR-0020)

The planning doc raised 8 open questions. PM answers below are
BINDING — they become constraints in ADR-0020 when drafted.

### Q1: Where do cartridges live?

**Answer: A in v1 (directory in workspace); B in 7A.1.1 (git-installable).**
Don't build a registry speculatively. A cartridge is a directory;
`mc cartridge install <url>` clones it into the workspace. Registry
is Phase 7+ productization work.

### Q2: How is the marketing cartridge first authored?

**Answer: C (Phase 6D's templates evolve into the cartridge).**
The 14 templates already shipping in `demo/narratives/display-like.yaml`
ARE the marketing cartridge starter set. Phase 7A.1 formalizes them
into the cartridge directory structure. Don't rewrite; evolve.

### Q3: LLM authors NEW templates or only refines?

**Answer: C (both), with strong bias toward B (refine).**
The plugin skill should default to "extend this template" and
"add a variant of this template." Creating from scratch is
supported but the skill's prompt framing should show existing
templates as the starting point.

### Q4: What happens when evidence is partially Null?

**Answer: C (per-binding Null behavior), with default `skip`.**
Each binding can declare `on_null: skip | placeholder | propagate`.
Default is `skip` (whole template skips if any binding is Null).
Power users opt into `placeholder: "(unavailable)"` where partial
data is acceptable.

### Q5: Cross-language templates?

**Answer: A (English only in v1). Defer i18n to demand-driven.**
No speculative i18n. When a real customer asks for Spanish, that's
its own ADR.

### Q6: Notability scoring algorithm?

**Answer: A (computed from `when:` evaluation context) with B (static override) as escape hatch.**
Templates can declare `notability_base: 0.5`. The engine adjusts
based on deviation magnitude from the `when:` predicate's values.
ML scoring is Phase 8+; don't build it.

### Q7: How does the ledger handle cube changes?

**Answer: A (entries are immutable; model_hash captures schema) with C (query layer handles versions).**
Don't rewrite history. Old entries keep their old schema reference.
The query layer understands "this entry was generated against
model_hash X; the current model is hash Y; join by measure name
not by ID."

### Q8: Privacy default for benchmark contribution?

**Answer: A unconditionally (opt-in only).**
Privacy defaults MUST be the safe choice. Period. No exceptions.
Workspaces don't contribute to benchmarks unless explicitly
configured. This is a non-negotiable binding constraint.

---

## Phase 7A scope split (4 sub-phases + Phase 7B)

The work splits into 4 sub-phases sized for sequential review and shipping. Each sub-phase has its own ADR with proper design treatment. Phase 7B (visual template editor) is separate and depends on 7A.1 + Phase 6B web UI.

### Phase 7A.1 — Narrative Engine Productionization

**Strategic centerpiece.** Extract Phase 6D's `narrative.rs` into a permanent `mc-narrative` crate; harden the API; ship `mc model narrate` CLI verb; document the template YAML schema; provide migration path for Phase 6D demos.

**In scope:**
- New `crates/mc-narrative/` crate (extracted from `mc-demo-server/src/narrative.rs`)
- Public API: `evaluate_templates(templates, cube, refs) -> Vec<NarrativeOutput>`
- Template YAML schema (formal, validated, with `MC7xxx` diagnostic codes)
- Composition: per-cell narratives, per-section narratives, per-report templates
- Conditional branching via formula engine (`when:`, `bindings:` with `if/else`)
- Format hints (`currency`, `percent_1`, `percent_2`, `count`, `date_short`, `date_long`)
- Notability filters (skip output when nothing changed materially)
- Test coverage: regression tests for every shipped template family
- New CLI verb: `mc model narrate <model> --period <p> [--output json|text|markdown]`
- New MCP tool: `mosaic.model.narrate` (parallels existing `mosaic.model.query`)
- Plugin skill: `mosaic-plugin/skills/narratives/SKILL.md` (teaches LLM to author templates at cartridge design time)

**Out of scope (deferred to 7A.2+):**
- Persistent ledger
- Cross-period analysis
- Benchmark aggregation
- User-built templates (Phase 7B)

**Estimated effort:** 5-7 sessions (1-2 weeks). Heavy lifting is in template YAML schema design, validator rules, plugin skill content. Implementation is mostly mechanical extraction from Phase 6D.

**Success criteria:**
- Phase 6D demo migrates to use `mc-narrative` crate without behavioral change
- New `mc model narrate` verb produces identical output to Phase 6D's HTTP endpoint
- Template YAML schema documented with examples
- Plugin skill teaches LLM to author 3+ template families (display, search, sales, FP&A)
- All existing tests pass; new regression tests for each shipped template

---

### Phase 7A.2 — Interpretation Ledger

**Strategic centerpiece.** Every narrative output is durably persisted as a structured analysis event. Creates the foundation for cross-period analysis, benchmark aggregation, and account health scoring.

**In scope:**
- Ledger persistence: `.mosaic/analysis-ledger.jsonl` (or `.tessera/narratives/<period>.jsonl`)
- Ledger schema (binding):

```json
{
  "schema_version": "1.0",
  "ledger_entry_id": "uuid-v4",
  "generated_at": "2026-05-07T09:15:00Z",
  "model": "monthly-marketing-report.yaml",
  "model_hash": "sha256:abc...",
  "cartridge": "home-services-marketing",
  "report_period": "2026-04",
  "report_period_start": "2026-04-01",
  "report_period_end": "2026-04-30",
  "scope": {
    "advertiser": "Scotts RV",
    "market": "Rockford",
    "channel": "Targeted Display"
  },
  "narrative": {
    "id": "clicks_down_yoy",
    "section": "Paid Search",
    "severity": "warning",
    "text": "Tampa Paid Search generated 8,420 clicks, down 4.1% from the same month last year.",
    "template_id": "clicks_down_yoy",
    "template_version": "1.2",
    "notability_score": 0.72
  },
  "evidence": {
    "Market": "Tampa",
    "Channel": "Paid Search",
    "Clicks": 8420,
    "Clicks_Last_Year": 8780,
    "Clicks_YoY_Pct": -0.041
  },
  "benchmarks_referenced": [
    {
      "id": "home_services_paid_search_click_growth",
      "industry": "Home Services",
      "period": "April",
      "value": 0.018,
      "comparison": "below_benchmark"
    }
  ],
  "warnings": [
    "Clicks are below prior-year trend and below industry benchmark."
  ]
}
```

- New CLI verbs:
  - `mc model narrate --save-ledger` (writes entries during narrate)
  - `mc model query-ledger --severity <s> [--repeated <n>] [--since <p>]`
  - `mc model ledger-export --format jsonl|csv|parquet`
- New MCP tools: `mosaic.ledger.query`, `mosaic.ledger.export`
- Ledger versioning + migration story (schema_version on every entry; readers handle old versions)
- Privacy boundary: ledger entries never contain raw external IDs (advertiser names, customer PII) by default; configurable per-cartridge

**Architectural questions to resolve in the ADR:**
- File format: JSONL (append-only, simple) vs SQLite (queryable, structured)? Recommend JSONL for v1; SQLite as Phase 7A.2.1 if queries get slow.
- Retention: forever, or with TTL? Default forever; configurable per-workspace.
- Multi-workspace: one ledger per workspace, or global? Per-workspace for isolation.
- Concurrency: append-only writes from multiple processes? Use file locking (existing `mc-tessera` pattern).

**Estimated effort:** 4-6 sessions (1 week). Ledger format design is the slow part; writes and queries are mechanical.

**Success criteria:**
- Every `mc model narrate` invocation writes durable ledger entries
- `mc model query-ledger --severity warning --repeated 3` returns warnings that fired in 3+ consecutive periods
- Ledger entries are append-only and concurrent-safe
- Privacy boundary documented and enforced (no PII leakage by default)

---

### Phase 7A.3 — Cross-Period Analysis

**Strategic centerpiece.** Trend detection from the ledger. Deterministic rules fire against prior ledger entries + current cube values. The "this is the third consecutive month" capability.

**In scope:**
- New template type: `cross_period_template` with access to ledger queries:

```yaml
narratives:
  - id: paid_search_persistent_decline
    family: trend
    severity: critical
    when: |
      ledger_query(
        template_id: "clicks_down_yoy",
        scope: current_scope(),
        since: "3-periods-ago"
      ).count >= 3
    template: |
      {scope.channel} clicks have declined for {ledger_count} consecutive months
      and are now {benchmark_delta_pct:+.1f}% below the {benchmark_name} benchmark.
    bindings:
      ledger_count: "ledger_query(template_id: 'clicks_down_yoy', since: '6-periods-ago').count"
      benchmark_delta_pct: "..."
```

- Ledger query primitives (formula engine extension):
  - `ledger_query(template_id, scope?, since?, severity?) -> LedgerResult`
  - `LedgerResult.count`, `.entries`, `.first_period`, `.last_period`
  - `ledger_lookup(ledger_entry_id) -> Evidence` (read evidence from a specific entry)
- New CLI verb: `mc model narrate-trends --last <n>-periods`
- Trend templates ship as part of the marketing cartridge starter library

**Architectural questions:**
- Performance: ledger queries hit disk; how to keep them fast? Index by `template_id` + `scope` at write time.
- Cycle prevention: can a cross-period template reference itself via the ledger? Yes, but with a depth limit (default 1; configurable).
- Validation: how to test cross-period templates? Provide a `--mock-ledger` flag for tests; ship golden ledgers per cartridge.

**Estimated effort:** 5-7 sessions. Ledger query formula primitives are the hard part; trend templates are mechanical.

**Success criteria:**
- Trend templates fire deterministically from ledger contents
- "This is the 3rd consecutive month..." narratives work end-to-end
- Cross-period templates have golden test coverage
- Performance: ledger queries < 5ms median, < 50ms P99

---

### Phase 7A.4 — Benchmark Aggregation (Privacy-Aware)

**Strategic centerpiece.** Anonymized aggregate intelligence emerges from the ledger. Internal benchmarks build over time as customers run reports. The moat.

**In scope:**
- Aggregation pipeline:
  - Reads ledger entries across all workspaces (with explicit opt-in per-workspace)
  - Anonymizes evidence (strips PII, hashes identifiers, k-anonymity threshold)
  - Aggregates by industry × geography × period × metric
  - Produces benchmark entries with sample size, percentile distribution, refresh date
- Benchmark library schema (extends Phase 3G `benchmarks:` block):

```yaml
benchmarks:
  - id: local_search_ctr
    domain: marketing
    metric: CTR
    industry: HVAC
    geography: US
    value: 0.041
    distribution:
      p10: 0.018
      p25: 0.029
      p50: 0.041
      p75: 0.054
      p90: 0.072
    period: "2025-Q4"
    sample_size: 1842
    source: "Mosaic anonymized benchmark"
    source_methodology_url: "https://docs.mosaic.dev/benchmarks/methodology"
    refreshed_at: "2026-01-15"
    stale_after_days: 180
    privacy_review: "k=20 minimum; reviewed 2026-01-10"
```

- New CLI verbs:
  - `mc benchmark refresh` (rebuilds benchmark library from local ledger)
  - `mc benchmark export --industry <i>` (exports filtered benchmark slice)
  - `mc benchmark contribute` (opt-in: contributes anonymized aggregates to community library)
- Privacy ADR (separate; Phase 7A.4 must have its own privacy review):
  - K-anonymity threshold (default k=20; configurable)
  - Differential privacy noise (optional, per-cartridge)
  - Opt-in aggregation (workspaces don't contribute by default)
  - PII detection + rejection at write time
  - Audit log of every aggregation run

**Architectural questions:**
- Single source vs federated: do customers contribute to a central benchmark, or are benchmarks per-organization? Both: per-org by default; central is opt-in.
- Refresh cadence: how often are benchmarks recomputed? Configurable; default monthly.
- Stale benchmark warnings: lint MC7xxx fires when a referenced benchmark is past `stale_after_days`.
- Distribution format: shipped with cartridges, or fetched at runtime? Both; cartridges ship a snapshot, runtime can refresh.

**Estimated effort:** 6-9 sessions. Privacy review is gating; aggregation pipeline is mechanical; opt-in mechanics need careful UX.

**Success criteria:**
- Per-organization benchmark library refreshes from local ledger
- Cross-organization aggregation works for opt-in workspaces with k-anonymity preserved
- Lint warnings fire for stale benchmarks
- Privacy methodology documented and reviewed
- At least one cartridge ships with starter benchmark library populated from synthetic data

---

### Phase 7B — Visual Template Editor (deferred; depends on 6B web UI)

**Strategic centerpiece.** Users build their own narrative templates without touching YAML. UI for live preview, version control, sharing.

**In scope:**
- Template editor in the web UI (Phase 6B)
- Live preview against current cube state
- Visual builder for `when:` predicates (form-based, generates formula)
- Format hint picker (currency, percent, date)
- Severity selector
- Section assignment
- Save to workspace; export to YAML; import from YAML
- Template versioning with rollback
- Template sharing within an organization (Phase 7B.1)
- Template marketplace (Phase 7B.2; long-term)

**Estimated effort:** 8-12 sessions. Depends on Phase 6B (web UI) being complete and stable.

**Success criteria:**
- Non-engineering user creates a working template in <5 minutes
- Templates created in UI export to clean YAML that engineers can review
- Live preview is accurate and fast (<200ms refresh)

---

## Cross-cutting design decisions

These apply across Phase 7A.1 through 7A.4 and should be locked early.

### Decision: Templates compose hierarchically

A monthly marketing report is dozens of templates assembled into a structured report. The composition model:

- **Cell-level narratives:** one cell, one sentence (e.g., "Tampa Paid Search generated 8,420 clicks")
- **Section-level narratives:** one section, multiple sentences with structure (e.g., the Paid Search section with intro + per-channel breakdown + benchmark comparison)
- **Report-level narratives:** executive summary that pulls from per-section data; cross-tactic comparisons

```yaml
reports:
  - id: monthly_marketing_report
    sections:
      - id: executive_summary
        include_narratives:
          severity: ["critical", "warning"]
          limit: 5
          ordering: "notability_desc"
      - id: paid_search
        include_narratives:
          section: "Paid Search"
          ordering: "default"
      - id: anomalies
        include_narratives:
          severity: ["critical"]
```

### Decision: LLMs author templates at design time, NOT runtime

The Phase 4 plugin (Mosaic Claude Code plugin) gains a new skill: `skills/narratives/SKILL.md`. Teaches the LLM to:

1. Inspect a cube's measures and dimensions
2. Identify likely "interesting" narrative candidates (cells with thresholds, comparisons, benchmarks)
3. Generate template YAML that captures the intent
4. Validate the template against the cube
5. Test the template against canonical inputs

Output: a template YAML file that ships with the cartridge. Once shipped, the LLM is no longer needed for narrative generation. Reports run deterministically.

### Decision: Notability filters use the formula engine

A template's `when:` predicate is the notability filter. Don't add a separate notability-scoring system. The formula engine already has:

- Threshold comparisons (`abs(delta) > 0.05`)
- Benchmark comparisons (`value < benchmark('hvac_ctr') * 0.85`)
- Statistical tests (`abs(value - rolling_mean) > 2 * rolling_std`)
- Composite conditions (`changed_significantly and not_in_steady_state`)

The notability_score in the ledger is computed from the template's evaluation context (how many conditions fired, how strong the deviation was) — not a separate ML model.

### Decision: Format hints are declarative, not imperative

Templates declare format hints; the engine handles rendering:

```yaml
template:
  text: "{Channel} spent {Spend} ({Spend_vs_Budget:+.1%} vs budget)."
  format:
    Spend: "currency"           # → $11,500
    Spend_vs_Budget: "percent_1" # → +23.4%
```

Built-in formats:
- `currency` — locale-aware ($11,500 / €11.500 / ¥11,500)
- `percent_0`, `percent_1`, `percent_2` — varying decimal precision
- `count` — comma-separated integers (8,420)
- `count_short` — abbreviated (8.4K, 1.2M)
- `delta_signed` — with explicit + or - (+47, -312)
- `date_short` — Mar 2026
- `date_long` — March 2026
- `period_relative` — "last month", "Q1 2026"
- `decimal_2` — 0.42 → "0.42"

Format extensions are cartridge-scoped (industry-specific formatters).

### Decision: Severity ladder (binding)

Every narrative has a severity. The ladder:

| Severity | Meaning | UI treatment | Ledger treatment |
|---|---|---|---|
| `info` | Notable but not actionable ("CTR grew 3%") | Default text | Logged |
| `success` | Positive outcome ("Hit benchmark") | Green check | Logged |
| `warning` | Action recommended ("Below benchmark by 15%") | Yellow icon | Logged + flagged |
| `critical` | Action required ("Conversion tracking broken") | Red icon | Logged + escalated |

Severity is template-declared; not computed from data. A template that always fires `info` cannot escalate to `warning` based on data — different conditions fire different templates with different severities.

### Decision: Cartridges are the distribution unit

A complete cartridge ships:
- Cube schema YAML
- Tessera recipe(s) for ingestion
- Formula library (calculations specific to the domain)
- Benchmark library (industry standards, sourced and dated)
- Narrative template library (display/search/social/etc.)
- Report composition (which sections, which narratives, which order)
- Plugin skill (LLM authoring guidance for that domain)
- Test fixtures (canonical inputs + expected narratives)

Cartridge format is versioned (`cartridge_version: "1.0"`); upgrades are explicit; cartridges are signed (eventually).

### Decision: Structured output is the contract; rendered text is one view

The API contract returns structured findings:

```json
{
  "schema_version": "1.0",
  "model": "monthly-marketing.yaml",
  "report_period": "2026-04",
  "narratives": [
    {
      "id": "clicks_down_yoy",
      "section": "Paid Search",
      "severity": "warning",
      "text": "Tampa Paid Search generated 8,420 clicks, down 4.1% from the same month last year.",
      "evidence": { ... },
      "benchmarks_referenced": [ ... ],
      "template_id": "clicks_down_yoy",
      "notability_score": 0.72
    }
  ]
}
```

Consumers choose how to render:
- Web UI: rich cards with severity icons, expandable evidence
- CLI: plain text with severity prefixes (⚠ warning: ...)
- Markdown: structured headers + bullet lists
- PDF: typeset report with tables and charts
- Email: HTML with branding

The renderer is presentation logic; the engine produces structured data.

---

## Sequencing recommendation

**Phase 6D ships first** (in progress; ~1 week per ADR-0019). Demo proves the architecture.

**Then Phase 7A.1** (extraction + productionization) — 1-2 weeks. Migrates Phase 6D's narrative engine to permanent infrastructure. Low risk because it's mostly mechanical; high value because it makes the engine accessible from `mc` CLI and MCP.

**Then Phase 7A.2** (ledger) — 1 week. Schema design is the slow part. After this ships, every narrative is durably logged.

**Then Phase 7A.3** (cross-period) — 1.5 weeks. Trend detection requires ledger. After this, "consecutive months" narratives work.

**Then Phase 7A.4** (benchmark aggregation) — 2 weeks. Includes privacy review. After this, the moat exists.

**Phase 7B (visual editor)** depends on Phase 6B (web UI). Don't sequence these tightly; let 6B mature first.

**Total Phase 7A duration:** ~6 weeks of focused work, distributed across 4 sub-phases. Each sub-phase ships independently and is demo-able.

---

## Open architectural questions (need decisions before ADR-0020 drafts)

These questions need project-owner direction before drafting the formal ADRs:

### Q1: Where do cartridges live?

Options:
- **A:** Each cartridge is a directory in `cartridges/` at workspace root. Versioned with the workspace. Simple.
- **B:** Cartridges are git-installable (`mc cartridge install github.com/mosaic/marketing-cartridge`). Like cargo crates.
- **C:** Cartridges have a registry (mosaic.dev/cartridges) that maintains versions, signatures, ratings.

Recommendation: **A in v1; B in Phase 7A.1.1 (mechanical addition); C in Phase 7+ (productization)**. Don't build cartridge infrastructure speculatively.

### Q2: How is the marketing cartridge first authored?

Options:
- **A:** Hand-write the marketing cartridge during Phase 7A.1 implementation (you do it, with LLM help).
- **B:** LLM authors the cartridge end-to-end during Phase 7A.1; you review.
- **C:** Phase 6D's templates evolve into the cartridge over time (organic).

Recommendation: **C with A as fallback.** The Phase 6D demo's templates are already the marketing cartridge starter set. Phase 7A.1 formalizes them into the cartridge structure. Don't try to author 50+ templates speculatively; ship what 6D has and grow from real usage.

### Q3: Does the LLM author NEW templates at design time, or only refine existing ones?

Options:
- **A:** LLM authors new templates from scratch given a cube schema.
- **B:** LLM only refines/extends existing templates a human started.
- **C:** Both, depending on user request.

Recommendation: **C, with strong prior toward B.** New templates are bigger commitments than refinements; human review is more important. The plugin skill should bias toward "extend this template" rather than "create from scratch" but support both.

### Q4: What happens when a template fires but evidence is partially Null?

Options:
- **A:** Template fails to render; output skipped.
- **B:** Template renders with "(unavailable)" placeholders for Null fields.
- **C:** Template author declares per-binding Null behavior.

Recommendation: **C.** Each binding can declare `on_null: skip | placeholder | propagate`. Default is `skip` (the whole template skips). Power users can opt into `placeholder` for specific fields where partial data is acceptable.

### Q5: Cross-language templates?

Options:
- **A:** English only in v1.
- **B:** i18n from day one with locale-keyed templates.
- **C:** Defer to Phase 7B+.

Recommendation: **A with C as future.** Don't build i18n speculatively. When a real customer asks for Spanish narratives, that's its own ADR.

### Q6: Notability scoring algorithm

Options:
- **A:** Scoring is computed from `when:` predicate evaluation context (how many conditions fired, deviation magnitude).
- **B:** Scoring is template-declared (`notability: 0.7` static value).
- **C:** Scoring is ML-based (trained from ledger feedback).

Recommendation: **A as default with B as override.** Templates can declare `notability_base: 0.5` and the engine adjusts based on deviation magnitude. ML scoring is Phase 8+.

### Q7: How does the ledger handle cube changes?

If the cube schema changes (new measures, renamed dimensions, restructured hierarchy), what happens to old ledger entries?

Options:
- **A:** Ledger entries are immutable; old entries reference old schemas (model_hash captures this).
- **B:** Ledger migration tools rewrite old entries to new schema.
- **C:** Mixed schema versions in the same ledger; queries handle versioning.

Recommendation: **A with C for queries.** Don't rewrite history; query layer handles version differences.

### Q8: What's the privacy default for benchmark contribution?

Options:
- **A:** Opt-in only; workspaces explicitly enable contribution.
- **B:** Opt-out; workspaces contribute by default unless disabled.
- **C:** Different defaults per cartridge (marketing opt-in, FP&A opt-out).

Recommendation: **A unconditionally.** Privacy defaults must be the safe choice. Opt-in only.

---

## Diagnostic codes (preliminary; pre-flight sweep needed before ADR drafts)

Phase 7A introduces a new `MC7xxx` namespace for narrative-engine diagnostics. Estimated allocation:

**Phase 7A.1 (Narrative Engine):**
- MC7001: Template references unknown measure
- MC7002: Template references unknown dimension
- MC7003: `when:` predicate has invalid syntax
- MC7004: Format hint references undefined formatter
- MC7005: Template body has unresolved `{placeholder}` (binding not declared)
- MC7006: Template severity is invalid (must be info|success|warning|critical)
- MC7007: Template family is undeclared
- MC7008: Template ID collision (two templates with same ID)
- MC7009: Section reference in `reports:` is undefined
- MC7010: Notability_base outside [0, 1] range

**Phase 7A.2 (Ledger):**
- MC7020: Ledger entry write failed (disk full, permission denied)
- MC7021: Ledger schema version mismatch (entry from future schema)
- MC7022: Ledger query with invalid filter
- MC7023: Ledger query result exceeds memory limit (paginate)
- MC7024: PII detected in ledger entry; entry rejected
- MC7025: Ledger entry references unknown template_id

**Phase 7A.3 (Cross-Period):**
- MC7030: `ledger_query()` cycle detected
- MC7031: `ledger_query()` exceeds depth limit
- MC7032: Cross-period template references future periods (not yet in ledger)

**Phase 7A.4 (Benchmarks):**
- MC7040: Benchmark referenced is stale (past `stale_after_days`)
- MC7041: Benchmark sample size below k-anonymity threshold
- MC7042: Benchmark refresh requires opt-in not set
- MC7043: Benchmark privacy review not present

Pre-flight sweep against `main` HEAD before each ADR drafts; codes above are placeholders.

---

## Cross-links

- **Phase 6D ADR (the demo this productizes):** [`0019-phase-6d-marketing-report-demo-mvp.md`](./0019-phase-6d-marketing-report-demo-mvp.md)
- **Phase 6D handoff (what's being built right now):** [`../handoffs/phase-6d-demo-mvp-handoff.md`](../handoffs/phase-6d-demo-mvp-handoff.md)
- **Phase 3G ADR (benchmarks: block foundation):** [`0013-phase-3g-reference-data-blocks.md`](./0013-phase-3g-reference-data-blocks.md)
- **Phase 3J ADR (`ScalarValue::Str` foundation for narrative templates):** [`0016-phase-3j-formula-deferred-items.md`](./0016-phase-3j-formula-deferred-items.md)
- **Master phase plan:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md)
- **Process notes (ADR-first vs handoff-first):** [`../process-notes.md`](../process-notes.md)
- **Strategic positioning:** [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md)
- **GPT recommendations (this document synthesizes):** captured in this planning doc; expanded in 7A.1 ADR
- **Claude Desktop narrative-engine framing (this document synthesizes):** prior conversation thread
- **Project owner narrative-engine vision (this document synthesizes):** prior conversation thread

---

## Recommendation for proceeding

**Three steps before drafting ADR-0020:**

1. **Project-owner review of this planning document.** Especially the 8 open architectural questions (Q1-Q8). Lock the answers; they become binding for ADR-0020.

2. **Wait for Phase 6D to ship.** The ADR-0020 draft benefits from the Phase 6D completion report's lessons. Don't draft 7A.1 in parallel with 6D; let 6D land first so the productionization scope is grounded in shipped code.

3. **Hand to GPT/Desktop for review.** This is a substantial design surface (5 sub-phases, ~6 weeks of work, multiple new contract surfaces). The cross-coord-debt pattern from Phase 3 suggests early review pays off. GPT specifically had strong input on the ledger schema; their feedback should be incorporated.

After steps 1-3: draft ADR-0020 (Phase 7A.1) with the answers from Q1-Q8 baked in. Sequence ADR-0021/22/23/24 after 7A.1 ships, drafted from real implementation experience rather than speculation.

---

## Open question for the project owner

This planning document organizes the work into 4 sub-phases (7A.1-7A.4) plus Phase 7B. The split is defensible but not binding. Alternatives:

- **Combine 7A.1 + 7A.2:** ship narrative engine + ledger together. Risk: bigger ADR, longer to first ship.
- **Defer 7A.4:** skip benchmark aggregation until a customer asks for it. Risk: ledger sits unaggregated; moat doesn't form.
- **Reorder:** ship 7A.4 (benchmarks) before 7A.3 (cross-period). Risk: cross-period analysis is more immediately useful for users; benchmarks are longer-tail strategic value.

My recommendation is the sequence as written, but you have the operational context to decide if a different sequence makes sense given near-term goals. The planning doc captures the architecture; the sequencing is operational and your call.

---

## Appendix: GPT's full schema (for reference)

GPT provided a comprehensive schema for the narrative output API; the key elements are:

- `schema_version` for envelope versioning
- `generated_at` ISO-8601 UTC timestamp
- `model` + `model_hash` for traceability
- `cartridge` reference
- `report_period` + period boundaries
- Per-narrative: `id`, `section`, `severity`, `text`, `evidence`, `benchmarks`, `warnings`, `templates_fired`, `notability_score`

This is the canonical contract. Phase 7A.1 implements it; Phase 7A.2 persists it; Phase 7A.3 queries it; Phase 7A.4 aggregates it.

The structured-output-as-contract framing is the architectural commitment that makes everything downstream possible. Lock it in 7A.1 and don't break it.

---

**End of planning document. Awaiting project-owner review of open questions before ADR-0020 drafts.**
