# ADR-0022 — Phase 7A.5: Explanation Chains + Context Events

**Status:** Proposed  
**Date:** 2026-05-08  
**Author:** Edwin Lovett III  
**Depends on:** ADR-0020 (Phase 7A narrative engine plan), Phase 7A.4 (benchmark aggregation)  
**Research note:** [`../research-notes/explanation-chains-and-template-versioning.md`](../research-notes/explanation-chains-and-template-versioning.md)

---

## Context

Phase 7A.1-7A.4 built the narrative intelligence pipeline: template evaluation → interpretation ledger → cross-period analysis → own-workspace benchmarks. The engine can now observe *what* happened ("Impressions declined 30%") and compare it to the workspace's own history ("below your p25"). But it cannot explain *why* something happened.

Without causal context, narratives risk implying wrong conclusions:

- "Impressions declined 30%" when the real cause is "budget was cut in half" — the engine implies underperformance where there is none
- "CTR dropped below historical median" when 3 of 5 creatives were paused — the engine alarms on an operational decision
- "Revenue missed target" when February has fewer selling days — the engine ignores a calendar effect

This problem is domain-agnostic:

| Domain | Observed | Actual cause | Wrong conclusion without context |
|---|---|---|---|
| Marketing | Impressions down 30% | Budget cut 28% | "Delivery efficiency issue" |
| Finance | Recruiting spend up 40% | Headcount grew 35% | "Cost overrun" |
| Sports | Points scored down 25% | Minutes played down 30% | "Player efficiency decline" |

Phase 7A.5 adds **explanation chains** — a single primitive that lets the engine walk prioritized candidate explanations before concluding. The first explanation whose predicate passes fires; if none match, the engine honestly says "investigate."

---

## Decisions

### Decision 1: Explanation chains via `finding_id` + `explanation_priority`

Two new optional fields on template definitions:

```yaml
- id: impressions_declined_budget_proportional
  finding_id: impressions_declined_significant
  explanation_priority: 200
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

**Semantics:**

- Templates with the same `finding_id` form an **explanation group**
- Within a group, templates are evaluated in `explanation_priority` order (lower = higher priority = fires first)
- The first template in the group whose `when:` predicate passes fires; all remaining templates in that group are suppressed
- Templates without `finding_id` fire independently (current behavior, unchanged — zero breaking change)
- Templates without `explanation_priority` default to `500`

**Why a single primitive:** context events, data-driven correlation, benchmarks, seasonality, calendar effects, and YoY comparisons all plug into the same pattern. No special-case code per explanation type. A cartridge author adds a new explanation type by adding a template with the right `finding_id` and a priority that positions it in the chain.

---

### Decision 2: Priority collision is a load-time error

If two templates share the same `finding_id` AND the same `explanation_priority`, the template loader emits **MC7050** and fails. This is a template authoring bug — the author must assign distinct priorities.

**Why strict:** deterministic output requires deterministic evaluation order. "First-in-file wins" is fragile (file merge order, multi-file loading order). An explicit error catches the ambiguity at authoring time, not at report time.

---

### Decision 3: The honesty principle — mandatory fallback

Every `finding_id` group SHOULD include a bare-finding fallback template at `explanation_priority: 999`:

```yaml
- id: impressions_declined_unexplained
  finding_id: impressions_declined_significant
  explanation_priority: 999
  when: "current.Impressions < prev.Impressions * 0.85"
  severity: warning
  template: >
    Impressions declined {change_pct:.0f}% from {prev_period} to
    {current_period}. No clear explanation detected from available
    data — manual review recommended.
```

This is a SHOULD, not a MUST. If a group has no 999-priority fallback, and no explanation matches, the finding is silently skipped (same as any template whose `when:` doesn't match). The validator emits **MC7053** (info) for groups missing a fallback — not an error, but a nudge.

**Why:** a tool that confidently misattributes 5% of findings is worse than one that honestly says "investigate" 5% of the time. The product positioning is deterministic interpretation — confident wrong answers break that.

---

### Decision 4: Context events file

A new file `.mosaic/context-events.yaml` stores operational annotations that explain findings from causes outside the cube:

```yaml
schema_version: "1.0"

