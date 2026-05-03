---
description: "Run `mc model validate` on a Mosaic YAML file via MCP. Surfaces parse errors (MC1xxx) + structural validation errors (MC2xxx) + fixture/CSV errors (MC2012-MC2025). Pass the YAML path as the argument; defaults to the current file if open."
---

# /mosaic-validate — Validate a Mosaic YAML model

Run `mosaic.model.validate` via the MCP server, then render the result.

## Arguments

- **`[path]`** (optional) — the YAML file to validate. If omitted, use the path of the currently-open file (or ask the user).

## What this command does

1. **Resolve the path.** Prefer the argument; fall back to the open file; prompt if neither.
2. **Invoke `mosaic.model.validate <path>` via MCP** with `--format json` so the diagnostic envelope is structured.
3. **Parse the response.**
   - If `diagnostics: []` → render `✓ validate clean — no parse, structural, or fixture errors.`
   - If non-empty → render each diagnostic with code, severity, path, message, and the registry's fix pattern (look up via `skills/debugging/SKILL.md`).
4. **If errors fired:** offer to invoke mosaic-debugger to apply fixes.

## Output format

Clean case:

```
✓ validate clean — acme.yaml passed parse + structural + fixture/CSV validation.
```

Dirty case:

```
✗ validate failed — 2 errors.

1. MC2002 at /dimensions (yaml acme.yaml line 38)
   message: dim list is wrong; expected [Scenario, Version, Time, Channel, Market, Measure]
   fix: ensure all six dims are declared in canonical order.

2. MC2011 at /measures/5 (yaml acme.yaml line 183)
   message: measure "CPC": aggregation WeightedAverage requires weight_measure
   fix: add `weight_measure: "Spend"` to the CPC measure.

Run /mosaic-debugger to apply fixes interactively.
```

## What this command does NOT do

- **Does not run lint** — use `/mosaic-lint`.
- **Does not run goldens** — use `/mosaic-test`.
- **Does not edit YAML** — use `/mosaic-debugger` (or any agent in the architect → author → debugger → validator pipeline).

## Skills referenced

- `skills/debugging/SKILL.md` — full code registry. Cross-reference each MC code in the diagnostic envelope.
- `skills/authoring/SKILL.md` — the four-stage pipeline (parse → validate → resolve_inputs → compile). `mc model validate` runs the first three stages.

## Underlying CLI

```
mc model validate <path> --format json
```

The MCP server (`mc mcp`) wraps this as the `mosaic.model.validate` tool. The diagnostic envelope shape is the Phase 3B `{ schema_version: "1.0", diagnostics: [...] }` contract.
