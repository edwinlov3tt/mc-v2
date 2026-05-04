//! Tessera recipe schema — the public types every consumer (parser,
//! validator, future Stream D orchestrator, future Stream E LLM-authoring
//! plugin) deserializes / serializes against.
//!
//! Frozen by ADR-0010 Appendix B. Field names, enum variant names, and the
//! serde rename conventions below are part of the public contract — Phase
//! 5B (LLM-assisted recipe authoring) emits YAML against this exact schema,
//! and Phase 6 (UI) consumes it.
//!
//! Wire-format conventions:
//!
//! - Enum variants serialize as lowercase snake_case (`csv`, `duckdb_postgres`,
//!   `skip_row`, `replace`).
//! - Optional fields default to `None` / empty when absent in YAML.
//! - Collection fields (`defaults`, `credentials`) default to empty when
//!   absent. Required fields (`version`, `name`, `model`, `source`,
//!   `columns`) emit MC5007 at parse time when missing.
//!
//! See [`crate::diagnostic`] for the diagnostic envelope shape.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level Tessera recipe — the declarative contract between an external
/// data source and a Mosaic cube.
///
/// Per ADR-0010 Decision 7, a recipe declares: where data comes from
/// ([`source`](Recipe::source) + [`columns`](Recipe::columns)), how source
/// columns map to cube dimensions and measures
/// ([`columns`](Recipe::columns) + [`defaults`](Recipe::defaults)), and
/// what to do when things go wrong ([`on_error`](Recipe::on_error) +
/// [`on_missing_element`](Recipe::on_missing_element)).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Recipe {
    /// Recipe schema version. Must be `1` in Phase 5A; any other value
    /// fires MC5012.
    pub version: u32,

    /// Human-readable recipe name (free-form; not used for resolution).
    pub name: String,

    /// Optional description (free-form prose).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Path to the target Mosaic YAML model. Resolved relative to the
    /// recipe file's directory per ADR-0010 amendment #10. Path-escape
    /// outside the workspace root fires MC5017.
    pub model: String,

    /// Where the data comes from + how to read it.
    pub source: SourceConfig,

    /// One entry per source column: how that column maps into the cube.
    pub columns: Vec<ColumnMapping>,

    /// Static dimension-element assignments for dimensions not in the
    /// source data. Per ADR-0010 amendment #8, a dimension cannot appear
    /// in BOTH `columns:` and `defaults:` (MC5016).
    #[serde(default)]
    pub defaults: HashMap<String, String>,

    /// Phase 5A ships only [`WriteDisposition::Replace`]. `Append` and
    /// `Merge` are deferred to Phase 5C.
    #[serde(default)]
    pub write_disposition: WriteDisposition,

    /// Incremental load flag. Phase 5A: `false` only. `true` + watermark
    /// configuration deferred to Phase 5C.
    #[serde(default)]
    pub incremental: bool,

    /// Batch sizing for the streaming row-fetcher.
    #[serde(default)]
    pub batch: BatchConfig,

    /// Behavior when a row fails to materialize (type mismatch,
    /// ambiguous coordinate, kernel reject). Per ADR-0010 amendment #9.
    #[serde(default)]
    pub on_error: OnError,

    /// Behavior when a row references a dimension element that doesn't
    /// exist in the model. Phase 5A: only `Error`.
    #[serde(default)]
    pub on_missing_element: OnMissingElement,

    /// Credentials for source connection. Phase 5A supports `${env.VAR}`
    /// interpolation only; `${secret.ref}` is deferred to Phase 5E.
    #[serde(default)]
    pub credentials: HashMap<String, String>,
}

/// The source side of a recipe: which driver, and which driver-specific
/// fields are populated. Stream C (`mc-drivers`) consumes this to construct
/// the appropriate `SourceDriver` impl; Stream B (this crate) only validates
/// it.
///
/// Driver-specific field expectations (informally — driver-side validation
/// is Stream C's responsibility):
///
/// - **`csv`** — uses `path`. Other driver-specific fields ignored.
/// - **`sqlite`** / **`duckdb`** — uses `path` + (`query` XOR `table`).
/// - **`postgres`** — DSN supplied via `credentials`; uses `query`.
/// - **`duckdb_postgres`** — uses `path` (DuckDB) + `query` (Postgres).
/// - **`http_json`** — uses `url` + optional `json_path`.
///
/// MC5003 fires when both `query` and `table` are set.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SourceConfig {
    /// Which source driver to invoke.
    pub driver: DriverKind,

    /// File-system path (CSV, SQLite, DuckDB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// SQL query for query-based drivers (SQLite, DuckDB, Postgres,
    /// duckdb_postgres). Mutual exclusion with `table` (MC5003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,

    /// Plain table name for query-based drivers — equivalent to
    /// `SELECT * FROM <table>`. Mutual exclusion with `query` (MC5003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,

    /// HTTP(S) URL for the `http_json` driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// JSONPath expression for selecting rows from an HTTP/JSON response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_path: Option<String>,

    /// Source data layout format. Per ADR-0010 Amendment 2, `Wide` is
    /// the default (each column maps to one dimension or measure); `Long`
    /// means each row is one cell (measure name + value in dedicated
    /// columns).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<SourceFormat>,

    /// Configuration for long-format sources. Required when
    /// `format: Long`. Specifies which columns carry the measure name
    /// and the cell value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_format: Option<LongFormatConfig>,
}

