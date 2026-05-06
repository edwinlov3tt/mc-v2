# Data-In Audit — Phase 6A.1 Gap Analysis

## Reviewer: Claude Sonnet 4.6
## Date: 2026-05-06
## Scope: mc-recipe schema, mc-drivers (11 drivers), mc-tessera (transform, prepare, incremental, time_format), email-matchback Python ingestion scripts (flatten_ltd_comparison.py, build_ltv_cohort.py, prepare_v2_inputs.py, prepare_mmm_inputs.py), raw data shapes in email-matchback/data/, model YAML files in email-matchback/models/

---

## Closed by 6A/6A.1 (verification)

### G-CLOSED-1: `time_format` declared but never consumed at row-transform time
**Was:** `time_format` and `map_to_period` fields existed on `ColumnMapping` and MC5030 validated them at recipe-parse time, but `mc-tessera/src/transform.rs` did not call any parse logic — the raw string was passed directly to `refs.element()`. Non-ISO date columns silently failed element lookup on every row.
**Now:** `crates/mc-tessera/src/time_format.rs` implements a hand-rolled strptime subset (`%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%V`, `%b`, `%%`). `transform.rs:221` calls `maybe_canonicalize_time()` for every dimension column with `is_time_dim=true` and a `time_format` set; the function parses the raw value and canonicalizes via `canonicalize_period()` before name lookup. Wide-format (`transform_batch`) and long-format (`transform_batch_long`) both apply the fix.
**Evidence:** `crates/mc-tessera/src/transform.rs:218–235` (wide), `608–621` (long); `crates/mc-tessera/src/time_format.rs:59–176`.

### G-CLOSED-2: Long-format source support (wide-only in Phase 5A)
**Was:** `source.format: long` was defined in the schema but the transformer only had `transform_batch` (wide). The email-matchback models (`tide-matchback.yaml`, `tide-mmm.yaml`, `tide-ltv-cohort.yaml`) all use long-format inputs — each row is one cell with a Measure name column.
**Now:** `transform_batch_long` / `transform_batch_long_inner` are fully implemented in `crates/mc-tessera/src/transform.rs:431–795`. `SourceFormat::Long` is handled at the runner level; `long_format.measure_column` and `long_format.value_column` are used to dispatch to the long-format path.
**Evidence:** `crates/mc-recipe/src/schema.rs:156–180` (schema); `crates/mc-tessera/src/transform.rs:431`.

### G-CLOSED-3: `on_missing_element: create` deferred to Phase 5C (now shipped in Phase 5C)
**Was:** Phase 5A shipped `on_missing_element: error` only. `create` variant was defined in the enum but `OnMissingElement::Create` was not handled in the transformer.
**Now:** `transform.rs:239–244` handles `OnMissingElement::Create` by assigning a dynamic `ElementId` starting at `DYNAMIC_ELEMENT_BASE = 10_000_000` and inserting into `refs.elements`. Available in both wide and long format paths.
**Evidence:** `crates/mc-tessera/src/transform.rs:46–47, 238–245`.

### G-CLOSED-4: Phase 5C incremental loads (watermark / cursor)
**Was:** `incremental: true` was accepted by the recipe schema but the state-tracking machinery was unimplemented.
**Now:** `crates/mc-tessera/src/incremental.rs` implements `load_state`, `save_state`, `inject_watermark`, and `compute_new_watermark`. State is persisted to `.tessera/incremental/<recipe_name>.state.json`. Watermark injection supports both `{{watermark}}` template substitution and automatic `WHERE column > 'last_value'` append.
**Evidence:** `crates/mc-tessera/src/incremental.rs:60–189`.

### G-CLOSED-5: Phase 5C additional drivers (MySQL, D1, Snowflake, SQL Server, BigQuery)
**Was:** Phase 5A shipped CSV, SQLite, DuckDB, Postgres, DuckDB-Postgres, HTTP-JSON only.
**Now:** `crates/mc-drivers/src/` contains `mysql_driver.rs`, `d1_driver.rs`, `snowflake_driver.rs`, `sqlserver_driver.rs`, `bigquery_driver.rs`; all wired into `DriverKind` enum and the `construct_driver` dispatch in `crates/mc-tessera/src/prepare.rs:441–548`.
**Evidence:** `crates/mc-recipe/src/schema.rs:197–210` (DriverKind enum listing all 11 variants).

---

## Open gaps — clear path

