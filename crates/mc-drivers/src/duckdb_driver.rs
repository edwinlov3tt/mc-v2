//! DuckDB source driver.
//!
//! Wraps `duckdb-rs` with the `bundled` feature (statically linked DuckDB —
//! zero system library required).
//!
//! ## ICU limitation
//!
//! Bundled DuckDB 1.3.2 does NOT include the ICU extension. Locale-aware
//! date/time functions (`strftime` with locale specifiers, `monthname()`,
//! locale collation, etc.) are unavailable. Use ISO 8601 date formats and
//! locale-independent expressions in queries.
//!
//! ## Buffering model
//!
//! Same eager-materialisation strategy as the SQLite driver: all rows are
//! collected at construction time, then drained in `max_rows` chunks. See
//! `sqlite_driver` module docs for rationale.
//!
//! ## Schema inference rule
//!
//! Schema is taken from `Statement::column_type(i)`. The mapping covers
//! DuckDB's primary scalar types:
//!
//! | DuckDB type family | Mosaic `ColumnDataType` |
//! | --- | --- |
//! | `BOOLEAN` | `Bool` |
//! | `TINYINT`, `SMALLINT`, `INTEGER`, `BIGINT`, `HUGEINT`, all unsigned ints | `I64` |
//! | `FLOAT`, `DOUBLE`, `DECIMAL` | `F64` |
//! | `VARCHAR`, `BLOB`, `DATE`, `TIME`, `TIMESTAMP`, `UUID`, `INTERVAL`, lists, structs, maps | `Str` (rendered via `Display`) |
//!
//! Composite / unsupported types fall back to `Str` because Mosaic's
//! ingestion model is scalar; complex shapes are JSON-stringified at the
//! driver boundary so Stream D can decide whether to coerce or reject.
//! `nullable` is reported as `true` for all columns (DuckDB query results
//! do not expose statement-level NOT NULL constraints).

use std::path::{Path, PathBuf};

use duckdb::types::{TimeUnit, Type as DuckType, ValueRef};
use duckdb::Connection;

use crate::{
    Column, ColumnData, ColumnDataType, ColumnSchema, DriverError, RowBatch, SourceDriver,
};

/// Construct a DuckDB driver.
///
/// Pass `Path::new(":memory:")` for an in-memory database. Bundled DuckDB
/// has no ICU extension; see module docs.
pub fn duckdb_driver(path: &Path, query: &str) -> Result<DuckDbDriver, DriverError> {
    DuckDbDriver::new(path, query)
}

/// DuckDB driver. After construction, the underlying connection is closed
/// and `fetch_batch` drains in-memory column buffers.
#[derive(Debug)]
pub struct DuckDbDriver {
    // Surfaced via Debug for diagnostics; not read by the hot path. See
    // SqliteDriver's identical fields for rationale.
    #[allow(dead_code)]
    path: PathBuf,
    #[allow(dead_code)]
    query: String,
    schema: Vec<ColumnSchema>,
    buffers: Vec<ColumnBuffer>,
    cursor: usize,
    total_rows: usize,
    cancelled: bool,
}

impl DuckDbDriver {
    fn new(path: &Path, query: &str) -> Result<Self, DriverError> {
        if !path_is_memory(path) && !path.exists() {
            return Err(DriverError::SourceFileNotFound {
                path: path.to_path_buf(),
                message: "no such file".to_string(),
            });
        }
        let conn = open_connection(path)?;
        materialise(&conn, path, query)
    }

    /// Used by the DuckDB-federated-Postgres driver, which needs to run
    /// extension setup statements on the same connection before issuing
    /// the user query.
    pub(crate) fn from_prepared_connection(
        conn: &Connection,
        path: &Path,
        query: &str,
    ) -> Result<Self, DriverError> {
        materialise(conn, path, query)
    }
}

pub(crate) fn open_connection(path: &Path) -> Result<Connection, DriverError> {
    if path_is_memory(path) {
        Connection::open_in_memory().map_err(|e| DriverError::ConnectionFailed {
            target: ":memory:".to_string(),
            message: e.to_string(),
        })
    } else {
        Connection::open(path).map_err(|e| DriverError::ConnectionFailed {
            target: path.display().to_string(),
            message: e.to_string(),
        })
    }
}