events:
  - id: ce-2026-04-001
    period: "2026-04"
    scope:
      Channel: "Targeted Display"
    type: budget_change
    description: "Budget reduced 40% for Q1 close-out"
    source: "Account manager"

  - id: ce-2026-04-002
    period: "2026-04"
    scope:
      Channel: "Targeted Display"
    type: creative_pause
    description: "3 of 5 creatives paused pending new assets"
    source: "Creative team"
    expires_at: "2026-04-15"
```

**Fields:**

| Field | Required | Description |
|---|---|---|
| `id` | Yes | Unique identifier. Convention: `ce-{period}-{seq}` |
| `period` | Yes | The reporting period this event applies to |
| `scope` | No | Scope filter — `{ Channel: "X", Market: "Y" }`. Empty = all scopes |
| `type` | Yes | Free-form category string. Suggested values: `budget_change`, `creative_pause`, `targeting_change`, `platform_outage`, `seasonal`, `competitive`, `calendar` |
| `description` | Yes | Human-readable explanation. Used in template interpolation via `context_description()` |
| `source` | No | Provenance — who logged this event |
| `expires_at` | No | ISO date. After this date, the event no longer matches for the period. A "creative pause for one week" shouldn't explain declines two months later |

**Why YAML (not JSONL):** context events are hand-edited annotations, not machine-generated logs. YAML is friendlier for human authoring. The file is small (tens of events, not thousands). Rewritten in full when edited (not append-only like the ledger).

---

### Decision 5: Context event evaluator functions

Four new functions in the evaluator, alongside the `ledger_*` and `benchmark_*` families:

```
has_context_event(type)                    → f64 (1.0/0.0)
has_context_event(type, lookback_periods)  → f64 (1.0/0.0)
context_description(type)                  → string
context_event_count(type)                  → f64
context_event_count(type, lookback_periods) → f64
```

**Scope matching:** context events match when their `scope` is a subset of the current evaluation scope. An event with `scope: { Channel: "Targeted Display" }` matches any evaluation where the current scope includes `Channel: "Targeted Display"`. An event with empty scope matches everywhere.

**Period matching:** `has_context_event('budget_change')` checks the current period only. `has_context_event('budget_change', 3)` checks the current period and the 2 prior periods. Respects `expires_at` — an expired event doesn't match even if the period is in range.

**`context_description` returns the first matching event's description.** If multiple events match, the first by `id` (sorted) wins. This is deterministic.

**When no context-events.yaml exists:** all context event functions return 0.0 / empty string. Graceful degradation, same as benchmark functions without a benchmark library.

---

### Decision 6: Auto-detected events

The engine automatically synthesizes context events from large input changes detected in the cube data. These are ephemeral (computed at evaluation time, not written to disk).

**Auto-detection rules:**

| Condition | Synthesized event type | Description pattern |
|---|---|---|
| `current.Budget < prev.Budget * 0.80` | `budget_decrease` | "Budget decreased {pct}% from {prev_period}" |
| `current.Budget > prev.Budget * 1.20` | `budget_increase` | "Budget increased {pct}% from {prev_period}" |
| `period_count == 1` | `single_period` | "Only one reporting period available" |

Auto-detected events have `explanation_priority` lower than manual context events but higher than bare findings. The typical chain becomes:

```
Priority 100: Manual context event (analyst logged it)
Priority 200: Auto-detected input change (engine sees budget/traffic move)
Priority 300: Data-driven correlation (proportional movement template)
Priority 400: Benchmark deviation (own-history comparison)
Priority 999: Bare finding (fallback)
```

**Why ephemeral:** auto-detected events are derivable from the cube data on every run. Writing them to disk creates a file that mutates on every evaluation, which conflicts with the "hand-edited annotations" nature of `context-events.yaml`. Auto-detected events live in the evaluator's `ContextIndex` alongside manually-loaded events, but they're rebuilt fresh each evaluation.

---

### Decision 7: `evaluate_all` signature update

```rust
pub fn evaluate_all(
    templates: &[TemplateDefinition],
    cubes: &[CubeData],
    ledger: Option<&[LedgerEntry]>,
    benchmark: Option<&BenchmarkLibrary>,
    context_events: Option<&[ContextEvent]>,  // NEW
) -> Vec<NarrativeOutput>
```

All existing callers pass `None` for the new parameter. Zero behavior change without context events.

The evaluation pipeline changes:

1. Group templates by `finding_id` (templates without `finding_id` go in a "standalone" group)
2. For each cube/scope:
   a. Evaluate standalone templates (current behavior, unchanged)
   b. For each `finding_id` group, evaluate templates in `explanation_priority` order; emit the first match; skip the rest
3. Auto-detect events from cube data, add to the context index
4. Return all emitted narratives

---

### Decision 8: Explanation groups do not cross `finding_id` boundaries

If `ctr_declined_significant` is explained by a budget change, that does NOT suppress `ctr_below_own_p25` (a separate finding). Each `finding_id` is independent.

**Why:** template authors compose the chains they want explicitly. Implicit suppression across findings creates surprising behavior where adding one explanation template silently hides another finding's output. If the author wants CTR benchmark narratives to respect the budget explanation, they add `finding_id: ctr_declined_significant` to the benchmark template — an explicit choice.

---

### Decision 9: Ledger records which explanation fired

When a template fires as part of an explanation group, the ledger entry includes:

```json
{
  "template_id": "impressions_declined_budget_proportional",
  "finding_id": "impressions_declined_significant",
  "explanation_priority": 200,
  "suppressed_explanations": [
    "impressions_declined_efficiency",
    "impressions_declined_unexplained"
  ]
}
```

The `suppressed_explanations` list records which templates in the group were skipped (their `when:` wasn't evaluated because a higher-priority explanation already matched, OR their `when:` didn't match). This makes the explanation chain inspectable after the fact — "last month the decline was explained by budget; this month nothing explained it."

**Why:** the ledger is the audit trail. Knowing which explanation WON is useful; knowing which explanations LOST is diagnostic gold for template refinement.

---

### Decision 10: Diagnostic codes MC7050-MC7054

| Code | Condition | Severity |
|---|---|---|
| MC7050 | Two templates share the same `finding_id` AND `explanation_priority` | Error (load fails) |
| MC7051 | Context event references a period not present in any loaded cube | Warning |
| MC7052 | Context event `expires_at` is before its `period` | Warning |
| MC7053 | A `finding_id` group has no template with `explanation_priority >= 900` (missing fallback) | Info |
| MC7054 | Context events file parse error (malformed YAML, missing required fields) | Error |

---

### Decision 11: CLI integration

**`mc model narrate` and `mc model narrate-trends`** — both verbs auto-load `.mosaic/context-events.yaml` if present. Same graceful degradation as benchmark library loading.

**New verb: `mc model context-events <model-dir> [--add|--list|--remove]`**

```bash
# List current events
mc model context-events ./scotts-rv --list

