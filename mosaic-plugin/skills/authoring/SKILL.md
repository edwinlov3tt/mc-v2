---
name: mosaic-authoring
description: How to write a Mosaic YAML model end-to-end — the top-level structure (metadata, dimensions, hierarchies, measures, rules, canonical_inputs, test_fixtures, golden_tests), the four-stage pipeline (parse → validate → resolve_inputs → compile), and which CLI verb (validate / inspect / lint / test) does what. Use whenever the user is starting a new model, asks how Mosaic YAML is organized, asks what each section means, or wants to know how the pieces fit together.
---

# Authoring a Mosaic Model

A Mosaic YAML file describes one *cube* — a multidimensional planning model with dimensions, measures, rules, and (optionally) input data and golden tests. This skill teaches the file's top-level structure and how the pieces relate.

For domain specifics, see `skills/domain-schemas/marketing-mix/SKILL.md`. For diagnostic codes when something fails, see `skills/debugging/SKILL.md`. For formula syntax inside rule bodies, see `skills/formulas/SKILL.md`. For input data and goldens, see `skills/testing/SKILL.md`. For the dim-order rule and aggregation choices, see `skills/schema-design/SKILL.md`.

## The four-stage pipeline

```
YAML bytes
    │
    ├──[ParseError]──► MC1xxx
    ▼
ParsedModel
    │
    ├──[ValidationError]──► MC2xxx
    ▼
ValidatedModel  ◄── (Optional) resolve_inputs reads sibling CSVs ──► MC2012-MC2025
    │
    ├──[EngineError]──► passthrough
    ▼
mc_core::Cube
```

Every Mosaic CLI verb runs some prefix of this pipeline:

