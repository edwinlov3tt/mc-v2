---
name: mosaic-testing
description: How to write Mosaic test data — canonical_inputs (always-loaded reference inputs; tabular YAML or sibling CSV), test_fixtures (named overlays scoped to one golden), golden_tests (inline assertions with epsilon tolerance), the strict CSV subset (UTF-8, header required, comma-separated, no quotes/newlines/comments), the snapshot/rollback mechanism between goldens, the --fixture filter flag, and the mc model test < 500 ms perf gate. Use when authoring goldens, writing input data, debugging fixture validators MC2012-MC2025, or deciding between inline tabular vs CSV.
---

# Test Data + Goldens

A Mosaic model's test layer has three distinct concepts:

1. **`canonical_inputs`** — always-loaded reference inputs. One per model. Applied before goldens run.
2. **`test_fixtures`** — named overlays. Loaded only when a golden references them by `fixture: <name>`.
3. **`golden_tests`** — inline assertions: "after applying these inputs, this coord should equal this value."

`mc model test <path>` runs the full pipeline:

1. Parse + validate + resolve_inputs (read sibling CSVs).
2. Compile to `mc_core::Cube`.
3. Apply `canonical_inputs` (writes the leaf cells the model declared).
4. Snapshot the cube state.
5. For each golden:
   - If the golden has `fixture: <name>`, apply the fixture's overlay rows on top of canonical_inputs.
   - Read the golden's `coord` and compare to `expect` (within 1e-9) or `expect_within_epsilon`.
   - If the fixture mutated state, rollback to the snapshot.
6. Report pass/fail/error per golden.

## `canonical_inputs`

The always-load reference data. **One block per model.** Two acceptable shapes (per ADR-0006 Decision 1):

### Tabular inline form (small fixtures)

```yaml
canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
  inline:
    rows:
      - ["Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "Spend", 11500.0]
      - ["Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "CPC",   1.50]
      # ...
```

Use this for small fixtures (≤ ~50 rows). Anything bigger should be a sibling CSV.

**Row shape (binding):** rows are POSITIONAL arrays, NOT map-shape objects. Each row has exactly `columns.len()` entries in column order. The last column is always literally `"value"`; the others must be declared dimension names. Exactly one of `source:` (sibling CSV) or `inline:` must be set on a `canonical_inputs:` (or `test_fixtures:`) block — never both, never neither.

### Sibling CSV form (preferred)

```yaml
canonical_inputs:
  source: "acme.inputs.csv"
  columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
```

CSV file structure:

```
Scenario,Version,Time,Channel,Market,Measure,value
Baseline,Working,Jan_2026,Paid_Search,Tampa,Spend,10500.0
Baseline,Working,Jan_2026,Paid_Search,Tampa,CPC,1.5
...
```

**Path-resolution rules (per Phase 3C):**

- The `source:` path is resolved relative to the YAML's parent directory.
- `..` and absolute paths are rejected with **MC2022**.
- The file must exist and be UTF-8.

**Strict CSV format (per ADR-0006 Decision 2):**

- UTF-8, no BOM. (BOM fires MC2024.)
- Header line required, exactly matching the YAML's `columns:`. Mismatch fires MC2024.
- Comma-separated. **No quotes**, **no embedded commas in values**, **no embedded newlines**, **no comments**, **no trailing whitespace** in fields. Any of these fire MC2023 / MC2024.
- One row per line (LF or CRLF both accepted).
- Empty lines and trailing newlines are OK.

The strictness is deliberate — Mosaic's CSV parser is hand-rolled (no `csv` crate dep) and stays robust by rejecting edge cases up front.

### `value` column type

Always F64. Strings, booleans, integers (auto-promote), scientific notation OK. NaN / inf / -inf fire **MC2021**.

## `test_fixtures`

Named overlays applied between goldens. Each fixture has the same source-XOR-inline shape as `canonical_inputs:`, but carries a `name:` so a golden can reference it.

```yaml
test_fixtures:
  - name: "spike_q4_paid_search"
    columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
    inline:
      rows:
        - ["Baseline", "Working", "Oct_2026", "Paid_Search", "Tampa", "Spend", 99999.0]

  - name: "drop_cpc_to_one_dollar"
    columns: ["Scenario", "Version", "Time", "Channel", "Market", "Measure", "value"]
    inline:
      rows:
        - ["Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "CPC", 1.0]
```

A fixture can also use `source: "..."` + `columns: [...]` to load from a sibling CSV.

**Fixtures are overlay-write semantic:** when a golden references `fixture: spike_q4_paid_search`, the fixture's rows are written on top of canonical_inputs. The fixture's coords override canonical_inputs at those exact coords. Other coords stay at canonical_inputs values.

**Constraints (each fires a separate MC2xxx):**

