# ADR-0028: Phase 5D — Tessera XLSX Driver and Layout Descriptors

**Status:** Proposed
**Date:** 2026-05-10
**Deciders:** project owner
**Phase:** 5D (Tessera driver expansion)
**Crate(s) touched:** `mc-drivers` (new driver), `mc-recipe` (schema additions), `mc-tessera` (driver instantiation)
**Prerequisite ADR:** ADR-0010 (Phase 5 Tessera architecture — frozen `SourceDriver` trait)

---

## Context

Mosaic's Tessera ingestion engine currently handles CSV, SQLite, DuckDB, Postgres, HTTP-JSON, MySQL, and D1 REST. The most common enterprise data format it CANNOT handle is **Excel (.xlsx)**.

Real-world evidence: the `email-matchback/scripts/mosaic/flatten_ltd_comparison.py` (238 lines) exists solely to convert an Excel workbook into CSV that Tessera can ingest. This is a 238-line workaround for a missing driver.

The gap is documented in:
- Master gap report M-22 (XLSX driver)
- Master gap report M-23 (year-blocked layout)
- Data-in audit G-OPEN-1, G-OPEN-2

Beyond the XLSX format itself, the recipe schema lacks **layout descriptors** — fields like `skip_rows`, `header_row`, and `sheet` that describe where data starts in a file. These are useful for XLSX and CSV alike.

Phase 5D closes this gap with: (1) an XLSX driver wrapping the `calamine` crate, and (2) layout descriptor fields in the recipe schema.

---

## Decisions

### Decision 1: Add `DriverKind::Xlsx` to the recipe schema

Add a new variant to the `DriverKind` enum in `crates/mc-recipe/src/schema.rs`:

```rust
pub enum DriverKind {
    // ... existing variants ...
    Xlsx,   // Phase 5D
}
```

Recipe YAML usage:
```yaml
source:
  driver: xlsx
  path: data/tide-cleaners-ltd-comparison.xlsx
  sheet: "Q1 2025"           # optional; defaults to first sheet
  skip_rows: 2               # optional; rows to skip before header
  header_row: 0              # optional; 0-based row index of the header (after skip)
```

### Decision 2: Layout descriptor fields on `SourceConfig` (driver-agnostic)

Add three optional fields to `SourceConfig` that apply to **any file-based driver** (CSV and XLSX both benefit):

```rust
pub struct SourceConfig {
    // ... existing fields ...
    pub sheet: Option<String>,          // Sheet name (XLSX only; ignored by CSV)
    pub skip_rows: Option<usize>,       // Rows to skip before the header row
    pub header_row: Option<usize>,      // 0-based index of the header row (after skip)
}
```

**Behavior:**
- `skip_rows: 3` means ignore the first 3 rows entirely (title rows, empty rows, year labels)
- `header_row: 0` (default) means the first row after skipping is the header
- `header_row: 1` means the second row after skipping is the header (first row after skip is also skipped)
- For CSV: `skip_rows` works the same way (skip N lines before parsing the header)
- For XLSX: `sheet` selects which sheet to read; default is the first sheet

**Recipe version:** These fields are additive (all `Option<T>`). Recipe `version: 1` remains valid. No version bump required.

### Decision 3: Use `calamine` crate for XLSX parsing

**Dependency:** `calamine` (pure Rust, MIT licensed, no system dependencies, supports .xlsx and .xls)

**Version pin:** `calamine = "0.26"` (or latest compatible with MSRV Rust 1.78). Verify MSRV compatibility before implementation — if calamine's current release requires Rust 1.85+, pin a compatible older release or use transitive dep pinning (same pattern as the criterion fix in Phase 1B).

