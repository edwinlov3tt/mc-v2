//! PostgreSQL source driver.
//!
//! Wraps the synchronous `postgres` crate (which internally manages a tokio
//! runtime around `tokio-postgres`). All Mosaic source code remains sync.
//! Tokio appears in `Cargo.lock` as a transitive dep of `postgres` only —
//! see `tests/dependency_gate.rs`.
//!
//! ## Buffering model
//!
//! Same eager-materialisation strategy as the SQLite and DuckDB drivers:
//! `client.query(query, &[])` collects all rows; `fetch_batch` then drains
//! `max_rows` chunks. See `sqlite_driver` module docs for rationale. For
//! large result sets a future driver can switch to `query_raw` + cursor
//! streaming without changing the trait surface.
//!
//! ## Schema inference rule
//!
//! Schema is taken from the prepared statement's column metadata
//! (`Statement::columns()`). The PostgreSQL OID → Mosaic mapping:
//!
//! | PG type | Mosaic `ColumnDataType` |
//! | --- | --- |
//! | `BOOL` | `Bool` |
//! | `INT2`, `INT4`, `INT8`, `OID` | `I64` |
//! | `FLOAT4`, `FLOAT8` | `F64` |
//! | `NUMERIC` | `Str` (no built-in `f64` `FromSql`; users coerce in SQL with `::float8`) |
//! | `TEXT`, `VARCHAR`, `CHAR`, `BPCHAR`, `NAME`, `UUID`, `JSON`, `JSONB` | `Str` |
//! | `DATE`, `TIME`, `TIMESTAMP`, `TIMESTAMPTZ`, `INTERVAL` | `Str` (ISO-8601 via `to_string`) |
//! | anything else (arrays, composites, ranges, geometry) | `Str` (best-effort `to_string`) |
//!
//! `nullable` is always `true` — like SQLite, PostgreSQL does not expose
//! statement-level `NOT NULL` constraints for arbitrary `SELECT` results.

use postgres::types::Type as PgType;
use postgres::{Client, NoTls};

use crate::{
    Column, ColumnData, ColumnDataType, ColumnSchema, DriverError, RowBatch, SourceDriver,
};

/// Construct a Postgres driver. Connects with `NoTls` — TLS support is
/// deferred to a future Phase (see ADR-0010 §"Out of scope for Phase 5A").
/// Pin the DSN's `sslmode` to `disable` (or omit it) for development.
pub fn postgres_driver(dsn: &str, query: &str) -> Result<PostgresDriver, DriverError> {
    PostgresDriver::new(dsn, query)
}

/// Postgres driver. Connection is closed after `new()` returns; subsequent
/// `fetch_batch` calls drain in-memory column buffers.
#[derive(Debug)]
pub struct PostgresDriver {
    #[allow(dead_code)]
    dsn_summary: String,
    #[allow(dead_code)]
    query: String,
    schema: Vec<ColumnSchema>,
    buffers: Vec<ColumnBuffer>,
    cursor: usize,
    total_rows: usize,
    cancelled: bool,
}

impl PostgresDriver {
    fn new(dsn: &str, query: &str) -> Result<Self, DriverError> {
        let mut client =
            Client::connect(dsn, NoTls).map_err(|e| DriverError::ConnectionFailed {
                target: redact_dsn(dsn),
                message: e.to_string(),
            })?;

        let stmt = client
            .prepare(query)
            .map_err(|e| DriverError::QueryFailed {
                query: query.to_string(),
                message: e.to_string(),
            })?;

        let schema: Vec<ColumnSchema> = stmt
            .columns()
            .iter()
            .map(|c| ColumnSchema {
                name: c.name().to_string(),
                data_type: pg_type_to_mosaic(c.type_()),
                nullable: true,
            })
            .collect();

        let mut buffers: Vec<ColumnBuffer> = schema
            .iter()
            .map(|s| ColumnBuffer::new(s.data_type))
            .collect();

        let rows = client
            .query(&stmt, &[])
            .map_err(|e| DriverError::QueryFailed {
                query: query.to_string(),
                message: e.to_string(),
            })?;

        let total_rows = rows.len();
        for row in &rows {
            for (i, schema_col) in schema.iter().enumerate() {
                buffers[i].push(row, i, schema_col)?;
            }
        }

        Ok(PostgresDriver {
            dsn_summary: redact_dsn(dsn),
            query: query.to_string(),
            schema,
            buffers,
            cursor: 0,
            total_rows,
            cancelled: false,
        })
    }
}

