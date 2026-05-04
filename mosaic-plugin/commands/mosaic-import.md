---
description: 'Translate a natural-language data-source description into a Mosaic Tessera recipe (`*.recipe.yaml`). Pass a description like "import monthly spend and CPC from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"; the command runs the mosaic-importer agent, produces a recipe, validates it (machine via `mc tessera dry-run` if available; structural self-check otherwise), iterates on MC5xxx errors, and hands the converged recipe to the user.'
---

# /mosaic-import — Author a Mosaic Tessera recipe from natural language

Run the importer pipeline on a natural-language data-source description. End-to-end: from `"import monthly spend and CPC from SQLite into Acme"` to a working recipe YAML that passes recipe validation.

## Arguments

- **`"<description>"`** (required) — a natural-language description of the data source, the target Mosaic model, and which measures to import. Examples:
  - `"import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"`
  - `"load Q3 actuals from this CSV into the Aggressive scenario of acme.yaml"`
  - `"pull Conservative-scenario monthly metrics from our REST endpoint into Acme"`
  - `"federate Postgres data through DuckDB into the marketing-mix cube"`

## What this command does

The command invokes a single agent — **mosaic-importer** — and orchestrates its iterate-and-validate loop.

### Stage 1 — Identify driver + target model

The mosaic-importer reads the description and identifies:

- **Source shape** → driver (`csv` / `sqlite` / `duckdb` / `postgres` / `duckdb_postgres` / `http_json`).
- **Target model** → the Mosaic YAML the recipe imports into (typically `acme.yaml`).
- **Measures** → which Inputs to map (Derived measures are off-limits — Rule 3 / MC5018).
- **Per-row vs constant dimensions** → drives `columns:` vs `defaults:`.
- **Credential needs** → DSN, API token, etc., always via `${env.VAR}` interpolation.

If the description is ambiguous (e.g., "import the data" with no source shape), the importer asks one clarifying question before drafting. Don't guess.

### Stage 2 — Draft the recipe

The mosaic-importer emits a complete recipe YAML in a single fenced ```yaml ... ``` block, conforming to:

- The `mc-recipe::Recipe` schema (`crates/mc-recipe/src/schema.rs`).
- All six semantic rules from ADR-0010 Decision 7.
- Phase 5A defaults: `version: 1`, `write_disposition: replace`, `incremental: false`, `on_missing_element: error`.

Worked references at `crates/mc-recipe/examples/recipes/` — the importer copies the closest example and adjusts.

### Stage 3 — Validate

Two paths, in priority order:

1. **Machine validation (preferred)** — if `mc tessera dry-run` is available on PATH, run `mc tessera dry-run <recipe.yaml> --format json`. Parse the diagnostic envelope (`{schema_version: "1.0", diagnostics: [{code, severity, path, message}, ...]}`).

2. **Structural self-validation (fallback)** — if `mc tessera dry-run` isn't available (Phase 5B-pre-Stream-D-merge state), the importer walks the recipe through the schema and the six semantic rules manually. Confirms:
   - YAML parses cleanly.
   - Required top-level fields present (`version`, `name`, `model`, `source`, `columns`).
   - `source.driver` is one of the 6 known drivers.
   - No `columns[i].measure` references a known Derived measure for the target model.
   - No dimension appears in BOTH `columns:` and `defaults:`.
   - No `columns[i]` has both `dimension:` and `measure:` set.

The importer reports which validation path was used.

### Stage 4 — Iterate

For each MC5xxx diagnostic, the importer:

1. Looks up the code in `skills/import/recipe-format/SKILL.md`.
2. Applies the fix pattern (e.g., MC5018 → swap Derived measure for its Inputs; MC5016 → remove dimension from `defaults:` or `columns:`).
3. Re-emits the corrected recipe.
4. Re-validates.

Loop until validation is clean, or until 5 rounds have elapsed. If the same code repeats 3+ times, surface to the user — the design is wrong, not the recipe.

### Stage 5 — Present the result

When validation is clean, render:

```
✓ Recipe authored: <path>

  driver:   sqlite
  model:    ../models/acme.yaml
  measures: Spend, CPC (Inputs)
  defaults: Scenario=Baseline, Version=Working
  validation: <machine via mc tessera dry-run | self-validation>

Next: review the recipe, adjust `path:` / `query:` to match your actual database, and run:
  mc tessera apply <path>
```

## Convergence and iteration cap

Default cap: **5 rounds**. Each round is a validate → diagnose → re-emit cycle.

If after 5 rounds validation still fires errors, surface to the user:

```
✗ The importer didn't converge in 5 rounds. The persistent errors are:
  - MC5018 at /columns/4/measure (still failing after 3 attempts)

The user's description likely targets a Derived measure (the kernel computes
this; recipes can't write to it). Recommend adjusting the description to
import the inputs that drive the target measure instead.
```

## Convergence failure scenarios