fn materialise(conn: &Connection, path: &Path, query: &str) -> Result<DuckDbDriver, DriverError> {
    let mut stmt = conn.prepare(query).map_err(|e| DriverError::QueryFailed {
        query: query.to_string(),
        message: e.to_string(),
    })?;

    // duckdb-rs 1.1.x populates Statement schema/column_type only AFTER
    // `query()` has executed and the first row has been pulled. We init
    // schema + buffers lazily on the first row, then push values directly
    // through ColumnBuffer::push for each subsequent row.
    let mut schema: Vec<ColumnSchema> = Vec::new();
    let mut buffers: Vec<ColumnBuffer> = Vec::new();
    let mut total_rows = 0usize;

    {
        let mut rows = stmt.query([]).map_err(|e| DriverError::QueryFailed {
            query: query.to_string(),
            message: e.to_string(),
        })?;

        loop {
            let row = rows.next().map_err(|e| DriverError::QueryFailed {
                query: query.to_string(),
                message: e.to_string(),
            })?;
            let row = match row {
                Some(r) => r,
                None => break,
            };

            if schema.is_empty() {
                let stmt_ref = row.as_ref();
                let n_cols = stmt_ref.column_count();
                schema.reserve(n_cols);
                buffers.reserve(n_cols);
                for i in 0..n_cols {
                    let name = stmt_ref
                        .column_name(i)
                        .map(|s| s.to_string())
                        .map_err(|e| DriverError::QueryFailed {
                            query: query.to_string(),
                            message: format!("could not read column name {}: {}", i, e),
                        })?;
                    let raw_type = stmt_ref.column_type(i);
                    let duck_type = DuckType::from(&raw_type);
                    let data_type = duck_type_to_mosaic(&duck_type);
                    schema.push(ColumnSchema {
                        name,
                        data_type,
                        nullable: true,
                    });
                    buffers.push(ColumnBuffer::new(data_type));
                }
            }

            for (i, schema_col) in schema.iter().enumerate() {
                let value = row.get_ref(i).map_err(|e| DriverError::QueryFailed {
                    query: query.to_string(),
                    message: format!("column {} read failed: {}", i, e),
                })?;
                buffers[i].push(value, &schema_col.name)?;
            }
            total_rows += 1;
        }
    }

    // Empty result set: schema is still empty. Mosaic surfaces empty-batch
    // results downstream as `fetch_batch -> Ok(None)` immediately, with
    // an empty schema vec. Stream D treats this as "no rows, no columns,
    // success" — a recipe `dry_run` against an empty table validates
    // cleanly but writes nothing.

    Ok(DuckDbDriver {
        path: path.to_path_buf(),
        query: query.to_string(),
        schema,
        buffers,
        cursor: 0,
        total_rows,
        cancelled: false,
    })
}

impl SourceDriver for DuckDbDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Ok(self.schema.clone())
    }

    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.cancelled || max_rows == 0 || self.cursor >= self.total_rows {
            return Ok(None);
        }
        let end = (self.cursor + max_rows).min(self.total_rows);
        let take = end - self.cursor;
        let columns = self
            .schema
            .iter()
            .zip(self.buffers.iter())
            .map(|(s, b)| Column {
                name: s.name.clone(),
                data: b.slice(self.cursor, end),
            })
            .collect();
        self.cursor = end;
        Ok(Some(RowBatch {
            columns,
            row_count: take,
        }))
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.cursor = self.total_rows;
    }
}

fn path_is_memory(path: &Path) -> bool {
    path.as_os_str() == ":memory:"
}

fn duck_type_to_mosaic(t: &DuckType) -> ColumnDataType {
    match t {
        DuckType::Boolean => ColumnDataType::Bool,
        DuckType::TinyInt
        | DuckType::SmallInt
        | DuckType::Int
        | DuckType::BigInt
        | DuckType::HugeInt
        | DuckType::UTinyInt
        | DuckType::USmallInt
        | DuckType::UInt
        | DuckType::UBigInt => ColumnDataType::I64,
        DuckType::Float | DuckType::Double | DuckType::Decimal => ColumnDataType::F64,
        // Everything else (text, dates, blobs, intervals, composites,
        // unknown extension types) renders to string at the boundary.
        _ => ColumnDataType::Str,
    }
}

#[derive(Debug)]
enum ColumnBuffer {
    F64(Vec<Option<f64>>),
    I64(Vec<Option<i64>>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}

impl ColumnBuffer {
    fn new(t: ColumnDataType) -> Self {
        match t {
            ColumnDataType::F64 => ColumnBuffer::F64(Vec::new()),
            ColumnDataType::I64 => ColumnBuffer::I64(Vec::new()),
            ColumnDataType::Str => ColumnBuffer::Str(Vec::new()),
            ColumnDataType::Bool => ColumnBuffer::Bool(Vec::new()),
        }
    }

