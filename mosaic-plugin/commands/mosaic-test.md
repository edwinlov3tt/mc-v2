---
description: "Run `mc model test` on a Mosaic YAML file via MCP. Loads canonical_inputs, applies any fixtures, runs golden_tests, and reports pass/fail/error per golden. Supports `--fixture <name>` to filter to a subset."
---

# /mosaic-test — Run goldens on a Mosaic YAML model

Run `mosaic.model.test` via the MCP server. The full pipeline runs: parse → validate → resolve_inputs → compile → apply canonical_inputs → run goldens.

## Arguments

- **`[path]`** (optional) — YAML file. Defaults to open file or prompts.
- **`--fixture <name>`** (optional) — filter to goldens whose `fixture:` field equals `<name>`. Filter-only semantic; doesn't inject the fixture.

## What this command does

1. **Resolve the path.**
2. **Invoke `mosaic.model.test <path>` via MCP** with `--format json` (and `--fixture` if specified).
3. **Parse the result envelope:** `{ schema_version, skipped, goldens: [...] }`.
4. **Render the per-golden status:** Pass / Fail / Error, with expected/actual/delta/epsilon when applicable.
5. **Summary line:** total goldens, passed, failed, errored, skipped.

## Output format

Clean case:

```
✓ test passed — 9/9 goldens pass.

PASS spend_input_anchor_mar_paid_search_tampa (expected 11500.0, actual 11500.0)
PASS cpc_input_anchor_mar_paid_search_tampa (expected 1.5, actual 1.5)
PASS aov_input_anchor_mar_paid_search_tampa (expected 200.0, actual 200.0)
PASS clicks_derived_anchor_mar_paid_search_tampa (expected 7666.667, actual 7666.667, Δ 0.0e0, ε 1.0e-9)
... (5 more passes)

Goldens: 9/9 passed, 0 failed
```

Failing case:

```
✗ test failed — 7/9 goldens pass, 2 failed.

PASS spend_input_anchor_mar_paid_search_tampa (expected 11500.0, actual 11500.0)
... (6 more passes)
FAIL revenue_derived_anchor_mar_paid_search_tampa (expected 3066.6666666666674, actual 3066.6666666666660, Δ -1.4e-12, ε 1.0e-9)
ERROR gross_profit_after_cogs_change (read error: undeclared dependency in rule_gross_profit)

Goldens: 7/9 passed, 1 failed, 1 error
```

## Investigating failures

When a golden fails, three things might be wrong (per `agents/mosaic-validator.md`):

1. **The rule is wrong** — formula computes a different value.
2. **The input is wrong** — canonical_inputs has a typo / unit error / missing row.
3. **The expected value is wrong** — model author miscomputed by hand.

To diagnose:

- Read the failing golden's coord.
- Read the rule that produces that measure.
- Compute by hand: input cells × formula = predicted value.
- Compare the hand-computed value to the cube's actual.

If hand-computed = cube actual ≠ stated expected → expected was wrong; update it.

If hand-computed ≠ cube actual → something deeper. Cascade Null? Wrong rule chain order? Wrong consolidation aggregation? Walk through with mosaic-debugger.

**Don't loosen `epsilon` to make a failing golden pass.** If the actual deviates by more than 1e-9 on a chain that should be deterministic, find the actual bug.

## Performance contract

`mc model test` on an Acme-shaped model (6 dims, 11 measures, 5 rules, 2,520 input cells, 9 goldens) runs in **~32 ms wall-clock** at HEAD. The Phase 3C contract is **< 500 ms** wall-clock. If your model exceeds this, file a SPEC QUESTION — that's a regression.

## Skills referenced

- `skills/testing/SKILL.md` — canonical_inputs, fixtures, goldens, snapshot/rollback semantics, the `--fixture` filter.
- `skills/debugging/SKILL.md` — when goldens fail, the diagnostic codes that surface (MC2xxx if validate fired during the run, MC0002 if compile failed).

## Underlying CLI

```
mc model test <path> [--format text|json] [--fixture <name>]
```

The MCP server wraps this as `mosaic.model.test`. The output envelope shape is `{ schema_version: "1.0", skipped: N, goldens: [...] }` — different from `mc model validate / lint`'s diagnostic envelope because goldens carry per-test state (expected, actual, delta, epsilon, note) that diagnostics don't.

## What this command does NOT do

- **Does not edit YAML** — fix failing goldens via mosaic-debugger.
- **Does not skip stages** — test runs the full parse + validate + resolve_inputs + compile + apply_canonical_inputs + goldens pipeline. If validate would fail, test fails too.
- **Does not run `mc demo`.** Per ADR-0005 amendment #12, `mc demo --model` does not run goldens; that's exclusively `mc model test`'s job.
