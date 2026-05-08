# Research Note: Explanation Chains + Template Versioning

> **Status:** Research (pre-ADR)  
> **Date:** 2026-05-08  
> **Relates to:** Phase 7A narrative engine, Phase 7B editor  
> **Scope:** Domain-agnostic causal attribution + YAML template evolution strategy

---

## 1. The Problem

The current narrative engine observes *what* happened ("Impressions declined 30%") but not *why*. Without causal context, narratives risk implying wrong conclusions:

- "Creative underperformed" when the real cause is "budget was cut in half"
- "CTR declined" when the real cause is "3 of 5 creatives paused"
- "Revenue missed target" when the real cause is "fewer selling days in February"

This applies across all domains:
- **Marketing:** budget changes, creative pauses, targeting shifts, seasonality
- **Finance:** calendar effects, hiring freezes, one-time charges
- **Sports:** player injuries, schedule strength, minutes played

The engine needs to distinguish between "something interesting happened" (the finding) and "here's why" (the explanation).

---

## 2. Explanation Chains (architectural pattern)

### Core concept

When the engine detects a finding, it walks a prioritized list of candidate explanations. The first explanation whose `when:` predicate passes fires; the rest are suppressed. If no explanation matches, the bare finding fires as a fallback.

```
Finding: Impressions declined 30%
    ├─ Priority 100: Context event logged? (manual annotation)
    │       └─ "Budget reduced 40% for Q1 close-out"
    ├─ Priority 200: Proportional input change? (data-driven)
    │       └─ Budget down 28% → "Delivery scaled with spend"
    ├─ Priority 300: Benchmark deviation? (own-history)
    │       └─ "Below your own p25 for this channel"
    ├─ Priority 400: Seasonal pattern? (YoY)
    │       └─ "April is historically your lowest month"
    ├─ Priority 999: Bare finding (fallback)
            └─ "Impressions declined 30%; investigate causes"
```

### Template-level implementation

Two new fields on templates:

```yaml
- id: impressions_declined_budget_proportional
  finding_id: impressions_declined_significant  # groups templates by finding
  explanation_priority: 200                     # lower = fires first
  when: |
    current.Impressions < prev.Impressions * 0.85
    AND current.Budget < prev.Budget * 0.85
    AND abs(impr_change_pct - budget_change_pct) < 10
  severity: info
  template: >
    Impressions declined {impr_change_pct:.0f}%, consistent with the
    {budget_change_pct:.0f}% budget reduction. Delivery scaled
    proportionally — no efficiency degradation detected.
```

When templates share the same `finding_id`, the evaluator processes them in `explanation_priority` order. First match wins. Templates without `finding_id` fire independently (current behavior, unchanged).

### Why this beats "Layer 1 + Layer 2"

- **Single primitive, N explanation types.** Context events, data correlation, benchmarks, seasonality, calendar effects, YoY comparison all plug into the same pattern. No special-case code per explanation type.
- **Cartridge authors control priority.** Marketing prioritizes operational events; finance prioritizes calendar effects; sports prioritizes player availability. The cartridge declares the hierarchy.
- **Graceful degradation.** Bare findings always exist as the lowest-priority fallback. The engine never fails to produce output.
- **Inspectable.** The ledger records which explanation fired and which candidates didn't — useful for debugging and trend analysis.

---

## 3. Data-Driven Attribution (no annotation needed)

The "proportional movement" principle: when an INPUT measure and an OUTPUT measure change proportionally, the input change explains the output change.

| If this moves... | ...and this moves proportionally | Conclusion |
|---|---|---|
| Budget | Impressions | Delivery scaled with spend (expected) |
| Impressions | Clicks | Click volume scaled with delivery (expected) |
| Traffic | Revenue | Revenue tracked traffic (expected) |
| Minutes played | Points scored | Production tracked playing time (expected) |
| Headcount | Recruiting spend | Spend tracked team size (expected) |

| If this stays flat... | ...and this moves | Conclusion |
|---|---|---|
| Budget | Impressions down | Delivery efficiency issue |
| Impressions | Clicks down | CTR decline / engagement issue |
| Traffic | Revenue down | Conversion issue |
| Minutes played | Points down | Player efficiency decline |
| Headcount | Recruiting spend up | Per-hire cost increase |

