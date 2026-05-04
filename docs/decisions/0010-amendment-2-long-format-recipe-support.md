# ADR-0010 Amendment 2: Long-Format Recipe Support (Phase 5A.1)

**Status:** Accepted (lands immediately after Phase 5A ships)
**Date:** 2026-05-04 (filed during Stream D development)
**Filed by:** PM, after Stream D SPEC QUESTION on Acme CSV layout
**Amends:** [ADR-0010 Decision 7](./0010-phase-5-tessera-architecture.md) (recipe format schema)

---

## What changed

### The gap

ADR-0010 Decision 7's recipe schema supports **wide-format** data only — each source column maps 1:1 to either a dimension or a measure. But the project's own canonical fixture (`crates/mc-model/examples/acme.inputs.csv`) is **long-format**:

```csv
Scenario,Version,Time,Channel,Market,Measure,value
Baseline,Working,Jan_2026,Paid_Search,Tampa,Spend,10500
Baseline,Working,Jan_2026,Paid_Search,Tampa,CPC,1.5
```

7 columns: 5 dimension columns + "Measure" (the measure NAME as a column value) + "value" (the scalar). 2,520 data rows, one cell per row. The current `ColumnMapping` schema has no way to express "this column carries the measure name, and this other column carries the value."

Long-format is common in real-world data: SQL query results, ETL exports, time-series data, pandas/R tidy-data conventions, and sparse multi-dimensional fact tables. Shipping Phase 5A with wide-only means the first real customer with a typical SQL query result hits this wall.

### Phase 5A treatment

Phase 5A ships with a **wide-format workaround** for the Acme equivalence test (Stream D generates a wide-format CSV at test setup time from `mc_fixtures::canonical_inputs_for()`). The test substance (byte-identical cube state) is preserved; only the input CSV shape differs.

### Phase 5A.1 treatment (this amendment)

Immediately after Phase 5A ships, a follow-up commit extends the recipe schema with long-format support and switches the equivalence test to read the actual `acme.inputs.csv`.

### Schema extension

**New fields in `mc-recipe::SourceConfig`:**

```rust
#[derive(Debug, Clone, Deserialize)]
pub enum SourceFormat {
    Wide,    // default — existing ADR-0010 behavior
    Long,    // each row is one cell; measure name + value in dedicated columns
}

#[derive(Debug, Clone, Deserialize)]
pub struct LongFormatConfig {
    pub measure_column: String,    // column whose values are measure names
    pub value_column: String,      // column carrying the numeric value
}

// Added to SourceConfig:
pub format: Option<SourceFormat>,           // default: Wide
pub long_format: Option<LongFormatConfig>,  // required iff format: Long
```

**Recipe example (long-format — reads the actual `acme.inputs.csv`):**

```yaml
version: 1
name: acme-import-long
model: ../../crates/mc-model/examples/acme.yaml

source:
  driver: csv
  path: ../../crates/mc-model/examples/acme.inputs.csv
  format: long
  long_format:
    measure_column: Measure
    value_column: value

columns:
  - source: Scenario
    dimension: Scenario
  - source: Version
    dimension: Version
  - source: Time
    dimension: Time
  - source: Channel
    dimension: Channel
  - source: Market
    dimension: Market
  # Note: Measure + value columns are consumed by long_format; no entries here.

defaults: {}

write_disposition: replace
on_error: abort
on_missing_element: error
```

**Semantics:**

- `format: wide` (default): existing ADR-0010 behavior. Each non-skipped column maps to exactly one dimension or measure.
- `format: long`: each row writes one cell. Dimension columns (declared in `columns:`) build the coordinate prefix. The `measure_column`'s value picks the measure for that row. The `value_column` carries the scalar. Long-format recipes MUST NOT have any `columns:` entries with `measure: X` (mutual exclusion — measures come from `measure_column`).

### Diagnostic codes added

| Code | Fires when |
|---|---|
| MC5019 | `format: long` specified but `long_format.measure_column` references a column not in the source schema |
| MC5020 | `format: long` specified but `long_format.value_column` references a column not in the source schema |
| MC5021 | `format: long` used with `measure: X` in `columns:` (mutual exclusion — measures come from `measure_column` in long format, not from column mappings) |
| MC5022 | `long_format.measure_column`'s values include names not declared as measures in the target model |

Total MC5xxx codes after this amendment: 22 (was 18 after Amendment 1).

### Implementation scope

| Crate | Change | Effort |
|---|---|---|
| `mc-recipe` | Add `SourceFormat` enum + `LongFormatConfig` struct to `SourceConfig`; add MC5019–MC5022 validation; roundtrip stability for the new fields | ~half day |
| `mc-tessera` | Add a "melt" step before the transformation pipeline: when `format: Long`, convert the long-format `RowBatch` (N rows × 7 cols) into a stream of `(CellCoordinate, ScalarValue)` pairs, one per row | ~half day |
| `mc-drivers` | **No change.** Drivers return `RowBatch`; format-awareness lives above them. | — |
| Equivalence test | Switch from generated wide-format CSV to the actual `crates/mc-model/examples/acme.inputs.csv` with `format: long` recipe | ~1 hour |

---

## Lesson learned (carry-forward to process-notes.md)

ADR-0010 Decision 7's recipe schema was designed against the wide-format mental model from the dbt/dlt/Singer prior-art research. The project's own canonical fixture (`acme.inputs.csv`) is long-format because that's the natural shape for sparse multi-dimensional data. **Future schema-design ADRs should explicitly enumerate the data shapes the schema must handle, with example data from the project's own fixtures as evidence.** The data format isn't a test detail — it's a design constraint.

Add to `docs/process-notes.md` alongside the Stream C "fresh-state verification" lesson.

---

## Cross-links

- [ADR-0010](./0010-phase-5-tessera-architecture.md) Decision 7 — the original recipe format this amends
- [Stream D SPEC QUESTION](../handoffs/phase-5a-stream-d-handoff.md) — filed during Stream D development
- `crates/mc-model/examples/acme.inputs.csv` — the actual long-format file
- [ADR-0010 Amendment 1](./0010-amendment-1-stream-c-pin-corrections.md) — the Stream C pin-corrections precedent for mid-flight amendments