- Fixture rows can't write `Derived` measures (**MC2015**).
- Fixture rows can't write consolidated coords (**MC2020** — only leaves).
- Fixture rows must have all 6 dim columns (**MC2019**) and a parseable F64 `value` (**MC2018**).
- Two fixtures can't share a name (**MC2016**).

For the full validator list see `skills/debugging/SKILL.md`.

## `golden_tests`

Inline assertions over computed cell values. Each golden has:

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

  - name: "spike_revenue_after_fixture"
    coord: { Scenario: Baseline, Version: Working, Time: Oct_2026, Channel: Paid_Search, Market: Tampa, Measure: Revenue }
    expect_within_epsilon: { value: 999990.0, epsilon: 1.0e-3 }
    fixture: "spike_q4_paid_search"          # optional; applies fixture before reading
```

**Required fields:** `name`, `coord` (all 6 dims), and one of `expect` or `expect_within_epsilon`.

**`expect` vs `expect_within_epsilon`:**

- `expect: 11500.0` — exact match within `1e-9`. Use for input-cell goldens or simple multiplications where IEEE-754 produces a clean result.
- `expect_within_epsilon: { value: 3066.666..., epsilon: 1.0e-9 }` — match within explicit tolerance. Use for chained ratio computations where floating-point accumulates a tail.

**Failure modes (per `mc model test`):**

- **PASS** — actual within tolerance of expected.
- **FAIL** — read succeeded, value differs more than tolerance.
- **ERROR** — read failed (e.g., undeclared dependency, coord doesn't exist, fixture apply error).

**Goldens are run in declaration order** — but each golden is independent (snapshot/rollback between mutating goldens), so no cross-golden ordering effects.

## The snapshot/rollback mechanism

Per ADR-0006 amendment #17: `mc model test` snapshots the cube once after canonical_inputs are applied, then rollbacks between goldens that mutated state via a fixture. Read-only goldens (no `fixture:`) skip rollback because the cube state is unchanged.

What this means for authoring:

- Goldens with `fixture: <name>` see canonical_inputs + that fixture's overlays only.
- Goldens without `fixture` see canonical_inputs only (and any prior read-only goldens' computed cells are cached but state-equivalent).
- You don't need to "reset" between goldens — Mosaic does it automatically.

## The `--fixture <name>` filter

```bash
mc model test acme.yaml --fixture spike_q4_paid_search
```

**Filter-only semantic** (per ADR-0006 Decision 7 + amendment): runs only goldens whose `fixture:` field equals the named fixture; reports the rest as "skipped." It does NOT inject the fixture into goldens that don't declare it.

This is for selectively re-running a subset during iteration. When debugging a specific scenario, filter to its fixture and watch only those goldens.

## Performance contract

`mc model test acme.yaml` must complete in **< 500 ms wall-clock** (Phase 3C contract). The Acme reference runs in ~32 ms at HEAD. If your model exceeds 500 ms, something is wrong:

- Too many goldens? (Acme has 9; consider whether each adds value.)
- Goldens reading a deep rule chain repeatedly? (The cache should hit; if not, the rule chain may be inefficiently structured.)
- Canonical inputs CSV is huge? (Acme's CSV is 2,520 rows ~ 130 KB; that's the perf budget benchmark.)

If goldens run > 500 ms on a model the size of Acme, file a SPEC QUESTION — that's a performance regression, not normal.

## Worked example: the Acme golden suite

Acme has 9 goldens:

- 3 input-cell anchors (Spend, CPC, AOV at the Mar_2026 / Paid_Search / Tampa coord) — `expect:` exact match.
- 5 derived-cell anchors (Clicks, Leads, Customers, Revenue, Gross_Profit at the same coord) — `expect_within_epsilon:` because each is a chained ratio.
- 1 consolidation-level golden (`Q1_2026 Spend at Paid_Search/Tampa = sum of Jan/Feb/Mar`) — `expect:` exact because it's an integer sum.

This shape is a good template: a few input anchors to verify the data loaded, derived anchors at one coord to verify the rule chain, and at least one consolidation golden to verify the hierarchy rolls up correctly.

## Anti-patterns (DON'T)

- **Don't write canonical_inputs against derived measures.** Mosaic rejects with MC2015. Derived measures are computed; only inputs are writable.
- **Don't write canonical_inputs at consolidated coords.** Mosaic rejects with MC2020. Inputs are at leaves; rollups are computed.
- **Don't quote CSV fields.** The strict subset rejects quotes. If your data has commas in values — restructure (e.g., use underscores in element names).
- **Don't loosen epsilon to make a failing golden pass.** If the actual deviates from expected by more than 1e-9 on a chain that should be deterministic, your model is wrong, not the tolerance. Recompute the expected value by hand.
- **Don't add 100 redundant goldens.** Each golden is a contract; pick the load-bearing ones (input anchors, end-of-chain anchors, key consolidations).
- **Don't share fixture names across models.** Names are scoped to one YAML file, but readability suffers if the same name means different things in different models.
