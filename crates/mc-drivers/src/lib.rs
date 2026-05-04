//! Mosaic source drivers — `SourceDriver` trait + 6 reference implementations.
//!
//! This crate is Phase 5A Stream C of the Tessera ingestion engine. It owns
//! the driver abstraction every external source implements, plus 6 concrete
//! drivers (CSV, SQLite, DuckDB, Postgres, DuckDB-federated-Postgres, HTTP/JSON).
//!
//! The trait + supporting types are frozen by ADR-0010 Appendix C. The drivers
//! are reference implementations: simple, synchronous, cancel-aware. Schema
//! inference is deterministic and documented per driver.
//!
//! All public types are `#[derive(Debug)]`. All drivers are sync — no `async`,
//! no `.await`, no `tokio::*` in source. The `postgres` crate is the direct
//! dependency (sync wrapper) and brings tokio in only as a transitive — the
//! `dependency_gate` test (gated behind feature `"dependency-gate"`) enforces
//! this mechanically.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::PathBuf;

mod csv_driver;
mod duckdb_driver;
mod duckdb_postgres_driver;
mod http_json_driver;
mod postgres_driver;
mod sqlite_driver;

pub use csv_driver::csv_driver;
pub use duckdb_driver::duckdb_driver;
pub use duckdb_postgres_driver::duckdb_postgres_driver;
pub use http_json_driver::http_json_driver;
pub use postgres_driver::postgres_driver;
pub use sqlite_driver::sqlite_driver;

// ============================================================================
// SourceDriver trait + supporting types (frozen by ADR-0010 Appendix C).
// ============================================================================

/// The contract every Mosaic source driver implements.
///
/// Drivers are stateful — `fetch_batch` advances an internal cursor and
/// returns `Ok(None)` when exhausted. `schema()` may be called at any time
/// (it does not consume rows). `cancel()` is cooperative: after it is called,
/// the next `fetch_batch()` returns `Ok(None)`.
///
/// The trait is object-safe so Stream D (Tessera) can hold a
/// `Box<dyn SourceDriver>`.
pub trait SourceDriver {
    /// Return the schema (column names + types) without fetching data.
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError>;

    /// Fetch the next batch of rows. Returns `Ok(None)` when exhausted
    /// (or after `cancel()` has been called).
    ///
    /// `max_rows` is an upper bound; drivers may return fewer rows per
    /// batch (e.g., when the underlying source streams in smaller chunks).
    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError>;

    /// Cooperative cancellation. After this is called, the next
    /// `fetch_batch()` returns `Ok(None)` (exhausted). Calling `cancel()`
    /// repeatedly is a no-op.
    fn cancel(&mut self);
}

/// A batch of rows from an external source, in column-oriented layout.
///
/// `columns.len()` is the number of columns; every `ColumnData` inside
/// `columns` has length equal to `row_count`.
#[derive(Debug, Clone)]
pub struct RowBatch {
    /// The columns in this batch, in source order.
    pub columns: Vec<Column>,
    /// The number of rows in this batch.
    pub row_count: usize,
}

/// A single named column inside a `RowBatch`.
#[derive(Debug, Clone)]
pub struct Column {
    /// The column's name as reported by the source.
    pub name: String,
    /// The column's typed payload. Length matches the enclosing
    /// `RowBatch::row_count`.
    pub data: ColumnData,
}

/// The typed, nullable payload of a single column.
///
/// Each variant carries `Vec<Option<T>>` — `None` is SQL NULL or its
/// equivalent in the source format (e.g., empty CSV cell, missing JSON key).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ColumnData {
    /// 64-bit floating-point column.
    F64(Vec<Option<f64>>),
    /// 64-bit signed integer column.
    I64(Vec<Option<i64>>),
    /// UTF-8 string column.
    Str(Vec<Option<String>>),
    /// Boolean column.
    Bool(Vec<Option<bool>>),
}

