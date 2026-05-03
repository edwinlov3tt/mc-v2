---
description: "Run `mc model lint` on a Mosaic YAML file via MCP. Surfaces advisory MC3xxx warnings (style, redundancy, naming consistency, ratio-aggregation smells, unused measures). Lint is advisory by default; pass `--deny-warnings` to make warnings fail CI."
---

# /mosaic-lint — Lint a Mosaic YAML model

Run `mosaic.model.lint` via the MCP server and render advisory warnings.

## Arguments

- **`[path]`** (optional) — the YAML file to lint. Defaults to open file or prompts.
- **`--deny-warnings`** (optional flag, passed to underlying `mc-cli`) — makes the run exit non-zero if any warnings fire. Useful for CI.

## What this command does

1. **Resolve the path.**
2. **Invoke `mosaic.model.lint <path>` via MCP** with `--format json`.
3. **Parse the diagnostic envelope.** All MC3xxx codes have severity `warning` except MC3010 which is `info`.
4. **Render each warning** with code, path, message, and the registry's fix pattern.
5. **Prompt the user** for each warning: fix or document?

## Lint codes (the MC3xxx series)

The full registry is in `skills/debugging/SKILL.md`. Highlights:

- **MC3001** — inconsistent element naming style within a dim. Pick `Title_Case_With_Underscores` (Acme convention) and stick with it.
- **MC3002** — duplicate description text across measures. Make each description specific.
- **MC3003** — empty / whitespace-only description. Write a one-sentence description.
- **MC3004** — rule body that's a no-op. Either delete the rule or compute something.
- **MC3005** — declared_dependencies mismatch the body's references.
- **MC3006** — measure with `aggregation: Sum` whose name suggests a ratio. Switch to `WeightedAverage`.
- **MC3007** — hierarchy edge with `weight: 0.0`. Remove or set non-zero.
- **MC3008** — **PERMANENTLY RETIRED.** Promoted to MC2011 (validation) in Phase 3B. Never reintroduce.
- **MC3009** — Input measure never written (no canonical_inputs / fixtures / writebacks reference it).
- **MC3010** — Derived measure never read (severity `info`, not `warning`). Either add a golden or accept as documentation.
- **MC3011** — golden references a coord whose expected value is unreachable from the inputs. Recheck rule chain.

## Acme reference: zero warnings

The canonical Acme YAML (`examples/models/acme-marketing.yaml`) lints at **zero warnings, zero info**. New models should aim for the same — every warning is a real authoring smell.

If you find a warning you genuinely want to keep (rare): add a YAML comment near the affected section explaining the choice. The lint output stays non-zero, but the comment shows it's deliberate.

## Output format

Clean case:

```
✓ lint clean — acme.yaml has no warnings or info diagnostics.
```

Dirty case:

```
⚠ lint produced 3 warnings + 1 info.

1. [warning] MC3003 at /measures/4
   measure "Close_Rate" has no description.
   fix: add `description: "Lead-to-customer close rate (customers/lead)."` or similar.

2. [warning] MC3006 at /measures/7
   measure "Customer_Conversion_Rate" with aggregation Sum looks like a ratio.
   fix: change to `aggregation: WeightedAverage` with an appropriate `weight_measure:`.

3. [warning] MC3001 at /dimensions/3/elements/2
   element naming inconsistent ("organic" lowercase among Title_Case_With_Underscores siblings).
   fix: rename to "Organic" (matching siblings).

4. [info] MC3010 at /measures/9
   Derived measure "Net_Margin" is never read by any golden or downstream rule.
   fix: add a golden testing this measure, or delete the measure if it's truly unused.

Run /mosaic-debugger to walk through fixes interactively, or document the choices in YAML comments.
```

## Skills referenced

- `skills/debugging/SKILL.md` — the full MC3xxx code registry + fix patterns.
- `skills/schema-design/SKILL.md` — aggregation rules (MC3006 fix often points here).

## Underlying CLI

```
mc model lint <path> [--format text|json] [--deny-warnings]
```

The MCP server (`mc mcp`) wraps this as `mosaic.model.lint`. The diagnostic envelope shape matches `mc model validate`'s — Phase 3B `{schema_version: "1.0", diagnostics: [...]}`.

## What this command does NOT do

- **Does not modify YAML** — that's mosaic-debugger.
- **Does not run validate or test** — use the dedicated commands.
- **Does not block CI by default.** Pass `--deny-warnings` to make the CLI exit non-zero on any warning.
