# Design Requirements: Narrative Studio (Template Authoring UI)

> **Status:** Research / design requirements  
> **Date:** 2026-05-08  
> **Supersedes:** `visual-template-builder.md` (pattern-picker concept — too sterile)  
> **Target phase:** Phase 7B  
> **Working name:** narrative.studio

---

## 1. Design Vision

The template authoring UI should feel like a **professional creative tool** — closer to a Zapier flow builder or Salesforce Expression Set editor than a SQL query box or code editor. The target user is a marketing analyst or account manager, not a developer. They should feel comfortable, not intimidated.

Three design principles:

1. **Flows, not code.** The primary interaction is visual — dropdowns, cards, connectors. The YAML is a compile target shown for power users, not the authoring surface.
2. **AI co-pilot, not AI replacement.** The LNM authoring agent observes the data and proposes templates. The user approves, edits, or rejects. Always in the loop.
3. **Live feedback.** As the user builds a template, they see what it would produce against their actual data — right now, not after saving.

---

## 2. UI Layout (Three Panels)

```
┌────────────────────────────────────────────────────────────────┐
│  narrative.studio / rule editor                    ● connected │
├──────────┬───────────────────────────┬─────────────────────────┤
│ LIBRARY  │  RULE BUILDER             │  OUTPUT / PREVIEW       │
│          │                           │                         │
│ ☐ Filter │  [rule name]  [family ▾]  │  COMPILED YAML          │
│          │  [severity ▾]             │  ─────────────          │
│ • rule_1 │                           │  id: untitled_rule      │
│   info   │  ┌─── WHEN ────────────┐  │  when: >                │
│          │  │                      │  │    current.Impressions  │
│ • rule_2 │  │  When [Metric ▾]    │  │    > prev.Impressions   │
│   warn   │  │  [increased ▾]      │  │  template: |            │
│          │  │  by [> ▾] [20] %    │  │    Impressions {pct}%   │
│ • rule_3 │  │                      │  │                         │
│   crit   │  └──────────────────────┘  │  ─────────────          │
│          │           │                │  LIVE PREVIEW           │
│          │           ▼                │  ─────────────          │
│          │  ┌─── THEN SAY ─────────┐  │  ✓ Would fire (3 of 6  │
│          │  │                      │  │    periods match)       │
│          │  │  "Impressions {dir}  │  │                         │
│          │  │   {pct}% between     │  │  "Impressions increased │
│          │  │   periods — from     │  │   22% between periods   │
│          │  │   {prev} to {curr}." │  │   — from 25,102 to     │
│          │  │                      │  │   30,655."              │
│          │  │  bindings: {pct}     │  │                         │
│          │  │           {prev}     │  │  Period: Aug_2025       │
│          │  │           {curr}     │  │  Evidence: { pct: 22.1, │
│          │  └──────────────────────┘  │    prev: 25102, ... }   │
│          │                           │                         │
│          │  [▸ Dry Run] [💾 Save]    │  yaml · deterministic   │
├──────────┴───────────────────────────┴─────────────────────────┤
│  AI Suggestions (3)                              [Refresh ↻]   │
│  ┌──────────┐ ┌──────────────┐ ┌─────────────────┐            │
│  │ CTR      │ │ Device       │ │ Conversion       │            │
│  │ decline  │ │ concentration│ │ zero alarm       │            │
│  │ detected │ │ > 80%        │ │ 4+ periods       │            │
│  └──────────┘ └──────────────┘ └─────────────────┘            │
│  Based on your data: 6 periods, 5 measures, 1 dimension        │
└────────────────────────────────────────────────────────────────┘
```

### Left Panel: Library

- List of all templates in the workspace (loaded from `narratives/*.yaml`)
- Filter by family, severity, active/inactive
- Blue dot = active, grey = disabled
- Click to load into the builder
- "New rule" button at bottom
- Drag to reorder (sets `sort_order`)

### Center Panel: Rule Builder

Two sections stacked vertically, connected by a flow arrow:

**WHEN section** (the condition):
- **Pattern-based builder** (default mode): dropdowns for metric, direction, operator, threshold, unit — generates the `when:` expression. Based on the 8 patterns from the visual-template-builder research note.
- **Flow mode** (advanced): Zapier-style card layout for compound conditions. Each condition is a card with Resource → Operator → Value fields. Cards connected by AND/OR connectors. Add Condition button. Matches the Salesforce Expression Set UX from the screenshots.
- **Raw mode** (power user escape hatch): direct `when:` expression editing with syntax highlighting. Toggle via a "code view" button.

All three modes keep the same underlying state in sync — switching modes doesn't lose work.

**THEN SAY section** (the template):
- Text area with `{placeholder}` auto-complete from available bindings
- Binding chips shown below the text area (clickable to insert)
- Severity picker (Info / Success / Warning / Critical) as colored pills
- Format hint picker per binding (Currency, Percent, Count, etc.)