- **MC5018 keeps firing** → the user wants to import a Derived measure. Recommend mapping the Inputs that drive it (for Acme: `Clicks` is `Spend / CPC` — map `Spend` + `CPC` instead).
- **MC5004 / MC5005 (unknown dim/measure)** → the target model doesn't have the field the user described. Either the description names something the model doesn't carry, or the target model needs a different model. Ask which.
- **MC5016 (mutual exclusion)** keeps reappearing → the user's description is ambiguous about whether a dimension varies per row or is constant. Ask explicitly.
- **MC1xxx-MC3xxx surface** → the target model itself is malformed. Hand off to mosaic-debugger; the recipe is fine.

## Validation path note (Phase 5B-current-state)

At Phase 5B execution time, `mc tessera dry-run` may or may not exist on PATH:

- If `mc-tessera` (Stream D's deliverable) has merged and the CLI is rebuilt, `mc tessera dry-run --format json` is the machine-validation path.
- If Stream D is still in-flight or the CLI hasn't been rebuilt with tessera verbs, the structural self-validation path applies.

The mosaic-importer probes for the verb and picks the available path automatically. The recipe emitted is identical either way; only the validation rigor differs. With self-validation the recipe is "structurally plausible per the schema" — the LLM can't verify dim/measure names against a live model load. The user should review before running `mc tessera apply`.

## Skills referenced

- **`skills/import/recipe-format/SKILL.md`** — the canonical recipe schema + 18 MC5xxx codes + 6 semantic rules.
- **`skills/import/csv-mapping/SKILL.md`** — CSV-shaped patterns (wide vs long format, type coercion, scale, skip).
- **`skills/import/sql-mapping/SKILL.md`** — SQL-family drivers (SQLite / DuckDB / Postgres / DuckDB-attached-Postgres).
- **`skills/import/api-mapping/SKILL.md`** — HTTP/JSON driver (`url:`, `json_path:`, auth via env interpolation).
- **`skills/domain-schemas/marketing-mix/SKILL.md`** — Acme's dim/measure namespace.

## Anti-patterns (don't)

- **Don't run `mc tessera apply` from this command.** This command produces a recipe; running it is a separate, explicit user action. `apply` actually mutates state — that's an explicit-confirmation operation per CLAUDE.md.
- **Don't override the iteration cap.** If 5 rounds don't converge, the issue is design, not iteration count.
- **Don't author the target model.** That's `/mosaic-author`'s job. Recipes target an existing model.
- **Don't write recipes against a model that doesn't exist.** Verify the model path before drafting; if it's missing, surface and redirect to `/mosaic-author`.
- **Don't propose long-format recipes silently.** The `format:` / `long_format:` fields are 5A.1-pending; today they fire MC5001. Default to wide format and mention 5A.1 if long is genuinely needed.
- **Don't import Derived measures.** Phase 5A is Inputs-only.
- **Don't inline credentials.** Always use `${env.VAR}` interpolation.
- **Don't fabricate driver names.** The six drivers are exhaustive.

## Worked example

```
user: /mosaic-import "import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"
assistant: I'll run the mosaic-importer agent to draft a SQLite recipe.

[mosaic-importer]
Driver: sqlite (file-based source).
Target model: Acme (../models/acme.yaml).
Measures: Spend, CPC (both Inputs in Acme).
Per-row dimensions: Time, Channel, Market.
Constant dimensions: Scenario=Baseline, Version=Working.

Drafting recipe...

```yaml
version: 1
name: hubspot_monthly_spend_cpc
description: "Monthly Spend + CPC import from HubSpot SQLite database."
model: ../models/acme.yaml

source:
  driver: sqlite
  path: ./hubspot.sqlite
  query: |
    SELECT month, channel, market, spend, cpc
    FROM campaign_metrics
    WHERE scenario = 'Baseline'

columns:
  - { source: month,   dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market,  dimension: Market }
  - { source: spend,   measure: Spend, type: f64 }
  - { source: cpc,     measure: CPC,   type: f64 }

defaults:
  Scenario: Baseline
  Version: Working

write_disposition: replace
incremental: false
batch: { size: 50000 }
on_error: abort
on_missing_element: error
credentials: {}
```

Validating (self-validation; mc tessera dry-run not available)... ✓ structurally valid.
- YAML parses cleanly.
- All required fields present.
- driver `sqlite` is supported.
- Spend and CPC are Inputs in Acme (Rule 3 satisfied).
- No dimension appears in both `columns:` and `defaults:` (Rule 2 satisfied).
- All `columns[i]` have exactly one of dimension/measure set (Rule 1 satisfied).

✓ Recipe authored: hubspot_monthly_spend_cpc.recipe.yaml

  driver:   sqlite
  model:    ../models/acme.yaml
  measures: Spend, CPC (Inputs)
  defaults: Scenario=Baseline, Version=Working
  validation: self-validation

Next: review the recipe, adjust `path:` to your actual SQLite file and the
`query:` to match your table schema, then run:
  mc tessera apply hubspot_monthly_spend_cpc.recipe.yaml
```