### G-OPEN-1: No Excel/xlsx driver — the primary real-world source format
**Use case:** The biggest real-world input for Tide Cleaners is "Tide Cleaners - LTD Comparison.xlsx" — a multi-sheet, year-blocked Excel workbook. This is the source that drove 220 lines of Python (`flatten_ltd_comparison.py`).
**Evidence (Python):** `email-matchback/scripts/mosaic/flatten_ltd_comparison.py:1–238` — the entire file exists only to read `.xlsx` via `openpyxl`. The raw data never touches Tessera; Python pre-processes it to `data/ltd-comparison-long.csv` which then feeds the model.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs:184–210` — `DriverKind` enum has `Csv`, `Sqlite`, `Duckdb`, `Postgres`, `DuckdbPostgres`, `HttpJson`, `Mysql`, `D1`, `Snowflake`, `Sqlserver`, `Bigquery` — no `Xlsx`, `Excel`, or `Spreadsheet` variant.
**Impact:**
  - Lines of Python eliminated: ~220 (all of `flatten_ltd_comparison.py`), plus substantial portions of `prepare_v2_inputs.py` that process its output.
  - Other affected use cases: any FP&A, demand planning, or HR planning user whose data lives in Excel (the majority of enterprise planning data). DuckDB can read xlsx via its Excel extension, but that requires users to know DuckDB — not a recipe author's UX.
**Proposed fix shape:** Add a `DriverKind::Xlsx` that wraps a pure-Rust xlsx reader (e.g., `calamine` crate, MIT-licensed, no system deps). The recipe schema gains `source.sheet: "Houston"` (optional; defaults to first sheet) and `source.skip_rows: 3` (to skip year-label rows). Single-sheet reads with a named header row would cover ~80% of real Excel ingestion.
**Phase mapping:** Phase 5D (Document/OCR Ingestion) is the closest placeholder, but xlsx is not OCR — it is a structured format. Alternatively a Phase 5C amendment or a new Phase 5C.1 driver-expansion sub-phase. Needs scoping before coding.

### G-OPEN-2: Year-blocked / banded layout not expressible in any recipe
**Use case:** The LTD Comparison workbook uses a "year block" layout: for each sheet (market), there are three year-blocks (2024, 2025, 2026) each with a year-label row, then a header row of drop-date column labels, then measure-value rows. The header is not row 1 of the sheet, and there are multiple headers in a single sheet. No CSV driver supports this.
**Evidence (Python):** `email-matchback/scripts/mosaic/flatten_ltd_comparison.py:37` — `YEAR_BLOCKS = {2024: (1, 2, 8), 2025: (9, 10, 17), 2026: (18, 19, 26)}` — hardcoded row offsets for three header-location blocks.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs:114–153` — `SourceConfig` has only `path`, `query`, `table`, `url`, `json_path`, `format`, `long_format`. No `skip_rows`, `header_row`, `sheet`, `row_blocks`, or `layout_descriptor` fields.
**Impact:**
  - Lines of Python eliminated: ~220 (`flatten_ltd_comparison.py` plus ~30 lines of header-parsing in `build_ltv_cohort.py:109–116`).
  - Other affected use cases: government data releases, financial reporting (SEC filings), retail planners who receive "banded" trade plans where each brand/SKU has its own header block.
**Proposed fix shape:** A `layout:` block in `SourceConfig` with fields: `skip_rows: N` (skip N rows before the header), `header_row: N` (0-based), and `data_start_row: N`. Multi-block layouts (multiple header blocks per sheet) likely require a pre-transform step or a recipe chaining mechanism (see G-DESIGN-1) rather than a declarative layout descriptor.
**Phase mapping:** Phase 5D (Document/OCR placeholder) or Phase 5C amendment. Needs ADR.

### G-OPEN-3: No `map_to_period` week-from-date computation (ISO week requires explicit `%V`)
**Use case:** `flatten_ltd_comparison.py` receives drop-date column headers like `"8/6 * 8/13"` (two week ranges within a month) and extracts just the month number. If a future user had weekly data, they'd need Tessera to compute ISO week number from `%Y-%m-%d` automatically.
**Evidence (Python):** `flatten_ltd_comparison.py:70–75` — regex extracts the leading `month` from `"8/6 * 8/13"` strings; no week-number computation is attempted.
**Evidence (Mosaic absence):** `crates/mc-tessera/src/time_format.rs:263–293` — `canonicalize_period` with `"week"` period requires `parts.iso_week` (from `%V` token); if `%V` was not parsed, returns `None`. The module doc at line 269 explicitly states: "ISO-week computation from a y/m/d triple is intentionally not implemented in Phase 6A.1 — recipes that need week-bucketing must include `%V` in their `time_format`."
**Impact:**
  - Lines of Python eliminated: ~20 (week-extraction pattern).
  - Other affected use cases: retail weekly planning (almost universally uses ISO weeks), media/advertising (weekly impression data), supply chain (52-week horizons).
**Proposed fix shape:** Add ISO-week-from-date computation in `canonicalize_period`: when `period="week"` and `parts.iso_week` is `None` but `parts.year`, `parts.month`, and `parts.day` are all present, compute the ISO week number using the standard algorithm (Jan 4 rule). This is deterministic and has no external dependencies.
**Phase mapping:** Fits naturally in a Phase 6A.2 polish pass; no ADR required for a 1-function addition.

