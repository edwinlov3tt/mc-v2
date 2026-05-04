# Phase 5A Stream B Handoff — Recipe Format Parser & Validator (`mc-recipe`)

> **Audience:** the Claude Code instance running in a git worktree at
> `../mc-v2-stream-b` on branch `phase-5a/stream-b-recipe-format`.
> **You inherit Phase 4B** (416/0 tests passing, commit `b5b6229`, tag
> `phase-4b-python-adapters`).
>
> **Stream B does NOT touch any existing crate.** It creates
> `crates/mc-recipe/` only. No modifications to `mc-core`, `mc-model`,
> `mc-fixtures`, `mc-cli`, or `mc-drivers`. The locked-surfaces
> guarantee from all prior phases carries forward unconditionally.
>
> **Read [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
> Decision 7 + Appendix B BEFORE this handoff.** ADR-0010 is the
> Accepted strategic gate for Phase 5. Appendix B is the frozen
> interface contract for this stream.

---

## The one paragraph you must internalize before writing code

**The recipe is the declarative contract between external data and the
cube.** It is what makes ingestion schema-validated and LLM-authorable.
A recipe declares: where data comes from (source + driver), how source
columns map to cube dimensions and measures (column mappings + defaults),
and what to do when things go wrong (on_error + diagnostics). The recipe
format IS the Phase 5B LLM-authoring surface — get it right in 5A, and
5B is pure translation work (the plugin skill teaches an LLM to emit
valid recipes the same way it teaches valid model YAML today). The
mc-recipe crate is a parser + validator library ONLY. It does not
connect to sources, fetch data, or write to cubes. Those are Stream C
(drivers) and Stream D (orchestrator) responsibilities.

---

## ADR-0010 amendments affecting Stream B

| # | Amendment | Impact on Stream B |
|---|---|---|
| **7** | Column mappings are 1:1 in Phase 5A. | Validator rejects any recipe where a single source column targets multiple cube dimensions/measures. One `source:` column maps to ONE `dimension:` OR ONE `measure:`. |
| **8** | Defaults vs. columns mutual exclusion + MC5016. | A dimension cannot appear in both `columns:` (with `dimension: X`) and `defaults: { X: ... }`. Validator fires MC5016. |
| **9** | `on_error` semantics (abort/skip_row/quarantine). | Schema enums + serde deserialization must accept all three; behavioral semantics are Stream D's problem but the types live here. |
| **10** | `model:` path resolution relative to recipe file directory + MC5017. | Validator resolves the model path relative to the recipe file's parent directory. Path-escape outside the workspace root fires MC5017. |
| **2** | Input measures only + MC5018. | Column mappings targeting a Derived measure are rejected at recipe validation time. Validator loads the `ValidatedModel` and checks each mapped measure's `role` field. |
| **4** | `write_disposition: replace` = coordinate-level overwrite only. | Phase 5A ships only `Replace` in the `WriteDisposition` enum. `Append` and `Merge` are deferred to Phase 5C. |

---

## Where Phase 4B ended

- **Phase 4B commit / tag:** `b5b6229` — tag `phase-4b-python-adapters`.
- **Test status:** 416/0 passing across all workspace targets. 10/10 deterministic.
- **Toolchain:** Rust 1.78. All existing Cargo.lock pins intact (Phase 1B: `clap`, `clap_lex`, `half`; Phase 3A: `indexmap`, `hashbrown`).
- **Workspace members:** `mc-core`, `mc-fixtures`, `mc-cli`, `mc-model`.
- **Diagnostic-code registry:** MC1001-MC1006 (parse), MC2001-MC2025 (validation), MC3001-MC3011 (lint, MC3008 retired), MC4xxx (reserved). JSON envelope: `{ "schema_version": "1.0", "diagnostics": [...] }`.
- **Dependencies already in workspace `Cargo.toml`:** `serde`, `serde_yaml`, `thiserror` (all used by `mc-model`). Stream B uses these — no new workspace-level deps needed.

---

## Stream B prompt (verbatim binding contract)

> We are starting Mosaic Phase 5A Stream B: the Recipe Format Parser & Validator.
>
> **Goal.** Ship a complete `crates/mc-recipe/` crate that parses Tessera recipe YAML into typed Rust structs, validates recipes against a loaded `mc-model::ValidatedModel`, and emits MC5xxx diagnostic codes in the Phase 3B JSON envelope shape.
>
> **Phase 5A Stream B scope** (binding contract):
>
> 1. **New crate `crates/mc-recipe/`** with `Cargo.toml`:
>    ```toml
>    [package]
>    name = "mc-recipe"
>    version = "0.1.0"
>    edition = "2021"
>    description = "Mosaic Tessera recipe format parser and validator."
>
>    [dependencies]
>    serde = { workspace = true }
>    serde_yaml = { workspace = true }
>    thiserror = { workspace = true }
>    mc-model = { path = "../mc-model" }
>    ```
>    The `mc-model` dependency is required for validation against `ValidatedModel`. No other dependencies.
>
> 2. **Recipe struct + serde deserialization** matching ADR-0010 Appendix B types exactly:
>
>    ```rust
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub struct Recipe {
>        pub version: u32,                              // must be 1
>        pub name: String,
>        pub description: Option<String>,
>        pub model: String,                             // path to target model YAML
>        pub source: SourceConfig,
>        pub columns: Vec<ColumnMapping>,
>        pub defaults: HashMap<String, String>,         // dim_name -> element_name
>        pub write_disposition: WriteDisposition,
>        pub incremental: bool,
>        pub batch: BatchConfig,
>        pub on_error: OnError,
>        pub on_missing_element: OnMissingElement,
>        pub credentials: HashMap<String, String>,
>    }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub struct SourceConfig {
>        pub driver: DriverKind,
>        pub path: Option<String>,
>        pub query: Option<String>,
>        pub table: Option<String>,       // mutual exclusion with query
>        pub url: Option<String>,         // for http_json driver
>        pub json_path: Option<String>,
>    }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub enum DriverKind { Csv, Sqlite, Duckdb, Postgres, DuckdbPostgres, HttpJson }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub struct ColumnMapping {
>        pub source: String,
>        pub dimension: Option<String>,
>        pub measure: Option<String>,
>        #[serde(rename = "type")]
>        pub data_type: Option<String>,
>        pub scale: Option<f64>,
>        pub format: Option<String>,
>        pub skip: Option<bool>,
>    }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub enum WriteDisposition { Replace }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub enum OnError { Abort, SkipRow, Quarantine }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub enum OnMissingElement { Error }
>
>    #[derive(Debug, Clone, Deserialize, Serialize)]
>    pub struct BatchConfig {
>        pub size: Option<usize>,         // default 50_000
>    }
>    ```
>
>    Serde rename rules: `DriverKind` variants serialize as lowercase snake_case (`csv`, `sqlite`, `duckdb`, `postgres`, `duckdb_postgres`, `http_json`). `WriteDisposition::Replace` serializes as `"replace"`. `OnError` variants serialize as `"abort"`, `"skip_row"`, `"quarantine"`. `OnMissingElement::Error` serializes as `"error"`.
>
> 3. **Recipe validator** that checks a parsed `Recipe` against a loaded `mc_model::ValidatedModel`:
>    - Dimension name resolution: every `columns[i].dimension` must match a dimension in the model.
>    - Measure name resolution: every `columns[i].measure` must match a measure in the model.
>    - Measure type compatibility: column `data_type` (when specified) must be compatible with the target measure's declared `data_type`.
>    - Input-measure-only check: every mapped measure must have `role: "Input"` in the model. Derived measures fire MC5018.
>    - Defaults mutual exclusion: no dimension may appear in both `columns` mappings and `defaults`. Fires MC5016.
>    - Default dimension/element resolution: every key in `defaults` must be a declared dimension; every value must be a declared element within that dimension.
>    - Model path resolution: `model:` field resolved relative to recipe file directory. Path-escape outside workspace root fires MC5017.
>
> 4. **18 diagnostic codes MC5001-MC5018** in the Phase 3B JSON envelope shape:
>
>    | Code | Fires when |
>    |---|---|
>    | MC5001 | Recipe YAML parse error (syntax) |
>    | MC5002 | Unknown driver kind |
>    | MC5003 | Both `table:` and `query:` specified (mutual exclusion) |
>    | MC5004 | Column references unknown dimension in target model |
>    | MC5005 | Column references unknown measure in target model |
>    | MC5006 | Column type incompatible with target measure type |
>    | MC5007 | Missing required field (e.g., `source.driver`, `model`, `columns`) |
>    | MC5008 | Default references unknown dimension |
>    | MC5009 | Default references unknown element in the named dimension |
>    | MC5010 | Duplicate column mapping (same source column mapped twice) |
>    | MC5011 | No dimension/measure mapping and `skip: false` (column goes nowhere) |
>    | MC5012 | Invalid `version:` (not 1) |
>    | MC5013 | Credential interpolation failure (`${env.X}` where X is unset) |
>    | MC5014 | Source file not found or not readable (path error, permission denied) |
>    | MC5015 | Source connection failure (unreachable DSN, HTTP endpoint down) |
>    | MC5016 | Dimension appears in both `columns:` and `defaults:` (mutual exclusion) |
>    | MC5017 | `model:` path escapes the workspace root (path-traversal protection) |
>    | MC5018 | Column maps to non-writeable measure (Derived role in model) |
>
>    Diagnostic envelope shape (unchanged from Phase 3B):
>    ```json
>    {
>      "schema_version": "1.0",
>      "diagnostics": [
>        { "code": "MC5004", "severity": "error", "path": "/columns/2", "message": "..." }
>      ]
>    }
>    ```
>    Deterministic emission order: `(severity desc, code asc, path asc, message asc)`.
>
> 5. **Roundtrip stability:** `parse(serialize(parse(recipe))) == parse(recipe)` for all example recipes. Implement `Serialize` on all recipe types. A `#[test]` asserts this property.
>
> 6. **Library of example recipes** at `crates/mc-recipe/examples/recipes/`:
>    - `acme-csv-import.recipe.yaml` — minimal CSV import into the Acme model (maps Spend + CPC to Input measures; defaults Scenario=Baseline, Version=Working).
>    - `acme-sqlite-import.recipe.yaml` — SQLite query import with all 6 input measures.
>    - `acme-duckdb-import.recipe.yaml` — DuckDB query import.
>    - `acme-postgres-import.recipe.yaml` — Postgres query import (credentials use `${env.PG_DSN}`).
>    - `acme-http-json-import.recipe.yaml` — HTTP/JSON endpoint import.
>    - `acme-invalid-derived.recipe.yaml` — intentionally maps to Derived measure Clicks (fires MC5018).
>    - `acme-invalid-mutual-exclusion.recipe.yaml` — dimension in both columns and defaults (fires MC5016).
>    - `acme-invalid-unknown-dim.recipe.yaml` — references non-existent dimension (fires MC5004).
>
> 7. **Recipe-format documentation as a plugin skill file** at `mosaic-plugin/skills/import/recipe-format/SKILL.md`. Teaches an LLM:
>    - The full recipe schema with field descriptions.
>    - The 6 semantic rules (1:1 mappings, defaults exclusion, input-only, replace semantics, model path resolution, on_error behavior).
>    - All 18 MC5xxx codes with firing conditions and fix patterns.
>    - Example recipes for each driver type.
>
> 8. **NO runtime execution.** mc-recipe does not connect to data sources, fetch rows, transform data, or write to cubes. It is a parser + validator library only. Runtime execution is Stream D's responsibility.
>
> 9. **NO changes to existing crates.** `mc-core`, `mc-model`, `mc-fixtures`, `mc-cli`, `mc-drivers` are all untouched. `git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-model/ crates/mc-fixtures/ crates/mc-cli/` must return zero lines.
>
> 10. **`schema_version` stays at `"1.0"`** in the diagnostic JSON envelope.
>
> **Hard rules:**
>
> - No existing crate modifications. Stream B creates `crates/mc-recipe/` only (plus the plugin skill file and workspace Cargo.toml member addition).
> - No execution logic. No `SourceDriver` usage. No `WriteBatch` usage. No network calls. No file reads beyond loading the model YAML for validation.
> - mc-recipe is a parser + validator library. Its public API is: parse a recipe from YAML bytes/string/path, validate against a `ValidatedModel`, emit diagnostics.
> - No `unsafe`. No `async`. No `tokio`. No threads.
> - No `unwrap()` / `expect()` / `panic!()` in `crates/mc-recipe/src/`. All fallible paths return `Result`.
> - The only dependencies are `serde`, `serde_yaml`, `thiserror`, and `mc-model` (path dep).
> - All recipe type names, field names, enum variants, and diagnostic codes match ADR-0010 Appendix B exactly. No synonyms, no abbreviations.
>
> **SPEC QUESTION triggers:**
>
> 1. The recipe schema needs a field nobody anticipated (e.g., a driver-specific config block that doesn't fit `SourceConfig`'s current shape). Surface before adding.
> 2. A diagnostic code in the MC5xxx namespace collides with something another stream is implementing. Surface before proceeding.
> 3. The validator needs `mc-core` types it cannot access without adding `mc-core` as a dependency (e.g., `MeasureRole` enum for the Input-only check). Resolve by reading the role from `ValidatedModel.parsed.measures[i].role` (a string), NOT by importing mc-core types. If this is insufficient, surface.
> 4. Serde deserialization of `HashMap<String, String>` for `defaults` conflicts with YAML's type coercion (e.g., YAML interpreting `true` as a boolean instead of the string `"true"`). Surface if quoting rules are needed.
> 5. The model path resolution needs filesystem operations (canonicalize, exists check) that feel like "execution." Path resolution for validation purposes (does the model file exist and load?) is IN scope. Actually connecting to the source is NOT.
>
> **Acceptance gates:**
>
> 1. All example recipes parse cleanly via `mc_recipe::parse()`.
> 2. All example recipes validate against the Acme fixture model OR produce expected diagnostic envelopes when intentionally broken.
> 3. Roundtrip stability: `parse(serialize(parse(recipe))) == parse(recipe)` for all valid example recipes.
> 4. `schema_version` stays at `"1.0"` in the diagnostic JSON envelope.
> 5. All 416 existing tests still pass unchanged.
> 6. `git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-model/ crates/mc-fixtures/ crates/mc-cli/` returns zero lines.
> 7. `cargo fmt --check --all` exits 0.
> 8. `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
> 9. `cargo test --workspace` passes (416 + Stream B additions).
> 10. Zero `unwrap()` / `expect()` / `panic!()` in `crates/mc-recipe/src/`.

---

## Context: The Acme model's dimension/measure inventory

Recipes validate column mappings and defaults against the model. The Acme model (the canonical fixture for testing) has:

**Dimensions** (canonical order): Scenario, Version, Time, Channel, Market, Measure.

**Dimension elements (leaf-level for recipe defaults):**
- Scenario: Baseline, Aggressive, Conservative
- Version: Working, Submitted, Approved
- Time: Jan_2026 through Dec_2026 (12 months) + Q1-Q4_2026 + FY_2026 (consolidated)
- Channel: Paid_Search, Paid_Social, Display, Email, Organic (leaves); Paid_Media, Owned_Earned, All_Channels (consolidated)
- Market: Tampa, Orlando, Miami, Atlanta, Charlotte, New_York_City, Boston (leaves); Florida, Georgia, North_Carolina, New_York_State, Massachusetts, Southeast, Northeast, USA (consolidated)

**Measures (Input — writable by recipes):**
- Spend (F64, Sum)
- CPC (F64, WeightedAverage by Spend)
- CVR (F64, WeightedAverage by Clicks)
- Close_Rate (F64, WeightedAverage by Leads)
- AOV (F64, WeightedAverage by Customers)
- COGS_Rate (F64, WeightedAverage by Revenue)

**Measures (Derived — NOT writable, MC5018 fires):**
- Clicks (F64, Sum) — `Spend / CPC`
- Leads (F64, Sum) — `Clicks * CVR`
- Customers (F64, Sum) — `Leads * Close_Rate`
- Revenue (F64, Sum) — `Customers * AOV`
- Gross_Profit (F64, Sum) — `Revenue * (1 - COGS_Rate)`

The validator uses `ValidatedModel.parsed.measures[i].role` (string `"Input"` or `"Derived"`) to determine writeability. It uses `ValidatedModel.dim_index_by_name` and `ValidatedModel.element_index_by_name` for name resolution. It uses `ValidatedModel.measure_index_by_name` to look up measures.

---

## Context: The 6 recipe semantic rules (from ADR-0010 amendments)

These are binding for the validator:

1. **Column mappings are 1:1.** A single source column maps to either one dimension OR one measure. A column mapping with BOTH `dimension` and `measure` set is invalid. A column mapping with neither set and `skip` not true fires MC5011.

2. **Defaults vs. columns mutual exclusion.** A dimension name cannot appear in both a `columns[i].dimension` field and a key in `defaults`. Fires MC5016 naming the conflicting dimension.

3. **Input measures only.** Every `columns[i].measure` must reference a measure with `role: "Input"` in the model. Mapping to a Derived measure fires MC5018 naming the measure and explaining that derived cells are computed by rules.

4. **`write_disposition: replace` = coordinate-level overwrite.** Phase 5A only supports `Replace`. The validator does not need to enforce runtime semantics, but the schema enum must reject unknown values at parse time.

5. **`model:` path resolution.** Resolved relative to the recipe file's directory (same pattern as Phase 3C `canonical_inputs.source`). Path-escape outside the workspace root (detected by canonicalizing both paths and checking prefix containment) fires MC5017.

6. **`on_error:` semantics.** The validator accepts `abort`, `skip_row`, and `quarantine` as valid enum values. Behavioral enforcement is Stream D's job. The types must be correctly defined and (de)serializable.

---

## Context: The Phase 3B diagnostic shape

MC5xxx diagnostics follow the same JSON envelope as MC1xxx-MC3xxx:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC5004",
      "severity": "error",
      "path": "/columns/2/dimension",
      "message": "column \"market_region\" references unknown dimension \"Region\" in target model"
    }
  ]
}
```

Fields: `code` (string, `MC5xxx`), `severity` (string, `"error"` for all MC5xxx in Phase 5A), `path` (JSON pointer into the recipe YAML structure), `message` (human-readable).

Deterministic emission order: sort by `(severity desc, code asc, path asc, message asc)`. This matches the existing `mc-model` diagnostic ordering.

The `RecipeError` enum in mc-recipe should follow the same pattern as `mc_model::ValidationError`: each variant has a `.code() -> &'static str` method returning the stable MC5xxx code. Consider also implementing `thiserror::Error` with structured display messages.

---

## Pointers to existing files for reference

| Why | File |
|---|---|
| Model schema types (what validator checks against) | `crates/mc-model/src/schema.rs` |
| Existing diagnostic code pattern | `crates/mc-model/src/error.rs` |
| Acme model YAML (canonical fixture for tests) | `crates/mc-model/examples/acme.yaml` |
| Acme inputs CSV | `crates/mc-model/examples/acme.inputs.csv` |
| ADR-0010 (frozen interface contract) | `docs/decisions/0010-phase-5-tessera-architecture.md` |
| Existing workspace Cargo.toml | `Cargo.toml` (workspace root) |
| Plugin skill directory (where recipe-format SKILL.md goes) | `mosaic-plugin/skills/` |

---

## Reproducible commands

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# Pre-stream-B gate (must remain green throughout)
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                    # 416/0

# Iteration loop (Stream B only)
cargo build -p mc-recipe
cargo test -p mc-recipe
cargo clippy -p mc-recipe -- -D warnings

# Forbidden pattern check
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-recipe/src/
# expected: zero matches

grep -rn "unsafe" crates/mc-recipe/src/
# expected: zero matches

# Locked surfaces verification
git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-model/ crates/mc-fixtures/ crates/mc-cli/
# expected: zero output

# Determinism gate
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Roundtrip stability (run after tests exist)
cargo test -p mc-recipe -- roundtrip
```

---

## Final checklist before calling Stream B done

- [ ] `crates/mc-recipe/` exists with `Cargo.toml`, `src/lib.rs`, and module files.
- [ ] `Cargo.toml` at workspace root lists `crates/mc-recipe` as a member.
- [ ] All Recipe types match ADR-0010 Appendix B signatures exactly.
- [ ] Serde (de)serialization works for all recipe types with correct rename rules.
- [ ] `Serialize` is derived/implemented (needed for roundtrip stability).
- [ ] All 18 diagnostic codes (MC5001-MC5018) are implemented with correct firing conditions.
- [ ] Diagnostic envelope uses `schema_version: "1.0"` and deterministic sort order.
- [ ] Validator loads `ValidatedModel` via `mc_model` and checks all 6 semantic rules.
- [ ] Roundtrip test passes: `parse(serialize(parse(r))) == parse(r)` for all valid examples.
- [ ] 8 example recipes exist at `crates/mc-recipe/examples/recipes/`.
- [ ] Valid examples parse and validate cleanly against the Acme model.
- [ ] Invalid examples produce the expected MC5xxx diagnostics.
- [ ] Plugin skill file exists at `mosaic-plugin/skills/import/recipe-format/SKILL.md`.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-recipe/src/`.
- [ ] No `unsafe`, no `async`, no `tokio`, no threads.
- [ ] Only deps: `serde`, `serde_yaml`, `thiserror`, `mc-model`.
- [ ] All 416 existing tests still pass.
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] Locked surfaces: zero-line diff on mc-core, mc-model, mc-fixtures, mc-cli.
- [ ] No runtime execution logic (no network, no source connections, no WriteBatch).
- [ ] **You did NOT modify any existing crate.**
- [ ] **You did NOT start Stream A, C, or D work.**
- [ ] **You did NOT commit, tag, or push.** The user does that after review.

---

*Phase 5A Stream B handoff drafted 2026-05-04 immediately after [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) was Accepted (same day). Stream B is self-contained: new crate only, no kernel or model changes, no cross-stream dependencies beyond the frozen Appendix B interface contract.*