**Why calamine:**
- Pure Rust (no C dependencies, no OpenXML system library)
- MIT license (compatible with Mosaic's licensing)
- Actively maintained (2024 releases)
- Supports both .xlsx (Office Open XML) and .xls (legacy binary)
- Read-only (Mosaic never writes Excel; only reads)
- Does NOT evaluate formulas (treats formula cells as their cached value — correct for Tessera's "read source data" role)

**Alternative considered:** `umya-spreadsheet` — heavier, also writes, more dependencies. Calamine is the right choice for read-only ingestion.

### Decision 4: Single sheet per recipe (multi-sheet is recipe chaining)

A recipe targets **one sheet** in a workbook. To ingest multiple sheets from the same file, use multiple recipes.

**Why:** Multi-sheet ingestion within one recipe introduces complexity (different schemas per sheet, different column mappings). Recipe chaining (multiple recipes pointing at the same file with different `sheet:` values) is simpler, more explicit, and composes with the existing orchestrator.

**Example (multi-sheet via chaining):**
```yaml
# recipe-q1.yaml
source:
  driver: xlsx
  path: data/quarterly-report.xlsx
  sheet: "Q1 2025"

# recipe-q2.yaml
source:
  driver: xlsx
  path: data/quarterly-report.xlsx
  sheet: "Q2 2025"
```

### Decision 5: Year-blocked layouts — handled via skip_rows, NOT a new abstraction

The "year-blocked layout" pattern (repeating header+data blocks per year within one sheet) is handled by:

1. **For simple cases:** `skip_rows` to jump past the year label row + previous blocks. Users write one recipe per block.
2. **For complex cases:** Pre-process with DuckDB (already available as a Tessera driver) using SQL to flatten the layout, then feed the flattened output to Tessera.
3. **For automated multi-block parsing:** Deferred to Phase 5E (recipe chaining with computed offsets). Not in scope for Phase 5D.

**Why not a declarative multi-block descriptor:** The year-blocked pattern varies wildly across real Excel files (blank rows between blocks? year in column A vs merged header? variable block heights?). A declarative descriptor that handles 80% of cases would still leave 20% requiring code — and the descriptor's complexity would be substantial. Better to ship `skip_rows` (covers the simple case) and defer the complex case.

### Decision 6: Schema inference (same pattern as CSV driver)

The XLSX driver infers column schemas by sampling the first 100 data rows:
- Columns with all numeric values → `F64`
- Columns with all integer values → `I64`
- Mixed or text columns → `Str`
- Empty columns → `Str` (nullable)

Header row determines column names. Columns with empty headers are named `_col_0`, `_col_1`, etc.

### Decision 7: Merged cells are transparent

Calamine reports merged-cell ranges. The driver treats them as:
- The top-left cell of a merge has the value
- All other cells in the merge are empty/null

This matches "export to CSV" behavior and is predictable. No special merge-awareness logic. Users who need merged-cell semantics must flatten their Excel file before ingestion.

### Decision 8: New diagnostic codes (MC5019–MC5021)

| Code | Severity | Fires when |
|---|---|---|
| MC5019 | Error | Sheet name specified in recipe doesn't exist in workbook |
| MC5020 | Error | `header_row` value exceeds available rows (after skip) |
| MC5021 | Warning | `skip_rows` skips all rows in the sheet (empty data result) |

These extend the MC5xxx range established by Phase 5A (MC5001–MC5018 already allocated across Tessera phases).

### Decision 9: Aggregation transforms (`group_by`) — in scope

Per the MASTER_PHASE_PLAN, Phase 5D also includes **aggregation transforms**:

```yaml
transforms:
  - group_by: [Channel, Market, Time]
    aggregate:
      - source: impressions
        function: sum
      - source: spend
        function: sum
      - source: cpc
        function: weighted_avg
        weight: spend
```

This is a post-fetch, pre-write transform that collapses rows from the source into cube-grain rows. It applies to ANY driver (CSV, XLSX, Postgres, etc.) and addresses the common pattern where source data is at a finer grain than the cube.

**Supported aggregate functions:**
- `sum` — sum of values in group
- `avg` — arithmetic mean
- `min` / `max` — minimum / maximum
- `count` — row count per group
- `weighted_avg` — weighted average (requires `weight` field referencing another column)
- `first` / `last` — first or last value in group (order-dependent; source order preserved)

**Implementation location:** New module `crates/mc-tessera/src/transforms/aggregate.rs`. Runs between `fetch_batch()` and `WriteBatch::push()` in the orchestrator pipeline.

---

## Implementation plan

### Step 1: Add layout fields to recipe schema

In `crates/mc-recipe/src/schema.rs`:
- Add `sheet`, `skip_rows`, `header_row` to `SourceConfig`
- Add `DriverKind::Xlsx` variant
- Add validation for new diagnostic codes (MC5019–MC5021)
- Add `transforms` schema for group_by/aggregate

### Step 2: Implement XLSX driver

Create `crates/mc-drivers/src/xlsx_driver.rs`:
- Constructor: `pub fn xlsx_driver(path: &Path, sheet: Option<&str>, skip_rows: usize, header_row: usize) -> Result<XlsxDriver, DriverError>`
- Implement `SourceDriver` trait (schema inference + batch fetching)
- Handle: sheet selection, row skipping, header parsing, type inference, merged cells
- Error mapping: file not found → `DriverError::SourceFileNotFound`, invalid sheet → new variant or `MalformedSource`

### Step 3: Wire driver in mc-tessera

In `crates/mc-tessera/src/prepare.rs`:
- Add match arm for `DriverKind::Xlsx` → call `xlsx_driver()`
- Pass `sheet`, `skip_rows`, `header_row` from recipe config

### Step 4: Add skip_rows/header_row support to CSV driver

Modify `crates/mc-drivers/src/csv_driver.rs`:
- Accept optional `skip_rows` and `header_row` parameters
- Skip N rows before parsing header
- This gives CSV the same layout capabilities as XLSX

### Step 5: Implement aggregate transforms

Create `crates/mc-tessera/src/transforms/aggregate.rs`:
- Accept `group_by` field names + aggregate specs
- Buffer rows until group boundary (or end of batch)
- Emit one aggregated row per group
- Wire into the orchestrator pipeline between fetch and write

### Step 6: Integration tests

- XLSX with default settings (first sheet, header in row 0)
- XLSX with explicit sheet name (valid + invalid → MC5019)
- XLSX with skip_rows + header_row
- XLSX with empty sheet → MC5021
- CSV with skip_rows (regression: existing CSV behavior unchanged when skip_rows absent)
- Aggregate transform: sum, avg, weighted_avg on grouped data
- End-to-end: XLSX → aggregate → WriteBatch → verify cube cell values

### Step 7: Convert flatten_ltd_comparison.py to Tessera recipe

Prove the driver works by replacing the 238-line Python workaround with a Tessera recipe:

```yaml
version: 1
name: tide_cleaners_q1_2025
source:
  driver: xlsx
  path: data/Tide Cleaners - LTD Comparison.xlsx
  sheet: "2025"
  skip_rows: 1        # skip year label row
  header_row: 0       # first row after skip is the header
columns:
  - source: Date
    dimension: Time
    time_format: "%m/%d/%Y"
    map_to_period: month
  # ... remaining column mappings
```

---

## Acceptance criteria

1. `DriverKind::Xlsx` is a valid driver in recipe YAML
2. XLSX files load correctly (single sheet, header detection, type inference)
3. `sheet:` field selects a named sheet; invalid name → MC5019
4. `skip_rows:` and `header_row:` work for both XLSX and CSV drivers
5. Merged cells are transparent (top-left has value; rest are null)
6. `group_by` + `aggregate` transforms collapse rows correctly
7. `weighted_avg` aggregate produces correct weighted averages
8. The 238-line Python workaround is replaceable by a Tessera recipe
9. Calamine dependency is compatible with MSRV Rust 1.78 (verified)
10. All existing Tessera tests pass unchanged (no regression)
11. New MC5019–MC5021 diagnostics fire correctly
12. `cargo test --workspace` passes
13. `cargo clippy --all-targets --workspace -- -D warnings` passes
14. No changes to `mc-core`

---

## Alternatives considered

### Alt 1: Use DuckDB's Excel extension instead of calamine

Considered. DuckDB has a `spatial` extension that can read Excel files.

**Rejected because:**
- Requires DuckDB as a transitive dependency for XLSX (over-couples)
- DuckDB's Excel support is an extension, not core — may lag or break
- calamine is purpose-built for reading Excel; DuckDB Excel is a convenience feature
- Users who want DuckDB's approach can already write `driver: duckdb` with an Excel-reading query

### Alt 2: Use `openpyxl` via Python subprocess

Considered. Shell out to Python for XLSX parsing.

**Rejected because:**
- Introduces Python as a runtime dependency for a core Tessera operation
- Subprocess I/O is slow and error-prone
- The whole point of Phase 5D is to eliminate the Python workaround

### Alt 3: Declarative multi-block layout descriptor

Considered. A `blocks:` field in the recipe that describes repeating row patterns.

**Rejected because:**
- Year-blocked patterns vary wildly (blank rows, year labels, variable block heights)
- A descriptor complex enough to handle real files would be harder to author than a DuckDB query
- `skip_rows` handles the simple case; recipe chaining handles multiple blocks
- Deferred to Phase 5E if customer demand surfaces

### Alt 4: Multi-sheet ingestion in one recipe

Considered. A recipe could target multiple sheets with per-sheet column mappings.

**Rejected because:**
- Different sheets typically have different schemas → need different column mappings
- Multiple recipes (one per sheet) are explicit, simple, and compose with the existing orchestrator
- No loss of functionality; just more recipe files

---

## Out of scope

- Multi-sheet ingestion in one recipe (use recipe chaining)
- Declarative multi-block layout descriptors (Phase 5E if needed)
- Excel formula evaluation (calamine reads cached values; formulas are opaque)
- .xlsb format (binary Excel; rare; add if requested)
- Writing Excel files (Mosaic never writes back to Excel)
- Password-protected workbooks (calamine has limited support; defer)
- Streaming large workbooks >500MB (calamine loads in memory; sufficient for planning data)

---

## Dependencies

**New external dependency:**
- `calamine = "0.26"` (or latest MSRV-1.78-compatible version) — added to `crates/mc-drivers/Cargo.toml`
- Pure Rust, MIT license, no system deps
- MSRV verification required before implementation

**No other new dependencies.** Aggregate transforms use std collections only.

---

## Cross-links

- **ADR-0010:** Phase 5 Tessera architecture (frozen `SourceDriver` trait, recipe schema, driver instantiation)
- **ADR-0010 Appendix C:** `SourceDriver` trait contract (implement exactly)
- **ADR-0014:** Time representation (date parsing in `time_format` field)
- **Master gap report:** M-22 (XLSX driver), M-23 (year-blocked layout)
- **Data-in audit:** G-OPEN-1, G-OPEN-2
- **Existing workaround:** `email-matchback/scripts/mosaic/flatten_ltd_comparison.py` (238 lines to be replaced)
- **MASTER_PHASE_PLAN:** Phase 5D row
- **CLAUDE.md §1:** Allowed dependencies — calamine needs to be added to the permitted list for mc-drivers

---

## Notes

**Why XLSX matters disproportionately.** Most enterprise planning data starts life in Excel. Budget templates, media plans, financial models, client reports — they're all .xlsx. Every competitor (TM1, Anaplan, Cube.dev) can read Excel. Mosaic's inability to do so is a gap that forces users into Python workarounds or manual CSV exports. Closing this gap removes the most common friction point for new users.

**The layout descriptors are the real win.** `skip_rows` and `header_row` apply to CSV too. Many real CSV files have title rows, blank rows, or metadata rows before the header. Today users must manually strip these. After Phase 5D, a recipe handles it declaratively. This improves the experience for ALL file-based ingestion, not just XLSX.

**Aggregate transforms complete the Tessera story.** Source data is often at a finer grain than the cube (daily data into monthly cubes, transaction-level into account-level). Without aggregation, users must pre-aggregate externally. The `group_by` transform closes this gap at the recipe level, eliminating another class of Python/SQL workarounds.