**Top bar:**
- Rule name (editable inline)
- Family dropdown (display-like, trend, benchmark, explanation, custom)
- Severity dropdown
- Dry Run button (evaluates against current data without saving)
- Save button

### Right Panel: Output / Preview

**Top half — Compiled YAML:**
- Live-updating YAML output as the user builds
- Read-only but copyable
- Shows exactly what gets saved to `narratives/*.yaml`
- Status bar: "yaml · deterministic"

**Bottom half — Live Preview:**
- Shows whether the template would fire against the currently loaded data
- Shows the rendered narrative text with actual values substituted
- Shows which periods matched and which didn't
- Shows the evidence object (binding values)
- Updates in real-time as the user modifies the builder

### Bottom Bar: AI Suggestions

- 3-5 template suggestions based on the currently loaded data
- Each suggestion is a card showing: pattern name, key metric, brief description
- Click a suggestion to load it into the builder (pre-populated but not saved)
- "Refresh" button to get new suggestions
- Separate from the library — these are proposals, not saved templates
- The suggestions come from the LNM authoring agent (see §4)

---

## 3. Flow Builder Detail (Zapier/Expression Set Mode)

For compound conditions, the flow builder uses connected cards:

```
┌─────────────────────────────────────────────────┐
│  ▼  Impressions Declined                    ··· │
│     Condition                                   │
│                                                 │
│  Condition Requirements: All Conditions (AND) ▾ │
│                                                 │
│  Resource *          Operator *     Value *      │
│  [Impressions ▾]  ×  [Less Than ▾]  [prev * 0.85]│
│                                                 │
│  + Add Condition                                │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│  ▼  Budget Explains Decline                 ··· │
│     Condition                                   │
│                                                 │
│  Resource *          Operator *     Value *      │
│  [Budget ▾]       ×  [Less Than ▾]  [prev * 0.85]│
│                                                 │
│  + Add Condition                                │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│  fx  Proportional Check                     ··· │
│     Calculation                                 │
│                                                 │
│  Formula *                    Output Variable * │
│  [abs(impr_pct - budget_pct)] = [gap]           │
│                                                 │
│  Then: gap < 10                                 │
└─────────────────────────────────────────────────┘
```

Card types:
- **Condition** (blue icon): Resource → Operator → Value, with AND/OR grouping
- **Calculation** (purple fx icon): Formula → Output Variable (creates a binding)
- **Lookup Table** (green grid icon): maps a dimension value to a reference value (for benchmark comparisons, thresholds per channel, etc.)

Cards connect top-to-bottom with a flow line. The flow compiles to the `when:` expression + `bindings:` map. The YAML output panel updates live.

---

## 4. LNM Narrative Authoring Agent

The AI component is NOT a chat interface. It's an **observation-driven suggestion engine** that watches the data and proposes templates.

### 4.1 Behavior

When the user loads data (uploads CSV/PPTX or selects a workspace):

1. The agent scans the cube's structure: measures, dimensions, time periods, value distributions
2. It identifies patterns worth reporting: significant changes, outliers, zero-value alarms, concentration, trends
3. It proposes 3-5 template suggestions as cards in the bottom bar
4. Each suggestion is a fully-formed template (when + template + bindings) ready to load into the builder

### 4.2 Three suggestion modes

**Auto-suggestions (default):** based purely on the data structure and observed patterns. No user input needed.

```
Suggested: "CTR declined 15% — below campaign average"
Based on: CTR dropped from 0.54% to 0.31% between Aug and Sep
Pattern: period_change + relative_to_average
```

**User-directed:** the user types a natural-language request:

```
User: "Tell me when any channel's spend is more than 50% of total spend"
Agent: generates a concentration template for Spend across Channel dimension
```

**Cube-aware guardrails:** when the user requests a template the cube can't support:

```
User: "Alert me when ROAS drops below 2.0"
Agent: "The current cube doesn't have a ROAS measure. Available measures:
        Impressions, Clicks, CTR, Conversions, Spend. 
        
        Alternative: I can create a template that monitors Cost Per Conversion
        (Spend / Conversions) instead — would that work?"
```

### 4.3 Implementation

The agent is a Mosaic plugin skill (Phase 4A pattern):

```yaml
# mosaic-plugin/skills/narrative-author.md
---
name: narrative-author
description: >
  Teaches an LLM how to author narrative templates for Mosaic.
  Not domain-specific — examines cube data structure and proposes
  templates based on observed patterns. Validates proposals against
  the cube's available measures and dimensions.
---
```

The skill:
- Receives cube metadata (measures, dimensions, sample values, time periods)
- Knows the 8 pattern types and the YAML template schema
- Proposes templates using the pattern compiler (same as the form-based builder)
- Validates that every measure/dimension referenced in the proposal exists in the cube
- Explains why it proposed each template (links to the data pattern it detected)

The agent runs at **design time** (when the user is building templates), not at **runtime** (when reports are generated). This is the amortized-intelligence model: LLM cost at authoring time, deterministic execution forever after.

---

## 5. Live Preview Architecture

The preview panel needs to evaluate a template-in-progress against real data without saving it to disk.

