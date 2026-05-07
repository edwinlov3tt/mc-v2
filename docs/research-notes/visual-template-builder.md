# Visual Template Builder — Pattern-Picker Design

**Status:** `research note — not adopted; design reference for Phase 7B`
**Created:** 2026-05-07
**Last touched:** 2026-05-07
**Target phase:** Phase 7B (visual template editor) or earlier as a demo UI amendment (~3-4 hours)

---

## The insight

Users don't need to learn formula syntax. Every narrative template's `when:` predicate falls into one of ~8 natural-language patterns. A dropdown-based UI that maps plain-English selections to formula expressions eliminates the need for users to understand `abs()`, `>=`, parentheses, or operator precedence.

## The 8 patterns (covers all 14 shipped templates)

| # | Pattern name | User sees | Generates |
|---|---|---|---|
| 1 | Simple comparison | "When **[Clicks]** is **[greater than]** **[1000]**" | `current.Clicks > 1000` |
| 2 | Period change | "When **[Impressions]** **[increased]** by **[more than]** **[20]** percent" | `(current.Impressions - prev.Impressions) / prev.Impressions * 100 > 20` |
| 3 | Dimension extreme | "When the **[lowest]** **[CTR]** across **[Device]** is **[less than]** **[0.1]**" | `min_over(CTR, Device) < 0.1` |
| 4 | Relative to average | "When **[lowest Device CTR]** is **[less than]** **[25]** percent of **[campaign average]**" | `min_over(CTR, Device) < campaign_avg.CTR * 0.25` |
| 5 | Zero check | "When **[Conversions]** is **[zero]** across **[all periods]**" | `sum.Conversions == 0` |
| 6 | Threshold + volume | "When **[Clicks]** is **[zero]** and **[Impressions]** is **[greater than]** **[50]**" | `Clicks == 0 AND Impressions > 50` |
| 7 | Concentration | "When the **[top]** **[City]** accounts for **[more than]** **[70]** percent of **[Impressions]**" | `max_over(Impressions, City) / sum.Impressions > 0.70` |
| 8 | Data sufficiency | "When there are **[at least]** **[2]** reporting periods" | `period_count >= 2` |

Bold items in brackets are user-selectable (dropdown or number input). Everything else is static label text.

## Plain-English operator labels

| Formula syntax | User sees |
|---|---|
| `>` | "greater than" |
| `>=` | "at least" |
| `<` | "less than" |
| `<=` | "at most" |
| `==` | "is equal to" |
| `!=` | "is not equal to" |
| `AND` | "and" |
| `OR` | "or" |
| `*` | "multiplied by" |
| `/` | "divided by" |
| `abs()` | (hidden — the system applies it when the pattern requires it, e.g., "changed by more than X percent" always uses absolute value) |

## Architecture

```
[User fills in form with dropdowns + text inputs]
         ↓
[Form state: { pattern: "period_change", metric: "Clicks",
               direction: "increased", threshold: 20, unit: "percent" }]
         ↓
[Pattern compiler: maps form → YAML when: + template: + bindings:]
         ↓
[Generated YAML saved to narratives/ directory]
         ↓
[mc-narrative evaluates it like any other template — no special handling]
```

The pattern compiler is a pure function: `PatternConfig → YAML string`. Lives in the frontend. The backend never sees the patterns — it only sees the generated YAML, which is the same shape as hand-authored templates.

## Dropdown population

Dropdowns are populated from the cube's metadata (returned by the API):

- **Metric dropdown:** populated from cube measures (Impressions, Clicks, CTR, Conversions, Spend, CPM, etc.)
- **Dimension dropdown:** populated from cube dimensions (Device, City, Creative, Time, etc.)
- **Aggregation dropdown:** "highest" → `max_over`, "lowest" → `min_over`, "average" → `avg_over`, "total" → `sum_over`
- **Comparison dropdown:** "greater than" → `>`, "less than" → `<`, "at least" → `>=`, "at most" → `<=`, "is equal to" → `==`
- **Direction dropdown:** "increased" → positive delta, "decreased" → negative delta, "changed" → absolute delta

## The parenthesis problem

Eliminated entirely. Users never write free-form expressions. They choose a pattern and fill in values. The pattern compiler handles:

- Operator precedence (multiplication before addition)
- Parentheses (wrapping subexpressions correctly)
- `abs()` calls (applied automatically for "changed by" patterns)
- Division-by-zero guards (period-change patterns check `prev > 0`)

## "Then say" template authoring

After the `when:` condition, the user writes the narrative sentence. The UI provides:

- A text area with `{placeholder}` auto-suggestions (based on the bindings the pattern generates)
- A "severity" dropdown (Info / Success / Warning / Critical)
- A "format hint" picker per placeholder (Currency, Percent, Count, etc.)

Example flow:
1. User picks pattern "Period change"
2. Fills in: metric = Clicks, direction = increased, threshold = 50, unit = percent
3. System generates bindings: `click_pct`, `prev_clicks`, `current_clicks`, `prev_period`, `current_period`
4. User types: "Clicks {direction} {click_pct}% from {prev_period} ({prev_clicks}) to {current_period} ({current_clicks})."
5. Picks severity: Info
6. Clicks "Add Template"
7. YAML appended; next upload fires the template

## Quick-win demo version (~3-4 hours)

For the next leadership demo touchpoint, a minimal version:

- 3 pattern types only (Period change, Zero check, Dimension extreme)
- No live preview (user clicks "Add Template" → restarts → re-uploads to see)
- Dropdowns hardcoded to the Scotts RV sample data's measures/dims
- "Generated YAML" shown at the bottom (collapsed by default; shown to prove it's real)

This proves: "A non-technical user created an analytical rule in 30 seconds and it fires deterministically forever."

## Full Phase 7B version

The complete visual template editor adds:

- All 8 patterns
- Live preview against current cube state (evaluate template as user builds it)
- Template versioning (edit history with rollback)
- Template library browser (see all templates, enable/disable, reorder)
- Template sharing within an organization (export/import)
- Template marketplace (Phase 7B.2 — long-term)
- Custom pattern authoring for power users (escape hatch to raw YAML)

Depends on Phase 6B (web UI) being stable enough to host the editor.

## Cross-links

- **Phase 7B in MASTER_PHASE_PLAN.md:** planned, depends on 6B web UI
- **ADR-0020 planning doc §"Phase 7B":** scope outline for the visual editor
- **demo/narratives/display-like.yaml:** the YAML the builder generates
- **mc-narrative evaluator:** the engine that runs the generated YAML (doesn't need changes for the builder)
