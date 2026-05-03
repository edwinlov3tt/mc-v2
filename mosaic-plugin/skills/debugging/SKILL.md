---
name: mosaic-debugging
description: How to read Mosaic diagnostic JSON envelopes and look up MC1xxx (parse), MC2xxx (validation), MC3xxx (lint warnings), and MC4xxx (reserved) error codes. Use whenever `mc model validate`, `mc model lint`, or `mc model test` reports an error or warning, or whenever a YAML model fails to parse, validate, or compile. Includes the JSON envelope shape, the deterministic sort order, the MC3008-is-retired rule, and a fix pattern for every code through Phase 3D.
---

# Mosaic Diagnostic Codes

Every error or warning Mosaic produces carries a stable 6-character code in the `MC[1-4]NNN` namespace. **Codes are forever** (CVE-style retirement per ADR-0005 amendment #11) — if a code is retired, its slot stays empty. Never reintroduce a retired code.

This skill is the load-bearing reference for the iteration loop: emit YAML → run `mc model validate/lint/test --format json` → parse the envelope → look up each code here → propose a specific fix → re-emit → repeat.

## The JSON envelope (Phase 3B contract)

Every `mc model` verb supports `--format json`:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC2001",
      "severity": "error",
      "path": {
        "file": "acme.yaml",
        "yaml_pointer": "/dimensions/0",
        "model_path": "dimensions[0]",
        "span": { "line": 38, "column": 5 }
      },
      "message": "duplicate name 'Scenario' in dimensions",
      "suggestion": "rename one occurrence or remove the duplicate"
    }
  ]
}
```

**Stable fields (`schema_version: "1.0"` contract):**

- `code` — the MC code (string, always 6 chars: `MC` + 4 digits).
- `severity` — `"error"`, `"warning"`, or `"info"`.
- `path` — `ModelPath { file, yaml_pointer, model_path, span? }`. The `yaml_pointer` is RFC 6901-style (`/dimensions/0/elements/2`); `model_path` is human-friendly (`dimensions[0].elements[2]`).
- `message` — the human-readable description.
- `suggestion` — optional fix hint; absent for codes whose fix is implicit in the message.

**Sort order (deterministic across runs):** `(severity desc, code asc, yaml_pointer asc, message asc)`. Errors before warnings before info; within a severity, lower codes come first; ties broken by YAML pointer alphabetical, then message.

`mc model test` emits a different envelope shape (`{schema_version, skipped, goldens}`) — the goldens are individual test results, not Diagnostics. The shape is in `skills/testing/SKILL.md`.

## Severity taxonomy

| Severity | Behavior | Codes |
|---|---|---|
| `error` | Blocks `mc model validate/lint/test`; non-zero exit; cannot proceed to compile | MC0001, MC0002, MC1xxx, MC2xxx |
| `warning` | Advisory; does NOT block by default; non-zero exit only with `mc model lint --deny-warnings` | MC3xxx |
| `info` | Informational; never blocks | MC3xxx (currently MC3010 only) |

The Acme reference lints at **zero warnings, zero info, zero errors** at HEAD. New models should target the same.

## Code registry

### MC0xxx — CLI / IO errors (not a stable namespace; emitted by `mc-cli` only)

| Code | Fires when | Fix |
|---|---|---|
| MC0001 | `mc-cli` cannot read the YAML file (missing path, permission denied) | Check the path is correct; `ls <path>`. |
| MC0002 | The compile stage failed after validate cleared (rare; usually means a kernel constraint not yet caught by `mc-model` validation) | Read the message; if it names a measure or dim, that's where to look. |

These appear via `mc-cli` only and are not part of the `mc_model` library's `Diagnostic`. They share the envelope shape.

### MC1xxx — parse errors (text input is malformed)

YAML or formula syntax problems. The four-stage pipeline halts at parse; nothing else runs.

#### MC1001 — YAML syntax error

**Fires:** the YAML cannot be parsed at all (bad indentation, unclosed quote, invalid character).

**Fix pattern:** read the line + column from `path.span`; usually it's an indentation mismatch (mixing tabs and spaces, or one element under a list shifted left/right). YAML is whitespace-sensitive — every list item under the same parent indents the same.

```yaml
# WRONG (Bob and Alice indented differently):
dimensions:
  - name: "Scenario"
    elements:
     - { name: "Baseline" }   # 5 spaces
      - { name: "Aggressive" } # 6 spaces — MC1001
