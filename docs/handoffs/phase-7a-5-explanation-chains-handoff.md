# Phase 7A.5 Handoff — Explanation Chains + Context Events

> **Audience:** the Claude Code instance that implements Phase 7A.5.
> **You inherit `main` at 1007 / 0 tests. You'll work on the branch
> `phase-7a-5/explanation-chains`.**
>
> **This phase turns the narrative engine from "describes what happened"
> to "explains what happened."** Phase 7A.4 can say "CTR is below
> your historical median." Phase 7A.5 adds: "and that's because your
> budget was cut 40% — delivery scaled proportionally, no efficiency
> issue." Same data, different conclusion, deterministic logic.
>
> **The binding design is in
> [`docs/decisions/0022-phase-7a-5-explanation-chains.md`](../decisions/0022-phase-7a-5-explanation-chains.md).
> Read it in full before starting.**

---

## The one paragraph you must internalize

Templates already have `id`, `when:`, `template:`, `bindings:`. Phase
7A.5 adds two optional fields: `finding_id` and `explanation_priority`.
Templates sharing the same `finding_id` form an **explanation group**.
The evaluator processes groups in priority order (lower number = fires
first). The first template whose `when:` passes fires; the rest are
suppressed. Templates without `finding_id` fire independently — zero
behavior change. A second new concept, **context events**, lets the
workspace owner annotate periods with operational context ("budget cut
40%") stored in `.mosaic/context-events.yaml`. Templates query these
via `has_context_event()` / `context_description()`. Auto-detection
synthesizes events from large input changes (budget ±20%) as
ephemeral in-memory events. The ledger records which explanation won,
which were skipped (short-circuited), and which were rejected
(evaluated, `when:` false).

---

## What gets built (5 sessions estimated)

### Session 1 (~3-4h): Schema + explanation-chain evaluation logic

**Goal:** `finding_id` and `explanation_priority` fields work in YAML
templates. The evaluator groups and evaluates explanation chains.

**Deliverables:**

1. **Schema additions** in `mc-narrative/src/schema.rs`:

   Add two optional fields to `TemplateDefinition`:

   ```rust
   /// Finding group — templates sharing the same finding_id form
   /// an explanation chain evaluated in priority order.
   #[serde(default)]
   pub finding_id: Option<String>,

   /// Priority within an explanation group (lower = fires first).
   /// Default: 500. Templates without finding_id ignore this field.
   #[serde(default = "default_explanation_priority")]
   pub explanation_priority: u32,
   ```

   Add to `NarrativeOutput`:

   ```rust
   /// Phase 7A.5: which finding group produced this narrative (if any).
   #[serde(skip_serializing_if = "Option::is_none")]
   pub finding_id: Option<String>,

   /// Phase 7A.5: templates skipped because a higher-priority
   /// explanation matched first (never evaluated).
   #[serde(skip_serializing_if = "Vec::is_empty")]
   pub skipped_explanations: Vec<String>,

   /// Phase 7A.5: templates evaluated but whose when: predicate
   /// returned false (considered and rejected).
   #[serde(skip_serializing_if = "Vec::is_empty")]
   pub rejected_explanations: Vec<String>,
   ```

2. **Evaluation pipeline change** in `mc-narrative/src/lib.rs`:

   The current `evaluate_all` iterates templates linearly. Change to:

   a. **Pre-group** templates by `finding_id` at the start of `evaluate_all`:
      - Templates with no `finding_id` → standalone group (evaluated as before)
      - Templates with the same `finding_id` → explanation group, sorted by `explanation_priority`

   b. **For each cube/scope**, evaluate:
      - All standalone templates (current behavior, unchanged)
      - For each explanation group:
        - Iterate templates in priority order
        - Evaluate `when:` for each template
        - If `when:` is truthy → fire this template, record all remaining templates as `skipped_explanations`, record any prior templates that were evaluated but returned false as `rejected_explanations`. Stop evaluating this group.
        - If `when:` is falsy → add to `rejected_explanations`, continue to next template in group
        - If no template matches → the group produces no output (unless there's a priority-999 fallback whose `when:` is always true)

3. **Validation additions** in `validate_templates()`:

   - **MC7050:** Two templates share the same `finding_id` AND `explanation_priority` → Error (load fails)
   - **MC7053:** A `finding_id` group has no template with `explanation_priority >= 900` → Info
   - **MC7055:** A `finding_id` is referenced by only one template → Info (likely typo)

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Default `explanation_priority` when omitted | **500** (serde default). Standalone templates (no `finding_id`) ignore this value. |
| What if `finding_id` is set but `explanation_priority` is not? | **Uses default 500.** The template is part of the group at default priority. |
| Pre-grouping cost | **Once per `evaluate_all` call.** Build a `HashMap<String, Vec<&TemplateDefinition>>` keyed by `finding_id`. O(N) where N = template count. |
| Do explanation groups interact with `deduplicate: true`? | **Yes, independently.** A template can be both deduplicated (fires at most once across all cubes) AND part of an explanation group (fires at most once within its group per cube). |
| Do explanation groups interact with `sort_order`? | **No.** `sort_order` controls the order standalone templates fire. Within an explanation group, `explanation_priority` controls order. |

**Regression tests (5 minimum):**
1. `test_explanation_chain_first_match_fires`
2. `test_explanation_chain_fallback_fires_when_no_match`
3. `test_explanation_chain_skipped_and_rejected_recorded`
4. `test_templates_without_finding_id_fire_independently`
5. `test_mc7050_priority_collision_error`

---

### Session 2 (~3-4h): Context events schema + evaluator functions

**Goal:** `.mosaic/context-events.yaml` loads and `has_context_event()`
returns correct values.

**Deliverables:**

1. **Context events module** — `mc-narrative/src/context_events.rs`:

   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ContextEventsFile {
       pub schema_version: String,
       pub events: Vec<ContextEvent>,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ContextEvent {
       pub id: String,
       pub period: String,
       #[serde(default)]
       pub scope: BTreeMap<String, String>,
       #[serde(rename = "type")]
       pub event_type: String,
       pub description: String,
       #[serde(default)]
       pub source: Option<String>,
       #[serde(default)]
       pub expires_at: Option<String>,
   }
   ```

   Read/write functions following the same pattern as `benchmark.rs`:
   - `read_context_events(dir: &Path) -> Result<Vec<ContextEvent>, ...>`
   - Reads `.mosaic/context-events.yaml`; returns empty vec if absent

2. **ContextIndex** in `evaluator.rs` (alongside `LedgerIndex` and `BenchmarkIndex`):

   ```rust
   pub struct ContextIndex {
       /// Events grouped by (event_type, scope_key).
       entries: HashMap<(String, String), Vec<ContextIndexEntry>>,
       pub current_period: Option<String>,
   }
   ```

   Built once per `evaluate_all` call. Period/scope matching:
   - Scope match: event scope is a subset of current evaluation scope
   - Period match: `has_context_event('type')` checks current period only; `has_context_event('type', 3)` checks 3 periods total (current + 2 prior)
   - `expires_at` filtering: expired events excluded

3. **Evaluator functions** — add to dispatch in `evaluator.rs`:

   ```
   has_context_event(type)                    → f64 (1.0/0.0)
   has_context_event(type, lookback_periods)  → f64 (1.0/0.0)
   context_description(type)                  → Val::Str
   context_event_count(type)                  → f64
   context_event_count(type, lookback_periods) → f64
   ```

4. **`evaluate_all` signature update:**

   ```rust
   pub fn evaluate_all(
       templates: &[TemplateDefinition],
       cubes: &[CubeData],
       ledger: Option<&[LedgerEntry]>,
       benchmark: Option<&BenchmarkLibrary>,
       context_events: Option<&[ContextEvent]>,  // NEW
   ) -> Vec<NarrativeOutput>
   ```

   All existing callers pass `None`. Zero behavior change.

5. **Validation** — MC7051, MC7052, MC7054:
   - MC7051: event references period not in any cube → Warning
   - MC7052: event `expires_at` is before its `period` → Warning
   - MC7054: context events file parse error → Error

**Regression tests (5 minimum):**
1. `test_has_context_event_matches_current_period`
2. `test_has_context_event_lookback_3_periods`
3. `test_context_description_returns_first_match`
4. `test_context_event_scope_subset_matching`
5. `test_context_events_absent_returns_zero`

---

### Session 3 (~3h): Auto-detection + CLI verb

**Goal:** Engine auto-synthesizes budget change events. `mc model
context-events` CLI verb works.

**Deliverables:**

1. **Auto-detection** in `evaluate_all` or a helper called from it:

   Before evaluating templates for each cube, scan for large input changes
   and synthesize ephemeral `ContextEvent` entries:

   | Condition | Synthesized `event_type` | Description |
   |---|---|---|
   | `current.Budget < prev.Budget * 0.80` | `budget_decrease` | "Budget decreased {pct:.0f}% from {prev_period}" |
   | `current.Budget > prev.Budget * 1.20` | `budget_increase` | "Budget increased {pct:.0f}% from {prev_period}" |
   | `period_count == 1` | `single_period` | "Only one reporting period available" |

   Synthesized events use `id: "auto-{type}-{period}"` and have empty
   scope (apply to all). They're added to the `ContextIndex` alongside
   manual events. They are NOT written to disk.

   **Thresholds (80%/120%) are v1 constants.** Add a `// TODO(cartridge-config):` comment at the definition site.

2. **CLI verb** — `mc model context-events`:

   ```
   mc model context-events <model-dir> --list
   mc model context-events <model-dir> --add \
     --period 2026-04 --type budget_change \
     --scope 'Channel=Targeted Display' \
     --description "Budget reduced 40%"
   mc model context-events <model-dir> --remove ce-2026-04-001
   ```

   `--add` generates `id` automatically (`ce-{period}-{NNN}` where NNN
   is the next sequential number for that period).
   `--list` prints a table of all events.
   `--remove` removes by id and rewrites the file.

   Wire into `mc-cli/src/main.rs` alongside `build-benchmarks` and
   `show-benchmarks`.

3. **`narrate` and `narrate-trends` auto-load** context events from
   `.mosaic/context-events.yaml` if present, same pattern as benchmark
   library loading in `mc-cli/src/narrate.rs` and `narrate_trends.rs`.

**Regression tests (4 minimum):**
1. `test_auto_detect_budget_decrease_event`
2. `test_auto_detect_single_period_event`
3. `test_auto_events_coexist_with_manual_events`
4. `test_context_events_cli_add_and_list`

---

### Session 4 (~3h): Explanation templates + demo integration

**Goal:** Ship 8-10 explanation templates. Wire demo server.

**Deliverables:**

1. **New template file** — `demo/narratives/explanation-templates.yaml`:

   Build explanation groups for the most common marketing findings.
   Each group needs: a context-event explanation, a data-driven
   correlation explanation, and a bare-finding fallback at priority 999.

   Example group for `impressions_declined_significant`:

   ```yaml
   # Priority 100: manual context event explains it
   - id: impressions_declined_context_event
     finding_id: impressions_declined_significant
     explanation_priority: 100
     when: >
       current.Impressions < prev.Impressions * 0.85
       AND has_context_event('budget_change') == 1
     severity: info
     template: >
       Impressions declined {change_pct:.0f}%. A budget change was
       logged for this period: {event_desc}. The decline may be
       attributable to this operational decision.
     bindings:
       change_pct: "(current.Impressions - prev.Impressions) / prev.Impressions * 100"
       event_desc: "context_description('budget_change')"

   # Priority 200: auto-detected budget change
   - id: impressions_declined_auto_budget
     finding_id: impressions_declined_significant
     explanation_priority: 200
     when: >
       current.Impressions < prev.Impressions * 0.85
       AND has_context_event('budget_decrease') == 1
     severity: info
     template: >
       Impressions declined {change_pct:.0f}%. The engine detected a
       significant budget decrease in this period. Delivery likely
       scaled with spend.
     bindings:
       change_pct: "(current.Impressions - prev.Impressions) / prev.Impressions * 100"

   # Priority 999: bare finding fallback
   - id: impressions_declined_unexplained
     finding_id: impressions_declined_significant
     explanation_priority: 999
     when: "current.Impressions < prev.Impressions * 0.85"
     severity: warning
     template: >
       Impressions declined {change_pct:.0f}% from {prev_period} to
       {current_period}. No clear explanation detected — manual review
       recommended.
     bindings:
       change_pct: "(current.Impressions - prev.Impressions) / prev.Impressions * 100"
   ```

   Build similar groups for: `ctr_declined_significant`,
   `clicks_declined_significant`, `conversions_zero_alarm`.
   Target: 8-10 templates across 3-4 finding groups.

2. **Demo server integration** in `mc-demo-server`:
   - Load `.mosaic/context-events.yaml` at startup (same pattern as benchmark library)
   - Pass to `evaluate_all` as the 5th parameter
   - `GET /api/context-events` endpoint: returns events JSON if present
   - Terminal log: `[context] Loaded N events for M periods`

3. **Sample context events** — `demo/sample-data/context-events.yaml`:
   Ship a sample file with 2-3 events for the Scotts RV dataset so
   the demo can show the explanation chain in action.

**Regression tests (3 minimum):**
1. `test_explanation_templates_fire_with_context_event`
2. `test_explanation_fallback_fires_without_context_event`
3. `test_demo_server_loads_context_events`

---

### Session 5 (~2-3h): Diagnostics + ledger integration + acceptance gates

**Goal:** Ship-ready with all MC7050-MC7055, ledger records
explanation metadata, and all gates green.

**Deliverables:**

1. **MC7050-MC7055 sweep:** verify all 6 codes are emitted in at least
   one code path. Pre-flight:
   ```bash
   for code in MC7050 MC7051 MC7052 MC7053 MC7054 MC7055; do
     grep -rn "$code" crates/ | wc -l
   done
   ```

2. **Ledger integration** — update `write_demo_ledger` in
   `mc-demo-server/src/upload.rs` and the ledger write path in
   `mc-narrative/src/ledger.rs` to include the new fields:

   ```json
   {
     "template_id": "impressions_declined_budget_proportional",
     "finding_id": "impressions_declined_significant",
     "skipped_explanations": ["impressions_declined_unexplained"],
     "rejected_explanations": ["impressions_declined_seasonal"]
   }
   ```

   The `LedgerEntry` struct's `NarrativeRecord` or a new adjacent
   field carries `finding_id`, `skipped_explanations`, and
   `rejected_explanations`. Existing entries without these fields
   are backwards-compatible (serde default to None/empty).

3. **Performance check:** evaluate_all with 50 templates including
   10 explanation groups must complete in < 5ms overhead vs. the
   non-grouped baseline. The sub-200ms contract from Phase 6D is
   preserved.

4. **Polish:**
   - Ensure `narrate --save-ledger` writes the enriched entries
   - Verify all existing tests still pass (zero regression)
   - `cargo fmt --check --all` + `cargo clippy` + `cargo build --release`

**Regression tests (3 minimum):**
1. `test_ledger_records_finding_id_and_explanations`
2. `test_ledger_backwards_compat_without_finding_id`
3. `test_explanation_chain_performance_under_5ms`

---

## Hard Rules (binding)

1. **`mc-core`, `mc-model`, `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera` all locked.**
2. **New context events module lives in `mc-narrative/src/context_events.rs`.** Evaluator functions live in `evaluator.rs` alongside `ledger_*` and `benchmark_*` families.
3. **`evaluate_all` gains `context_events: Option<&[ContextEvent]>` as 5th param.** All existing callers pass `None`. Zero behavior change without context events.
4. **Templates without `finding_id` fire independently** — the explanation chain logic only activates for templates that opt in.
5. **Auto-detected events are ephemeral** — never written to disk. Rebuilt each evaluation.
6. **`skipped_explanations` and `rejected_explanations` are distinct lists** in the ledger. "Never evaluated" vs "evaluated and rejected" is diagnostically important.
7. **Per-session commits (Rule 11).** 5 commits minimum.

---

## Acceptance Gates

- [ ] `cargo fmt --check --all` exits 0
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0
- [ ] `cargo build --release --workspace` zero warnings
- [ ] `cargo test --workspace` passes (1007 → expect ~1022)
- [ ] Explanation chain: first match fires, rest skipped/rejected
- [ ] Templates without `finding_id` unchanged (zero behavior change)
- [ ] MC7050 fires on priority collision at load time
- [ ] MC7055 fires on single-template finding_id
- [ ] `has_context_event()` returns correct values with loaded events
- [ ] Auto-detected budget change fires when `current.Budget < prev.Budget * 0.80`
- [ ] Ledger records `finding_id`, `skipped_explanations`, `rejected_explanations`
- [ ] `mc model context-events --add/--list/--remove` works
- [ ] Performance: < 5ms overhead for explanation-chain evaluation
- [ ] MC7050-MC7055 codes swept free before implementation, then shipped
- [ ] Demo server loads context events if present
- [ ] Locked surfaces: zero diff

---

## SPEC QUESTION candidates

- Session 1: Should `sort_order` affect the order explanation groups
  are evaluated relative to standalone templates? (PM default: no —
  standalone templates fire in `sort_order`; explanation groups fire
  after all standalone templates, in arbitrary group order. Within
  each group, `explanation_priority` controls order.)

- Session 2: `context_description` returns the first matching event's
  description. Should it concatenate all matching descriptions if
  multiple events match? (PM default: no — first by `id` (sorted)
  wins. Concatenation produces unpredictable output length.)

- Session 4: Should explanation templates in
  `explanation-templates.yaml` replace any templates in
  `display-like.yaml` that cover the same findings? (PM default:
  no — explanation templates are additive. The existing
  `impressions_mom_decline` template in `display-like.yaml` has
  no `finding_id` so it fires independently. If the author wants
  it part of a group, they add `finding_id` to it.)

---

## Completion context

After 7A.5 ships, the narrative engine can produce:

> "Impressions declined 30%. A budget change was logged for this period:
> Budget reduced 40% for Q1 close-out. The decline is attributable to
> this operational decision."

vs. the previous output without explanation chains:

> "Impressions declined 30%. This persistent trend warrants investigation."

Same data. Different conclusion. The first is correct; the second
implies underperformance where there is none. The ledger records that
the engine checked for seasonal patterns (rejected), checked for
proportional budget movement (matched), and skipped the bare-finding
fallback. The audit trail is complete.

Phase 7B (visual template editor) inherits this and adds
`narrative_format_version: 2` with template versioning. Phase 7C adds
LLM-assisted template generation. But the explanation-chain primitive
from 7A.5 is the foundation both phases build on.

---

*End of handoff. Phase 7A.5 is the last piece before the visual editor.
After this ships, the narrative engine can observe, trend, benchmark,
AND explain — all deterministically, all from structured evidence, all
without an LLM at runtime.*