### G-OPEN-4: No carry-forward / last-observation-carry-forward (LOCF) transform in recipes
**Use case:** Houston's spreadsheet leaves Nov-Dec 2026 AdSpend blank. `prepare_v2_inputs.py` detects the gap and carries forward the last observed spend. This pattern ("assume next period equals last period unless overridden") is pervasive in FP&A and demand planning.
**Evidence (Python):** `email-matchback/scripts/mosaic/prepare_v2_inputs.py:155–176` — explicit LOCF loop: finds the latest Actual AdSpend time entry, then for missing months `M_2026_11` / `M_2026_12` appends a new row with the same value. `prepare_mmm_inputs.py:46–65` has identical logic.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs` — no `fill_missing:`, `interpolate:`, or `carry_forward:` transform directives anywhere. The recipe format is a pure extract-and-map layer with no row-generation capability.
**Impact:**
  - Lines of Python eliminated: ~25 (LOCF loops in both scripts).
  - Other affected use cases: any forecast that assumes "most recent actuals carry forward until overridden" — extremely common in FP&A, HR headcount planning, and subscription revenue models.
**Proposed fix shape:** A `fill_missing:` directive on a recipe or dimension mapping specifying `strategy: carry_forward` and `scope: dimension` (e.g., carry forward within each (Market, Channel) combination across the Time dimension). This is a post-fetch, pre-write transform, not a source-side operation.
**Phase mapping:** Needs ADR; fits in Phase 5C or a new Phase 5D pre-transform sub-feature.

### G-OPEN-5: Plan→Actual mirroring requires Python — no cross-scenario row duplication in recipes
**Use case:** Forecast months (Apr-Dec 2026) have AdSpend in `Scenario=Plan` only. The unified-revenue formula evaluates at `Scenario=Actual`. To make the formula see planned spend, `prepare_v2_inputs.py` duplicates each Plan AdSpend row and writes it as Actual.
**Evidence (Python):** `email-matchback/scripts/mosaic/prepare_v2_inputs.py:141–152` — `mirror Plan→Actual AdSpend rows for forecast months (only where missing)`. `prepare_mmm_inputs.py:36–51` — identical mirroring.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs` — no `mirror:`, `duplicate_to:`, or cross-scenario projection directive. The recipe writes each source row exactly once.
**Impact:**
  - Lines of Python eliminated: ~25 (mirroring loops in two scripts).
  - Other affected use cases: any model where actuals arrive with a different "scenario tag" than what the formula evaluator reads — common in systems where actuals and budgets come from different source systems with different scenario encodings.
**Proposed fix shape:** A `row_transform: mirror_dimension` recipe directive that, for specified rows, also writes a copy with a specific dimension element substituted (e.g., `mirror: { dimension: Scenario, from: Plan, to: Actual, measures: [AdSpend] }`). Alternatively this belongs in the engine as a formula: an `actual_ref`-like `plan_ref()` that reads across the scenario boundary in the other direction.
**Phase mapping:** Needs ADR. Could be a recipe transform (Phase 5C) or a formula addition (Phase 3J).

