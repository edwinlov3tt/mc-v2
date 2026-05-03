---
description: 'End-to-end natural-language-to-working-YAML pipeline for Mosaic. Pass a description like "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality"; the command runs mosaic-architect → mosaic-author → mosaic-debugger → mosaic-validator until the YAML passes validate / lint / test, then hands the result to the user.'
---

# /mosaic-author — Author a Mosaic model from natural language

Run the full architect → author → debugger → validator pipeline on a natural-language model description. End-to-end: from `"marketing-mix for a 5-channel B2C SaaS"` to a working YAML that passes `mc model validate / lint / test`.

## Arguments

- **`"<description>"`** (required) — a natural-language description of the model. Examples:
  - `"marketing-mix model for a 5-channel B2C SaaS with monthly seasonality"`
  - `"marketing model with 7 cities, 5 channels, 12 months, and a Q4 lift scenario"`
  - `"plan FY27 marketing across paid + owned for the Southeast region"`

## What this command does

The command orchestrates the four-agent pipeline:

### Stage 1 — Design (mosaic-architect)

Invoke **mosaic-architect** with the user's description. The architect produces a structured *plan*:

- Model identity (name, description).
- Dimensions (6 entries in canonical order, with leaf elements + consolidation tiers).
- Hierarchies (Time / Channel / Market default trees).
- Measures (Input vs Derived, aggregation rules, weight measures).
- Rules (target, formula body, declared dependencies).
- Open questions for anything ambiguous.

Render the plan to the user. Wait for confirmation (or ask for revisions). Don't move to Stage 2 with unanswered open questions.

### Stage 2 — Write (mosaic-author)

Once the plan is confirmed, invoke **mosaic-author** with the plan as input. The author writes the YAML to disk and runs `mosaic.model.validate` via MCP.

**If validate is clean:** proceed to Stage 4.
**If validate fires errors:** proceed to Stage 3.

### Stage 3 — Debug (mosaic-debugger)

Invoke **mosaic-debugger** with the YAML path + the diagnostic envelope. The debugger:

- Looks up each MC code in `skills/debugging/SKILL.md`.
- Proposes specific YAML edits (before/after).
- Applies the edits.
- Re-runs `mosaic.model.validate`.

Loop until validate is clean (or until the same code repeats 3+ times — at which point surface to the user; see "Convergence failure" below).

### Stage 4 — Verify (mosaic-validator)

Invoke **mosaic-validator** to run the full validate → lint → test sequence:

- **Validate** — should already be clean from Stage 2/3.
- **Lint** — surface MC3xxx warnings; ask the user whether to fix or document each.
- **Test** — runs goldens. If any fail, the validator hands back to mosaic-debugger or surfaces to the user.

When all three gates pass, render the final report:

```
✓ Mosaic model authored: <path>

  validate: clean
  lint: 0 warnings, 0 info
  test: N/N goldens pass

Run `mc demo --model <path>` to see the full demo flow.
```

## Convergence and iteration cap

The default iteration cap is **5 rounds** through Stage 3. Each round:

1. Read diagnostics.
2. Apply fixes.
3. Re-validate.

If after 5 rounds validate still fires errors, surface to the user:

```
✗ The pipeline didn't converge in 5 rounds. The persistent errors are:
  - MC2002 at /dimensions (still failing after 3 attempts)
  - MC2011 at /measures/4 (added during round 4)

Suggest re-running mosaic-architect to refine the plan; the current plan
appears to have a structural issue the debugger can't resolve through
local edits.
```

## Convergence failure scenarios

- **Same code, same path repeating** → the fix isn't actually fixing. Re-read the message; the registry's fix pattern is generic; the message tells you the specific dim/measure/rule.
- **One fix introduces new errors** → cascading mismatch (e.g., renaming an element broke goldens that referenced the old name). Walk the plan through; surface to user.
- **Goldens fail systematically** → either the rules don't compute what was expected, or the expected values were miscomputed. Don't loosen `epsilon`; investigate.
- **Lint warnings the user wants to keep** → document in YAML comments; surface for explicit confirmation.

## Skills referenced

- All 6 skills cross-reference into this command:
  - `skills/authoring/SKILL.md` — top-level YAML structure (used by mosaic-author).
  - `skills/debugging/SKILL.md` — code registry (used by mosaic-debugger).
  - `skills/schema-design/SKILL.md` — design rules (used by mosaic-architect).
  - `skills/formulas/SKILL.md` — rule body grammar (used by mosaic-author).
  - `skills/testing/SKILL.md` — fixtures and goldens (used by mosaic-author + validator).
  - `skills/domain-schemas/marketing-mix/SKILL.md` — Acme reference (used throughout).

## Anti-patterns (don't)

- **Don't skip the architect stage.** Even for "simple" models, the plan-first approach catches design issues before they become validation errors.
- **Don't override the iteration cap.** If 5 rounds don't converge, the issue isn't number-of-attempts — it's that the plan needs revision.
- **Don't assume natural language is unambiguous.** "5 channels" is unspecified — which 5? Surface as an open question; don't guess.
- **Don't author non-marketing-mix domains in Phase 4A.** If the user describes FP&A, sports betting, prospecting, sales forecasting, or demand planning, redirect to the deferral note in `commands/mosaic-init.md`.

## Worked example

```
user: /mosaic-author "marketing-mix for FY27 with 5 channels and 7 cities"
assistant: I'll run the architect → author → debugger → validator pipeline.

[Stage 1 — Architect]
<renders plan>
- name: MyCo_Marketing_FY27
- 6 dims: Scenario(3), Version(3), Time(monthly FY27 + Q1-Q4 + FY), Channel(5+2+1), Market(7+states+regions+US), Measure(11)
- 11 measures: Spend, CPC, CVR, Close_Rate, AOV, COGS_Rate, Clicks, Leads, Customers, Revenue, Gross_Profit
- 5 rules: clicks, leads, customers, revenue, gross_profit
- Open question: which 5 channels?
user: paid_search, paid_social, display, email, organic
assistant: confirmed; <updates plan>

[Stage 2 — Author]
Writing YAML to MyCo_Marketing_FY27.yaml...
Running mosaic.model.validate... ✓ clean.

[Stage 4 — Validator]
Running mosaic.model.lint... ✓ 0 warnings.
Running mosaic.model.test... ✓ 9/9 pass.

✓ Authoring complete: MyCo_Marketing_FY27.yaml
```
