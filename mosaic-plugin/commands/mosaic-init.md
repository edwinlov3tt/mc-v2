---
description: "Scaffold a new Mosaic YAML model. Phase 4A supports `marketing-mix` only — pass `marketing-mix` (or omit; it's the default) as the domain."
---

# /mosaic-init — Scaffold a Mosaic model

Drop a starter Mosaic YAML at the user's chosen path, configured for the requested domain. Phase 4A supports **only the marketing-mix domain** — Acme is the canonical reference (per ADR-0008 amendment F).

## Arguments

- **`<domain>`** (optional, default `marketing-mix`) — the domain schema to use. Phase 4A only accepts `marketing-mix`. Any other value (`fp&a`, `sports-betting`, `prospect-scoring`, `sales-forecasting`, `demand-planning`) should be rejected with a note that those are demand-driven future phases.

## What this command does

1. **Determine the target path.** If the user specified one, use it. Otherwise default to `<workdir>/<model_name>.yaml` after asking the user what to name the model.

2. **Refuse non-marketing-mix domains.** If the user passed something other than `marketing-mix`:
   ```
   Phase 4A ships only the marketing-mix domain (per ADR-0008 amendment F).
   The other domains (FP&A, sports-betting, prospect-scoring, sales-forecasting,
   demand-planning) are demand-driven future phases. To author a marketing-mix
   model, run `/mosaic-init marketing-mix` (or just `/mosaic-init`).
   ```

3. **Read the canonical reference.** The plugin's `examples/models/acme-marketing.yaml` is the marketing-mix template. Don't copy it byte-for-byte (that's the Acme model itself); use it as the structural reference for the new model.

4. **Ask the user for context** to fill in the placeholders:
   - Model `name` (e.g., `MyCo_Marketing_FY27`).
   - One-line `description`.
   - Time grain (months / weeks / quarters) and horizon.
   - Channel mix (which channels matter to the user).
   - Market mix (geography or account-based).
   - Whether to seed with a small canonical_inputs fixture or leave it empty for the user to populate.

5. **Hand off to mosaic-architect** with the user's responses to draft a plan, then mosaic-author to write the YAML. The architect → author → debugger → validator pipeline is the canonical authoring loop.

## Skills referenced

The user may want context from these skills as the loop runs:

- `skills/authoring/SKILL.md` — top-level YAML structure.
- `skills/schema-design/SKILL.md` — dim order, measure roles, aggregation rules.
- `skills/formulas/SKILL.md` — Phase 3D formula syntax for rule bodies.
- `skills/testing/SKILL.md` — canonical_inputs + goldens.
- `skills/domain-schemas/marketing-mix/SKILL.md` — Acme as the canonical reference.

## Worked example

```
user: /mosaic-init
assistant: I'll scaffold a marketing-mix model. A few quick questions:
  - What should the model be named? (e.g., MyCompany_Marketing_FY27)
  - One-line description?
  - Time grain — months or weeks? Horizon — FY26 or FY27?
  - Which channels matter? Default: Paid_Search, Paid_Social, Display, Email, Organic.
  - Which markets? Default: 7 cities (Tampa, Orlando, Miami, Atlanta, Charlotte, NYC, Boston).
  - Seed with Acme-shaped sample data or leave canonical_inputs empty?
user: <answers>
assistant: <invokes mosaic-architect with the answers>
```

After the architect produces a plan and the user confirms, mosaic-author writes the YAML and the validator gates it.

## Anti-patterns (don't)

- **Don't accept `<domain>` values other than `marketing-mix`.** Surface the deferral and stop.
- **Don't write the YAML directly from this command.** Hand off to architect → author. The pipeline is the canonical path.
- **Don't seed canonical_inputs with random numbers.** If the user wants seed data, use Acme's deterministic formula (`Spend = 10_000 + 500*time_idx`, etc. — see `skills/domain-schemas/marketing-mix/SKILL.md`); otherwise leave the canonical_inputs block out and let the user populate it.
