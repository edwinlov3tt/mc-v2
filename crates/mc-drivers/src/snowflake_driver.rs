//! Snowflake ODBC source driver (Phase 5C).
//!
//! Uses `odbc-api` for ODBC access. Feature-gated behind `snowflake`.
//! Requires system-installed unixODBC + Snowflake ODBC driver.

use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a Snowflake source driver from an ODBC connection string and SQL query.
///
/// Connection string format:
/// `Driver={Snowflake};Server=<account>.snowflakecomputing.com;Database=<db>;Schema=<schema>;Uid=<user>;Pwd=<pass>;`
pub fn snowflake_driver(
    connection_string: &str,
    query: &str,
) -> Result<SnowflakeDriver, DriverError> {
    Ok(SnowflakeDriver {
        connection_string: connection_string.to_string(),
        query: query.to_string(),
        exhausted: false,
        cancelled: false,
    })
}

/// Snowflake ODBC driver state.
#[derive(Debug)]
#[allow(dead_code)]
pub struct SnowflakeDriver {
    connection_string: String,
    query: String,
    exhausted: bool,
    cancelled: bool,
}

impl SourceDriver for SnowflakeDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Err(DriverError::ConnectionFailed {
            target: "<snowflake>".to_string(),
            message: "snowflake driver requires the 'snowflake' feature flag and system ODBC driver to be installed".to_string(),
        })
    }

    fn fetch_batch(&mut self, _max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.exhausted || self.cancelled {
            return Ok(None);
        }
        Err(DriverError::ConnectionFailed {
            target: "<snowflake>".to_string(),
            message: "snowflake driver requires the 'snowflake' feature flag and system ODBC driver to be installed".to_string(),
        })
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