### G-OPEN-6: Q1 anchor broadcast — per-leaf constant replication is Python-side work
**Use case:** `prepare_v2_inputs.py` computes Q1-2026 per-dollar anchor constants (one per market × measure pair) and then broadcasts each constant to EVERY time leaf, generating hundreds of rows. This is equivalent to a `lookup_table` with a constant value, but the current recipe format cannot generate rows — it only maps existing source rows.
**Evidence (Python):** `email-matchback/scripts/mosaic/prepare_v2_inputs.py:181–193` — double loop over `(market, measure)` anchors × all time leaves. With 5 markets × 5 measures × 29 time leaves = 725 anchor rows generated entirely in Python. Comment at line 18: "Q1 anchors are constants (broadcast to every Time leaf so the formula reads them locally)."
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs` — no `expand:` or `broadcast:` directive. All recipe column mappings are 1:1 from source rows.
**Impact:**
  - Lines of Python eliminated: ~15 (anchor broadcast loop).
  - Other affected use cases: any model that needs a scalar constant replicated across a dimension (seasonality indices before `lookup_tables` existed, inflation factors, capacity constraints that apply across all time periods).
**Proposed fix shape:** A `broadcast_to_dimension:` recipe directive that takes a single scalar value (from a source row or a `defaults:`-style constant) and replicates it across all elements of a named dimension. Alternatively, this is better handled as a Mosaic formula feature: `broadcast(value, DimRef)` would resolve this entirely within the model layer.
**Phase mapping:** Formula fix likely Phase 3J; recipe-side broadcast is Phase 5C or 5D.

### G-OPEN-7: No multi-row-header or comment-row skipping in the strict CSV parser (mc-model)
**Use case:** `mc-model/src/csv.rs` is the strict-subset parser used for `canonical_inputs` and `test_fixtures`. It explicitly rejects quoted fields, BOM, and empty rows, and requires the header on row 1.
**Evidence (Python):** `email-matchback/scripts/mosaic/flatten_ltd_comparison.py:122–139` — after reading the workbook, the script manually locates each year-block's header row by scanning for sentinel strings like `"AUDIENCE TARGETING"`. This cannot be expressed in the current strict CSV parser which has no `skip_rows` or `comment_prefix` support.
**Evidence (Mosaic absence):** `crates/mc-model/src/csv.rs:1–30` — module doc explicitly states: "No comments. ... Header row as the first line." No parameters for header row location or comment prefix.
**Impact:**
  - Lines of Python eliminated: ~30 (header location logic).
  - Other affected use cases: any real-world CSV export from Excel, SAP, Oracle, or government databases that includes a "report title row" before the actual header.
**Proposed fix shape:** Add `skip_rows: N` and `comment_prefix: "#"` optional parameters to `parse_strict` (or to the recipe's `source:` block for the CSV driver path). The Tessera CSV driver already uses the `csv` crate which supports flexible header positioning via `csv::ReaderBuilder::has_headers(false)` + manual seek.
**Phase mapping:** Phase 6A.2 or Phase 5D; minor change, no ADR required.

### G-OPEN-8: Schema-inference flips column type when first 100 rows are all integers but row 101+ has floats
**Use case:** The `csv_driver` infers column types from the first 100 rows. If a measure column has integers in the first 100 rows but floats later (e.g., a spend column where early months are round numbers), the column is inferred as `I64` and rows 101+ fail with `TypeMismatch`.
**Evidence (Python):** No specific Python evidence; general correctness concern observable at `crates/mc-drivers/src/csv_driver.rs:186–192` — `all_int[i]` is set to `false` only if a value fails `parse::<i64>()`. A value like `"21000"` passes as i64 but `"21000.5"` would fail after inference is locked.
**Evidence (Mosaic absence):** `crates/mc-drivers/src/csv_driver.rs:29` — `const SCHEMA_INFERENCE_ROWS: usize = 100` — fixed sample size with no fallback path when a post-inference row violates the inferred type. The coercion step at line 218–255 returns `DriverError::TypeMismatch` which stops or skips the row depending on `on_error` policy.
**Impact:**
  - Lines of Python eliminated: 0 (Tide Cleaners didn't hit this; but any dataset with heterogeneous early rows would).
  - Other affected use cases: any large CSV from a transactional system where early records happen to be clean round numbers.
**Proposed fix shape:** Two options: (a) widen all numeric inference to `F64` (simplest: always parse numbers as f64; i64 is never strictly needed since the kernel stores `ScalarValue::F64`), or (b) do a full-file scan for schema inference (costly for large files; not recommended). Option (a) is a 3-line change.
**Phase mapping:** Phase 6A.2 bug fix; no ADR required.

### G-OPEN-9: No `${secret.ref}` resolver — credentials are env-variable-only
**Use case:** Phase 5E (Grout) is the placeholder for a secrets layer. Currently `credentials: { KEY: "${env.VAR}" }` is the only supported interpolation. Enterprise deployments need vault references, AWS Secrets Manager ARNs, or GCP Secret Manager paths.
**Evidence (Python):** `email-matchback` scripts access source files directly via filesystem paths — no credential management. This is the theoretical gap for production multi-user deployments.
**Evidence (Mosaic absence):** `crates/mc-tessera/src/secrets.rs` — only `EnvVarSecretResolver` is implemented. The `SecretResolver` trait exists (`resolve(&self, reference: &str) -> Result<String, SecretError>`) but no vault, AWS SM, GCP SM, or Azure KV resolver is wired in.
**Impact:**
  - Lines of Python eliminated: 0 (no Python evidence; theoretical enterprise blocker).
  - Other affected use cases: any team where DBA credentials can't be stored in environment variables (regulated industries, shared workstations, CI pipelines with secret injection).
**Proposed fix shape:** Implement `${secret.ref}` interpolation where `ref` is a key in an external store. The `SecretResolver` trait is already the right abstraction; the fix is adding a `PhaseRouter` that tries env-vars first, then a configured external resolver. Grout proper (Phase 5E) is the home for this.
**Phase mapping:** Phase 5E (as designed).

### G-OPEN-10: No gzipped-CSV or compressed-file support in the CSV driver
**Use case:** Many real data exports (from S3, GCS, data warehouses) deliver CSV files gzip-compressed as `.csv.gz`. The current csv_driver opens the file with `File::open()` directly — no decompression layer.
**Evidence (Python):** `email-matchback` scripts receive uncompressed CSV files. Theoretical gap for production data pipelines.
**Evidence (Mosaic absence):** `crates/mc-drivers/src/csv_driver.rs:131–148` — `open_reader` creates a `BufReader<File>` with no decompression. `csv::ReaderBuilder` expects an uncompressed reader.
**Impact:**
  - Lines of Python eliminated: ~5 (gunzip step before feeding to a recipe).
  - Other affected use cases: S3 data exports (always gzipped by default), BigQuery exports (gzip optional), Snowflake COPY INTO (gzip optional).
**Proposed fix shape:** Add optional `source.compression: gzip` field to `SourceConfig`. When set, the CSV driver wraps the `File` in a `flate2::read::GzDecoder` before `BufReader`. `flate2` is pure Rust, MIT-licensed, 1.78-clean.
**Phase mapping:** Phase 5C driver expansion; minor addition.

### G-OPEN-11: Quarantine file never retried — `mc tessera retry-quarantine` not implemented
**Use case:** ADR-0010 Decision 7 semantic rule #6 explicitly states: "Quarantined rows are NOT auto-reprocessed; a future `mc tessera retry-quarantine <import_id>` command (Phase 5C) handles re-processing." This command does not exist.
**Evidence (Python):** No Python evidence — users would need this for production workflows where bad-data rows need re-submission after data correction.
**Evidence (Mosaic absence):** `crates/mc-tessera/src/sidecar.rs:77–81` — `quarantine_path()` exists and quarantine files are written by the runner, but `crates/mc-cli/` has no `retry-quarantine` verb. Confirmed by checking that the schedule commands directory does not include retry logic.
**Impact:**
  - Lines of Python eliminated: ~30 (users currently hand-write retry logic or fix the source and re-run the full import).
  - Other affected use cases: any production import where 1-in-1000 rows fail due to data entry errors that are corrected later.
**Proposed fix shape:** A `mc tessera retry-quarantine <import_id>` verb that reads the quarantine JSONL, reconstructs rows, re-validates against the current model, and attempts to commit the previously-failed rows. The retry should use a NEW `import_id` to preserve the original failure record.
**Phase mapping:** Phase 5C (explicitly deferred per ADR-0010).

### G-OPEN-12: `%e` (space-padded day) and `%j` (day-of-year) tokens missing from strptime
**Use case:** Many enterprise export formats use space-padded day numbers (`" 5"` instead of `"05"`) and day-of-year offsets. The US government, in particular, uses `%j` for Julian day in some exports.
**Evidence (Python):** `flatten_ltd_comparison.py:71–74` — regex extracts month from `"1/7*1/14   1/20*1/27"` style headers. Python's full `strptime` handles `%e` and `%j`; Mosaic's subset does not.
**Evidence (Mosaic absence):** `crates/mc-tessera/src/time_format.rs:155–166` — the `other` branch returns `ParseError` with message listing supported tokens: `%Y %m %d %H %M %S %V %b %%`. `%e`, `%j`, `%p` (AM/PM), `%I` (12-hour), `%u` (weekday) are all absent.
**Impact:**
  - Lines of Python eliminated: ~5 (format token workarounds).
  - Other affected use cases: any dataset from legacy US government, academic, or mainframe-era systems.
**Proposed fix shape:** Add `%e` (space-padded day, 1–31), `%j` (day of year, 001–366), `%I` (12-hour, 01–12), and `%p` (AM/PM, case-insensitive) to the `parse_strptime` match arm. Pure parsing additions with no deps.
**Phase mapping:** Phase 6A.2 polish pass; no ADR required.

### G-OPEN-13: No S3 / GCS / Azure Blob driver — object-store prefix scanning not possible
**Use case:** Production data pipelines routinely deliver partitioned files to cloud object stores (e.g., `s3://bucket/exports/2026/05/*.csv`). No Tessera driver can enumerate a prefix and stream multiple files.
**Evidence (Python):** Not demonstrated by email-matchback (local files only). Theoretical enterprise gap.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs:184–210` — no `S3`, `Gcs`, `AzureBlob`, or `ObjectStore` DriverKind variant. The HTTP driver can fetch a single URL but not enumerate a bucket prefix.
**Impact:**
  - Lines of Python eliminated: 0 (no current Python evidence; future enterprise blocker).
  - Other affected use cases: data engineering teams who deliver daily exports to S3 under a date-partitioned prefix, data lake consumers.
**Proposed fix shape:** A `driver: object_store` with `source.bucket`, `source.prefix`, and optional `source.file_pattern` (glob). Each matching file is fetched and processed as a CSV batch. The `object_store` crate (Apache-2.0, multi-cloud) covers S3/GCS/Azure from one dependency — but its MSRV must be verified against Rust 1.78.
**Phase mapping:** Needs ADR; likely Phase 5C-next or Phase 5D.

---

## Open gaps — needs design

### G-DESIGN-1: No recipe chaining — multi-stage pipelines require Python orchestration
**Use case:** The email-matchback pipeline is inherently multi-stage: (1) read xlsx → (2) flatten to long CSV → (3) compute seasonality anchors → (4) broadcast anchors to all time leaves → (5) mirror Plan→Actual → (6) load into Mosaic. Today steps 1–5 are Python; only step 6 is Tessera. Even if individual gaps close (xlsx driver, LOCF, mirroring), there is no way to compose recipe steps declaratively.
**Evidence:** `email-matchback/scripts/mosaic/prepare_v2_inputs.py:1–230` (entire file is a multi-step pipeline); `flatten_ltd_comparison.py:106–223` (two-pass algorithm within one step); `build_ltv_cohort.py:93–161` (aggregate then emit).
**Why design is non-obvious:** A recipe chain could be: (a) a DAG of recipe files with `depends_on:` fields — requires a scheduler and a shared intermediate format; (b) a recipe DSL with inline `steps:` blocks — approaches a programming language, which ADR-0010 explicitly rejected; (c) DuckDB as a pre-transform layer (write intermediate result to a DuckDB table, then load from it) — works today but requires DuckDB knowledge; (d) a dedicated `mc-etl` crate that orchestrates recipe + transform + load as named stages.
**Alternatives:**
  - Option A — Recipe DAG (`depends_on:` field): declarative, LLM-authorable, but requires a scheduler and intermediate result storage in the sidecar.
  - Option B — Inline `steps:` block in a recipe: allows sequential transforms but approaches the TM1 TI scripting anti-pattern explicitly rejected in ADR-0010.
  - Option C — DuckDB as universal pre-transform (today's partial workaround): users write SQL to transform, load into DuckDB, then Tessera reads from DuckDB. Powerful but requires SQL knowledge.
  - Option D — A `pre_transform:` hook (path to a Wasm/JS/Python script) run before each recipe fetch. Compromises the "no runtime code" design goal.
**Phase mapping:** Needs ADR before phase scoping. Closest existing placeholder is Phase 5D.

### G-DESIGN-2: Indicator/one-hot encoding generation — model concern vs. ingestion concern
**Use case:** `prepare_mmm_inputs.py` generates 464 `IsHouston`/`IsAustin`/`IsDenver`/`IsAmarillo` rows (4 markets × 29 months × 4 indicators). These are one-hot features the fitted Lasso MMM consumes. Today they are synthetic rows in the input CSV — not data from any source.
**Evidence:** `email-matchback/scripts/mosaic/prepare_mmm_inputs.py:70–85` — the indicator generation step; `prepare_mmm_inputs.py:8–10` docstring explicitly calls these "one-hot features the fitted MMM consumes."
**Why design is non-obvious:** Three very different fix shapes compete:
  - Ingestion-side: a Tessera `generate_indicators:` directive that, given a dimension name, auto-generates one-hot measures for each element. Simple, declarative, but puts categorical-encoding logic in the ingestion layer.
  - Model-side: an `indicator_role` measure type (e.g., `role: Indicator; dimension: Market`) where the engine auto-populates `Is{Element}` = 1.0 at the matching element, 0.0 elsewhere. Cleaner semantics but requires an engine or model-layer change.
  - Formula-side: a new formula function `is_dim_member(DimRef)` returning 1.0 if the current element of `DimRef` matches a literal, 0.0 otherwise. Entirely model-side, no new data rows, but requires the formula evaluator to support string equality.
**Alternatives:**
  - Option A — Tessera `generate_indicators:` directive: recipe-declarative, but mixes data transformation with ingestion.
  - Option B — `role: Indicator; dimension: Market` measure: engine-side, clean, but requires mc-core surface expansion (locked per Phase 1 rules; needs ADR).
  - Option C — `is_dim_member("Market", "Houston")` formula function: fully declarative, requires Phase 3J string-value support which is already a known deferred item.
  - Option D — Keep as input data: current approach; requires Python to generate rows on every data refresh.
**Phase mapping:** Needs ADR before phase scoping; Option C depends on Phase 3J.

### G-DESIGN-3: Scenario/dimension key translation — source uses different vocabulary than model
**Use case:** The source data uses `"Actual"` / `"Plan"` as Scenario values; the model must agree. In real enterprise scenarios, a source system might use `"ACT"` / `"BUD"` / `"FORE"` while the model uses canonical English names. Today there is no key-translation layer in the recipe format.
**Evidence (Python):** `email-matchback/scripts/mosaic/prepare_v2_inputs.py:134–152` — the Plan→Actual mirroring is partly a vocabulary translation (moving rows from one scenario tag to another). `flatten_ltd_comparison.py:197` — scenario assignment logic (`"Actual" if key in months_with_actuals else "Plan"`) is a custom inference rule, not a simple lookup.
**Why design is non-obvious:** A key-translation directive could be: (a) a per-column `value_map: { ACT: Actual, BUD: Plan }` dictionary — simple but verbose for large vocabularies; (b) a `lookup_dimension_element:` join against a separate reference CSV — joins two sources in one recipe; (c) a SQL CASE expression in the source query (works only for SQL drivers, not CSV); (d) a separate `dimension_element_aliases` block in the model YAML that lets the engine accept multiple source strings for the same element.
**Alternatives:**
  - Option A — `value_map:` per column: declarative and LLM-authorable; verbose for large vocabularies; already partially implied by the `format:` field on `ColumnMapping` but not semantically defined.
  - Option B — Source-query CASE expression: works for SQL drivers today with zero Mosaic changes; not available for CSV driver.
  - Option C — Model-level `aliases:` on elements: the engine accepts alternative names during ingestion but stores canonical names. Cleaner than duplicating translation logic in every recipe.
  - Option D — No change: users are expected to pre-process or write SQL CASE expressions.
**Phase mapping:** Needs ADR; no existing phase placeholder exactly covers this.

### G-DESIGN-4: Multi-file ingest from a single recipe — today each recipe maps to one source
**Use case:** `build_ltv_cohort.py` reads FOUR separate xlsx files (one per market: Houston, Austin, Denver, Amarillo) and aggregates them into a single output. Today a single Tessera recipe maps to a single source. There is no way to declare "union these four CSVs" in one recipe.
**Evidence (Python):** `email-matchback/scripts/mosaic/build_ltv_cohort.py:34–39` — `MARKET_FROM_FILE` dict maps 4 filenames to 4 market values; the loop at line 102–138 iterates all four files. The aggregation (lines 146–161) is what produces the canonical_inputs shape.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs:114–153` — `SourceConfig` has a single `path`, `query`, `url` — one source per recipe. No `sources: [...]` array or `union:` directive.
**Why design is non-obvious:** Multi-file ingest could be: (a) multiple recipe files, one per source, all targeting the same model slice — currently works but produces N separate import_ids and N separate audit records, losing the "one cohesive import" story; (b) a `sources: [...]` array in `SourceConfig` with UNION semantics — extends the recipe schema significantly; (c) a DuckDB recipe using `UNION ALL` across multiple CSV paths via DuckDB's `read_csv_auto()` function — works today for technical users; (d) recipe DAG (see G-DESIGN-1) where a parent recipe specifies child recipes.
**Alternatives:**
  - Option A — Multiple recipes targeting the same model: works today; loses the single-import-id story.
  - Option B — `sources: [...]` with UNION semantics: powerful but large schema change; requires re-thinking `on_error` semantics across sources.
  - Option C — DuckDB SQL UNION (today's workaround for SQL-capable users).
  - Option D — Recipe chaining (G-DESIGN-1): if recipe DAG lands, this is naturally a parent recipe that unions children.
**Phase mapping:** Needs ADR. Likely Phase 5C or 5D.

### G-DESIGN-5: Aggregation transforms — group_by + sum/avg before cube write
**Use case:** `build_ltv_cohort.py` reads customer-level rows (one row per customer) and aggregates to cohort-level (one row per Market × TenureBucket combination) before writing to the cube. The aggregation includes `count()`, `sum(TotalSales)`, and `sum(TotalVisits)`. No recipe can express this — it would require the recipe to see ALL rows before emitting any cell.
**Evidence (Python):** `email-matchback/scripts/mosaic/build_ltv_cohort.py:93–161` — the entire aggregation pipeline. With ~5,000 input rows producing ~300 output rows, this is a 16:1 fan-in that cannot be expressed in a 1:N or 1:1 recipe column mapping.
**Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs` — no `aggregate:`, `group_by:`, or `rollup:` directive. The recipe mapping model is "one source row → N cells" not "N source rows → 1 cell."
**Why design is non-obvious:** Aggregation can be: (a) a SQL query in the recipe (`query: SELECT market, tenure_bucket, COUNT(*) ...`) — works for SQL drivers today with zero Mosaic changes; the gap is that CSV and xlsx sources don't have a query interface; (b) a DuckDB pre-transform (read CSV into DuckDB, aggregate in SQL, then Tessera reads from DuckDB) — works today but requires two steps and DuckDB knowledge; (c) a first-class `aggregate:` block in the recipe — requires streaming aggregation or full-materialization before write; conflicts with the streaming batch design.
**Alternatives:**
  - Option A — SQL query in recipe: works today for SQL sources; not available for CSV/xlsx.
  - Option B — DuckDB federation (read CSV file in DuckDB with `read_csv_auto()` then apply SQL GROUP BY): works today via `driver: duckdb` with a DuckDB in-memory DB and an `ATTACH` / `read_csv_auto` query. Powerful but requires recipe-author DuckDB knowledge.
  - Option C — First-class `aggregate:` directive: clean but requires full streaming aggregation before commit — incompatible with the streaming `WriteBatch` design without an intermediate buffer.
  - Option D — `mc-etl` pre-transform layer (see G-DESIGN-1).
**Phase mapping:** Needs ADR. Option B is available today as a workaround.

---

## Edge cases / latent bugs found during audit

### E-1: Incremental watermark injection appends WHERE clause naively — may produce invalid SQL
**Where:** `crates/mc-tessera/src/incremental.rs:142–146`
**What I expected:** Safe SQL injection that respects existing WHERE clauses in the base query.
**What I observed:** The fallback path when `{{watermark}}` is absent appends ` WHERE {column} > '{last_value}'` to the raw query string. If the base query already contains a `WHERE` clause (e.g., `SELECT * FROM t WHERE year = 2026`), the appended `WHERE` produces invalid SQL: `SELECT * FROM t WHERE year = 2026 WHERE updated_at > '2026-01-01'`.
**Impact:** Any incremental recipe whose source query already has a WHERE clause will fail at the database driver with a SQL syntax error on the second run. The first run (no prior state) works fine, so this bug is latent until the second execution.

### E-2: Schema inference reads 100 rows then re-opens the file — doubled I/O on first prepare
**Where:** `crates/mc-drivers/src/csv_driver.rs:65–76`
**What I expected:** A single pass to infer schema and then stream data.
**What I observed:** `CsvDriver::new()` calls `infer_schema(path)` (which opens and reads up to 100 rows, then closes) and then calls `open_reader(path)` (which reopens the file from the start). For large files this is doubled I/O just for prepare. For files smaller than 100 rows, every row is read twice.
**Impact:** Minor performance issue, not a correctness bug. On network-mounted filesystems or slow storage (e.g., S3 FUSE mount), the doubled open could be significant.

### E-3: `on_missing_element: create` generates non-deterministic ElementIds across runs
**Where:** `crates/mc-tessera/src/transform.rs:240–243`
**What I expected:** Dynamically created elements would have stable IDs across separate `prepare()` calls for reproducibility.
**What I observed:** `let new_id = ElementId(DYNAMIC_ELEMENT_BASE + refs.elements.len() as u64)`. The new ID depends on how many elements have been dynamically created so far in the current session. If a second `apply()` call processes a different subset of rows first, the ID for the same element name differs. Since `ModelRefs` is not persisted between sessions, a rollback scenario (restore snapshot, re-apply) will produce different ElementIds for dynamically created elements.
**Impact:** Correctness issue for `on_missing_element: create` with rollback workflows. The snapshot captures cells by coordinate (which embeds ElementIds); after rollback, dynamically created element IDs may differ if the recipe is re-applied, producing orphaned coordinates that can't be read back by name.

### E-4: CSV driver's `column_value_as_string` passes float-formatted strings to element lookup
**Where:** `crates/mc-tessera/src/transform.rs:889–897`
**What I expected:** Dimension element names match model-declared element names exactly.
**What I observed:** `column_value_as_string` for `ColumnData::F64` returns `n.to_string()` — Rust's default float formatting. If a dimension column was inferred as F64 (e.g., because a year column like `"2026"` was parsed as a float), the lookup string becomes `"2026"` (fine) but an element name with a fractional part (e.g., an ID like `"1.0"` vs `"1"`) would mismatch. More critically: if an integer-valued float is stored as `F64` (e.g., `2026.0`), `to_string()` produces `"2026"` in Rust, which may or may not match the declared element name `"2026"`.
**Impact:** Low-severity but subtle: users whose dimension values are numeric strings (year codes, integer IDs) may see intermittent element-lookup failures if the CSV driver infers the column as F64. The fix is to format I64-typed columns as integers explicitly and F64-typed columns via the integer path when the value has no fractional part.

### E-5: `maybe_canonicalize_time` returns `Ok(raw)` for non-Time dims regardless of `time_format` set
**Where:** `crates/mc-tessera/src/transform.rs:828–830`
**What I expected:** A recipe-validation warning if `time_format` is set on a non-Time dimension column.
**What I observed:** If a recipe author accidentally sets `time_format: "%Y-%m-%d"` on a `Channel` or `Market` dimension column (not a `kind: "Time"` dimension), `maybe_canonicalize_time` silently returns the raw value unchanged (`if !is_time_dim { return Ok(raw); }`). The recipe passes validation (MC5030 only fires when a Time-dim column lacks `time_format`, not when a non-Time column has it). The user gets no signal that their `time_format` is being ignored.
**Impact:** Silent misconfiguration; a recipe author who mistakenly sets `time_format` on the wrong column gets no error or warning. Mostly a UX issue; no data corruption.

### E-6: `build_ltv_cohort.py` reads from `../Feb 2026/` — path contains a space and is fragile
**Where:** `email-matchback/scripts/mosaic/build_ltv_cohort.py:30` — `SRC_DIR = REPO / "Feb 2026"`
**What I expected:** A canonical data directory path.
**What I observed:** The source Excel files for LTV cohort analysis live in `email-matchback/Feb 2026/` (a directory whose name contains a space and a year). This path is hardcoded. Any rename of the source directory or change in snapshot month (e.g., to "Mar 2026") requires editing the script. There is also no recipe-side path — this data never flows through Tessera at all.
**Impact:** The LTV cohort source data (customer-level xlsx files) has no Tessera recipe equivalent. Even with an xlsx driver, the file-discovery and multi-file aggregation patterns (G-DESIGN-4, G-DESIGN-5) would need to be solved before this can migrate off Python.

---

## Confirmed working (sanity checks only)

- Long-format CSV ingestion: `SourceFormat::Long` with `long_format.measure_column` / `long_format.value_column` is correctly dispatched in both `transform_batch_long` and `transform_batch_long_inner` (`transform.rs:431–795`).
- Non-ISO date parsing: `time_format: "%m/%d/%Y"` is applied via `maybe_canonicalize_time` for `kind: "Time"` dimension columns; `canonicalize_period` produces correct ISO month/quarter/year/day strings from parsed parts.
- `on_error: quarantine` path: quarantine file path is correctly constructed via `Sidecar::quarantine_path()` and separate from the audit log.
- `on_missing_element: create` path: dynamically created elements get IDs starting at `DYNAMIC_ELEMENT_BASE = 10_000_000` — well above model-defined element IDs in a 6-dimension cube.
- Schema inference for the three model CSV files: `ltd-comparison-long.csv`, `mmm-inputs.csv`, and `ltv-cohort.csv` all have consistent types across their ~300–650 rows; the 100-row inference sample is sufficient for these files.
- Recipe-level defaults: `defaults: { Scenario: Actual, Version: Working }` correctly pre-fills dimension slots in the transformer without needing those columns in the source CSV (`prepare.rs:662–703`).
- MC5030 enforcement: a Time-kind dimension column without `time_format` when the recipe declares non-ISO values correctly fires MC5030 at validation time (`mc-recipe/src/validate.rs`).
- Credential interpolation: `${env.VAR}` is resolved by `EnvVarSecretResolver` before driver construction (`prepare.rs:300–315`).

---

## What I couldn't verify

1. **Whether DuckDB's `read_csv_auto()` or `read_excel()` extension would work via the `duckdb` driver** as a workaround for G-OPEN-1 (xlsx) and G-DESIGN-5 (aggregation). This would require running DuckDB with the extension installed, which wasn't available in this read-only audit.

2. **The `%e` (space-padded day) behavior** in the strptime subset: the code path at `time_format.rs:159–166` returns `ParseError` for unsupported tokens, but I couldn't confirm whether any current recipe file actually uses `%e` and fails silently vs. never getting that far.

3. **Whether `on_missing_element: create` (E-3) actually causes rollback-collision issues** in practice — this would require running two sequential `apply()` + `rollback()` cycles with the same recipe and checking ElementId consistency across sessions. Not verifiable without execution.

4. **Whether the incremental state injection double-WHERE bug (E-1) fires in the current test suite** — the test at `crates/mc-tessera/src/incremental.rs:197+` may not test a query with a pre-existing WHERE clause.

5. **Exact line count of Python eliminated if xlsx driver + layout descriptors + multi-file union landed**: the estimate of ~220 lines for `flatten_ltd_comparison.py` is per the file's total line count, but some of that (channel inference, scenario inference, multi-window coalescing) is business logic that would still need recipe-level expression.