```

#### MC1002 — YAML safe-subset violation

**Fires:** the YAML is parseable but uses a banned feature — anchors (`&`), aliases (`*`), merge keys (`<<`), custom tags (`!!something`), or non-string keys.

**Fix pattern:** the safe subset is binding (per ADR-0004 Decision 1). Inline the value where you'd normally use an anchor; rewrite a merge into explicit keys; remove custom tags. Quote every string-like value.

```yaml
# WRONG:
default_measure: &dm { name: "Spend", role: "Input" }
measures:
  - *dm                   # MC1002 — alias

# RIGHT:
measures:
  - { name: "Spend", role: "Input" }
```

#### MC1003 — formula unbalanced or unexpected paren

**Fires:** a formula body has unbalanced `(` / `)`, or a paren in an invalid position.

**Fix pattern:** count opens vs closes. Look for a closing paren that doesn't match an open one.

```yaml
# WRONG:
- body: "Spend / (CPC + 1"            # MC1003 — unbalanced
- body: "Spend / )CPC("               # MC1003 — paren in wrong position

# RIGHT:
- body: "Spend / (CPC + 1)"
```

#### MC1004 — formula unexpected token OR unknown function call

**Fires (per ADR-0007 amendment #25 — both meanings collapsed into MC1004 for Phase 3D):**

1. An unexpected token appears (a stray `.`, `,`, `=`, `<`, `>`, `&`, `|`).
2. A function call references a name other than `if_null`. Phase 3D supports exactly **one** function: `if_null(primary, fallback)`. Anything else fires MC1004.

**Fix pattern:** if you used `min(a,b)`, `max(a,b)`, `if(...)`, `==`, `<`, `>`, or any operator from another formula language — Mosaic doesn't support it. Restructure the rule, or split it into multiple rules. The structured-tree form has the same operator set; switching forms doesn't add operators.

```yaml
# WRONG:
- body: "min(Spend, 1000)"    # MC1004 — unknown function
- body: "Spend == CPC"         # MC1004 — comparison not supported
- body: "Spend ; CPC"          # MC1004 — unexpected token

# RIGHT (no min — restructure):
- body: "Spend"                  # use a separate measure with cap logic in your data layer
```

#### MC1005 — formula expected expression

**Fires:** a formula ends with an operator (`Spend +`), starts with a binary operator (`+ Spend`), or has two operators in a row (`Spend + + CPC`).

**Fix pattern:** every binary operator needs an expression on both sides. Unary `+` and `-` are allowed at the start of an expression or after an open paren only.

```yaml
# WRONG:
- body: "Spend + "             # MC1005 — trailing operator
- body: "+ Spend"              # MC1005 — leading binary +
- body: "Spend ++ CPC"         # MC1005 — operator without expression between

# RIGHT:
- body: "Spend + CPC"
- body: "-Spend"               # unary minus is fine
- body: "Spend + (-CPC)"       # unary inside parens is fine
```

#### MC1006 — formula invalid number literal

**Fires:** a number literal can't be parsed as F64 — common cases: `1..5`, `1e`, `1.2.3`, `1_000` (no underscores), `0x1A` (no hex).

**Fix pattern:** use plain decimal notation. Scientific notation is OK (`1.5e-3`). Negative numbers written as `(-3.0)` parse as unary minus on `3.0`.

```yaml
# WRONG:
- body: "Spend * 1_000"        # MC1006 — underscores not supported
- body: "Spend * 1..5"          # MC1006 — double dot
- body: "Spend * 1e"            # MC1006 — exponent without value
- body: "Spend * 0x1A"          # MC1006 — hex not supported

# RIGHT:
- body: "Spend * 1000"
- body: "Spend * 1.5e-3"
```

### MC2xxx — validation errors (parse OK; model is structurally wrong)

The model file parses but doesn't satisfy a structural invariant. `validate()` returns ALL of these in one pass so you fix them in batch.

#### Structural validators (MC2001 – MC2010, plus MC2011 promoted from lint)

| Code | Variant | Fires when |
|---|---|---|
| MC2001 | `DuplicateName` | Two dims, two elements within a dim, two measures, or two rules share a name. |
| MC2002 | `MissingDimension` | The dim list isn't exactly `[Scenario, Version, Time, Channel, Market, Measure]` — wrong count, wrong order, or wrong `kind`. |
| MC2003 | `InvalidHierarchyEdge` | An edge references an element that doesn't exist on its dim, or the weight is outside `[0.0, 1.0]`, or parent == child. |
| MC2004 | `HierarchyCycle` | The hierarchy edges form a cycle (A → B → A). |
| MC2005 | `RuleReferencesUnknownMeasure` | A rule's `target_measure` or `declared_dependencies` names a measure that doesn't exist, or its body's `ref` (structured form) does. |
| MC2006 | `DerivedMeasureWithoutRule` | A measure has `role: Derived` but no rule targets it. |
| MC2007 | `InputMeasureHasRule` | A measure has `role: Input` but a rule targets it. (Inputs are written via `canonical_inputs` / `test_fixtures` / writebacks, never derived.) |
| MC2008 | `RuleCycle` | The rule-dependency graph has a cycle (rule A depends on rule B, B on C, C on A). |
| MC2009 | `UnsupportedAggregation` | An aggregation other than `Sum` / `WeightedAverage` / `Min` / `Max` was declared. |
| MC2010 | `Schema` | Generic structural misshape: missing required field, wrong type, `model_format_version` ≠ 1. |
| MC2011 | `WeightedAverageMissingWeight` | A measure declared `aggregation: WeightedAverage` did NOT declare a `weight_measure:`. **(Promoted from lint MC3008 in Phase 3B per ADR-0005 amendment #4.)** |

**Fix pattern for MC2002 (the most common LLM mistake):** the dimension list MUST be exactly six entries in this order:

```yaml
dimensions:
  - { name: "Scenario", kind: "Scenario", elements: [...] }
  - { name: "Version",  kind: "Version",  elements: [...] }
  - { name: "Time",     kind: "Standard", elements: [...] }
  - { name: "Channel",  kind: "Standard", elements: [...] }
  - { name: "Market",   kind: "Standard", elements: [...] }
  - { name: "Measure",  kind: "Measure",  elements: [] }
