---
name: mosaic-csv-mapping
description: How to map CSV files into Mosaic cubes via Tessera recipes — wide-format (one source column per measure) and long-format (measure name + value in dedicated columns), per-column type coercion (`type:`), numeric scaling (`scale:`), date format hints (`format:`), and explicit `skip:` for irrelevant columns. Use when the source is a `.csv` file (local or fetched), when the user shows a CSV header, or when debugging an MC5xxx fired against a `driver: csv` recipe. Builds on `skills/import/recipe-format/SKILL.md` — read that first for the full recipe schema and the 18 MC5xxx codes.
---

# Authoring Mosaic Recipes for CSV Sources

This skill goes deeper on the **`driver: csv`** path than the general recipe-format skill. The recipe schema, the six semantic rules, and the full MC5xxx code table live in [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md); this file is the CSV-specific deep-dive — wide vs long format, the column-coercion fields (`type:` / `scale:` / `format:`), and the CSV-shaped pitfalls that don't show up for SQL or HTTP sources.

## When to use the CSV driver

Use `driver: csv` when:

- The source is a local CSV file accessible by relative path from the recipe.
- The data fits the Tessera schema's row-oriented model (one row → one or more cube cells).
- Your CSV has either:
  - **Wide format**: each non-skipped column maps 1:1 to a dimension or measure (current Phase 5A default), or
  - **Long format**: each row carries one cell — dimension columns + a measure-name column + a value column (Phase 5A.1; see "Long format" below).

Don't use `driver: csv` when:

- The source needs filtering / aggregation / joining before import — use `driver: sqlite` (or `duckdb`) with a `query:` instead. CSV recipes have no `query:` field.
- The source is multi-file (e.g., a directory of monthly drops) — Phase 5A reads one file per recipe. Run multiple recipes or pre-concatenate.
- The data is JSON/TSV/Excel — those aren't supported drivers in Phase 5A.

---

## Wide format (the default — Phase 5A)

Every non-skipped source column maps to either one dimension or one measure. This is the shape every Phase 5A example recipe in `crates/mc-recipe/examples/recipes/` uses.

### Anatomy of a wide CSV

```
month,channel,market,spend,cpc
Jan_2026,Paid_Search,Tampa,10500,1.50
Jan_2026,Paid_Search,Orlando,9800,1.55
Jan_2026,Paid_Social,Tampa,7300,2.10
```

5 columns: 3 dimension columns + 2 measure columns. Each row writes 2 cells (one Spend, one CPC) at coordinate `(Baseline, Working, <month>, <channel>, <market>, <measure>)`.

### Wide-format recipe

```yaml
version: 1
name: q3_actuals
description: "Wide-format CSV import — Spend + CPC into Acme."
model: ../models/acme.yaml

source:
  driver: csv
  path: ./q3_actuals.csv

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

The `columns:` array names every CSV header (`source:`) you intend to use. If a header is absent from `columns:` AND the recipe has no skip entry for it, the validator fires **MC5011** — there's no silent drop.

### Two dimensions are constant — use `defaults:`

Acme has six dimensions. The CSV above carries only Time / Channel / Market in its rows, so Scenario and Version are pinned via `defaults:`. The Measure dimension is implicit — every measure column writes one cell into the corresponding `Measure` element (`Spend`, `CPC`).

A dimension is either varying-per-row (`columns:` entry with `dimension:`) **or** constant (`defaults:` entry); never both. Both fires **MC5016**.

---

## Long format (Phase 5A.1 — filed but not yet in schema)

Long-format CSVs have one row per cell: dimension columns + a "measure name" column + a "value" column. This is the natural shape for sparse multi-dimensional fact tables, dbt/Singer outputs, and SQL `UNPIVOT` results.

### Anatomy of a long CSV

The project's own canonical fixture (`crates/mc-model/examples/acme.inputs.csv`) is long-format:

```
Scenario,Version,Time,Channel,Market,Measure,value
Baseline,Working,Jan_2026,Paid_Search,Tampa,Spend,10500
Baseline,Working,Jan_2026,Paid_Search,Tampa,CPC,1.5
Baseline,Working,Jan_2026,Paid_Search,Orlando,Spend,9800
Baseline,Working,Jan_2026,Paid_Search,Orlando,CPC,1.55
```

Each row writes one cell. The `Measure` column carries the measure name; the `value` column carries the scalar.

### Long-format recipe shape (per ADR-0010 Amendment 2)

```yaml
version: 1
name: acme_long
model: ../models/acme.yaml