This is pure template logic — no engine changes needed. Templates check both the target metric AND its most common input metric before concluding.

---

## 4. Context Events (for causes outside the data)

Some causes live outside the cube: paused creatives, competitor launches, platform outages, manual decisions. These need an annotation mechanism.

### Proposed schema: `.mosaic/context-events.yaml`

```yaml
events:
  - id: ce-2026-04-001
    period: "2026-04"
    scope:
      Channel: "Targeted Display"
    type: budget_change
    description: "Budget reduced 40% for Q1 close-out"
    explains:
      - impressions_declined_significant
      - clicks_declined_significant
    source: "Account manager"

  - id: ce-2026-04-002
    period: "2026-04"
    scope:
      Channel: "Targeted Display"
    type: creative_pause
    description: "3 of 5 creatives paused pending new assets"
    explains:
      - ctr_declined_significant
    source: "Creative team"
    expires_at: "2026-04-15"
```

### Evaluator functions

```
has_context_event(type, lookback_periods?)  → bool (1.0/0.0)
context_description(type?)                  → string (for template interpolation)
context_event_count(type?, lookback?)       → f64
```

### Three sources of context events (priority order)

1. **Hand-written** — analyst logs events as they happen (Phase 7B editor UI)
2. **Imported** — campaign management tools, project trackers (Tessera recipes)
3. **Auto-detected** — engine synthesizes events from large input changes (e.g., budget drops 40% → auto-generate a `budget_change` event). This is the killer feature: zero manual work for the most common explanations.

---

## 5. The Honesty Principle

**When the engine doesn't know why something happened, it says so.**

A tool that confidently misattributes 5% of findings is worse than one that honestly says "investigate causes" 5% of the time. The product positioning is deterministic interpretation — confident wrong answers break that.

Every finding MUST have a bare-finding fallback template at priority 999:

```yaml
- id: impressions_declined_unexplained
  finding_id: impressions_declined_significant
  explanation_priority: 999
  severity: warning
  when: "current.Impressions < prev.Impressions * 0.85"
  template: >
    Impressions declined {change_pct:.0f}% from {prev_period} to
    {current_period}. No clear explanation detected from available
    data — manual review recommended.
```

---

## 6. Template Versioning Strategy

Templates will evolve significantly as new explanation types are added, as cartridge authors refine their logic, and as the editor enables rapid iteration. Need a versioning strategy from the start.

### Problem

- Templates may be referenced by ledger entries (`template_id` field)
- Template semantics may change (same `id`, different `when:` logic or wording)
- Removing templates breaks streak/count queries that reference them
- Multiple template files may define conflicting `id` values

### Proposed approach: content-addressed versions with stable IDs

```yaml
narrative_format_version: 2   # bump from 1 → 2 when adding versioning fields

templates:
  - id: ctr_above_own_median
    version: 3                # monotonic per-template version counter
    changelog: "Added sample_count guard; reworded for clarity"
    supersedes: ~             # null = this is the active version
    deprecated: false         # true = skip during evaluation, keep for ledger queries
    ...
```

**Rules:**

1. **`id` is forever.** Once a template ships, its `id` never changes. Same principle as diagnostic codes (CVE-style retirement).
2. **`version` increments on any semantic change.** Wording-only changes don't need a version bump; `when:` predicate or `bindings:` changes always do.
3. **Deprecated templates don't fire** but remain queryable by `ledger_count` / `ledger_streak` (they reference the `template_id`). A deprecated template with active streak history still contributes to streak calculations.
4. **`supersedes`** — when one template replaces another (e.g., `ctr_trend` is replaced by `ctr_above_own_median`), the new template declares `supersedes: ctr_trend`. Ledger queries for the old `template_id` are transparently redirected.
5. **`narrative_format_version: 2`** — version 1 files (no `version`/`deprecated` fields) are interpreted as version 1, not deprecated. Backwards compatible.

### Migration path