```
Frontend (builder state)
    ↓ POST /api/preview-template
    { when: "...", template: "...", bindings: {...}, cube_data: "current" }
    ↓
Backend (mc-narrative evaluate_single)
    ↓
    { fires: true/false, 
      rendered_text: "...", 
      evidence: {...},
      periods_matched: ["Aug_2025", "Sep_2025"],
      periods_skipped: ["Jul_2025"] }
    ↓
Frontend (live preview panel updates)
```

A new API endpoint `POST /api/preview-template` takes a single template definition + reference to the current cube data, evaluates it, and returns the result. The backend calls `mc_narrative::evaluate_single()` (a new function that evaluates one template against loaded cubes without needing it on disk).

---

## 6. Explanation Chain Integration

The builder should support Phase 7A.5 explanation chains natively:

- **Finding group selector:** when authoring a template, optionally assign it to a `finding_id` group
- **Priority slider:** set `explanation_priority` within the group (visual, not a raw number — drag to position in the chain)
- **Chain view:** when a `finding_id` is selected, show all templates in that group as a priority-ordered list. The user sees the full explanation chain and where their new template fits.

```
Finding: impressions_declined_significant
  Priority 100: [Context event match]     ← existing
  Priority 200: [Auto budget detection]   ← existing
  Priority 350: [YOUR NEW TEMPLATE]       ← inserting here
  Priority 999: [Bare finding fallback]   ← existing
```

---

## 7. Template Library Features

Beyond authoring, the library panel should support:

- **Enable/disable** per template (toggle, doesn't delete)
- **Duplicate** (copy a template as starting point for a variant)
- **Version history** (Phase 7B ships `narrative_format_version: 2` with `version` field — see template versioning research note)
- **Tags** for organization (beyond family — custom tags like "Q4 review", "client-specific", "experimental")
- **Import/export** (download as YAML, upload YAML, share between workspaces)
- **Search** across template text, binding names, and `when:` conditions

---

## 8. Design Constraints

- **No code required for basic templates.** The pattern builder + flow cards must cover 90%+ of use cases without touching YAML.
- **YAML is always visible** but collapsed by default. Power users can switch to raw mode; the YAML is the source of truth.
- **Mobile not required.** This is a desktop productivity tool.
- **Dark mode default.** Matches the narrative.studio brand from the prototype. Light mode as a toggle.
- **Performance:** live preview must respond in <200ms (same contract as `mc model narrate`). The builder should feel instant.
- **Offline-capable for the builder.** The AI suggestions require network (LLM API call), but the builder itself, pattern compiler, and YAML generation work without network. Save locally, sync later.

---

## 9. Phasing

### Phase 7B.1: Core Builder (no AI)

- Three-panel layout
- Pattern-based WHEN builder (8 patterns)
- THEN SAY text area with auto-complete
- Compiled YAML panel (live updating)
- Library panel (list, load, save, delete)
- `POST /api/preview-template` endpoint + live preview panel
- Explanation chain integration (finding_id + priority)
- Save to `narratives/*.yaml`

### Phase 7B.2: Flow Builder

- Zapier-style card layout for compound conditions
- Condition, Calculation, Lookup Table card types
- AND/OR connectors
- Drag-to-reorder cards
- Compiles to the same YAML as the pattern builder

### Phase 7B.3: AI Co-Pilot

- LNM narrative authoring agent (plugin skill)
- Auto-suggestions based on data structure
- User-directed template creation
- Cube-aware guardrails (measure/dimension validation)
- 3-5 suggestion cards in the bottom bar

### Phase 7B.4: Library Management

- Template versioning (format version 2)
- Enable/disable, duplicate, tags
- Import/export
- Search
- Version history with rollback

---

## 10. Open Questions

1. **Should the AI suggestions persist between sessions?** Or regenerate fresh each time data is loaded? (Lean: regenerate — suggestions should reflect current data, not stale proposals.)

2. **Should the flow builder support explanation chains visually?** E.g., a "chain view" where you see all templates in a `finding_id` group as connected flow cards, with the priority order as the flow direction. (Lean: yes for 7B.2, it's the natural visual representation.)

3. **Should the preview show ALL periods or just the matching ones?** (Lean: show all periods with match/skip indicators — the user needs to understand both why it fires and why it doesn't.)

4. **Should templates authored in the UI be separate from hand-authored YAML?** Or mixed in the same files? (Lean: same files. The UI reads and writes the same `narratives/*.yaml` files. No separate storage. The YAML is the source of truth regardless of how it was authored.)

5. **What does the LNM agent need from the cube to propose good templates?** Minimum: measure names + types, dimension names + element counts, time period count, value ranges (min/max/mean per measure), and sample values for the latest 2 periods. This is the "cube metadata" the agent receives.

---

*End of design requirements. narrative.studio is a professional template authoring environment with visual flow building, live preview against real data, and an AI co-pilot that proposes templates based on observed patterns. The YAML is always the source of truth — the UI is a compiler that produces it.*
