# Phase 5B proof — OpenAI adapter (best-of-3, `--mode propose-recipe`)

**Date:** 2026-05-04
**Adapter:** `mosaic-plugin/examples/adapters/openai-python/author.py`
**Model:** `gpt-5.5` (per `MODEL` constant in `author.py`; the SDK's `responses` API was used per ADR-0008 amendment G).
**Provider SDK:** `openai>=1.50` (per adapter `pyproject.toml`)
**Plugin root:** `mosaic-plugin/`
**System-prompt size:** 102,504 chars (same import-focused content set as the Anthropic adapter — see `transcript-anthropic.md` for the full content manifest).
**Validation path:** **structural** (`mc tessera dry-run` not on PATH — same fallback as the Anthropic run; see `transcript-anthropic.md` for the full discussion).

## Canonical acceptance prompt

```
import monthly spend and CPC data from a SQLite database of HubSpot campaign
metrics into the Acme marketing-mix model
```

## Per-run results

| Run | Outcome | Iterations | Errors | Output file |
|---|---|---|---|---|
| 1 | **converged** | 1/5 | 0 | `runs/openai-1.recipe.yaml` |
| 2 | **converged** | 1/5 | 0 | `runs/openai-2.recipe.yaml` |
| 3 | **converged** | 1/5 | 0 | `runs/openai-3.recipe.yaml` |

**Pass rate:** 3 / 3 (gate: ≥ 2 / 3). Headline gate passed by maximum margin.

## Run 1 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling gpt-5.5 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

## Run 1 — emitted recipe

```yaml
version: 1
name: hubspot_monthly_spend_cpc
description: "Monthly Spend + CPC import from HubSpot SQLite campaign metrics into Acme."
model: ../models/acme.yaml

source:
  driver: sqlite
  path: ./hubspot.sqlite
  query: |
    SELECT month, channel, market, spend, cpc
    FROM campaign_metrics
    WHERE scenario = 'Baseline'
      AND version = 'Working'

columns:
  - { source: month, dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market, dimension: Market }
  - { source: spend, measure: Spend, type: f64 }
  - { source: cpc, measure: CPC, type: f64 }

defaults:
  Scenario: Baseline
  Version: Working

write_disposition: replace
incremental: false

batch:
  size: 50000

on_error: abort
on_missing_element: error

credentials: {}
```

## Run 2 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling gpt-5.5 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

(Run 2's emitted recipe is substantively identical to Run 1 — same driver, same dim/measure mappings, same defaults; minor whitespace differences only.)

## Run 3 — stderr log

```
[mosaic] plugin root: /Users/edwinlovettiii/Projects/mc-v2/mosaic-plugin
[mosaic] system prompt: 102,504 chars
[mosaic] validation path: structural
[mosaic] calling gpt-5.5 (initial recipe draft)...
[mosaic][iter 1] recipe converged (structural)
```

(Run 3's emitted recipe is substantively identical to Run 1.)

## Substance audit

The audit list is identical to the Anthropic transcript — every OpenAI run satisfies the same structural and semantic checks:

1. ✓ `version: 1` — MC5012.
2. ✓ Required fields present — MC5007.
3. ✓ `driver: sqlite` — MC5002.
4. ✓ `query:` set, `table:` absent — MC5003.
5. ✓ Maps `Spend` + `CPC` only (Inputs); no Derived measures — **MC5018 / Rule 3 satisfied**.
6. ✓ No dimension in both `columns:` and `defaults:` — MC5016 / Rule 2.
7. ✓ Each `columns[i]` has exactly one target — MC5011 / Rule 1.
8. ✓ Phase 5A defaults respected.
9. ✓ Wide format (no `format: long`).
10. ✓ Workspace-relative `model:` path — MC5017 / Rule 5.

The OpenAI runs additionally include `AND version = 'Working'` in the SQL `WHERE` clause (more restrictive scoping than the Anthropic runs). Both shapes are correct; the recipe layer doesn't validate query semantics.

## Cross-provider observation

Both providers converged on the **first iteration** for all 3 runs (no retries). This indicates the skill content + worked recipe examples in the system prompt give the LLM enough information to produce a structurally valid recipe on the first attempt for the canonical acceptance prompt. The 5-round iteration cap was not exercised in this gate; future test prompts that exercise more pathological cases (e.g., user describes a Derived measure as the target) would be needed to validate the iteration loop.

## Cross-reference

See `transcript-anthropic.md` for:
- The full content manifest of the import system prompt.
- The list of structural-validator checks (and the documented gaps).
- The proof bundle file layout.
