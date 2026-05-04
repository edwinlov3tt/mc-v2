//! DuckDB-federated Postgres source driver.
//!
//! **First-class driver, not a fallback.** This driver enables
//! cross-database joins where DuckDB mediates between a local DuckDB
//! database (typically a parquet/csv-attached read store) and a remote
//! Postgres instance via the `postgres_scanner` extension. Queries are
//! issued against the DuckDB engine, which transparently pushes
//! Postgres-side filters/projections.
//!
//! Use `postgres_driver` (NOT this driver) for plain SELECTs against a
//! Postgres database.
//!
//! ## Wire-up
//!
//! On construction this driver:
//! 1. Opens or creates the DuckDB database at `duckdb_path`.
//! 2. `INSTALL postgres;` then `LOAD postgres;` (idempotent — repeated
//!    calls are no-ops if the extension is already installed).
//! 3. `ATTACH '<pg_dsn>' AS pg (TYPE postgres, READ_ONLY);` — names the
//!    remote as the schema `pg`.
//! 4. Materialises the user `query` against this attached connection
//!    (delegating to `DuckDbDriver::from_prepared_connection`).
//!
//! Queries should reference Postgres tables as `pg.<schema>.<table>`
//! (e.g., `SELECT * FROM pg.public.orders`). DuckDB mixes these freely
//! with local tables in a single SQL statement, which is the point of
//! using this driver over `postgres_driver`.
//!
//! ## Schema inference
//!
//! Inherited from `DuckDbDriver` (the federated query is executed by
//! DuckDB; column types are reported by DuckDB's prepared-statement
//! metadata after the federation translation). See
//! `duckdb_driver` module docs.
//!
//! ## Bundled DuckDB extension availability
//!
//! `libduckdb-sys 1.1.1` (`bundled` feature) ships a DuckDB binary that
//! supports `INSTALL postgres; LOAD postgres;` over the network. The
//! extension is downloaded on first use (DuckDB's standard behavior). No
//! Phase 5A code path requires offline-bundled extensions.

use std::path::{Path, PathBuf};

use crate::duckdb_driver::{open_connection, DuckDbDriver};
use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a DuckDB-federated-Postgres driver.
///
/// `duckdb_path` is the local DuckDB database file (use `:memory:` for an
/// ephemeral federation hub). `pg_dsn` is a libpq-format DSN parsed by
/// DuckDB's `postgres_scanner` extension. `query` is executed by DuckDB
/// and may reference Postgres tables as `pg.<schema>.<table>`.
pub fn duckdb_postgres_driver(
    duckdb_path: &Path,
    pg_dsn: &str,
    query: &str,
) -> Result<DuckdbPostgresDriver, DriverError> {
    DuckdbPostgresDriver::new(duckdb_path, pg_dsn, query)
}

/// DuckDB-federated-Postgres driver. After construction, the underlying
/// DuckDB connection is closed; `fetch_batch` drains buffered columns
/// (delegated to the inner `DuckDbDriver`).
#[derive(Debug)]
pub struct DuckdbPostgresDriver {
    #[allow(dead_code)]
    duckdb_path: PathBuf,
    inner: DuckDbDriver,
}

impl DuckdbPostgresDriver {
    fn new(duckdb_path: &Path, pg_dsn: &str, query: &str) -> Result<Self, DriverError> {
        let conn = open_connection(duckdb_path)?;

        // INSTALL is idempotent in DuckDB; both INSTALL and LOAD are
        // required for first-time use, and harmless on repeat calls.
        conn.execute_batch("INSTALL postgres; LOAD postgres;")
            .map_err(|e| DriverError::ExtensionLoadFailed {
                extension: "postgres".to_string(),
                message: e.to_string(),
            })?;

        // ATTACH the remote Postgres as schema `pg`. We DETACH any
        // pre-existing attachment first so re-runs are clean.
        let _ = conn.execute_batch("DETACH pg;");
        let attach_sql = format!(
            "ATTACH '{}' AS pg (TYPE postgres, READ_ONLY);",
            escape_single_quotes(pg_dsn)
        );
        conn.execute_batch(&attach_sql)
            .map_err(|e| DriverError::ConnectionFailed {
                target: format!("duckdb→postgres ({})", redact_dsn(pg_dsn)),
                message: e.to_string(),
            })?;

        let inner = DuckDbDriver::from_prepared_connection(&conn, duckdb_path, query)?;

        Ok(DuckdbPostgresDriver {
            duckdb_path: duckdb_path.to_path_buf(),
            inner,
        })
    }
}

impl SourceDriver for DuckdbPostgresDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        self.inner.schema()
    }

    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        self.inner.fetch_batch(max_rows)
    }

    fn cancel(&mut self) {
        self.inner.cancel();
    }
}

/// Escape single quotes in a DSN so it can be safely embedded in a
/// DuckDB ATTACH statement. DuckDB's SQL parser treats `''` as a
/// literal single quote inside `'…'` strings.
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "''")
}

fn redact_dsn(dsn: &str) -> String {
    if let Some(scheme_end) = dsn.find("://") {
        if let Some(at_pos) = dsn[scheme_end + 3..].find('@') {
            let auth_end = scheme_end + 3 + at_pos;
            return format!("{}***@{}", &dsn[..scheme_end + 3], &dsn[auth_end + 1..]);
        }
    }
    if dsn.contains("password=") {
        return format!("{}…", &dsn[..dsn.len().min(32)]);
    }
    dsn.to_string()
}