- Phase 7A.1-7A.4: `narrative_format_version: 1`. No versioning fields. Templates evolve freely because ledgers are young.
- Phase 7B (editor ships): bump to `narrative_format_version: 2`. Add `version` field to all templates (start at 1). Editor tracks version history.
- Phase 7C+: deprecation/supersession logic for long-lived workspaces with deep ledger history.

### Template file organization

```
demo/narratives/
├── display-like.yaml          # observational templates (7A.1)
├── trend-templates.yaml       # cross-period templates (7A.3)
├── benchmark-templates.yaml   # own-workspace benchmark templates (7A.4)
├── explanation-templates.yaml # causal attribution templates (7A.5/7B)
└── context-events.yaml        # manual annotations (7A.5/7B)
```

Each file is independently loadable. The evaluator merges all templates at runtime. No file can redefine an `id` from another file (MC7050 diagnostic: duplicate template ID across files).

---

## 7. Implementation Phasing

| Phase | What ships | Engine changes needed |
|---|---|---|
| 7A.4 (current) | Own-workspace benchmarks | `benchmark_*()` functions (already planned) |
| 7A.5 (new) | Explanation chains + context events | `finding_id` / `explanation_priority` fields, `has_context_event()` family, context event loader, auto-detection for input-change events |
| 7B (editor) | Template versioning + visual authoring | `version` / `deprecated` / `supersedes` fields, `narrative_format_version: 2`, editor UI |
| 7C (future) | LLM-assisted template generation | LLM proposes templates in editor, user reviews + approves, templates accumulate in workspace |

### 7A.5 scope estimate: 4-5 sessions

1. Schema: `finding_id`, `explanation_priority`, explanation-chain evaluation logic
2. Context events: schema, loader, `has_context_event()` / `context_description()` functions
3. Auto-detection: synthesize context events from large input changes (budget, traffic, etc.)
4. Starter explanation templates: 8-10 templates covering the top marketing/generic patterns
5. MC7050-MC7054 diagnostics + polish

---

## 8. Open Questions (for ADR)

1. **Explanation priority collisions.** If two templates have the same `finding_id` and `explanation_priority` and both match, which fires? Options: first-in-file wins (deterministic but fragile), error (strict), both fire (user decides). Recommendation: error at load time — collisions are a template authoring bug.

2. **Auto-detected events vs. template logic.** Should auto-detected events (e.g., "budget dropped 40%") live in `context-events.yaml` (written there by the engine) or should they be purely in-evaluator logic? If written to file, they're inspectable but the file mutates on every run. If in-evaluator only, they're ephemeral but clean. Recommendation: write auto-detected events to a separate `.mosaic/auto-events.yaml` that's regenerated each run (not append-only like the ledger).

3. **Cross-template explanation inheritance.** If `ctr_declined_significant` is explained by a budget change, should `ctr_below_own_p25` (which references the same CTR value) also be suppressed? Recommendation: no — each `finding_id` is independent. Template authors compose the chains they want explicitly.

4. **Template `version` and ledger references.** If a template at version 3 fires, what does the ledger record? Just `template_id` (current behavior) or `template_id@version`? Recommendation: record `template_id` only for streak/count queries (version is metadata, not identity). The ledger entry's `generated_at` timestamp + the template's version history is sufficient for audit.

---

## 9. The Strategic Framing

The narrative engine's differentiation from LLM-generated reports:

| Dimension | LLM report | Mosaic narrative |
|---|---|---|
| Cost | Per-token, scales with volume | Zero marginal cost after template authoring |
| Consistency | Varies across runs | Deterministic, same input = same output |
| Auditability | Opaque ("the AI said it") | Full evidence trail, versioned templates |
| Compounding | Every report starts from zero | Every report builds institutional knowledge |
| Cross-account | Cannot compare structured findings | Ledger enables portfolio-wide analytics |
| Causal attribution | Guesses confidently | Explains when it can, admits when it can't |

The explanation-chain pattern strengthens the last row. The engine doesn't hallucinate causes — it walks a priority list of testable hypotheses and reports the first one that passes, or honestly says "investigate" when none do.

---

*End of research note. Phase 7A.5 (explanation chains + context events) should be scoped after 7A.4 ships. Template versioning ships with Phase 7B (editor). Both build on the primitives 7A.1-7A.4 establish.*