    fn push(&mut self, value: ValueRef<'_>, column: &str) -> Result<(), DriverError> {
        match self {
            ColumnBuffer::Bool(v) => match value {
                ValueRef::Null => v.push(None),
                ValueRef::Boolean(b) => v.push(Some(b)),
                ValueRef::TinyInt(i) => v.push(Some(i != 0)),
                ValueRef::SmallInt(i) => v.push(Some(i != 0)),
                ValueRef::Int(i) => v.push(Some(i != 0)),
                ValueRef::BigInt(i) => v.push(Some(i != 0)),
                other => return Err(type_mismatch(column, "Bool", &other)),
            },
            ColumnBuffer::I64(v) => match value {
                ValueRef::Null => v.push(None),
                ValueRef::TinyInt(i) => v.push(Some(i as i64)),
                ValueRef::SmallInt(i) => v.push(Some(i as i64)),
                ValueRef::Int(i) => v.push(Some(i as i64)),
                ValueRef::BigInt(i) => v.push(Some(i)),
                ValueRef::HugeInt(i) => v.push(Some(i as i64)),
                ValueRef::UTinyInt(i) => v.push(Some(i as i64)),
                ValueRef::USmallInt(i) => v.push(Some(i as i64)),
                ValueRef::UInt(i) => v.push(Some(i as i64)),
                ValueRef::UBigInt(i) => v.push(Some(i as i64)),
                ValueRef::Boolean(b) => v.push(Some(if b { 1 } else { 0 })),
                other => return Err(type_mismatch(column, "I64", &other)),
            },
            ColumnBuffer::F64(v) => match value {
                ValueRef::Null => v.push(None),
                ValueRef::Float(f) => v.push(Some(f as f64)),
                ValueRef::Double(f) => v.push(Some(f)),
                ValueRef::TinyInt(i) => v.push(Some(i as f64)),
                ValueRef::SmallInt(i) => v.push(Some(i as f64)),
                ValueRef::Int(i) => v.push(Some(i as f64)),
                ValueRef::BigInt(i) => v.push(Some(i as f64)),
                ValueRef::Decimal(d) => v.push(Some(decimal_string_to_f64(&d.to_string()))),
                other => return Err(type_mismatch(column, "F64", &other)),
            },
            ColumnBuffer::Str(v) => match value {
                ValueRef::Null => v.push(None),
                ValueRef::Text(bytes) => v.push(Some(
                    std::str::from_utf8(bytes)
                        .map_err(|e| DriverError::TypeMismatch {
                            column: column.to_string(),
                            message: format!("invalid UTF-8 in text column: {}", e),
                        })?
                        .to_string(),
                )),
                ValueRef::Boolean(b) => v.push(Some(b.to_string())),
                ValueRef::TinyInt(i) => v.push(Some(i.to_string())),
                ValueRef::SmallInt(i) => v.push(Some(i.to_string())),
                ValueRef::Int(i) => v.push(Some(i.to_string())),
                ValueRef::BigInt(i) => v.push(Some(i.to_string())),
                ValueRef::HugeInt(i) => v.push(Some(i.to_string())),
                ValueRef::UTinyInt(i) => v.push(Some(i.to_string())),
                ValueRef::USmallInt(i) => v.push(Some(i.to_string())),
                ValueRef::UInt(i) => v.push(Some(i.to_string())),
                ValueRef::UBigInt(i) => v.push(Some(i.to_string())),
                ValueRef::Float(f) => v.push(Some(f.to_string())),
                ValueRef::Double(f) => v.push(Some(f.to_string())),
                ValueRef::Decimal(d) => v.push(Some(d.to_string())),
                ValueRef::Date32(d) => v.push(Some(d.to_string())),
                ValueRef::Time64(unit, t) => v.push(Some(format_time64(unit, t))),
                ValueRef::Timestamp(unit, t) => v.push(Some(format_time64(unit, t))),
                ValueRef::Blob(_) => {
                    return Err(DriverError::UnsupportedType {
                        column: column.to_string(),
                        type_name: "BLOB".to_string(),
                    });
                }
                other => v.push(Some(format!("{:?}", other))),
            },
        }
        Ok(())
    }

    fn slice(&self, start: usize, end: usize) -> ColumnData {
        match self {
            ColumnBuffer::F64(v) => ColumnData::F64(v[start..end].to_vec()),
            ColumnBuffer::I64(v) => ColumnData::I64(v[start..end].to_vec()),
            ColumnBuffer::Str(v) => ColumnData::Str(v[start..end].to_vec()),
            ColumnBuffer::Bool(v) => ColumnData::Bool(v[start..end].to_vec()),
        }
    }
}

fn type_mismatch(column: &str, expected: &str, val: &ValueRef<'_>) -> DriverError {
    DriverError::TypeMismatch {
        column: column.to_string(),
        message: format!(
            "cannot coerce DuckDB value {:?} into column type {}",
            val, expected
        ),
    }
}

/// Convert a stringified `DECIMAL` (as DuckDB renders it via
/// `rust_decimal::Decimal::to_string`) to `f64`. Uses lossy parse — Mosaic
/// stores all numerics as `f64`, so any precision loss is part of the
/// type-coercion contract documented in the module schema-inference rule.
fn decimal_string_to_f64(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

fn format_time64(unit: TimeUnit, t: i64) -> String {
    match unit {
        TimeUnit::Second => format!("{}s", t),
        TimeUnit::Millisecond => format!("{}ms", t),
        TimeUnit::Microsecond => format!("{}us", t),
        TimeUnit::Nanosecond => format!("{}ns", t),
    }
}