/// A single column's name + declared type + nullability, as reported by
/// `SourceDriver::schema()`.
#[derive(Debug, Clone)]
pub struct ColumnSchema {
    /// Column name.
    pub name: String,
    /// Column data type.
    pub data_type: ColumnDataType,
    /// Whether the column may contain NULL values.
    pub nullable: bool,
}

/// The four data types Mosaic accepts from external sources.
///
/// Wider types (e.g., decimals, timestamps) are mapped to one of these
/// at the driver layer. Stream D / `mc-tessera` is responsible for any
/// further coercion into Mosaic `ScalarValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ColumnDataType {
    /// 64-bit floating-point.
    F64,
    /// 64-bit signed integer.
    I64,
    /// UTF-8 string.
    Str,
    /// Boolean.
    Bool,
}

// ============================================================================
// DriverError — thiserror-derived, with enough context for MC5014/MC5015
// diagnostic mapping in Stream D (mc-tessera).
// ============================================================================

/// Errors returned by drivers and driver constructors.
///
/// Stream D (`mc-tessera`) maps these variants to MC5xxx diagnostic codes:
/// `SourceFileNotFound` → MC5014, `ConnectionFailed` → MC5015. The other
/// variants surface as MC5xxx codes per the ADR-0010 Appendix B table.
/// Variant payloads always include the originating path / DSN / URL /
/// query so Stream D can populate diagnostic envelopes without re-reading
/// the recipe.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DriverError {
    /// The source file could not be opened (typically file-not-found,
    /// permission-denied, or unreadable). Maps to MC5014.
    #[error("source file not found or not readable: {path:?} ({message})")]
    SourceFileNotFound {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying OS / library message.
        message: String,
    },

    /// A general I/O error against the source file (read failure mid-stream,
    /// EOF inside a record, etc.).
    #[error("i/o error on {path:?}: {message}")]
    Io {
        /// Path that errored.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },

    /// Connection to a remote source (Postgres DSN, HTTP URL) failed
    /// before any data could be fetched. Maps to MC5015.
    #[error("connection failed to {target}: {message}")]
    ConnectionFailed {
        /// Connection target (DSN, URL, or human-readable label).
        target: String,
        /// Underlying message.
        message: String,
    },

    /// A SQL query failed at prepare or execute time.
    #[error("query failed: {message}\n  query: {query}")]
    QueryFailed {
        /// The SQL text that failed.
        query: String,
        /// Underlying engine message.
        message: String,
    },

    /// The source reported a column type the driver does not know how to
    /// map onto `ColumnDataType` (F64 / I64 / Str / Bool).
    #[error("unsupported column type for {column:?}: {type_name}")]
    UnsupportedType {
        /// Column name.
        column: String,
        /// Source type name (driver-specific).
        type_name: String,
    },

    /// While fetching a row, the value did not match the column's declared
    /// type (e.g., a string in an `i64` column, or a non-finite float).
    #[error("type mismatch on column {column:?}: {message}")]
    TypeMismatch {
        /// Column name.
        column: String,
        /// Description of the mismatch.
        message: String,
    },

    /// The source returned malformed data (e.g., short CSV record, invalid
    /// JSON, truncated payload).
    #[error("malformed source data: {message}")]
    MalformedSource {
        /// Description of the malformation.
        message: String,
    },

    /// HTTP request returned a non-2xx status.
    #[error("http {status} from {url}: {body_preview}")]
    HttpStatus {
        /// URL that was requested.
        url: String,
        /// HTTP status code.
        status: u16,
        /// First ~200 bytes of the response body for diagnostic context.
        body_preview: String,
    },

    /// The HTTP/JSON `json_path` selector did not resolve to an array of
    /// rows in the response.
    #[error("json path {json_path:?} did not select an array in response from {url}")]
    JsonPathNotArray {
        /// URL that was requested.
        url: String,
        /// JSON path that was attempted.
        json_path: String,
    },

    /// A DuckDB extension (e.g., `postgres_scanner`) failed to install
    /// or load.
    #[error("duckdb extension {extension:?} failed to load: {message}")]
    ExtensionLoadFailed {
        /// Extension name.
        extension: String,
        /// Underlying message.
        message: String,
    },
}