source:
  driver: csv
  path: ./acme.inputs.csv
  format: long                   # NEW field — Phase 5A.1
  long_format:                   # NEW block — Phase 5A.1
    measure_column: Measure      # column whose VALUES are measure names
    value_column: value          # column carrying the scalar

columns:
  - { source: Scenario, dimension: Scenario }
  - { source: Version,  dimension: Version }
  - { source: Time,     dimension: Time }
  - { source: Channel,  dimension: Channel }
  - { source: Market,   dimension: Market }
  # Measure + value are consumed by long_format — no entries here.

defaults: {}
write_disposition: replace
incremental: false
on_error: abort
on_missing_element: error
credentials: {}
```

### Long-format-specific rules

| Rule | Why |
|---|---|
| `format: long` requires `long_format: { measure_column, value_column }` (both fields). | Otherwise the loader has no way to find the measure name or scalar — fires MC5019/MC5020 if the named columns are missing from the source schema. |
| Long-format recipes MUST NOT have any `columns:` entries with `measure: X`. | Measures come from the `measure_column`'s **values**, not from column mappings. Both shapes is mutually exclusive — fires **MC5021**. |
| The `measure_column`'s values must be declared measure names in the model. | A row with `Measure: Margin` against a model that has no `Margin` measure fires **MC5022**. |
| Dimension columns are still declared in `columns:` with `dimension: X`. | Same as wide format — they build the coordinate prefix for each row. |

### Phase 5A.1 status (binding for today)

The `format:` and `long_format:` fields are **filed in [ADR-0010 Amendment 2](../../../docs/decisions/0010-amendment-2-long-format-recipe-support.md) but not yet present in the live `mc-recipe` schema**. Emitting them in a recipe today fires **MC5001** (unrecognized field at YAML parse time).

Default to **wide format** for all Phase 5A recipe authoring. If the source data is unambiguously long (row-per-cell with a measure-name column), say so to the user, mention that long-format support lands in 5A.1, and either:

- Emit a wide-format recipe against an alternate (wide) source, OR
- Emit the long-format recipe with a comment marking it as 5A.1-pending.

Don't emit long-format silently when wide is achievable — the LLM should never produce a recipe that fails parse without flagging it.

---

## Per-column transformation fields

These fields live on `columns[i]` entries and refine how the source value reaches the cube.

### `type:` — declared source type (drives MC5006)

Optional. Names the source column's logical type so the validator can compare it against the target measure's `data_type`. Case-insensitive.

| `type:` value | Compatible measure `data_type` |
|---|---|
| `f64` | `F64` (most common — every Acme measure is F64) |
| `i64` | `I64` |
| `string` | `String` |
| `bool` | `Bool` |
| `category` | `String` (string-valued enum-ish) |

```yaml
- { source: spend, measure: Spend, type: f64 }     # Acme Spend is F64 → ✓
- { source: spend, measure: Spend, type: string }  # → MC5006 (incompatible)
```

When in doubt, use `f64` for any Acme measure (all 11 are F64). Omitting `type:` is permitted — it just disables the early type-mismatch check.

### `scale:` — multiplier applied at row-transform time

Optional `f64`. Stream D multiplies the source value by `scale:` before writing. Use for unit conversions:

```yaml
# Source ships spend in cents; cube stores USD:
- { source: spend_cents, measure: Spend, type: f64, scale: 0.01 }

# Source ships click count in thousands; one click per cube unit:
- { source: clicks_k, measure: Clicks_Input_Hypothetical, type: f64, scale: 1000.0 }
```

`scale:` is recorded by the recipe layer (no validation beyond "is it an `f64`?"). The value is opaque to `mc-recipe`.

### `format:` — driver-specific format hint

Optional string. For CSV the canonical use is **date parsing**: a column whose source values are formatted dates that need parsing into a Time-dimension element name.

```yaml
# Source has ISO-8601 dates; Acme Time elements look like "Jan_2026":
- { source: date, dimension: Time, format: "%Y-%m" }

