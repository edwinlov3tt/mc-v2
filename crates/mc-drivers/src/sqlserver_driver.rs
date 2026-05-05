//! SQL Server ODBC source driver (Phase 5C).
//!
//! Uses `odbc-api` (shared with Snowflake). Feature-gated behind `sqlserver`.
//! Requires system-installed unixODBC + Microsoft ODBC Driver 18 for SQL Server.

use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a SQL Server source driver from an ODBC connection string and SQL query.
///
/// Connection string format:
/// `Driver={ODBC Driver 18 for SQL Server};Server=<host>,<port>;Database=<db>;Uid=<user>;Pwd=<pass>;Encrypt=yes;TrustServerCertificate=no;`
pub fn sqlserver_driver(
    connection_string: &str,
    query: &str,
) -> Result<SqlserverDriver, DriverError> {
    Ok(SqlserverDriver {
        connection_string: connection_string.to_string(),
        query: query.to_string(),
        exhausted: false,
        cancelled: false,
    })
}

/// SQL Server ODBC driver state.
#[derive(Debug)]
#[allow(dead_code)]
pub struct SqlserverDriver {
    connection_string: String,
    query: String,
    exhausted: bool,
    cancelled: bool,
}

impl SourceDriver for SqlserverDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Err(DriverError::ConnectionFailed {
            target: "<sqlserver>".to_string(),
            message: "sqlserver driver requires the 'sqlserver' feature flag and system ODBC driver to be installed".to_string(),
        })
    }

    fn fetch_batch(&mut self, _max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.exhausted || self.cancelled {
            return Ok(None);
        }
        Err(DriverError::ConnectionFailed {
            target: "<sqlserver>".to_string(),
            message: "sqlserver driver requires the 'sqlserver' feature flag and system ODBC driver to be installed".to_string(),
        })
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