- `mc model validate <path>` — parse + validate + resolve_inputs (filesystem-aware), then exit. No cube built.
- `mc model inspect <path>` — same as validate, plus render the model summary.
- `mc model lint <path>` — same as validate, plus run advisory MC3xxx lint rules.
- `mc model test <path>` — full pipeline: parse + validate + resolve_inputs + compile to `mc_core::Cube` + apply `canonical_inputs` + run goldens.
- `mc demo --model <path>` — same as test, but does NOT run goldens (per ADR-0005 amendment #12, goldens are exclusively `mc model test`'s job).

## Top-level structure

A Mosaic YAML file always declares these top-level keys (order doesn't matter, but the canonical layout is below):

```yaml
model_format_version: 1

canonical_inputs:           # optional; reference inputs always loaded by `mc model test`
  source: "<path>.csv"
  columns: [...]

metadata:                   # required
  name: "<model_name>"
  description: "<one-liner>"
  author: "..."
  created: "YYYY-MM-DD"

dimensions: [...]           # required — exactly 6 in canonical order (see below)
hierarchies: [...]          # 0..N hierarchies; one per dim is the Phase 1 cap
measures: [...]             # required — populates the Measure dim's elements
rules: [...]                # 0..N derived-measure rules

test_fixtures: [...]        # optional named overlays for goldens
golden_tests: [...]         # optional inline assertions
```

### `model_format_version`

Integer. Phase 3A is `1`. Future schema bumps land as a new integer; never edit this without a corresponding ADR.

### `metadata`

```yaml
metadata:
  name: "Acme_MarketingFinance"          # required; used in inspect output
  description: "Brief §4 reference cube" # optional; one-liner
  author: "..."                          # optional
  created: "2026-05-02"                  # optional ISO date
```

### `dimensions`

Mosaic cubes have exactly 6 dimensions in the canonical order:

```
[Scenario, Version, Time, Channel, Market, Measure]
```

This order is **non-negotiable** (per brief §3 + ADR-0001). The kernel's `CellCoordinate` is positional against `cube.dimensions`; reordering breaks the storage contract. See `skills/schema-design/SKILL.md` for the full rule + rationale.

Each dimension declaration:

```yaml
- name: "Time"
  description: "Calendar time periods used for plan-vs-actual rollups."
  kind: "Standard"        # one of: Scenario, Version, Standard, Measure
  elements:
    - { name: "Jan_2026" }
    - { name: "Feb_2026" }
    # ... etc.
```

The Scenario / Version / Measure dims have specialized `kind:` values. The Measure dim's `elements:` is always `[]` — its elements come from the top-level `measures:` block.

### `hierarchies`

Optional rollup trees over a dimension. Phase 1 caps each dim at one default hierarchy:

```yaml
- dimension: "Time"
  name: "Calendar"
  default: true
  edges:
    - { parent: "Q1_2026", child: "Jan_2026", weight: 1.0 }
    - { parent: "Q1_2026", child: "Feb_2026", weight: 1.0 }
    - { parent: "Q1_2026", child: "Mar_2026", weight: 1.0 }
    - { parent: "FY_2026", child: "Q1_2026", weight: 1.0 }
    # ...
```

Edge weights live in `[0.0, 1.0]`. A consolidated element (any element with at least one child edge) is non-writable — writebacks against `Q1_2026 Spend` reject; writes against the leaves (`Jan_2026 Spend`, `Feb_2026 Spend`, `Mar_2026 Spend`) succeed.

### `measures`

Defines the Measure dim's elements. Each measure has:

- `name` — string, must match references in rule bodies + goldens.
- `description` — short prose; the lint rules (MC3001–MC3007 + MC3009–MC3011) require this on every measure.
- `role` — `Input` or `Derived`. Phase 1 does NOT support `Both` (per brief change-log).
- `data_type` — `F64` (the only supported type currently).
- `aggregation` — `Sum`, `WeightedAverage`, `Min`, or `Max`. **Defaulting to Sum is wrong for ratios.** See `skills/schema-design/SKILL.md` aggregation section.
- `weight_measure` — required iff `aggregation: WeightedAverage`. Names another measure (typically Spend, Clicks, Customers, Revenue).

Example:

```yaml
- name: "Spend"
  description: "Marketing dollars allocated (USD)."
  role: "Input"
  data_type: "F64"
  aggregation: "Sum"

- name: "CPC"
  description: "Cost per click (USD/click); rolls up as a Spend-weighted average."
  role: "Input"
  data_type: "F64"
  aggregation: "WeightedAverage"
  weight_measure: "Spend"
```

### `rules`

Defines derived-measure computations. Phase 3D introduced friendly formula syntax; both forms are accepted (use whichever is clearer):

```yaml
# Formula form (Phase 3D — recommended for human authors):
- name: "rule_clicks"
  description: "Clicks = Spend / CPC."
  target_measure: "Clicks"
  scope: "AllLeaves"
  body: "Spend / CPC"
  declared_dependencies: ["Spend", "CPC"]

# Structured form (still supported indefinitely):
- name: "rule_clicks"
  target_measure: "Clicks"
  scope: "AllLeaves"
  body:
    div:
      - { ref: "Spend" }
      - { ref: "CPC" }
  declared_dependencies: ["Spend", "CPC"]
```

`scope: "AllLeaves"` is the only Phase 1 scope. `declared_dependencies` MUST list every measure read in the body — the kernel rejects undeclared reads with `EngineError::UndeclaredDependency` at runtime, which surfaces as a compile failure during `mc model test`.

For full formula grammar see `skills/formulas/SKILL.md`.

### `canonical_inputs`

Always-loaded reference data. Two acceptable shapes (per ADR-0006 Decision 1 — per-row inline was dropped pre-acceptance). Both forms require `columns:`; exactly one of `source:` (sibling CSV) OR `inline:` (tabular YAML) must be set.

```yaml
# Tabular inline form (small fixtures only — rows are POSITIONAL arrays
# matching `columns:` order, NOT map-shape entries):
canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
  inline:
    rows:
      - ["Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "Spend", 11500.0]
      - ["Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "CPC",   1.50]
      # ...

# Sibling CSV form (preferred for non-trivial sizes):
canonical_inputs:
  source: "acme.inputs.csv"
  columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
```

Path-escape rules (per Phase 3C): no `..`, no absolute paths, must resolve under the model's parent directory. The CSV format is strict — UTF-8, header required, comma-separated, no quotes, no embedded newlines, no comments. See `skills/testing/SKILL.md`.

The last column is always literally `"value"` (the cell payload); every other column must match a declared dimension name. Exactly one of `source:` / `inline:` must be set, never both, never neither (resolve_inputs surfaces a structural error otherwise).

### `test_fixtures`

Named overlays applied between goldens. Each fixture has the same source-XOR-inline shape as `canonical_inputs`, but carries a `name:` so a golden can reference it.

```yaml
test_fixtures:
  - name: "spike_q4_paid_search"
    columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
    inline:
      rows:
        - ["Baseline", "Working", "Oct_2026", "Paid_Search", "Tampa", "Spend", 99999.0]
```

### `golden_tests`

Inline assertions over computed cell values. Run by `mc model test`:

```yaml
golden_tests:
  - name: "revenue_anchor_mar_paid_search_tampa"
    coord:
      Scenario: "Baseline"
      Version: "Working"
      Time: "Mar_2026"
      Channel: "Paid_Search"
      Market: "Tampa"
      Measure: "Revenue"
    expect_within_epsilon: { value: 3066.6666666666674, epsilon: 1.0e-9 }
    fixture: "spike_q4_paid_search"   # optional; applies the named fixture before this golden
```

Use `expect: <value>` for exact (within 1e-9) match; use `expect_within_epsilon: { value, epsilon }` when you need a wider tolerance (typically for chained ratio computations).

## CLI usage

| Goal | Command |
|---|---|
| Catch parse + structural + fixture errors | `mc model validate <path>` |
| See the model summary | `mc model inspect <path>` |
| Surface advisory warnings (style, redundancy) | `mc model lint <path>` |
| Make warnings fail CI | `mc model lint <path> --deny-warnings` |
| Run goldens | `mc model test <path>` |
| Run only a subset of goldens | `mc model test <path> --fixture <name>` |
| Run end-to-end (no goldens) | `mc demo --model <path>` |
| Get JSON for an LLM iteration loop | append `--format json` to any of the above |

The `--format json` flag emits the Phase 3B envelope:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    { "code": "MC2001", "severity": "Error", "path": "/dimensions/0", "message": "...", "suggestion": "..." }
  ]
}
```

Diagnostics are sorted `(severity desc, code asc, yaml_pointer asc, message asc)` deterministically across runs.

## Authoring loop

A typical end-to-end author session:

1. `/mosaic-init marketing-mix` (or `/mosaic-author "..."`) — scaffold or design the schema.
2. Write or edit the YAML in your editor.
3. `/mosaic-validate <path>` — catch parse + validation errors first. Errors block; fix them top-down.
4. `/mosaic-lint <path>` — clean up advisory MC3xxx warnings. The Acme reference lints at zero warnings; aim for the same.
5. `/mosaic-test <path>` — run goldens. If goldens fail, the values you expected don't match what the rules compute. Either the rules are wrong, the inputs are wrong, or the expected values are wrong — figure out which.
6. `/mosaic-inspect <path>` — sanity-check dim counts, measure roles, rule chain depth.

The diagnostic JSON envelope is the LLM's grounding rail. Every iteration should consume `--format json` output and look up the codes in `skills/debugging/SKILL.md` rather than trying to interpret free-form messages.
