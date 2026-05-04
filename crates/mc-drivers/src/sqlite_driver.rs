//! SQLite source driver.
//!
//! Wraps `rusqlite` with the `bundled` feature (statically linked SQLite —
//! zero system library required). Returns rows from a user-supplied SQL
//! query.
//!
//! ## Buffering model
//!
//! All rows are materialised at construction time into Mosaic-native
//! column buffers; `fetch_batch` then drains those buffers in chunks of
//! at most `max_rows`. This is the reference-implementation pattern
//! defined by the handoff: a future production driver may stream
//! row-by-row, but Phase 5A trades memory for simplicity. The trait
//! contract is unaffected — callers see the same incremental
//! `fetch_batch → fetch_batch → Ok(None)` sequence.
//!
//! ## Schema inference rule
//!
//! Schema is taken from the *declared* column types of the prepared
//! statement (`sqlite3_column_decltype`). The mapping:
//!
//! | SQLite declared type contains | Mosaic `ColumnDataType` |
//! | --- | --- |
//! | `INT` (`INTEGER`, `BIGINT`, …) | `I64` |
//! | `BOOL` | `Bool` |
//! | `REAL`, `FLOA`, `DOUB`, `NUMERIC`, `DECIMAL` | `F64` |
//! | anything else (`TEXT`, `CHAR`, `CLOB`, no decl) | `Str` |
//!
//! NULLability cannot be reliably read from a SQLite query (declared
//! `NOT NULL` only applies on tables, not arbitrary `SELECT` results), so
//! all columns are reported as `nullable: true`.

use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::{
    Column, ColumnData, ColumnDataType, ColumnSchema, DriverError, RowBatch, SourceDriver,
};

/// Construct a SQLite driver. Opens `path` for the lifetime of the call,
/// runs `query`, and materialises all rows into Mosaic column buffers
/// (see module docs). Pass `Path::new(":memory:")` for an in-memory
/// database (useful only for `ATTACH`-driven setups; an empty in-memory
/// database is rarely useful as a source).
pub fn sqlite_driver(path: &Path, query: &str) -> Result<SqliteDriver, DriverError> {
    SqliteDriver::new(path, query)
}

