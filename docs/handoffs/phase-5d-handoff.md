# Phase 5D Handoff — Tessera XLSX Driver + Layout Descriptors

**Status:** Proposed (next to start)
**Date:** 2026-05-10
**ADR:** [ADR-0028](../decisions/0028-phase-5d-tessera-xlsx-driver.md) (Proposed — accept before implementation)
**Predecessor:** Phase 5C (complete), Phase 4C (complete)
**Estimated effort:** 3–4 sessions
**Crate(s) touched:** `mc-drivers` (new driver), `mc-recipe` (schema additions), `mc-tessera` (wire driver), `mc-cli` (no changes expected)

---

## What this phase does

Add XLSX file reading to Tessera via the `calamine` crate, plus layout descriptor fields (`skip_rows`, `header_row`, `sheet`) that benefit both XLSX and CSV. After this phase, users can write:

```yaml
source:
  driver: xlsx
  path: data/quarterly-report.xlsx
  sheet: "Q1 2025"
  skip_rows: 2
  header_row: 0
```

---

## Scope

### Build

1. **`DriverKind::Xlsx`** variant in `crates/mc-recipe/src/schema.rs`
2. **Layout fields** on `SourceConfig`: `sheet: Option<String>`, `skip_rows: Option<usize>`, `header_row: Option<usize>`
3. **XLSX driver** at `crates/mc-drivers/src/xlsx_driver.rs` implementing `SourceDriver` trait
4. **Shared schema inference** utility (extract from CSV driver, reuse in XLSX)
5. **CSV driver update** to respect `skip_rows` and `header_row` when present
6. **Wire** `DriverKind::Xlsx` in `crates/mc-tessera/src/prepare.rs`
7. **Diagnostics** MC5019 (invalid sheet), MC5020 (header out of bounds), MC5021 (skip exceeds rows)

### Do NOT build

- Aggregate transforms (`group_by`) — split to Phase 5D.1 / ADR-0029
- Multi-sheet in one recipe — use recipe chaining
- Merged-cell awareness — transparent (top-left has value; rest null)
- Formula evaluation — calamine reads cached values only
- Password-protected workbooks
- .xlsb format

---

## Key dependency

**`calamine = "0.30.1"`** — pinned. MSRV 1.75, compatible with our Rust 1.78. Add to `crates/mc-drivers/Cargo.toml`:

```toml
[dependencies]
calamine = "=0.30.1"
```

---

## Implementation path

### Step 1: Add layout fields to recipe schema

File: `crates/mc-recipe/src/schema.rs`

```rust
pub struct SourceConfig {
    // ... existing fields ...
    pub sheet: Option<String>,
    pub skip_rows: Option<usize>,
    pub header_row: Option<usize>,
}
```

Add `Xlsx` to `DriverKind` enum. These are additive optional fields — recipe `version: 1` stays valid.

### Step 2: Extract shared schema inference utility

Currently the CSV driver has type-inference logic (sample N rows → determine F64/I64/Str per column). Extract this into a shared module:

File: `crates/mc-drivers/src/infer.rs` (new)

```rust
/// Sample up to `max_rows` of data and infer column types.
/// Returns Vec<ColumnSchema> with inferred types and nullability.
pub fn infer_schema(
    headers: &[String],
    sample_rows: &[Vec<Option<String>>],  // raw string values
) -> Vec<ColumnSchema> {
    // For each column: try I64 → F64 → Str (same logic as CSV today)
}
```

Then both `csv_driver.rs` and `xlsx_driver.rs` call `infer::infer_schema()`.

### Step 3: Implement XLSX driver

File: `crates/mc-drivers/src/xlsx_driver.rs` (new)

```rust
use calamine::{open_workbook, Reader, Xlsx, DataType};

pub struct XlsxDriver {
    rows: Vec<Vec<Option<String>>>,  // all data rows (after skip + header)
    schema: Vec<ColumnSchema>,
    cursor: usize,
    cancelled: bool,
}

pub fn xlsx_driver(
    path: &Path,
    sheet: Option<&str>,
    skip_rows: usize,
    header_row: usize,
) -> Result<XlsxDriver, DriverError> {
    // 1. Open workbook with calamine
    // 2. Select sheet (by name, or first if None)
    //    → MC5019 if sheet name doesn't exist
    // 3. Read all rows into Vec<Vec<DataType>>
    // 4. Apply skip_rows (drop first N rows)
    //    → MC5021 if skip_rows >= total rows
    // 5. Extract header row at offset header_row
    //    → MC5020 if header_row >= remaining rows
    //    → Empty header cells get _col_N names
    // 6. Remaining rows = data rows
    // 7. Convert DataType cells to Option<String> for schema inference
    // 8. Call infer::infer_schema() to determine column types
    // 9. Return XlsxDriver with rows + schema
}

impl SourceDriver for XlsxDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Ok(self.schema.clone())
    }

    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.cancelled || self.cursor >= self.rows.len() {
            return Ok(None);
        }
        // Slice rows[cursor..cursor+max_rows]
        // Convert to ColumnData (typed columns)
        // Advance cursor
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
```