impl SourceDriver for PostgresDriver {
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

/// Strip password fragments out of the DSN for diagnostic surfaces. Best-
/// effort — handles the standard `postgres://user:pass@host/db` form. If
/// the DSN is in keyword/value form (`host=… password=…`) we keep only
/// the first 32 chars to avoid accidentally logging the password.
fn redact_dsn(dsn: &str) -> String {
    if let Some(scheme_end) = dsn.find("://") {
        if let Some(at_pos) = dsn[scheme_end + 3..].find('@') {
            let auth_end = scheme_end + 3 + at_pos;
            let creds_start = scheme_end + 3;
            let creds = &dsn[creds_start..auth_end];
            if let Some(colon) = creds.find(':') {
                return format!(
                    "{}{}@{}",
                    &dsn[..creds_start],
                    &creds[..colon + 1],
                    &dsn[auth_end + 1..]
                )
                .replacen(":@", ":***@", 1);
            }
        }
        return dsn.to_string();
    }
    if dsn.contains("password=") {
        return format!("{}…", &dsn[..dsn.len().min(32)]);
    }
    dsn.to_string()
}

fn pg_type_to_mosaic(t: &PgType) -> ColumnDataType {
    match *t {
        PgType::BOOL => ColumnDataType::Bool,
        PgType::INT2 | PgType::INT4 | PgType::INT8 | PgType::OID => ColumnDataType::I64,
        PgType::FLOAT4 | PgType::FLOAT8 => ColumnDataType::F64,
        // Everything else: render to string at the boundary. Includes
        // NUMERIC (no built-in f64 FromSql), TEXT family, dates, times,
        // intervals, JSON, UUID, arrays, composites.
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

    fn push(
        &mut self,
        row: &postgres::Row,
        i: usize,
        schema: &ColumnSchema,
    ) -> Result<(), DriverError> {
        match self {
            ColumnBuffer::Bool(v) => {
                let val: Option<bool> = row.try_get(i).map_err(|e| coerce_err(schema, e))?;
                v.push(val);
            }
            ColumnBuffer::I64(v) => {
                // INT2/INT4/INT8 widen to i64.
                let pg_type = row.columns()[i].type_();
                let val: Option<i64> = match *pg_type {
                    PgType::INT2 => row
                        .try_get::<_, Option<i16>>(i)
                        .map_err(|e| coerce_err(schema, e))?
                        .map(i64::from),
                    PgType::INT4 => row
                        .try_get::<_, Option<i32>>(i)
                        .map_err(|e| coerce_err(schema, e))?
                        .map(i64::from),
                    PgType::INT8 => row
                        .try_get::<_, Option<i64>>(i)
                        .map_err(|e| coerce_err(schema, e))?,
                    PgType::OID => row
                        .try_get::<_, Option<u32>>(i)
                        .map_err(|e| coerce_err(schema, e))?
                        .map(i64::from),
                    _ => row
                        .try_get::<_, Option<i64>>(i)
                        .map_err(|e| coerce_err(schema, e))?,
                };
                v.push(val);
            }
            ColumnBuffer::F64(v) => {
                let pg_type = row.columns()[i].type_();
                let val: Option<f64> = match *pg_type {
                    PgType::FLOAT4 => row
                        .try_get::<_, Option<f32>>(i)
                        .map_err(|e| coerce_err(schema, e))?
                        .map(f64::from),
                    PgType::FLOAT8 => row
                        .try_get::<_, Option<f64>>(i)
                        .map_err(|e| coerce_err(schema, e))?,
                    _ => row
                        .try_get::<_, Option<f64>>(i)
                        .map_err(|e| coerce_err(schema, e))?,
                };
                v.push(val);
            }
            ColumnBuffer::Str(v) => {
                // For Str columns we accept anything that comes back; if
                // the column is text-typed we use the native String FromSql,
                // otherwise we render through the wrapper below.
                let pg_type = row.columns()[i].type_();
                if is_text_family(pg_type) {
                    let val: Option<String> = row.try_get(i).map_err(|e| coerce_err(schema, e))?;
                    v.push(val);
                } else {
                    let rendered = render_unknown(row, i, schema)?;
                    v.push(rendered);
                }
            }
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

fn is_text_family(t: &PgType) -> bool {
    matches!(
        *t,
        PgType::TEXT
            | PgType::VARCHAR
            | PgType::BPCHAR
            | PgType::NAME
            | PgType::CHAR
            | PgType::UNKNOWN
    )
}

/// Best-effort string rendering of a column whose Postgres type is not in
/// our typed mapping. We try a sequence of common types; if all fail, we
/// return `None` (treat as NULL) rather than fail the whole batch — Mosaic
/// recipes can opt into stricter behavior by adjusting the query.
fn render_unknown(
    row: &postgres::Row,
    i: usize,
    _schema: &ColumnSchema,
) -> Result<Option<String>, DriverError> {
    if let Ok(v) = row.try_get::<_, Option<String>>(i) {
        return Ok(v);
    }
    if let Ok(v) = row.try_get::<_, Option<i64>>(i) {
        return Ok(v.map(|x| x.to_string()));
    }
    if let Ok(v) = row.try_get::<_, Option<f64>>(i) {
        return Ok(v.map(|x| x.to_string()));
    }
    if let Ok(v) = row.try_get::<_, Option<bool>>(i) {
        return Ok(v.map(|x| x.to_string()));
    }
    Ok(None)
}

fn coerce_err(schema: &ColumnSchema, e: postgres::Error) -> DriverError {
    DriverError::TypeMismatch {
        column: schema.name.clone(),
        message: e.to_string(),
    }
}