# Source has US-style dates:
- { source: date, dimension: Time, format: "%m/%d/%Y" }
```

`mc-recipe` does **not** parse the format string — it records the value verbatim. The actual parse happens in Stream D's transformation pipeline. If `format:` is set on a column, treat the source value as text-formatted-by-`format:` rather than a literal element name.

For non-date columns (numeric, plain string), omit `format:`.

### `skip:` — explicit drop

Optional `bool`. When `true`, the source column is ignored — the row's value never reaches the cube. Use whenever the CSV has a column that exists for upstream reasons but isn't a cube target:

```yaml
- { source: campaign_id,    skip: true }   # opaque source ID, not a dim
- { source: notes,          skip: true }   # operator notes column
- { source: extracted_at,   skip: true }   # ETL provenance timestamp
```

Without `skip: true`, an unmapped column fires **MC5011**. Set it explicitly so the LLM's intent is visible in the recipe.

---

## Skip patterns (the most common shape)

Real CSVs almost always have columns the cube doesn't need. Default to **map what you need + `skip: true` for everything else**. Don't try to make the CSV match the cube exactly upstream.

```yaml
columns:
  # Mapped (the ones the cube cares about):
  - { source: month,   dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market,  dimension: Market }
  - { source: spend,   measure: Spend, type: f64 }
  - { source: cpc,     measure: CPC,   type: f64 }

  # Explicit skips (everything else in the CSV header):
  - { source: campaign_id,        skip: true }
  - { source: campaign_name,      skip: true }
  - { source: account_id,         skip: true }
  - { source: extracted_at,       skip: true }
  - { source: ingestion_batch_id, skip: true }
```

If you don't know whether an upstream column is going to be mapped, explicitly skip it — better than letting MC5011 fire at validate time.

---

## Type coercion in practice

The recipe layer doesn't actually coerce — it records `type:` and lets Stream D enforce it at write time. But the LLM should still emit `type:` for every measure column so MC5006 catches mismatches early.

Pattern:

| Acme measure | Recipe `type:` |
|---|---|
| `Spend` | `f64` |
| `CPC`, `CVR`, `Close_Rate`, `AOV`, `COGS_Rate` | `f64` (all are ratios stored as F64) |
| `Clicks`, `Leads`, `Customers`, `Revenue`, `Gross_Profit` | **Don't write** — these are Derived (MC5018). |

For dimension columns the `type:` field is generally omitted — dimensions resolve element names through the model, not through type coercion.

---

## Common CSV-shaped pitfalls

These don't fire MC5xxx codes (out of recipe scope) but are real-world failure modes the LLM should warn the user about when authoring a CSV recipe.

### Encoding

Tessera reads files as UTF-8. A CSV exported from Excel as `windows-1252` or `latin-1` will produce decode errors at runtime. Mention to the user: re-export as UTF-8 if you see codec errors.

### Delimiter

Phase 5A's CSV driver uses `,` as the delimiter. TSV (`\t`), `;`, and `|` are not supported. If the user's data uses a different delimiter, ask them to convert upstream — the recipe has no `delimiter:` field.

### Header row

The CSV driver assumes the first row is a header. The `source:` names in `columns:` must match those headers exactly (case-sensitive, including underscores and capitalization). A recipe with `source: Month` against a CSV header `month` will not find that column.

### Quoting and embedded commas

Standard CSV quoting (`"a,b","c"`) works. Hand-rolled CSVs that embed commas in unquoted fields will mis-tokenize. If the user shows you a CSV with addresses or freeform text in a column, recommend quoting before import.

### BOM

UTF-8 BOM (`﻿`) at the start of the file may be included in the first column's name. If `source: Time` doesn't match the header, ask the user to strip the BOM (e.g., `sed -i '1s/^\xef\xbb\xbf//' file.csv`).

These pitfalls are CSV-source problems, not recipe authoring problems. Surface them to the user; don't try to encode them into the recipe.

---

## Worked example — Acme wide-format minimal

The full recipe for "import Spend + CPC from a wide CSV into Acme":

```yaml
version: 1
name: acme_csv_minimal
description: "Minimal CSV import — Spend + CPC into Acme."
model: ../models/acme.yaml

source:
  driver: csv
  path: ./acme.minimal.csv

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

This is `crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml` — the canonical wide-format reference. Treat it as the starting point for any wide-format CSV recipe; copy it and adjust columns to fit the source.

---

## Cross-references

- General recipe schema + the 18 MC5xxx codes: [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md).
- SQL-family drivers (SQLite / DuckDB / Postgres): [`../sql-mapping/SKILL.md`](../sql-mapping/SKILL.md).
- HTTP/JSON driver: [`../api-mapping/SKILL.md`](../api-mapping/SKILL.md).
- Acme reference model (the dim + measure namespace): [`../../domain-schemas/marketing-mix/SKILL.md`](../../domain-schemas/marketing-mix/SKILL.md).
- Long-format spec: [`../../../docs/decisions/0010-amendment-2-long-format-recipe-support.md`](../../../docs/decisions/0010-amendment-2-long-format-recipe-support.md).
- Worked CSV example: `crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml`.