**Note on memory:** Calamine loads the entire sheet into memory. This is fine for planning data (typical sheets < 100K rows, < 50MB). For streaming large workbooks, a future enhancement could use calamine's `rows()` iterator — but for Phase 5D, load-all-then-serve is correct and simple.

### Step 4: Update CSV driver with layout fields

File: `crates/mc-drivers/src/csv_driver.rs`

Add `skip_rows` and `header_row` parameters to the CSV driver constructor. When present:
- Skip N lines before parsing
- Header row is at the specified offset (after skip)
- When absent (None): behavior is identical to today (header = first line, no skip)

**Critical:** Existing behavior must be unchanged when these fields are absent. Add a regression test proving this.

### Step 5: Wire in mc-tessera

File: `crates/mc-tessera/src/prepare.rs`

Add match arm:
```rust
DriverKind::Xlsx => {
    let skip = recipe.source.skip_rows.unwrap_or(0);
    let header = recipe.source.header_row.unwrap_or(0);
    let sheet = recipe.source.sheet.as_deref();
    xlsx_driver(&path, sheet, skip, header)?
}
```

Also update the CSV arm to pass `skip_rows` and `header_row` if present.

### Step 6: Tests

**Unit tests (in mc-drivers):**
- XLSX loads with default settings (first sheet, header row 0)
- XLSX with explicit valid sheet name
- XLSX with invalid sheet name → MC5019 error
- XLSX with skip_rows + header_row
- XLSX with skip_rows exceeding row count → MC5021
- XLSX with header_row out of bounds → MC5020
- XLSX with merged cells (verify top-left has value, others null)
- XLSX with empty header cells → `_col_N` names
- Schema inference: numeric, string, mixed columns
- CSV with skip_rows (verify behavior matches XLSX)
- CSV without skip_rows (regression: identical to today)

**Integration tests (in mc-tessera or mc-cli):**
- Full pipeline: XLSX recipe → load → column mapping → WriteBatch → verify cube values
- Real workbook test (subset of Tide Cleaners data if available, or synthetic)

### Step 7: Test fixture

Create a small test XLSX file at `crates/mc-drivers/tests/fixtures/test_data.xlsx`:
- Sheet 1 "Sales": header + 10 rows, 4 columns (Date, Channel, Revenue, Units)
- Sheet 2 "Costs": header + 5 rows, 3 columns (Date, Category, Amount)
- Some merged cells in Sheet 1 header (to test transparent handling)
- A title row before the header (to test skip_rows)

Generate this file with a small Python script in `tests/fixtures/generate_test_xlsx.py` (run once, commit the .xlsx). Or use a pre-built fixture committed directly.

---

## Diagnostic codes

| Code | Severity | When |
|---|---|---|
| MC5019 | Error | Sheet name in recipe doesn't exist in workbook |
| MC5020 | Error | `header_row` exceeds available rows after skip |
| MC5021 | Warning | `skip_rows` skips all rows (empty data result) |

Verify these codes are unallocated against current main before implementing.

---

## Acceptance criteria

1. `driver: xlsx` works in recipe YAML
2. XLSX files load correctly (single sheet, type inference, column mapping)
3. `sheet:` selects named sheet; invalid → MC5019
4. `skip_rows:` + `header_row:` work for XLSX and CSV
5. Merged cells transparent (top-left value; rest null; `_col_N` for empty headers)
6. Existing CSV recipes produce identical output (regression test)
7. Schema inference is shared between CSV and XLSX (one implementation)
8. `calamine = "0.30.1"` compiles at Rust 1.78
9. All existing Tessera tests unchanged
10. MC5019–MC5021 fire correctly
11. `cargo test --workspace` passes
12. `cargo clippy --all-targets --workspace -- -D warnings` passes
13. No changes to `mc-core`

---

## Cross-links

- **ADR-0028:** Binding decisions for this phase
- **ADR-0010:** Phase 5 Tessera architecture (frozen SourceDriver trait)
- **CSV driver:** `crates/mc-drivers/src/csv_driver.rs` (reference implementation + shared inference source)
- **Tessera orchestrator:** `crates/mc-tessera/src/prepare.rs` (driver instantiation site)
- **Recipe schema:** `crates/mc-recipe/src/schema.rs` (DriverKind enum + SourceConfig)

---

**End of handoff. Clean focused scope: one new driver + three layout fields + shared inference. No aggregation transforms (that's ADR-0029).**