```

If your domain doesn't have a meaningful Channel or Market, declare a dim with a single `All` element — don't omit the dim. The dim order is a positional contract with the kernel; reordering breaks the storage layout. See `skills/schema-design/SKILL.md` for the full rule.

**Fix pattern for MC2011 (the most common WeightedAverage mistake):**

```yaml
# WRONG:
- name: "CPC"
  aggregation: "WeightedAverage"          # MC2011 — missing weight_measure

# RIGHT:
- name: "CPC"
  aggregation: "WeightedAverage"
  weight_measure: "Spend"                 # weighted by Spend (the canonical CPC weight)
```

The weight measure must:
- Be a separate measure that exists in `measures:`.
- Typically be a `Sum`-aggregated quantity that "drives" the ratio (Spend → CPC, Clicks → CVR, Leads → Close_Rate, Customers → AOV, Revenue → COGS_Rate).

#### Fixture / input validators (MC2012 – MC2025, Phase 3C)

These fire during `resolve_inputs` (read sibling CSVs and validate fixture/canonical_inputs row data).

| Code | Variant | Fires when |
|---|---|---|
| MC2012 | `FixtureUnknownDimensionKey` | A row has a column name that isn't one of the 6 dim names (or `value`). |
| MC2013 | `FixtureUnknownElementValue` | A row's value for a dim column isn't an element of that dim. |
| MC2014 | `FixtureUnknownMeasure` | A row's `Measure` value isn't a measure in the model. |
| MC2015 | `FixtureWritesDerivedMeasure` | A row writes against a `role: Derived` measure (only inputs are writable). |
| MC2016 | `DuplicateFixtureName` | Two fixtures share the same `name:`. |
| MC2017 | `GoldenReferencesUnknownFixture` | A golden's `fixture: <name>` names a fixture that isn't declared. |
| MC2018 | `FixtureValueTypeMismatch` | A row's `value` field isn't a parseable F64. |
| MC2019 | `FixtureMissingDimension` | A row is missing one of the 6 required dim columns. |
| MC2020 | `FixtureWritesConsolidatedCell` | A row's coord names a consolidated element (e.g., `Time: Q1_2026`). Inputs only write at leaves. |
| MC2021 | `FixtureValueIsNaN` | A row's `value` is `NaN`, `+inf`, or `-inf`. |
| MC2022 | `FixtureSourceUnreadable` | The `source:` CSV path doesn't exist, or path-escapes (`..`, absolute path), or isn't UTF-8. |
| MC2023 | `FixtureCsvRowColumnCountMismatch` | A CSV row has a different number of fields than the header declared. |
| MC2024 | `FixtureCsvHeaderMismatch` | The CSV header doesn't match the YAML's `columns:` declaration. |
| MC2025 | `FixtureDuplicateCoordinate` | The same coord appears twice in one input set. |

**Fix pattern for MC2020 (consolidated-cell write):** hierarchies are always present in the canonical Acme cube. If your CSV has rows like `Time: Q1_2026, Channel: Paid_Media, Market: Florida, Measure: Spend, value: 100000.0` — that's writing at the consolidated level. Replace with the leaf rows that roll up to it (Jan/Feb/Mar 2026 × Paid_Search/Paid_Social/Display × Tampa/Orlando/Miami). The kernel computes the rollup by consolidating leaves; you don't write rollups directly.

**Fix pattern for MC2022 (path escape):** sibling CSV paths are resolved relative to the YAML's parent dir. `..` and absolute paths are rejected. If your model lives at `models/acme.yaml`, the CSV must be `models/<name>.csv` or `models/<subdir>/<name>.csv` — not `../shared/data.csv` and not `/abs/path/to/data.csv`.

### MC3xxx — lint warnings (advisory; never blocks unless `--deny-warnings`)

Style and quality concerns. Acme passes lint at zero warnings; new models should aim for the same.

| Code | What it flags | Fix pattern |
|---|---|---|
| MC3001 | Inconsistent naming style across elements within a dim (snake_case mixed with PascalCase) | Pick one style; rename. |
| MC3002 | Duplicate description text across multiple measures | Make descriptions specific. |
| MC3003 | Empty or whitespace-only `description` on a dim/measure/rule | Write a one-sentence description. |
| MC3004 | A rule body that's a no-op (just `Spend`, just `0.0`) | Either delete the rule or compute something. |
| MC3005 | A rule's declared dependencies don't match what its body references (missing or extra deps) | Sync `declared_dependencies:` with the body's `ref`s. |
| MC3006 | A measure with `aggregation: Sum` whose values look ratio-like (e.g., its name or description suggests a rate) | Switch to `WeightedAverage` with the right weight measure. |
| MC3007 | A consolidation hierarchy edge with `weight: 0.0` | Either remove the edge or set a non-zero weight. |
| MC3008 | **PERMANENTLY RETIRED.** Was a lint check for `WeightedAverage` measures missing `weight_measure`; promoted to **MC2011** (validation error) in Phase 3B per ADR-0005 amendment #4. **Never reintroduce this code.** |
| MC3009 | An `Input` measure that's never written by any `canonical_inputs`, `test_fixtures`, or runtime writeback | Either populate it or mark it `Derived` (and write a rule). |
| MC3010 | A `Derived` measure whose rule is never referenced by another rule and is never read in any golden (severity `info`, not warning) | Add a golden, or accept the info as documentation. |
| MC3011 | A golden that references a coord whose values can't be derived from the inputs (the golden's expected value is unreachable) | Verify the inputs match the rule chain; recompute the expected value. |

### MC4xxx — reserved namespace

Reserved for LLM-authoring-specific concerns (Phase 4+). **No active codes through Phase 4A.** When a code is added, it lands in this skill with the same fix-pattern format.

## The MC3008 retirement rule (binding)

> **MC3008 is permanently retired.** It was a Phase 3B lint check that fired when a `WeightedAverage` measure didn't declare a `weight_measure`. Per [ADR-0005](../../../docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md) acceptance amendment #4, the check was promoted to a hard validation error and assigned **MC2011** so models with broken aggregation couldn't ship. **Do not write a lint rule that emits "MC3008".** The code is gone forever; the slot stays empty for code-stability auditing.

If you're tempted to add an MC3008 anywhere — stop. Check whether your concern is already covered by MC2011 (validation) or MC3006 (ratio-looks-like-Sum lint). If you genuinely need a new code, request an MC3012+ allocation via an ADR.

## Iteration loop

```
1. Run `mc model validate <path> --format json` — get the envelope.
2. If diagnostics is empty: validation clean. Skip to step 5.
3. For each diagnostic, in order:
     - Look up the code in this registry.
     - Identify the YAML location from path.yaml_pointer + path.span.
     - Apply the fix pattern.
4. Re-run validate. If new diagnostics appeared (errors that earlier errors masked), repeat from 3.
5. Run `mc model lint <path> --format json`.
6. For each MC3xxx warning: either fix or document the choice.
7. Run `mc model test <path> --format json`.
8. For each non-pass golden: read the diagnostic. If `expected != actual`, decide whether the rules are wrong, the inputs are wrong, or the expected was wrong.
```

The loop converges in ≤ 5 iterations on well-designed schemas. If the same code keeps firing despite fixes — read the message more carefully; the fix pattern in this skill is generic and the specific message tells you exactly which dim / measure / rule is the culprit.

## Anti-patterns (DON'T)

- **Don't suppress lint warnings just to make the output cleaner.** Each MC3xxx code is there because it catches a real authoring smell. Either fix or document the deliberate choice.
- **Don't change the YAML schema to fit unusual data — change the data.** If your inputs include consolidated rows, restructure to leaves; don't bend the schema.
- **Don't skip MC1xxx parse errors and try to fix downstream errors first.** Parse must succeed before validate runs. The downstream errors are noise until parse is clean.
- **Don't add `MC3008` to anything.** Permanently retired.
- **Don't trust free-form error messages over the code lookup.** The message is human-readable, the code is the contract. Prefer the code.