/// Source data layout format. Per ADR-0010 Amendment 2.
///
/// - `Wide` (default): each non-skipped column maps 1:1 to a dimension or
///   measure.
/// - `Long`: each row is one cell; a dedicated column carries the measure
///   name and another carries the scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    /// Default — existing ADR-0010 behavior.
    Wide,
    /// Each row is one cell; measure name + value in dedicated columns.
    Long,
}

/// Configuration for long-format source data. Per ADR-0010 Amendment 2.
///
/// When `source.format: long`, these two fields identify the columns that
/// carry the measure name and the cell value for each row.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LongFormatConfig {
    /// Column whose values are measure names.
    pub measure_column: String,
    /// Column carrying the numeric value.
    pub value_column: String,
}

/// The closed enum of source drivers Phase 5A supports. Adding a new
/// driver is a Phase 5C breaking-change: the recipe schema enum is part
/// of the public contract and unrecognized variants fire MC5002.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    /// Local CSV file (path-based).
    Csv,
    /// SQLite database file (path + query).
    Sqlite,
    /// DuckDB database file (path + query).
    Duckdb,
    /// Postgres via DSN in credentials (query).
    Postgres,
    /// DuckDB attached to a remote Postgres instance.
    DuckdbPostgres,
    /// HTTP(S) endpoint returning JSON.
    HttpJson,
}

/// One row of the recipe `columns:` array. Maps a single source column
/// into the cube. Per ADR-0010 amendment #7, mappings are **1:1**: at
/// most one of [`dimension`](Self::dimension) /
/// [`measure`](Self::measure) is set. Both-set fires MC5011 (ambiguous
/// target); neither-set with `skip` not `true` also fires MC5011.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ColumnMapping {
    /// Source column name (must match the source schema).
    pub source: String,

    /// Target dimension name in the model. Mutually exclusive with
    /// [`measure`](Self::measure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,

    /// Target measure name in the model. Mutually exclusive with
    /// [`dimension`](Self::dimension). Must reference a measure with
    /// `role: "Input"` (Derived measures fire MC5018).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<String>,

    /// Optional declared source-column type (`f64`, `i64`, `string`,
    /// `bool`, `category`). When set, must be compatible with the
    /// target measure's declared `data_type` (MC5006). Case-insensitive.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,

    /// Optional numeric scale factor applied at row-transform time
    /// (Stream D). The recipe layer just records the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,

    /// Optional format string (e.g., `"%Y-%m"` for date columns).
    /// Driver-specific interpretation; Stream B does not interpret it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// `true` to skip this source column entirely. When set, all other
    /// targeting fields are ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<bool>,
}

/// How an import writes into the target cube. Phase 5A: only `Replace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WriteDisposition {
    /// Coordinate-level overwrite of cells produced by this recipe.
    /// Existing cells in the target slice that aren't produced by this
    /// recipe are NOT cleared. Per ADR-0010 amendment #4.
    #[default]
    Replace,
}

/// What to do when a single row fails to materialize. Per ADR-0010
/// amendment #9, behavioral semantics live in Stream D; this crate
/// only validates that the value is one of the three accepted variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// Transactional: any row error fails the import. No partial commit.
    #[default]
    Abort,
    /// Skip the failing row; remaining rows proceed.
    SkipRow,
    /// Write the failing row to the quarantine log; remaining rows
    /// proceed.
    Quarantine,
}

/// What to do when a row references a dimension element that does not
/// exist in the target model. Phase 5A only supports `Error`; `Create`
/// (auto-create elements at import) is deferred to Phase 5C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnMissingElement {
    /// Abort the import (or fail the row, depending on `on_error`).
    #[default]
    Error,
}

/// Batching configuration for the streaming row-fetcher. Stream D
/// applies the default of 50_000 when [`size`](Self::size) is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct BatchConfig {
    /// Rows per fetched batch. `None` → driver default (50_000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
}