# Add an event interactively
mc model context-events ./scotts-rv --add \
  --period 2026-04 \
  --type budget_change \
  --scope 'Channel=Targeted Display' \
  --description "Budget reduced 40% for Q1 close-out"

# Remove an event by ID
mc model context-events ./scotts-rv --remove ce-2026-04-001
```

The `--add` verb generates the `id` automatically (`ce-{period}-{seq}`) and appends to `.mosaic/context-events.yaml`. If the file doesn't exist, it creates it with `schema_version: "1.0"`.

---

## Scope boundaries

**Phase 7A.5 ships:**
- `finding_id` + `explanation_priority` fields on templates
- Explanation-chain evaluation logic in the evaluator
- Context events YAML schema + loader
- `has_context_event()`, `context_description()`, `context_event_count()` evaluator functions
- Auto-detection for budget increase/decrease + single-period
- 8-10 explanation templates in `demo/narratives/explanation-templates.yaml`
- `mc model context-events` CLI verb
- MC7050-MC7054 diagnostic codes
- Ledger records `finding_id` + `suppressed_explanations`
- Demo server loads context events if present

**Phase 7A.5 does NOT ship:**
- Template versioning (`version`, `deprecated`, `supersedes` fields) — that's Phase 7B
- `narrative_format_version: 2` bump — that's Phase 7B
- Duplicate `id` detection across files (MC7050 covers priority collisions; cross-file ID uniqueness is Phase 7B)
- Context event import from external systems (Tessera recipes) — that's Phase 7C+
- LLM-assisted template generation — that's Phase 7C
- Visual editor for context events — that's Phase 7B

---

## Alternatives considered

### Separate "Layer 1" and "Layer 2" implementations (rejected)

The original framing split data-driven attribution (Layer 1) and context events (Layer 2) as distinct features with separate infrastructure. Rejected because:

1. Both serve the same role: candidate explanations for an observed finding
2. Separate implementations means separate dispatch paths, separate config, separate docs
3. Future explanation types (seasonality, calendar, YoY) would each need their own layer
4. A unified primitive — `finding_id` + `explanation_priority` — subsumes all explanation types with zero special-case code

### Auto-detected events written to disk (rejected)

Alternative: write auto-detected events to `.mosaic/auto-events.yaml` on each evaluation run. Rejected because:

1. Auto-detected events are derivable from the cube data — they contain no new information
2. Writing to disk on every `narrate` run creates a file that changes without user action
3. The file would need to be in `.gitignore` (it's ephemeral), which is a new pattern
4. Keeping auto-detected events in-memory (ephemeral, rebuilt per evaluation) is simpler and achieves the same result

### Implicit cross-finding suppression (rejected)

Alternative: if any explanation fires for `ctr_declined_significant`, also suppress templates in related findings like `ctr_below_own_p25`. Rejected because:

1. Defining "related" is ambiguous — by metric? by scope? by severity?
2. Implicit suppression creates surprising behavior (adding an explanation template hides unrelated narratives)
3. Template authors can explicitly compose chains by assigning the same `finding_id` where they want suppression

---

## Success criteria

- [ ] Templates with `finding_id` + `explanation_priority` are grouped and evaluated in priority order
- [ ] First matching explanation in a group fires; rest are suppressed
- [ ] Priority collision (same `finding_id` + same priority) fails at load time with MC7050
- [ ] Templates without `finding_id` fire independently (zero behavior change)
- [ ] `.mosaic/context-events.yaml` loads and `has_context_event()` returns correct values
- [ ] Auto-detected budget change event fires when `current.Budget < prev.Budget * 0.80`
- [ ] Ledger records `finding_id` and `suppressed_explanations` for explanation-chain templates
- [ ] `mc model context-events --add` creates/appends to the events file
- [ ] MC7050-MC7054 codes swept free before implementation, then shipped
- [ ] `cargo test --workspace` passes (1001 → expect ~+15 = ~1016)
- [ ] Locked surfaces (mc-core, mc-model, mc-fixtures, mc-recipe, mc-drivers, mc-tessera): zero diff

---

*Phase 7A.5 completes the narrative engine's transition from observation to explanation. After this phase, the engine can say "Impressions declined 30%, consistent with the 28% budget reduction — no efficiency issue" instead of "Impressions declined 30%." Same data, different conclusion, deterministic logic, no LLM.*
