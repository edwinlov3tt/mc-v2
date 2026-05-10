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

**Version pin:** `calamine = "0.30.1"` — verified MSRV-compatible. Calamine 0.31+ requires Rust 1.83; our MSRV is 1.78. Version 0.30.1 (MSRV 1.75) is the latest compatible release. MSRV verified 2026-05-10 via crates.io metadata.

**Why calamine:**
- Pure Rust (no C dependencies, no OpenXML system library)
- MIT license (compatible with Mosaic's licensing)
- Supports both .xlsx (Office Open XML) and .xls (legacy binary)
- Read-only (Mosaic never writes Excel; only reads)
- Does NOT evaluate formulas (treats formula cells as their cached value — correct for Tessera's "read source data" role)
- v0.30.1 has full xlsx reading, sheet selection, cell type detection — all we need

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

### Decision 7: Merged cells are transparent (with header caveat)

Calamine reports merged-cell ranges. The driver treats them as:
- The top-left cell of a merge has the value
- All other cells in the merge are empty/null

This matches "export to CSV" behavior and is predictable. No special merge-awareness logic.

**Merged-header caveat:** Users with merged header rows (common in Excel — e.g., "Q1 2025" spanning 3 columns) will get empty column names for the right-side cells of the merge. These columns receive auto-generated names per Decision 6's `_col_N` pattern. **Merged headers are not supported; use a flat single-row header.** Document this in the diagnostic message when columns receive `_col_N` names — suggest the user check for merged cells in the header row.

Users who need merged-header support must either:
- Flatten their Excel headers manually before ingestion
- Use `skip_rows` to jump past the merged header and point `header_row` at a flat row below

### Decision 8: New diagnostic codes (MC5019–MC5021)

| Code | Severity | Fires when |
|---|---|---|
| MC5019 | Error | Sheet name specified in recipe doesn't exist in workbook |
| MC5020 | Error | `header_row` value exceeds available rows (after skip) |
| MC5021 | Warning | `skip_rows` skips all rows in the sheet (empty data result) |

These extend the MC5xxx range established by Phase 5A (MC5001–MC5018 already allocated across Tessera phases).

### Decision 9: Aggregation transforms are OUT OF SCOPE (split to ADR-0029)

Per Claude Desktop review, aggregate transforms (`group_by` + aggregate functions) are a distinct architectural feature with unresolved design questions:
- Cross-batch group semantics (unbounded memory if groups span batches)
- `weighted_avg` edge cases (null/zero/negative weights)
- `first`/`last` ordering semantics (non-deterministic for SQL sources without ORDER BY)
- Diagnostic codes for aggregation errors
- Interaction with column mapping (aggregate before or after mapping?)
- Testing surface (8 functions × multiple edge cases)

**Decision:** Split aggregate transforms into a future ADR-0029 (Phase 5D.1 or 5E). Phase 5D ships the XLSX driver + layout descriptors only. This follows the Phase 3H pattern (3H.1 split from 3H.2 when scope grew).

**Consequence for the Python workaround:** The `flatten_ltd_comparison.py` script (238 lines) does conditional logic (channel/scenario inference from cell content) and duplicate coalescing that are NOT expressible in a Tessera recipe even with aggregate transforms. The XLSX driver eliminates the openpyxl dependency and file-reading boilerplate, but the conditional business logic remains a script concern until conditional transforms are designed. See "Honest assessment" in Notes.

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

### Step 5: Integration tests

- XLSX with default settings (first sheet, header in row 0)
- XLSX with explicit sheet name (valid + invalid → MC5019)
- XLSX with skip_rows + header_row
- XLSX with empty sheet → MC5021
- XLSX with merged header → `_col_N` names generated (verify no crash)
- CSV with skip_rows (regression: existing CSV behavior unchanged when skip_rows absent)
- CSV without skip_rows (regression: produces identical output to today)
- End-to-end: XLSX → column mapping → WriteBatch → verify cube cell values

### Step 6: Verify existing Tessera tests unchanged

All existing recipes and tests must produce identical results. Adding optional fields to SourceConfig must not change behavior when those fields are absent.

### Step 7: Prove the driver works on real data

Test against a real multi-sheet XLSX file (e.g., one sheet from the Tide Cleaners workbook):

```yaml
version: 1
name: tide_cleaners_houston_2025
source:
  driver: xlsx
  path: data/Tide Cleaners - LTD Comparison.xlsx
  sheet: "Houston"
  skip_rows: 9        # skip to 2025 block (rows 0-8 are 2024)
  header_row: 0       # first row after skip is the 2025 header
columns:
  - source: "Ad Spend"
    measure: AdSpend
    type: f64
  - source: "Matched Revenue"
    measure: MatchedRevenue
    type: f64
  # ... remaining measures
defaults:
  Market: Houston
  Channel: DirectMail
  Scenario: Actual
  Version: Working
```

Note: This recipe handles ONE sheet + ONE year-block. The full Python script's conditional logic (channel inference, scenario inference, duplicate coalescing) requires either aggregate transforms (ADR-0029) or a simplified source file. The goal of this step is to prove the XLSX driver reads correctly, not to fully replace the script.

---

## Acceptance criteria

1. `DriverKind::Xlsx` is a valid driver in recipe YAML
2. XLSX files load correctly (single sheet, header detection, type inference)
3. `sheet:` field selects a named sheet; invalid name → MC5019
4. `skip_rows:` and `header_row:` work for both XLSX and CSV drivers
5. Merged cells are transparent (top-left has value; rest are null; `_col_N` for empty headers)
6. Existing CSV recipes WITHOUT layout fields produce identical output to today (regression test)
7. `calamine = "0.30.1"` compiles cleanly at MSRV Rust 1.78 (verified pre-acceptance)
8. All existing Tessera tests pass unchanged (no regression)
9. New MC5019–MC5021 diagnostics fire correctly
10. Schema inference is shared between CSV and XLSX (no duplicate implementation)
11. `cargo test --workspace` passes
12. `cargo clippy --all-targets --workspace -- -D warnings` passes
13. No changes to `mc-core`

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
- `calamine = "0.30.1"` — added to `crates/mc-drivers/Cargo.toml`
- Pure Rust, MIT license, no system deps
- MSRV: 1.75 (verified compatible with our 1.78; latest calamine 0.31+ requires 1.83)

**No other new dependencies.**

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

**Honest assessment of the Python script replacement.** The `flatten_ltd_comparison.py` (238 lines) does significantly more than read Excel:
- Multi-sheet processing (4 market sheets)
- Year-block parsing with hardcoded row offsets
- Two-pass processing (first pass: determine channel + scenario; second pass: emit rows)
- Conditional channel inference ("Added Value" string → AddedValue channel)
- Conditional scenario inference (MatchedRevenue present → Actual, absent → Plan)
- Duplicate coalescing (multi-window drops in same month → sum)
- Measure name mapping and junk filtering

Phase 5D's XLSX driver eliminates the openpyxl dependency and handles the file-reading + year-block-skipping + sheet-selection portion (~40% of the script). The conditional business logic (channel/scenario inference from cell content) is NOT expressible in a declarative Tessera recipe. Full script replacement requires either:
- ADR-0029 aggregate transforms + conditional column mapping (future)
- A DuckDB pre-transform step (already possible but requires SQL expertise)
- Accepting that this specific script is partially-but-not-fully replaceable

The ADR does NOT claim "Phase 5D fully replaces the 238-line script." It claims "Phase 5D eliminates the XLSX-reading friction and handles the common case (single sheet, flat header, direct column mapping)."

**Schema inference should be shared code.** Per Claude Desktop review: the type inference logic (sample N rows, determine column types) should be factored into a shared utility in `mc-drivers`, not duplicated between CSV and XLSX implementations. Two implementations will drift.
