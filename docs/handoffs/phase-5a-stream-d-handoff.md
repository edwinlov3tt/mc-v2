# Phase 5A Stream D Handoff — Tessera Orchestrator (`mc-tessera`)

> **Audience:** the Claude Code instance running in the git worktree at
> `../mc-v2-stream-d` on branch `phase-5a/stream-d-tessera-orchestrator`.
> **You inherit the MERGED main branch** at commit `6da91a5`, with all
> three upstream streams already landed (502/0 tests passing). Streams A
> (WriteBatch in mc-core), B (Recipe in mc-recipe), and C (Source drivers
> in mc-drivers) are real, tested, merged code — not interface contracts.
>
> **This stream creates `crates/mc-tessera/`** (the Tessera orchestrator)
> and **modifies `crates/mc-cli/`** (adding `mc tessera {apply, dry-run,
> history, rollback, audit}` subcommands). It does NOT touch mc-core,
> mc-recipe, mc-drivers, mc-model, or mc-fixtures.
>
> **Read [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
> BEFORE this handoff.** Focus on Decision 1 (7 acceptance criteria),
> Decision 2.5 (`.tessera/` sidecar state model), Decision 7 (recipe
> semantic rules — all 6), Decision 13 (naming), and Appendix D (Stream D
> interface contract). The ADR is the strategic gate; this handoff commits
> the implementation contract against the REAL merged code.

---

## The one paragraph you must internalize before writing code

**Stream D is the integration point — the place where recipe, driver,
model, and kernel come together into a working end-to-end pipeline.**
Your job is to wire three upstream components (WriteBatch, Recipe, Source
Drivers) into a Tessera orchestrator that does: recipe parse + validate
-> model load -> driver construction -> batch fetch -> row transform ->
WriteBatch stage + commit -> sidecar persist -> audit log. The
orchestration logic is where the UX lives: dry-run validation,
error-handling policy (abort/skip_row/quarantine), timing breakdown,
rollback via pre-commit snapshots, and the audit trail. This is a
reference implementation — don't over-engineer. The upstream crates are
real and tested; your job is to connect them, not to rewrite them.

---

## ADR-0010 decisions affecting Stream D

### Decision 1 — 7 acceptance criteria

Phase 5A ships when ALL of these hold. Stream D is responsible for
criteria 1-4, 6, and the end-to-end half of 5:

1. A user writes a Tessera recipe and maps an external source to a cube.
2. `mc tessera dry-run <recipe>` validates the recipe against the target
   cube schema and reports mapping errors via MC5xxx diagnostics.
3. `mc tessera apply <recipe>` executes: connect, fetch, transform,
   bulk-write via WriteBatch, capture audit record, exit with summary.
4. `mc tessera rollback <import_id>` restores the cube to its pre-import
   snapshot exactly.
5. Headline perf: 100K-row SQLite recipe import in 3 seconds end-to-end.
6. Acme CSV equivalence: ingesting `acme.inputs.csv` via a Tessera recipe
   produces byte-identical cube state to `mc_fixtures::write_canonical_inputs()`.
7. Locked crates (mc-core, mc-recipe, mc-drivers, mc-model, mc-fixtures)
   remain unchanged by Stream D.

### Decision 2.5 — `.tessera/` sidecar state model

```
<model_dir>/.tessera/
+-- audit.jsonl                         # append-only; one JSON record per import
+-- imports/
|   +-- <import_id>.cells.jsonl         # persisted cells written by this import
+-- snapshots/
|   +-- <snapshot_id>.cells.jsonl       # pre-commit snapshot (for rollback)
+-- quarantine/
|   +-- <import_id>.jsonl               # quarantined rows (on_error: quarantine)
+-- active-imports.json                 # manifest of currently-active imports
```

**Hard rule:** `.tessera/` is NEVER committed to source control. The
model directory's `.gitignore` should include `.tessera/`.

### Decision 7 — 6 semantic rules (Stream D enforces at runtime)

1. **Column mappings are 1:1.** One source column maps to one dimension
   OR one measure. (Recipe validation in mc-recipe already checks this.)
2. **Defaults vs. columns mutual exclusion.** A dimension cannot appear
   in both `columns:` and `defaults:`. (MC5016 — already validated by
   mc-recipe.)
3. **Input measures only.** Column mappings that target a Derived measure
   are rejected at recipe validation (MC5018 — already validated by
   mc-recipe). Stream D should also catch this at transform time as
   defense-in-depth.
4. **`write_disposition: replace` = coordinate-level overwrite.** Does
   NOT clear existing values absent from incoming data.
5. **`model:` path resolution.** Relative to the recipe file's directory.
   Path-escape outside workspace root rejected (MC5017 — validated by
   mc-recipe).
6. **`on_error:` semantics.** `abort` = transactional, no partial commit;
   `skip_row` = log + skip; `quarantine` = write to quarantine log + skip.

### Decision 13 — naming

- Crate: `mc-tessera`
- CLI verbs: `mc tessera {apply, dry-run, history, rollback, audit}`
- The `SecretResolver` trait + `EnvVarSecretResolver` live in `mc-tessera`
  (Grout forward-compat; the future `mc-grout` crate ships in Phase 5E).

---

## Phase 5A Stream D prompt (verbatim binding contract)

> We are starting Phase 5A Stream D: the Tessera orchestrator.
>
> **Context.** Streams A, B, C have been merged to main at commit
> `6da91a5` (502/0 tests). The WriteBatch API exists in mc-core. The
> Recipe types exist in mc-recipe. The SourceDriver trait + 6 drivers
> exist in mc-drivers. The model layer exists in mc-model. Stream D
> wires them together into the end-to-end Tessera engine.
>
> **Goal.** Ship `crates/mc-tessera/` (new crate) + `mc tessera` CLI
> verbs such that:
>
> 1. `mc tessera apply <recipe.yaml>` runs the full pipeline: parse
>    recipe, load model, construct driver, fetch batches, transform
>    rows to `(CellCoordinate, ScalarValue)` pairs, stage via
>    `WriteBatch::push_batch()`, commit, persist to `.tessera/`
>    sidecar, append audit record.
> 2. `mc tessera dry-run <recipe.yaml>` validates the recipe against
>    the model, reports MC5xxx diagnostics, and exits without writing.
> 3. `mc tessera rollback <import_id>` restores the pre-import state.
> 4. `mc tessera history <model_dir>` lists import history from
>    `audit.jsonl`.
> 5. `mc tessera audit <model_dir>` prints detailed audit records
>    (same data as history, fuller format).
> 6. The Acme CSV equivalence test passes (HEADLINE acceptance test).
> 7. The 100K-row SQLite performance test passes (HEADLINE perf gate).
>
> **Phase 5A Stream D scope** (binding contract):
>
> 1. **New crate `crates/mc-tessera/`** with the `Tessera` orchestrator
>    struct. Public API: `Tessera::prepare()`, `Tessera::dry_run()`,
>    `Tessera::apply()`, `Tessera::rollback()`, `Tessera::history()`.
>
> 2. **`PreparedImport` struct** — loads the recipe via
>    `mc_recipe::parse()`, loads the target cube via
>    `mc_model::load(resolved_model_path)`, constructs the appropriate
>    `Box<dyn SourceDriver>` from the recipe's `source:` block, and
>    resolves column mappings into a `Vec<ResolvedColumnMapping>` that
>    binds each source column to a concrete dimension/measure target
>    with resolved `ElementId`s for defaults.
>
> 3. **Transformation layer** — converts `RowBatch` (column-oriented,
>    from `mc_drivers`) into `Vec<(CellCoordinate, ScalarValue)>` (the
>    format `WriteBatch::push_batch()` accepts). For each row:
>    - Resolve dimension-column values to `ElementId` via
>      `ModelRefs::element(dim_name, element_value)`.
>    - Apply defaults from `recipe.defaults` for dimensions not in the
>      source data.
>    - Build a `CellCoordinate` from the resolved element slots in the
>      canonical dimension order (`ModelRefs::dimension_order`).
>    - Extract measure-column values, coerce to `ScalarValue::F64`
>      (with optional `scale` factor from `ColumnMapping::scale`).
>    - One source row may produce N cells (one per mapped measure
>      column).
>
> 4. **Element resolution** — row dimension-element values (strings
>    from the source) are resolved to `ElementId` via
>    `ModelRefs::element(dim_name, value_string)`. On resolution
>    failure: fire the `on_missing_element` policy (Phase 5A: always
>    `Error`; `Create` is Phase 5C). Element resolution failure
>    triggers the `on_error` policy for that row.
>
> 5. **`.tessera/` sidecar state model** — per Decision 2.5:
>    - `audit.jsonl`: append one `AuditRecord` JSON line per import.
>    - `imports/<import_id>.cells.jsonl`: persist every cell written
>      (coordinate + value) so rollback + history are meaningful.
>    - `snapshots/<snapshot_id>.cells.jsonl`: persist the pre-commit
>      snapshot state for rollback.
>    - `active-imports.json`: manifest tracking which imports are
>      currently active (not rolled back).
>    - `quarantine/<import_id>.jsonl`: quarantined rows when
>      `on_error: quarantine`.
>
> 6. **`SecretResolver` trait + `EnvVarSecretResolver`** — the Grout
>    forward-compat interface. `SecretResolver::resolve(&self,
>    reference: &str) -> Result<String, SecretError>`.
>    `EnvVarSecretResolver` resolves `${env.VAR_NAME}` references from
>    environment variables. Used to interpolate `credentials:` values
>    in the recipe before passing them to driver constructors.
>
> 7. **`ImportReport` struct with `TimingBreakdown`** — returned by
>    `Tessera::apply()`. Fields: `import_id`, `rows_written`,
>    `rows_failed`, `timing` (`fetch_ms`, `transform_ms`,
>    `validate_ms`, `commit_ms`, `total_ms`), `snapshot_id`,
>    `audit_path`.
>
> 8. **Error handling per `on_error` field:**
>    - `Abort` (default): on any row error, no partial commit.
>      WriteBatch is dropped (no side effects per the atomicity
>      contract). Cube state unchanged.
>    - `SkipRow`: log the row + diagnostic to the audit record with
>      `status: "skipped"`. Count toward `rows_failed`. Remaining
>      rows proceed.
>    - `Quarantine`: write the row to
>      `.tessera/quarantine/<import_id>.jsonl` with original row data
>      + diagnostic. Count toward `rows_failed`. Remaining rows
>      proceed.
>
> 9. **CLI verbs in `crates/mc-cli/src/main.rs`:**
>    - `mc tessera apply <recipe_path>` — runs `Tessera::prepare()`
>      then `Tessera::apply()`. Prints `ImportReport` summary.
>    - `mc tessera dry-run <recipe_path>` — runs `Tessera::prepare()`
>      then `Tessera::dry_run()`. Prints validation report + MC5xxx
>      diagnostics.
>    - `mc tessera history <model_dir>` — reads `.tessera/audit.jsonl`
>      and prints the import timeline.
>    - `mc tessera rollback <import_id> --model-dir <path>` — runs
>      `Tessera::rollback()`.
>    - `mc tessera audit <model_dir>` — reads `.tessera/audit.jsonl`
>      and prints detailed audit records.
>    - `--format text|json` modifier on all verbs. JSON output uses
>      the Phase 3B `schema_version: "1.0"` envelope shape for
>      diagnostic output.
>
> 10. **Integration tests:** end-to-end recipe -> driver -> transform
>     -> WriteBatch -> audit. At minimum:
>     - Acme CSV equivalence test (HEADLINE).
>     - 100K-row SQLite performance test (HEADLINE).
>     - Dry-run produces expected MC5xxx diagnostics for broken recipes.
>     - Rollback restores pre-import state exactly.
>     - `on_error: skip_row` skips bad rows, writes good ones.
>     - `on_error: quarantine` writes quarantine log.
>     - Audit log is valid JSONL after apply.
>     - History lists imports in chronological order.
>
> 11. **HEADLINE acceptance test: Acme CSV equivalence.** Write an
>     `acme-import.recipe.yaml` that reads `acme.inputs.csv` via the
>     CSV driver and maps columns to the Acme model's dimensions and
>     measures. After `Tessera::apply()`, compare every cell in the
>     cube against the state produced by
>     `mc_fixtures::write_canonical_inputs()`. Every coordinate must
>     have the same `ScalarValue` (within 1e-9 epsilon for F64).
>
> 12. **HEADLINE performance test: 100K-row SQLite import.** Generate
>     a 100K-row SQLite fixture (deterministic, committed as a test
>     fixture or generated at test time). Write a recipe that imports
>     it into a cube. Assert end-to-end completion in <= 3 seconds on
>     `--release` builds.
>
> 13. **Runtime diagnostic codes.** MC5013 (credential interpolation
>     failure), MC5014 (source file not found), MC5015 (connection
>     failure) are defined in mc-recipe's `RecipeError` enum but FIRED
>     by mc-tessera at runtime. When the `EnvVarSecretResolver` fails
>     to resolve a `${env.X}` reference, emit MC5013. When a driver
>     constructor returns `DriverError::SourceFileNotFound`, emit
>     MC5014. When it returns `DriverError::ConnectionFailed`, emit
>     MC5015.
>
> 14. **Plugin skill** at
>     `mosaic-plugin/skills/import/tessera-usage/SKILL.md` — how to
>     use `mc tessera` CLI verbs. Documents the recipe format, the
>     apply/dry-run/rollback/history/audit verbs, error handling
>     policies, and the `.tessera/` sidecar model.
>
> **Hard rules:**
>
> - **`crates/mc-core/` is LOCKED.** `WriteBatch` already exists; do
>   not modify it. `git diff 6da91a5 -- crates/mc-core/` returns 0
>   lines.
> - **`crates/mc-recipe/` is LOCKED.** Recipe types exist; do not
>   modify. `git diff 6da91a5 -- crates/mc-recipe/` returns 0 lines.
> - **`crates/mc-drivers/` is LOCKED.** Drivers exist; do not modify.
>   `git diff 6da91a5 -- crates/mc-drivers/` returns 0 lines.
> - **`crates/mc-model/` is LOCKED.** `git diff 6da91a5 --
>   crates/mc-model/` returns 0 lines.
> - **`crates/mc-fixtures/` is LOCKED.** `git diff 6da91a5 --
>   crates/mc-fixtures/` returns 0 lines.
> - **`crates/mc-tessera/` is NEW.** This is where Stream D lives.
> - **`crates/mc-cli/` gains tessera subcommands.** The existing
>   `demo`, `model`, and `mcp` verbs are unchanged.
> - **No new dependencies** beyond what's already in the workspace.
>   `mc-tessera` depends on `mc-core`, `mc-recipe`, `mc-drivers`,
>   `mc-model`, and `thiserror`. Uses `serde` + `serde_json` for
>   JSONL audit persistence (already in workspace via mc-model /
>   mc-recipe).
> - **No `unsafe`, no `async`, no `tokio`, no `rayon`, no threads.**
> - **No `unwrap()` / `expect()` in `crates/mc-tessera/src/`.**
>   Return `Result<_, TesseraError>` everywhere. Tests are exempt.
> - **All 502 existing tests must still pass.** New total >= 502 +
>   Stream D test additions.
> - **Rust toolchain stays at 1.78.** Cargo.lock pins stay intact.

---

## SPEC QUESTION triggers

Open a SPEC QUESTION (per CLAUDE.md section 11) before continuing if any of
these surface:

1. **WriteBatch API doesn't match what Stream D needs.** For example, if
   `WriteBatch::push_batch()` rejects valid coordinates that Stream D
   expects to write, or if `CommitResult` is missing a field the audit
   record needs. Do NOT modify mc-core; surface the gap.

2. **Element resolution has an edge case not covered.** For example, a
   source row has a dimension-element name that's a hierarchy rollup node
   (not a leaf). The recipe didn't specify whether to accept or reject
   non-leaf element writes. Surface before deciding.

3. **The `.tessera/` sidecar model needs a field nobody anticipated.**
   For example, the `active-imports.json` manifest needs to track the
   model path or cube-id to support multi-model directories. Surface
   the schema change.

4. **Performance gate missed.** The 100K-row SQLite import exceeds 3
   seconds on release builds. Before optimizing, surface the measured
   numbers and the breakdown (fetch/transform/commit) so the bottleneck
   is clear.

5. **Credential interpolation needs something beyond `${env.X}`
   syntax.** For example, the recipe has nested credential references
   or driver-specific auth patterns not covered by simple env-var
   substitution. Surface before extending.

6. **The Acme CSV equivalence test fails on a subset of coordinates.**
   This likely means the transformation layer is building coordinates
   wrong (dimension order, element name mismatch, default application).
   Debug the transformation before concluding WriteBatch is broken.

---

## Context: the ACTUAL upstream types Stream D integrates against

These are the real types from the merged codebase. The ADR appendices
were the pre-implementation contracts; the code below supersedes them
where they differ.

### mc-core: WriteBatch API (`crates/mc-core/src/batch.rs`)

```rust
// WritebackContext — audit metadata for bulk import
pub struct WritebackContext {
    pub source_name: String,     // e.g., "hubspot_q3_export.csv"
    pub import_id: String,       // unique per import; generated by mc-tessera
    pub principal: PrincipalId,  // who initiated the import
}

// WriteBatch — stages writes for atomic commit
pub struct WriteBatch<'cube> {
    cube: &'cube mut Cube,       // exclusive borrow
    context: WritebackContext,
    staged: Vec<(CellCoordinate, ScalarValue)>,
}

impl<'cube> WriteBatch<'cube> {
    pub fn new(cube: &'cube mut Cube, context: WritebackContext) -> Self;
    pub fn push(&mut self, coord: CellCoordinate, value: ScalarValue) -> Result<(), EngineError>;
    pub fn push_batch(&mut self, cells: &[(CellCoordinate, ScalarValue)]) -> Result<(), EngineError>;
    pub fn staged_count(&self) -> usize;
    pub fn commit(self) -> Result<CommitResult, EngineError>;
}

// CommitResult — returned on successful commit
pub struct CommitResult {
    pub rows_written: usize,
    pub rows_failed: usize,            // always 0 on Ok (validate-then-apply)
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub dirty_count_after: usize,      // cumulative dirty-set size
    pub newly_dirtied_count: usize,    // marginal: clean -> dirty this commit
    pub snapshot_id: String,           // format: "{import_id}@{revision_before}"
}
```

**Key implementation detail:** `push()` and `push_batch()` only check
coordinate arity and cube-id (cheap). Full validation (permission, type,
derived-cell rejection, lock, NaN/Inf) is deferred to `commit()` step 1.
On validation failure, `commit()` returns `Err` with no mutation and no
snapshot cost.

**Timestamp note:** `WriteBatch::commit()` currently hard-codes
`now_unix_seconds = 0` (the test-determinism convention). The batch.rs
source comments note that Stream D can thread a real timestamp through
`WritebackContext` when audit-trail records need wall-clock. For Phase
5A, accept the `0` convention; the audit record's timestamp comes from
`mc-tessera` itself (not from the kernel's per-cell timestamp).

### mc-recipe: Recipe types (`crates/mc-recipe/src/schema.rs`)

```rust
pub struct Recipe {
    pub version: u32,                           // must be 1
    pub name: String,
    pub description: Option<String>,
    pub model: String,                          // path to target YAML model
    pub source: SourceConfig,
    pub columns: Vec<ColumnMapping>,
    pub defaults: HashMap<String, String>,      // dim_name -> element_name
    pub write_disposition: WriteDisposition,     // Phase 5A: Replace only
    pub incremental: bool,                      // Phase 5A: false only
    pub batch: BatchConfig,
    pub on_error: OnError,                      // Abort | SkipRow | Quarantine
    pub on_missing_element: OnMissingElement,   // Phase 5A: Error only
    pub credentials: HashMap<String, String>,
}

pub struct SourceConfig {
    pub driver: DriverKind,     // Csv | Sqlite | Duckdb | Postgres | DuckdbPostgres | HttpJson
    pub path: Option<String>,
    pub query: Option<String>,
    pub table: Option<String>,
    pub url: Option<String>,
    pub json_path: Option<String>,
}

pub struct ColumnMapping {
    pub source: String,
    pub dimension: Option<String>,
    pub measure: Option<String>,
    pub data_type: Option<String>,    // serde rename: "type"
    pub scale: Option<f64>,
    pub format: Option<String>,
    pub skip: Option<bool>,
}

pub enum OnError { Abort, SkipRow, Quarantine }
pub struct BatchConfig { pub size: Option<usize> }  // None -> 50_000 default
```

**Key functions:**
- `mc_recipe::parse(yaml_str) -> Result<Recipe, RecipeError>` — parse
  recipe YAML.
- `mc_recipe::validate_recipe(&recipe, &model, path_ctx) -> Vec<RecipeError>`
  — validate a parsed recipe against a `ValidatedModel`. Returns empty
  vec on success.
- `mc_recipe::to_yaml(&recipe) -> String` — roundtrip serialization.
- `mc_recipe::diagnostics_to_json(&[Diagnostic]) -> String` — JSON
  envelope with `schema_version: "1.0"`.

### mc-drivers: SourceDriver trait (`crates/mc-drivers/src/lib.rs`)

```rust
pub trait SourceDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError>;
    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError>;
    fn cancel(&mut self);
}

pub struct RowBatch {
    pub columns: Vec<Column>,
    pub row_count: usize,
}

pub struct Column {
    pub name: String,
    pub data: ColumnData,
}

pub enum ColumnData {
    F64(Vec<Option<f64>>),
    I64(Vec<Option<i64>>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}
```

**Driver constructors (all return `impl SourceDriver`):**
- `csv_driver(path: &Path) -> Result<impl SourceDriver, DriverError>`
- `sqlite_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>`
- `duckdb_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>`
- `postgres_driver(dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- `duckdb_postgres_driver(duckdb_path: &Path, pg_dsn: &str, query: &str) -> Result<..>`
- `http_json_driver(url: &str, json_path: Option<&str>) -> Result<..>`

**DriverError variants** that map to MC5xxx codes:
- `DriverError::SourceFileNotFound { path, message }` -> MC5014
- `DriverError::ConnectionFailed { target, message }` -> MC5015

**The trait is object-safe** — Stream D holds `Box<dyn SourceDriver>`.

### mc-model: load + ModelRefs (`crates/mc-model/src/lib.rs`, `compile.rs`)

```rust
// Load a YAML model file -> fully-built Cube + refs
pub fn load(path: impl AsRef<Path>) -> Result<CompiledCube, Vec<Error>>;

pub struct CompiledCube {
    pub cube: Cube,
    pub root_principal: PrincipalId,
    pub refs: ModelRefs,
}

pub struct ModelRefs {
    pub cube_id: CubeId,
    pub dimensions: BTreeMap<String, DimensionId>,
    pub elements: BTreeMap<(String, String), ElementId>,  // (dim_name, elem_name) -> ElementId
    pub rules: BTreeMap<String, RuleId>,
    pub dimension_order: Vec<String>,                     // e.g., ["Scenario", "Version", ...]
}

impl ModelRefs {
    pub fn element(&self, dim: &str, element: &str) -> Option<ElementId>;
    pub fn coord_from_names(&self, names: &BTreeMap<String, String>) -> Option<CellCoordinate>;
}
```

**`mc_model::load()` does NOT apply inputs to the cube.** The returned
cube is empty. Stream D's job is to populate it via WriteBatch (or, for
the equivalence test, compare against the fixture-populated cube).

**`ModelRefs::coord_from_names()`** takes a `BTreeMap<String, String>`
where keys are dimension names and values are element names. It builds
a `CellCoordinate` in the canonical dimension order. This is the primary
tool Stream D uses for element resolution.

**`mc_model::validate()` returns `ValidatedModel`** which is what
`mc_recipe::validate_recipe()` accepts for schema-aware recipe
validation. Stream D calls `mc_model::parse()` + `mc_model::validate()`
(NOT `mc_model::load()`) when it only needs the `ValidatedModel` for
dry-run validation without building a Cube.

---

## End-to-end data flow

This is the complete pipeline `mc tessera apply <recipe.yaml>` executes:

```
1. Parse recipe
   recipe_yaml -> mc_recipe::parse(&yaml) -> Recipe

2. Resolve model path
   recipe.model (relative to recipe file dir) -> absolute path

3. Validate recipe against model (early fail)
   mc_model::parse(&model_yaml) -> ParsedModel
   mc_model::validate(parsed) -> ValidatedModel
   mc_recipe::validate_recipe(&recipe, &validated_model, path_ctx) -> Vec<RecipeError>
   If errors: emit MC5xxx diagnostics, exit non-zero.

4. Load model into Cube
   mc_model::load(model_path) -> CompiledCube { cube, root_principal, refs }

5. Resolve credentials
   EnvVarSecretResolver.resolve() for each ${env.X} in recipe.credentials
   On failure: emit MC5013, exit.

6. Construct driver
   Match recipe.source.driver:
     Csv       -> mc_drivers::csv_driver(path)
     Sqlite    -> mc_drivers::sqlite_driver(path, query)
     Duckdb    -> mc_drivers::duckdb_driver(path, query)
     Postgres  -> mc_drivers::postgres_driver(dsn, query)
     ...
   On SourceFileNotFound: emit MC5014. On ConnectionFailed: emit MC5015.
   Box the driver as Box<dyn SourceDriver>.

7. Build column plan
   For each ColumnMapping in recipe.columns (where skip != true):
     If mapping.dimension: resolve dim_name -> DimensionId via refs.dimensions
     If mapping.measure: resolve measure_name -> ElementId via refs.element("Measure", name)
   For each default in recipe.defaults:
     Resolve dim_name -> DimensionId, element_name -> ElementId
   The column plan is a Vec<ResolvedColumnMapping> plus resolved defaults.

8. Create WriteBatch
   WriteBatch::new(&mut cube, WritebackContext {
       source_name: recipe.name,
       import_id: generated_uuid_or_timestamp,
       principal: root_principal,
   })

9. Fetch + transform + stage loop
   batch_size = recipe.batch.size.unwrap_or(50_000)
   while let Some(row_batch) = driver.fetch_batch(batch_size)? {
       for row_idx in 0..row_batch.row_count {
           // For each row:
           //   a. Resolve dimension columns -> ElementId
           //   b. Apply defaults for missing dimensions
           //   c. For each measure column: extract value, coerce to ScalarValue::F64
           //   d. Build CellCoordinate via ModelRefs::coord_from_names() or direct slot construction
           //   e. Stage: batch.push(coord, value)?
           // On row error: apply on_error policy (abort/skip/quarantine)
       }
   }

10. Commit
    let commit_result = batch.commit()?;

11. Persist sidecar
    Write imports/<import_id>.cells.jsonl
    Write snapshots/<snapshot_id>.cells.jsonl (using commit_result.snapshot_id)
    Append audit.jsonl
    Update active-imports.json

12. Return ImportReport
    ImportReport { import_id, rows_written, rows_failed, timing, snapshot_id, audit_path }
```

---

## Pointers to the actual upstream code

These are the real files in the merged codebase that Stream D integrates
against. Read the actual code, not the ADR appendices.

| What | File | Key types / functions |
|---|---|---|
| WriteBatch API | `crates/mc-core/src/batch.rs` | `WriteBatch`, `WritebackContext`, `CommitResult` |
| Cube + Snapshot | `crates/mc-core/src/cube.rs` | `Cube::snapshot()`, `Cube::rollback_to()`, `Cube::revision()`, `Cube::dimensions()` |
| CellCoordinate | `crates/mc-core/src/coordinate.rs` | `CellCoordinate::from_parts()`, `CellCoordinate::element_at()` |
| ScalarValue | `crates/mc-core/src/value.rs` | `ScalarValue::F64()`, `ScalarValue::Null` |
| EngineError | `crates/mc-core/src/error.rs` | All error variants WriteBatch can return |
| Recipe parse | `crates/mc-recipe/src/parse.rs` | `parse()`, `to_yaml()` |
| Recipe schema | `crates/mc-recipe/src/schema.rs` | `Recipe`, `SourceConfig`, `ColumnMapping`, `OnError`, `BatchConfig` |
| Recipe validate | `crates/mc-recipe/src/validate.rs` | `validate_recipe()`, `PathContext` |
| Recipe errors | `crates/mc-recipe/src/error.rs` | `RecipeError`, `RecipeError::code()` |
| Recipe diagnostics | `crates/mc-recipe/src/diagnostic.rs` | `Diagnostic`, `diagnostics_to_json()`, `sort_diagnostics()` |
| SourceDriver trait | `crates/mc-drivers/src/lib.rs` | `SourceDriver`, `RowBatch`, `Column`, `ColumnData`, `DriverError` |
| CSV driver | `crates/mc-drivers/src/csv_driver.rs` | `csv_driver()` |
| SQLite driver | `crates/mc-drivers/src/sqlite_driver.rs` | `sqlite_driver()` |
| Model load | `crates/mc-model/src/lib.rs` | `load()`, `load_str()` |
| Model compile | `crates/mc-model/src/compile.rs` | `CompiledCube`, `ModelRefs`, `ModelRefs::element()`, `ModelRefs::coord_from_names()` |
| Model validate | `crates/mc-model/src/validate.rs` | `validate()`, `ValidatedModel` |
| CLI structure | `crates/mc-cli/src/main.rs` | `main()` arg dispatch, `print_help()` |
| Acme fixture | `crates/mc-fixtures/src/lib.rs` | `build_acme_cube()`, `write_canonical_inputs()`, `AcmeRefs` |
| Acme model YAML | `crates/mc-model/examples/acme.yaml` | The YAML model the equivalence recipe targets |
| Acme inputs CSV | `crates/mc-model/examples/acme.inputs.csv` | The CSV the equivalence recipe reads |

---

## Reproducible commands

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

source $HOME/.cargo/env

# Pre-Stream-D gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                     # 502 / 0
cargo run --release --bin mc -- demo                       # unchanged

# After mc-tessera crate exists:
cargo build -p mc-tessera
cargo test -p mc-tessera
cargo clippy -p mc-tessera --all-targets -- -D warnings

# After CLI tessera verbs exist:
cargo run --release --bin mc -- tessera dry-run <recipe.yaml>
cargo run --release --bin mc -- tessera apply <recipe.yaml>
cargo run --release --bin mc -- tessera history <model_dir>
cargo run --release --bin mc -- tessera rollback <import_id> --model-dir <path>
cargo run --release --bin mc -- tessera audit <model_dir>

# Headline test: Acme CSV equivalence
cargo test -p mc-tessera -- acme_csv_equivalence

# Headline test: 100K SQLite performance
cargo test -p mc-tessera --release -- perf_100k_sqlite

# Verify locked surfaces:
git diff 6da91a5 -- crates/mc-core/ crates/mc-recipe/ crates/mc-drivers/ crates/mc-model/ crates/mc-fixtures/
# expected: zero output

# Forbidden pattern grep (mc-tessera/src/ only):
grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-tessera/src/

# Determinism gate:
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done
```

---

## Final checklist before you call Stream D done

- [ ] `crates/mc-tessera/` exists with `Cargo.toml`, `src/lib.rs`, and module structure.
- [ ] `Tessera::prepare()` loads recipe + model + driver + column plan.
- [ ] `Tessera::dry_run()` validates without writing; emits MC5xxx diagnostics.
- [ ] `Tessera::apply()` runs the full pipeline end-to-end.
- [ ] `Tessera::rollback()` restores pre-import state.
- [ ] `Tessera::history()` reads `.tessera/audit.jsonl`.
- [ ] `PreparedImport` struct resolves column mappings to concrete IDs.
- [ ] Transformation layer converts `RowBatch` to `Vec<(CellCoordinate, ScalarValue)>`.
- [ ] Element resolution via `ModelRefs::element()` with `on_missing_element` policy.
- [ ] `.tessera/` sidecar: `audit.jsonl`, `imports/`, `snapshots/`, `active-imports.json`, `quarantine/`.
- [ ] `SecretResolver` trait + `EnvVarSecretResolver` implemented.
- [ ] `ImportReport` with `TimingBreakdown` returned by `apply()`.
- [ ] `on_error: abort` = no partial commit (WriteBatch dropped).
- [ ] `on_error: skip_row` = log + skip + proceed.
- [ ] `on_error: quarantine` = write to quarantine log + proceed.
- [ ] CLI: `mc tessera apply <recipe>` works end-to-end.
- [ ] CLI: `mc tessera dry-run <recipe>` validates and exits.
- [ ] CLI: `mc tessera history <model_dir>` prints timeline.
- [ ] CLI: `mc tessera rollback <import_id>` restores state.
- [ ] CLI: `mc tessera audit <model_dir>` prints records.
- [ ] CLI: `--format text|json` works on all verbs.
- [ ] MC5013 fired on credential resolution failure.
- [ ] MC5014 fired on `DriverError::SourceFileNotFound`.
- [ ] MC5015 fired on `DriverError::ConnectionFailed`.
- [ ] **HEADLINE: Acme CSV equivalence test passes** — byte-identical cube state.
- [ ] **HEADLINE: 100K-row SQLite import <= 3 seconds** on release builds.
- [ ] Plugin skill at `mosaic-plugin/skills/import/tessera-usage/SKILL.md` exists with non-trivial content.
- [ ] All 502 existing tests still pass; new total >= 502 + Stream D additions.
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] Locked surfaces: `git diff 6da91a5 -- crates/mc-core/ crates/mc-recipe/ crates/mc-drivers/ crates/mc-model/ crates/mc-fixtures/` returns 0 lines.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-tessera/src/` (grep clean).
- [ ] No `unsafe`, no `async`, no `tokio`, no `rayon`, no threads in mc-tessera.
- [ ] Toolchain: `rust-toolchain.toml` unchanged; Cargo.lock pins intact.
- [ ] **You did NOT commit or tag.** The user reviews first.
- [ ] **You did NOT modify any locked crate.**

---

## Operating principles (carry-forward)

**Read this handoff fully + read ADR-0010 (Decisions 1, 2.5, 7, 13 +
Appendix D) before writing any code.** Stream D's contract is this
handoff; ADR-0010 is the strategic gate.

**The upstream code is real.** Unlike Streams A-C which coded against
frozen interface contracts, Stream D codes against MERGED, TESTED code.
Read the actual source files, not the ADR appendices. If the real API
differs from the appendix (e.g., `CommitResult` field names, driver
constructor signatures), the real code wins.

**The transformation layer is the hard part.** Parsing recipes and
constructing drivers is mechanical wiring. The subtle work is: (a)
correctly resolving dimension-element names from source row values to
`ElementId` via `ModelRefs`, (b) building `CellCoordinate` in the right
dimension order, (c) handling the case where one source row produces
multiple cells (one per mapped measure column), and (d) applying the
`on_error` policy correctly at the row level.

**The Acme CSV equivalence test is your north star.** If that test
passes, the transformation layer is correct. Write it first, watch it
fail, then implement until it passes. The equivalence test compares
against `mc_fixtures::write_canonical_inputs()` which is the
gold-standard reference for what the Acme cube should contain.

**Sidecar persistence is simple JSON Lines.** Don't over-engineer it.
`serde_json::to_string(&record)` + newline + append to file. The
`.tessera/` directory is ephemeral, debuggable with `cat` and `jq`,
and `.gitignore`-able. It is NOT a database.

If you are uncertain at any point, the resolution order is:

1. This handoff document.
2. ADR-0010 (Decisions 1, 2.5, 7, 13 + Appendix D).
3. The actual merged code in the upstream crates.
4. CLAUDE.md (operating manual).
5. The Phase 4A handoff (structural template reference only).
6. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md
section 11, and wait.

---

*Phase 5A Stream D handoff drafted 2026-05-04 against the merged main
branch at commit `6da91a5` (502/0 tests). Streams A, B, C are real,
tested code. Stream D is the integration point that turns three
component crates into a working Tessera engine.*