/// SQLite driver. After construction, the underlying connection is
/// closed; `fetch_batch` drains buffered columns.
#[derive(Debug)]
pub struct SqliteDriver {
    // `path` and `query` are surfaced via Debug for diagnostic purposes
    // (e.g., panic messages during Stream D integration); they are set
    // once at construction and not otherwise read by the driver's hot
    // path.
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

impl SqliteDriver {
    fn new(path: &Path, query: &str) -> Result<Self, DriverError> {
        if !path_is_memory(path) && !path.exists() {
            return Err(DriverError::SourceFileNotFound {
                path: path.to_path_buf(),
                message: "no such file".to_string(),
            });
        }
        let conn = Connection::open(path).map_err(|e| DriverError::ConnectionFailed {
            target: path.display().to_string(),
            message: e.to_string(),
        })?;

        let mut stmt = conn.prepare(query).map_err(|e| DriverError::QueryFailed {
            query: query.to_string(),
            message: e.to_string(),
        })?;

        let schema = build_schema(&stmt, query)?;
        let mut buffers: Vec<ColumnBuffer> = schema
            .iter()
            .map(|s| ColumnBuffer::new(s.data_type))
            .collect();

        let mut rows = stmt.raw_query();
        let mut total_rows = 0usize;
        loop {
            let row = rows.next().map_err(|e| DriverError::QueryFailed {
                query: query.to_string(),
                message: e.to_string(),
            })?;
            let row = match row {
                Some(r) => r,
                None => break,
            };
            for (i, schema_col) in schema.iter().enumerate() {
                let value = row.get_ref(i).map_err(|e| DriverError::QueryFailed {
                    query: query.to_string(),
                    message: format!("column {} read failed: {}", i, e),
                })?;
                buffers[i].push(value, &schema_col.name)?;
            }
            total_rows += 1;
        }

        Ok(SqliteDriver {
            path: path.to_path_buf(),
            query: query.to_string(),
            schema,
            buffers,
            cursor: 0,
            total_rows,
            cancelled: false,
        })
    }
}

impl SourceDriver for SqliteDriver {
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

fn build_schema(
    stmt: &rusqlite::Statement<'_>,
    query: &str,
) -> Result<Vec<ColumnSchema>, DriverError> {
    let n_cols = stmt.column_count();
    let mut schema = Vec::with_capacity(n_cols);
    let cols = stmt.columns();
    for i in 0..n_cols {
        let name =
            stmt.column_name(i)
                .map(|s| s.to_string())
                .map_err(|e| DriverError::QueryFailed {
                    query: query.to_string(),
                    message: format!("could not read column name {}: {}", i, e),
                })?;
        let decl = cols
            .get(i)
            .and_then(|c| c.decl_type().map(|s| s.to_string()))
            .unwrap_or_default();
        schema.push(ColumnSchema {
            name,
            data_type: decl_to_type(&decl),
            nullable: true,
        });
    }
    Ok(schema)
}

fn decl_to_type(decl: &str) -> ColumnDataType {
    let upper = decl.to_ascii_uppercase();
    if upper.contains("INT") {
        ColumnDataType::I64
    } else if upper.contains("BOOL") {
        ColumnDataType::Bool
    } else if upper.contains("REAL")
        || upper.contains("FLOA")
        || upper.contains("DOUB")
        || upper.contains("NUMERIC")
        || upper.contains("DECIMAL")
    {
        ColumnDataType::F64
    } else {
        ColumnDataType::Str
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
        match (self, value) {
            (ColumnBuffer::F64(v), ValueRef::Null) => v.push(None),
            (ColumnBuffer::F64(v), ValueRef::Real(f)) => v.push(Some(f)),
            (ColumnBuffer::F64(v), ValueRef::Integer(i)) => v.push(Some(i as f64)),
            (ColumnBuffer::I64(v), ValueRef::Null) => v.push(None),
            (ColumnBuffer::I64(v), ValueRef::Integer(i)) => v.push(Some(i)),
            (ColumnBuffer::Bool(v), ValueRef::Null) => v.push(None),
            (ColumnBuffer::Bool(v), ValueRef::Integer(i)) => v.push(Some(i != 0)),
            (ColumnBuffer::Str(v), ValueRef::Null) => v.push(None),
            (ColumnBuffer::Str(v), ValueRef::Text(bytes)) => v.push(Some(
                std::str::from_utf8(bytes)
                    .map_err(|e| DriverError::TypeMismatch {
                        column: column.to_string(),
                        message: format!("invalid UTF-8 in text column: {}", e),
                    })?
                    .to_string(),
            )),
            (ColumnBuffer::Str(v), ValueRef::Integer(i)) => v.push(Some(i.to_string())),
            (ColumnBuffer::Str(v), ValueRef::Real(f)) => v.push(Some(f.to_string())),
            (ColumnBuffer::Str(_), ValueRef::Blob(_)) => {
                return Err(DriverError::UnsupportedType {
                    column: column.to_string(),
                    type_name: "BLOB".to_string(),
                });
            }
            (buf, val) => {
                return Err(DriverError::TypeMismatch {
                    column: column.to_string(),
                    message: format!(
                        "cannot coerce SQLite value {:?} into column type {:?}",
                        val,
                        buf.kind()
                    ),
                });
            }
        }
        Ok(())
    }

    fn kind(&self) -> ColumnDataType {
        match self {
            ColumnBuffer::F64(_) => ColumnDataType::F64,
            ColumnBuffer::I64(_) => ColumnDataType::I64,
            ColumnBuffer::Str(_) => ColumnDataType::Str,
            ColumnBuffer::Bool(_) => ColumnDataType::Bool,
        }
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
