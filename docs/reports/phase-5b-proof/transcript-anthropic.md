# Phase 5B proof — Anthropic adapter (best-of-3, `--mode propose-recipe`)

**Date:** 2026-05-04
**Adapter:** `mosaic-plugin/examples/adapters/anthropic-python/author.py`
**Model:** `claude-opus-4-7` (per `MODEL` constant in `author.py`)
**Provider SDK:** `anthropic>=0.40` (per adapter `pyproject.toml`)
**Plugin root:** `mosaic-plugin/`
**System-prompt size:** 102,504 chars (loaded from `skills/import/`, `skills/domain-schemas/`, `agents/mosaic-importer.md`, `commands/mosaic-import.md`, plus 5 valid worked recipe examples from `crates/mc-recipe/examples/recipes/`).
**Validation path:** **structural** (`mc tessera dry-run` not on PATH — Stream D's CLI verbs are still in-flight per ADR-0010 Decision 9; the adapter probed `mc tessera dry-run --help` at runtime and detected the absence, falling back to the documented self-validation path per Phase 5B handoff §6.).

## Canonical acceptance prompt

```
import monthly spend and CPC data from a SQLite database of HubSpot campaign
metrics into the Acme marketing-mix model
```

## Per-run results

| Run | Outcome | Iterations | Errors | Output file |
|---|---|---|---|---|
| 1 | **converged** | 1/5 | 0 | `runs/anthropic-1.recipe.yaml` |
| 2 | **converged** | 1/5 | 0 | `runs/anthropic-2.recipe.yaml` |
| 3 | **converged** | 1/5 | 0 | `runs/anthropic-3.recipe.yaml` |

**Pass rate:** 3 / 3 (gate: ≥ 2 / 3). Headline gate passed by maximum margin.

## Run 1 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling claude-opus-4-7 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

## Run 1 — emitted recipe

```yaml
version: 1
name: hubspot_monthly_spend_cpc
description: "Monthly Spend + CPC import from HubSpot SQLite database into Acme."
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

## Run 2 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling claude-opus-4-7 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

(Run 2's emitted recipe is byte-identical to Run 1 except the `batch:` block uses multi-line form; substance is identical.)

## Run 3 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling claude-opus-4-7 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

(Run 3's emitted recipe is byte-identical to Run 1.)

## Substance audit

For every run, the recipe:

1. ✓ `version: 1` (Phase 5A pin — MC5012 satisfied).
2. ✓ Required top-level fields present (`version`, `name`, `model`, `source`, `columns` — MC5007 satisfied).
3. ✓ `source.driver: sqlite` is a Phase 5A-supported driver (MC5002 satisfied).
4. ✓ `query:` set, `table:` not set (MC5003 satisfied).
5. ✓ Maps Input measures only (`Spend`, `CPC`) — does NOT map any Acme Derived (`Clicks`, `Leads`, `Customers`, `Revenue`, `Gross_Profit`). **MC5018 satisfied — semantic Rule 3 (Input-only) is the highest-stakes rule for the LLM authoring surface, and it lands clean across all 3 runs.**
6. ✓ No dimension appears in both `columns:` and `defaults:` (`Scenario` and `Version` are constant via `defaults:`; `Time`, `Channel`, `Market` vary per row via `columns:` — MC5016 satisfied — Rule 2).
7. ✓ Every `columns[i]` has exactly one of `dimension:` / `measure:` set (MC5011 satisfied — Rule 1).
8. ✓ Phase 5A defaults respected: `write_disposition: replace`, `incremental: false`, `on_missing_element: error`.
9. ✓ Wide format (no `format: long` — long-format is 5A.1-pending; the LLM correctly defaulted to wide).
10. ✓ `model:` path is workspace-relative (does not escape via `..`-chain — MC5017 satisfied — Rule 5).

## Notes on the structural-validation path

The adapter's structural validator (`structural_validate_recipe()` in `author.py`) checks:

- Tab indentation (MC5001 — would produce a YAML parse error in `mc-recipe`).
- Required top-level keys (`version`, `name`, `model`, `source`, `columns` — MC5007).
- `version: 1` pin (MC5012).
- `source.driver` is one of the 6 known drivers (MC5002).
- `query:` / `table:` mutual exclusion (MC5003).
- No mapping to Acme's hardcoded Derived-measure set (MC5018).
- No `format: long` (5A.1-pending — would produce MC5001 in current schema).

What it does **not** check (per Phase 5B handoff "documented limitation" §6):

- MC5004 / MC5005 (unknown dim / measure name) — requires a live model load, which `mc tessera dry-run` provides but the structural fallback cannot.
- MC5016 (mutual exclusion between `columns:` and `defaults:`) — requires indentation-aware YAML parsing that the regex fallback doesn't do.
- MC5009 (unknown element value in `defaults:`) — requires model load.
- MC5013 / MC5014 / MC5015 — runtime-stage codes; only fire at `mc tessera apply` time.

These gaps close when Stream D's `mc tessera dry-run --format json` ships and the adapter's runtime probe selects the machine path. The `propose_recipe()` flow is identical either way — only the `errs = …` line in the iteration loop differs.

## Files in this proof bundle

```
docs/reports/phase-5b-proof/
├── transcript-anthropic.md              (this file)
├── transcript-openai.md
├── output-anthropic.recipe.yaml         (= runs/anthropic-1.recipe.yaml)
├── output-openai.recipe.yaml            (= runs/openai-1.recipe.yaml)
└── runs/
    ├── anthropic-1.{recipe.yaml,stdout.txt,stderr.txt}
    ├── anthropic-2.{recipe.yaml,stdout.txt,stderr.txt}
    ├── anthropic-3.{recipe.yaml,stdout.txt,stderr.txt}
    ├── openai-1.{recipe.yaml,stdout.txt,stderr.txt}
    ├── openai-2.{recipe.yaml,stdout.txt,stderr.txt}
    └── openai-3.{recipe.yaml,stdout.txt,stderr.txt}
```
